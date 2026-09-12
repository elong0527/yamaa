"""Execute the committed examples that exercise the registered expressions.

Every case here runs the real public API end to end -- load the committed
specification, read its committed input, execute, and compare what comes out
with the committed artifact or error contract. Reading `expected/` happens
here, in the test, and never in runtime code.
"""

from __future__ import annotations

import shutil
from pathlib import Path

import pytest
import yaml

from yamaa.io import render_csv
from yamaa.io.project import ProjectResources
from yamaa.io.source import load_source_tables
from yamaa.runtime import (
    ExecutionFailure,
    ExecutionSuccess,
    execute_with_source_provider,
)
from yamaa.specification import SpecificationError, load_specification

REPOSITORY_ROOT = Path(__file__).parents[3]
SCHEMA_ROOT = REPOSITORY_ROOT / "yaml"
EXAMPLES = SCHEMA_ROOT / "examples"

# Every committed example whose derivations this component now executes.
ARTIFACT_EXAMPLES = [
    "adam-adsl-bmi-compute",
    "adam-adsl-identifier-parsing",
    "adam-adsl-mapping",
    "adam-adae-string-handlers",
]

# Committed error contracts this component reproduces field for field.
ERROR_EXAMPLES = [
    "negative-compute-division-by-zero",
    "negative-compute-sqrt-of-negative",
    "negative-compute-ln-of-zero",
    "negative-compute-integer-overflow",
    "negative-compute-aggregate-function",
    "negative-compute-comparison-operator",
    "negative-compute-qualified-identifier",
    "negative-cut-non-numeric-source",
    "negative-str-lower-non-string-source",
    "negative-str-extract-undeclared-group",
    "negative-adsl-subject-reference",
    "negative-greatest-incomparable-sources",
    "negative-least-incomparable-sources",
    "negative-adae-review-condition-arithmetic",
    "negative-mapping-unmapped-value",
]


def _run(directory: Path) -> object:
    specification = load_specification(
        directory / "spec.yaml", SCHEMA_ROOT
    ).specification
    resources = ProjectResources(directory)
    return execute_with_source_provider(
        specification,
        lambda datasets: load_source_tables(datasets, resources),
    )


def _committed_error(directory: Path) -> dict[str, object]:
    return yaml.safe_load((directory / "expected" / "error.yaml").read_text("utf-8"))


@pytest.mark.parametrize("name", ARTIFACT_EXAMPLES)
def test_a_committed_example_reproduces_its_committed_artifact(name: str) -> None:
    directory = EXAMPLES / name
    specification = load_specification(
        directory / "spec.yaml", SCHEMA_ROOT
    ).specification

    result = _run(directory)

    assert isinstance(result, ExecutionSuccess), result
    committed = (directory / "expected" / specification.output.path).read_bytes()
    assert render_csv(result.artifact) == committed


@pytest.mark.parametrize(
    ("name", "expected_counts"),
    [
        (
            "adam-adsl-identifier-parsing",
            [
                ("columns.SITEIDP.derivation.str_extract.missing", "missing", 0),
                ("columns.SITEIDP.derivation.str_extract.no_match", "no_match", 1),
                ("columns.SUBJREF.derivation.str_template.missing", "missing", 1),
            ],
        ),
        (
            "adam-adae-string-handlers",
            [
                ("columns.AEREFNUM.derivation.str_extract.missing", "missing", 2),
                ("columns.AEREFNUM.derivation.str_extract.no_match", "no_match", 1),
                ("columns.AERELLC.derivation.str_lower.missing", "missing", 1),
            ],
        ),
        (
            "adam-adsl-mapping",
            [
                ("columns.AGEGR1.derivation.cut.missing", "missing", 2),
            ],
        ),
    ],
)
def test_every_declared_handler_path_is_reported_with_its_count(
    name: str, expected_counts: list[tuple[str, str, int]]
) -> None:
    # R008-21: each handler path is reported, and a path that fired zero
    # times is reportable rather than absent.
    result = _run(EXAMPLES / name)

    assert isinstance(result, ExecutionSuccess)
    reported = {
        (count.spec_path, count.handler, count.count) for count in result.handler_counts
    }
    assert set(expected_counts) <= reported


@pytest.mark.parametrize("name", ERROR_EXAMPLES)
def test_a_committed_error_contract_is_reproduced(name: str) -> None:
    directory = EXAMPLES / name
    committed = _committed_error(directory)

    result = _run(directory)

    assert isinstance(result, ExecutionFailure), result
    diagnostic = result.diagnostics[0]
    assert diagnostic.phase == committed["phase"]
    assert diagnostic.condition == committed["condition"]
    assert list(diagnostic.spec_paths) == committed["spec_paths"]
    assert diagnostic.requirement == committed["requirement"]
    # Every field the contract records is reproduced; a runtime may report
    # more detail beside them, such as the overflowing value.
    assert committed["context"].items() <= diagnostic.context.items()


def test_a_nested_expression_is_rejected_before_execution() -> None:
    # The committed contract for this example is a load-time rejection, so
    # nothing reaches the dispatcher to be silently evaluated.
    directory = EXAMPLES / "negative-variable-nested-expression"
    committed = _committed_error(directory)

    with pytest.raises(SpecificationError) as caught:
        load_specification(directory / "spec.yaml", SCHEMA_ROOT)

    diagnostic = caught.value.diagnostics[0]
    assert diagnostic.phase == committed["phase"]
    assert diagnostic.condition == committed["condition"]
    assert list(diagnostic.spec_paths) == committed["spec_paths"]
    assert committed["context"].items() <= diagnostic.context.items()


def test_a_coalesce_cycle_is_reported_before_any_row_is_built() -> None:
    directory = EXAMPLES / "negative-coalesce-self-reference"
    committed = _committed_error(directory)

    result = _run(directory)

    assert isinstance(result, ExecutionFailure)
    diagnostic = result.diagnostics[0]
    assert diagnostic.condition == committed["condition"]
    assert diagnostic.requirement == committed["requirement"]
    assert committed["context"].items() <= diagnostic.context.items()
    # R001 owns the cycle report and names the derivation rather than the
    # operation the committed contract names; that path is not this
    # component's to change.
    assert diagnostic.spec_paths == ("columns.SEVAL.derivation",)


def test_changing_a_referenced_source_changes_the_artifact(tmp_path: Path) -> None:
    # A derivation that ignored its inputs would still reproduce the committed
    # artifact, so the same specification is run against a changed input.
    directory = tmp_path / "adam-adsl-bmi-compute"
    shutil.copytree(EXAMPLES / "adam-adsl-bmi-compute", directory)
    source = directory / "input" / "adsl.csv"
    original = source.read_text(encoding="utf-8")
    source.write_text(
        original.replace("CATH,CATH-001,180,81", "CATH,CATH-001,180,162"),
        encoding="utf-8",
    )

    result = _run(directory)

    assert isinstance(result, ExecutionSuccess)
    rows = result.artifact.frame.to_dicts()
    assert rows[0]["WEIGHTKG"] == 162.0
    assert rows[0]["BMI"] == 50.0
    committed = (
        EXAMPLES / "adam-adsl-bmi-compute" / "expected" / "adsl.csv"
    ).read_bytes()
    assert render_csv(result.artifact) != committed
