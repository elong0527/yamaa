"""Execute the committed examples this component's temporal and window work reaches.

Every case runs the real public API end to end. Reading `expected/` happens
here, in the test, and never in runtime code. Serialized equality alone
cannot prove collected precision, because R016-32 keeps precision out of the
artifact on purpose, so the precision-sensitive assertions read the values.
"""

from __future__ import annotations

from pathlib import Path

import pytest
import yaml

from yamaa.io import render_csv
from yamaa.io.project import ProjectResources
from yamaa.io.source import load_source_tables
from yamaa.models import MISSING, DateValue
from yamaa.runtime import (
    ExecutionFailure,
    ExecutionSuccess,
    execute_with_source_provider,
)
from yamaa.specification import load_specification

REPOSITORY_ROOT = Path(__file__).parents[3]
SCHEMA_ROOT = REPOSITORY_ROOT / "yaml"
EXAMPLES = SCHEMA_ROOT / "examples"

# The fully defined goldens this component executes, including every
# positive fixture anchor #221 names.
ARTIFACT_EXAMPLES = [
    "adam-adae-partial-dates",
    "adam-advs-analysis-visit",
    "adam-adae-treatment-emergent",
    "adam-adlb-bds",
    "adam-advs-once-measured-carry-forward",
    "adam-adae-worst-severity",
    "adam-adsl-analysis-age",
    "adam-adsl-duration-weeks-months",
    "adam-advs-prior-character-result",
    "adam-adae-severity-rank",
    "sdtm-vs-visit-study-day",
]

# Committed error contracts this component reproduces field for field.
ERROR_EXAMPLES = [
    "negative-date-impute-nonexistent-day",
    "negative-date-impute-month-out-of-range",
    "negative-date-impute-invalid-source",
    "negative-date-precision-invalid-source",
    "negative-study-day-datetime-input",
    "negative-to-date-date-source",
    "negative-date-diff-bounds-unit",
    "negative-date-diff-datetime-endpoints",
    "negative-baseline-flag-tied-date",
    "negative-baseline-value-multiple-flags",
    "negative-row-value-zero-offset",
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


@pytest.mark.parametrize("name", ERROR_EXAMPLES)
def test_a_committed_error_contract_is_reproduced(name: str) -> None:
    directory = EXAMPLES / name
    committed = yaml.safe_load(
        (directory / "expected" / "error.yaml").read_text("utf-8")
    )

    result = _run(directory)

    assert isinstance(result, ExecutionFailure), result
    diagnostic = result.diagnostics[0]
    assert diagnostic.phase == committed["phase"]
    assert diagnostic.condition == committed["condition"]
    assert list(diagnostic.spec_paths) == committed["spec_paths"]
    assert diagnostic.requirement == committed["requirement"]
    assert committed["context"].items() <= diagnostic.context.items()


def _partial_dates() -> list[dict[str, object]]:
    result = _run(EXAMPLES / "adam-adae-partial-dates")
    assert isinstance(result, ExecutionSuccess), result
    return result.table.frame.to_dicts()


def test_an_imputed_date_stores_the_day_it_names_and_reports_its_precision() -> None:
    # R016-32 keeps collected precision out of the artifact, so the CSV match
    # above cannot prove it. The derived precision column is where a
    # specification carries it past that boundary, which is what this reads.
    rows = _partial_dates()

    by_term = {row["AETERM"]: row for row in rows}
    assert by_term["NAUSEA"]["AESTDTC"] == "2025-01"
    assert by_term["NAUSEA"]["ASTDT"].isoformat() == "2025-01-15"
    assert by_term["NAUSEA"]["ASTDTPR"] == "M"
    assert by_term["NAUSEA"]["ASTDTF"] == "D"
    # A collected day supplied everything, so nothing was imputed.
    assert by_term["HEADACHE"]["ASTDTPR"] == "D"
    assert by_term["HEADACHE"]["ASTDTF"] is None


def test_a_bound_moves_a_supplied_day_and_never_a_collected_one() -> None:
    # R016-49 and R016-50.
    rows = {row["AETERM"]: row for row in _partial_dates()}

    # A month-precision source clamped forward to the treatment start.
    assert rows["PYREXIA"]["ASTDT"].isoformat() == "2025-03-20"
    assert rows["PYREXIA"]["ASTDTPR"] == "M"
    # A collected date before the bound is returned unchanged.
    assert rows["MIGRAINE"]["ASTDT"].isoformat() == "2025-01-05"
    assert rows["MIGRAINE"]["TRTEMFL"] is None
    # An interval admitting no day on or after the bound is missing.
    assert rows["COUGH"]["ASTDT"] is None


def test_a_source_below_the_declared_minimum_precision_stays_missing() -> None:
    # R016-47: a year-only source under `minimum_source_precision: month`.
    rows = {row["AETERM"]: row for row in _partial_dates()}

    assert rows["ECZEMA FLARE"]["AESTDTC"] == "2025"
    assert rows["ECZEMA FLARE"]["ASTDT"] is None
    assert rows["ECZEMA FLARE"]["ASTDTF"] is None


def test_an_imputed_date_still_answers_the_comparisons_it_reaches() -> None:
    # R016-37: an imputed start still decides whether an event is treatment
    # emergent, and precision does not stop it.
    rows = {row["AETERM"]: row for row in _partial_dates()}

    assert rows["NAUSEA"]["ASTDTPR"] == "M"
    assert rows["NAUSEA"]["TRTEMFL"] == "Y"


def test_repeated_execution_produces_an_identical_temporal_artifact() -> None:
    first = _run(EXAMPLES / "adam-adae-partial-dates")
    second = _run(EXAMPLES / "adam-adae-partial-dates")

    assert isinstance(first, ExecutionSuccess)
    assert isinstance(second, ExecutionSuccess)
    assert render_csv(first.artifact) == render_csv(second.artifact)


def test_a_completed_date_and_a_collected_one_are_one_partition_key() -> None:
    # R016-35 keeps precision out of every comparison, so a window or a join
    # that groups on a date does not split an imputed value from a collected
    # one naming the same day.
    collected = DateValue.parse("2025-01-15")
    imputed = collected.model_copy(update={"collected_precision": "month"})

    assert len({collected, imputed}) == 1
    assert collected == imputed
    assert MISSING not in {collected, imputed}
