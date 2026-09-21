from __future__ import annotations

import runpy
from pathlib import Path

import polars as pl
import pytest
from polars.testing import assert_frame_equal

from yamaa import yamaa_domain
from yamaa.functions import execute_with_project_functions, select_project_root
from yamaa.io import ProjectResources, load_source_tables
from yamaa.io.csv import fixed_point
from yamaa.planning import ExecutionDiagnostic
from yamaa.runtime import (
    ExecutionFailure,
    ExecutionUnsupported,
    execute_with_source_provider,
)
from yamaa.specification import (
    SpecificationError,
    ValidationDiagnostic,
    load_specification,
)
from yamaa.specification._yaml import read_yaml_document

EXAMPLES = Path(__file__).parents[2] / "benchmarks"
SCHEMA_ROOT = EXAMPLES.parent / "yaml"

KNOWN_REQUIREMENT_GAPS: dict[str, tuple[str | None, str | None]] = {}

KNOWN_SPEC_PATH_GAPS: dict[
    str, tuple[tuple[str, ...] | None, tuple[str, ...] | None]
] = {}


def positive_runners() -> tuple[Path, ...]:
    return tuple(
        runner
        for runner in sorted(EXAMPLES.glob("*/run.py"))
        if not (runner.parent / "expected/error.yaml").exists()
    )


def entry_spec(example: Path) -> Path:
    """The specification a benchmark's run.py executes.

    Mirrors the dashboard's entry resolution: `spec.yaml` when present,
    otherwise the `spec_*.yaml` file no other file names as a parent.
    """
    single = example / "spec.yaml"
    if single.exists():
        return single
    specs = sorted(example.glob("spec_*.yaml"))
    parented = set()
    for path in specs:
        document = read_yaml_document(path)
        parents = document.get("parents", []) if isinstance(document, dict) else []
        if isinstance(parents, str):
            parents = [parents]
        parented.update(
            Path(parent).name
            for parent in parents
            if isinstance(parent, str) and parent
        )
    entries = [path for path in specs if path.name not in parented]
    return entries[0] if entries else specs[0]


def positive_examples() -> tuple[Path, ...]:
    return tuple(
        example
        for example in sorted(EXAMPLES.iterdir())
        if example.is_dir()
        and list(example.glob("spec*.yaml"))
        and not (example / "expected/error.yaml").exists()
    )


def negative_contracts() -> tuple[Path, ...]:
    return tuple(sorted(EXAMPLES.glob("negative-*/expected/error.yaml")))


def _negative_diagnostic(
    example: Path,
) -> ValidationDiagnostic | ExecutionDiagnostic | None:
    try:
        loaded = load_specification(example / "spec.yaml", SCHEMA_ROOT)
    except SpecificationError as error:
        return error.diagnostics[0]
    resources = ProjectResources(example)

    def provide(datasets):
        return load_source_tables(datasets, resources)

    # REQ-0663: the runner selects the project root, so a negative example
    # that commits one is executed against it rather than reported as an
    # unimplemented call.
    project_root = select_project_root(example)
    if project_root is None:
        result = execute_with_source_provider(loaded.specification, provide)
    else:
        result = execute_with_project_functions(
            loaded.specification, provide, project_root, SCHEMA_ROOT
        )
    if isinstance(result, ExecutionUnsupported):
        return None
    assert isinstance(result, ExecutionFailure)
    return result.diagnostics[0]


def test_negative_example_requirements_match_committed_contracts() -> None:
    mismatches = {}
    for contract_path in negative_contracts():
        contract = read_yaml_document(contract_path)
        assert isinstance(contract, dict)
        diagnostic = _negative_diagnostic(contract_path.parents[1])
        actual = diagnostic.requirement if diagnostic is not None else None
        expected = contract["requirement"]
        if actual != expected:
            mismatches[contract_path.parents[1].name] = (expected, actual)
    assert mismatches == KNOWN_REQUIREMENT_GAPS


def test_producer_schema_without_workflow_is_unsupported() -> None:
    example = EXAMPLES / "adam-adsl-randomization"
    loaded = load_specification(example / "spec.yaml", SCHEMA_ROOT)
    resources = ProjectResources(example)

    result = execute_with_source_provider(
        loaded.specification,
        lambda datasets: load_source_tables(datasets, resources),
    )

    assert isinstance(result, ExecutionUnsupported)
    assert tuple(feature.model_dump(mode="python") for feature in result.features) == (
        {
            "operation": "workflow_schema_resolution",
            "spec_path": "input.DM.schema",
        },
    )
    assert result.handler_counts == ()


def test_negative_example_spec_paths_match_committed_contracts() -> None:
    mismatches = {}
    for contract_path in negative_contracts():
        contract = read_yaml_document(contract_path)
        assert isinstance(contract, dict)
        diagnostic = _negative_diagnostic(contract_path.parents[1])
        actual = diagnostic.spec_paths if diagnostic is not None else None
        expected = tuple(contract["spec_paths"])
        if actual != expected:
            mismatches[contract_path.parents[1].name] = (expected, actual)
    assert mismatches == KNOWN_SPEC_PATH_GAPS


def _reported_frame(frame: pl.DataFrame, decimals: int) -> pl.DataFrame:
    """Round every float column the way R020's ``output.decimals`` writes it.

    The committed CSV carries reported values (rounded once, at write,
    half away from zero); the run.py frame carries the unrounded engine
    values R011-28 requires. Reusing ``fixed_point`` keeps the test's
    rounding identical to the artifact writer's.
    """
    return frame.with_columns(
        [
            pl.col(name).map_elements(
                lambda value: float(fixed_point(value, decimals)),
                return_dtype=pl.Float64,
            )
            for name, dtype in frame.schema.items()
            if dtype == pl.Float64
        ]
    )


@pytest.mark.parametrize(
    "runner",
    positive_runners(),
    ids=lambda runner: runner.parent.name,
)
def test_positive_example_outputs_match_expected_csvs(
    runner: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    example = runner.parent
    expected = {
        path.stem: path for path in sorted((example / "expected").glob("*.csv"))
    }
    assert expected, f"{example.name} has run.py but no expected CSV"

    monkeypatch.chdir(example)
    namespace = runpy.run_path(runner.name)
    outputs = {
        name: value
        for name, value in namespace.items()
        if not name.startswith("_") and isinstance(value, pl.DataFrame)
    }

    specification = load_specification(entry_spec(example), SCHEMA_ROOT).specification
    if specification.output.decimals is not None:
        # R020 rounds every float column once, at write, half away from
        # zero; the committed CSV carries those reported values while the
        # run.py frame carries the unrounded engine values. Round the same
        # way before comparing so a decimals benchmark can carry run.py.
        outputs = {
            name: _reported_frame(frame, specification.output.decimals)
            for name, frame in outputs.items()
        }
    declared_log = specification.output.violation_log
    if declared_log is not None:
        # A violation sidecar is a second artifact the run.py convention
        # does not name: its stem is rarely a valid variable name, and the
        # facade below is what exposes it. The primary output still comes
        # from run.py so the file stays the standard four lines.
        log_stem = Path(declared_log).stem
        assert set(outputs) == set(expected) - {log_stem}
        actual_log = yamaa_domain(
            entry_spec(example), schema_root=SCHEMA_ROOT
        ).violation_log
        assert actual_log is not None
        committed_log = pl.read_csv(expected[log_stem], schema=actual_log.schema)
        assert_frame_equal(actual_log, committed_log, check_exact=True)
    else:
        assert set(outputs) == set(expected)
    for name, expected_path in expected.items():
        if declared_log is not None and name == Path(declared_log).stem:
            continue
        actual = outputs[name]
        committed = pl.read_csv(expected_path, schema=actual.schema)
        assert_frame_equal(actual, committed, check_exact=True)


def test_every_positive_example_carries_a_runner() -> None:
    """A positive benchmark that executes commits the snippet that runs it.

    `benchmarks/agents.md` makes `run.py` the mark of a benchmark whose entry
    executes and matches its artifact. Every positive benchmark does, so a
    missing runner is a benchmark that stopped executing rather than one
    nobody wrote a runner for.
    """
    missing = [
        example.name
        for example in positive_examples()
        if not (example / "run.py").is_file()
    ]

    assert missing == []
