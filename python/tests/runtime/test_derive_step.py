"""Test the aggregate derive step (REQ-1189 through REQ-1192).

The derive step binds per-record computed values before aggregation,
enabling #704 (text-to-number conversion) and #705 (date-to-epoch-day)
without new expression functions.
"""

from __future__ import annotations

from pathlib import Path

from yamaa.expressions import ExpressionDispatcher, MappingResolver
from yamaa.io import ProjectResources, load_source_tables
from yamaa.models import MISSING
from yamaa.models.values import DateValue
from yamaa.runtime import (
    ExecutionSuccess,
    execute_with_source_provider,
)
from yamaa.specification import load_specification
from yamaa.specification.models import Expression

REPOSITORY_ROOT = Path(__file__).parents[3]
SCHEMA_ROOT = REPOSITORY_ROOT / "yaml"


def _run_benchmark(name: str):
    bench = REPOSITORY_ROOT / "benchmarks" / name
    specification = load_specification(bench / "spec.yaml", SCHEMA_ROOT).specification
    resources = ProjectResources(bench)
    return execute_with_source_provider(
        specification,
        lambda datasets: load_source_tables(datasets, resources),
    )


def test_derive_converts_text_to_number_for_aggregation() -> None:
    """#704: text scores convert to float via the binding type, then sum."""
    result = _run_benchmark("adam-adqs-derive-text-score")

    assert isinstance(result, ExecutionSuccess)
    rows = result.artifact.frame.to_dicts()
    assert len(rows) == 1
    assert rows[0]["AVAL"] == 6.0
    assert rows[0]["PARAMCD"] == "PFSCORE"


def test_derive_converts_date_to_epoch_day() -> None:
    """#705: dates convert to integer epoch days via to_epoch_day."""
    result = _run_benchmark("adam-adex-derive-date-epoch")

    assert isinstance(result, ExecutionSuccess)
    rows = result.artifact.frame.to_dicts()
    assert len(rows) == 1
    # 2024-01-15 is 19737 days after 1970-01-01.
    assert rows[0]["EXSTDY"] == 19737


def test_to_epoch_day_epoch_is_zero() -> None:
    """#705: 1970-01-01 is day zero."""
    dispatcher = ExpressionDispatcher()
    expr = Expression.model_validate({"to_epoch_day": {"source": "EX.EXSTDT"}})
    result = dispatcher.evaluate(
        expr,
        MappingResolver({"EX.EXSTDT": DateValue(year=1970, month=1, day=1)}),
    )

    assert result.value == 0


def test_to_epoch_day_before_epoch_is_negative() -> None:
    """#705: dates before 1970-01-01 are negative."""
    dispatcher = ExpressionDispatcher()
    expr = Expression.model_validate({"to_epoch_day": {"source": "EX.EXSTDT"}})
    result = dispatcher.evaluate(
        expr,
        MappingResolver({"EX.EXSTDT": DateValue(year=1969, month=12, day=31)}),
    )

    assert result.value == -1


def test_to_epoch_day_missing_stays_missing() -> None:
    """#705: a missing date returns missing, not an error."""
    dispatcher = ExpressionDispatcher()
    expr = Expression.model_validate({"to_epoch_day": {"source": "EX.EXSTDT"}})
    result = dispatcher.evaluate(expr, MappingResolver({"EX.EXSTDT": MISSING}))

    assert result.value is MISSING
