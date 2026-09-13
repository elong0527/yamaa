from __future__ import annotations

import runpy
from pathlib import Path

import polars as pl
import pytest
from polars.testing import assert_frame_equal

from yamaa.io import ProjectResources, load_source_tables
from yamaa.planning import ExecutionDiagnostic
from yamaa.runtime import ExecutionFailure, execute_with_source_provider
from yamaa.specification import (
    SpecificationError,
    ValidationDiagnostic,
    load_specification,
)
from yamaa.specification._yaml import read_yaml_document

EXAMPLES = Path(__file__).parents[2] / "yaml/examples"
SCHEMA_ROOT = EXAMPLES.parent

KNOWN_REQUIREMENT_GAPS = {
    "negative-adsl-cyclic-parent": ("R017-42", None),
    "negative-adsl-inherited-output": ("R017-44", None),
    "negative-adsl-invalid-parent-clear": ("R017-47", None),
    "negative-adsl-parent-version-mismatch": ("R017-43", None),
    "negative-adsl-randomization-date-retyped": ("R014-10", None),
    "negative-adsl-remote-parent": ("R017-40", None),
    "negative-function-contract-mismatch": ("R018-38", None),
    "negative-keys-missing-value": ("R005-52", "R001-43"),
    "negative-output-duplicate-subject": ("R005-52", "R001-44"),
}


def positive_runners() -> tuple[Path, ...]:
    return tuple(
        runner
        for runner in sorted(EXAMPLES.glob("*/run.py"))
        if not (runner.parent / "expected/error.yaml").exists()
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
    try:
        result = execute_with_source_provider(
            loaded.specification,
            lambda datasets: load_source_tables(datasets, resources),
        )
    except NotImplementedError:
        return None
    if isinstance(result, ExecutionFailure):
        return result.diagnostics[0]
    return None


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

    assert set(outputs) == set(expected)
    for name, expected_path in expected.items():
        actual = outputs[name]
        committed = pl.read_csv(expected_path, schema=actual.schema)
        assert_frame_equal(actual, committed, check_exact=True)
