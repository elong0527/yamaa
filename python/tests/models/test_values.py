from __future__ import annotations

import datetime as dt
import math

import polars as pl
import pytest
from pydantic import ValidationError

from yamaa.models import (
    MISSING,
    ColumnType,
    ConditionResult,
    DateTimeValue,
    DateValue,
    TypedColumn,
    TypedTable,
    ValueResult,
    convert_value,
    normalize_runtime_value,
    runtime_type_name,
)


def _value(result: object) -> object:
    assert isinstance(result, ValueResult)
    return result.value


def _conversion_failure(result: object) -> ConditionResult:
    assert isinstance(result, ConditionResult)
    assert result.condition.phase == "convert"
    assert result.condition.condition == "conversion_failed"
    assert result.condition.applicable_handler == "conversion_failure"
    return result


@pytest.mark.parametrize("value", [None, math.inf, -math.inf, math.nan])
def test_normalizes_all_missing_boundaries(value: object) -> None:
    assert _value(normalize_runtime_value(value)) is MISSING


def test_rejects_boolean_as_an_integer_and_bounds_int64() -> None:
    assert runtime_type_name(True) == "bool"
    _conversion_failure(convert_value(True, "int"))
    assert _value(convert_value(-(2**63), "int")) == -(2**63)
    assert _value(convert_value(2**63 - 1, "int")) == 2**63 - 1

    overflow = normalize_runtime_value(2**63)
    assert isinstance(overflow, ConditionResult)
    assert overflow.condition.condition == "integer_overflow"


@pytest.mark.parametrize(
    ("source", "target", "expected"),
    [
        ("kept", "str", "kept"),
        (42, "str", "42"),
        (1.25, "str", "1.25"),
        (1e20, "str", "100000000000000000000"),
        (1e-7, "str", "0.0000001"),
        ("42", "int", 42),
        ("2.0", "int", 2),
        ("1e2", "int", 100),
        (42, "float", 42.0),
        ("1.25", "float", 1.25),
        (1.25, "float", 1.25),
        ("2025-01-02", "date", DateValue.parse("2025-01-02")),
        (
            "2025-01-02T03:04",
            "datetime",
            DateTimeValue.parse("2025-01-02T03:04:00"),
        ),
        (DateValue.parse("2025-01-02"), "str", "2025-01-02"),
        (
            DateValue.parse("2025-01-02"),
            "date",
            DateValue.parse("2025-01-02"),
        ),
        (
            DateTimeValue.parse("2025-01-02T03:04"),
            "str",
            "2025-01-02T03:04:00",
        ),
        (
            DateTimeValue.parse("2025-01-02T03:04"),
            "datetime",
            DateTimeValue.parse("2025-01-02T03:04:00"),
        ),
    ],
)
def test_supported_conversions(
    source: object,
    target: ColumnType,
    expected: object,
) -> None:
    assert _value(convert_value(source, target)) == expected


@pytest.mark.parametrize("target", ["int", "float"])
@pytest.mark.parametrize(
    "source",
    [".inf", "+.Inf", "-.INF", ".nan", ".NaN", ".NAN", "1e999"],
)
def test_numeric_text_that_becomes_non_finite_is_missing(
    source: str,
    target: ColumnType,
) -> None:
    assert _value(convert_value(source, target)) is MISSING


@pytest.mark.parametrize(
    ("source", "target"),
    [
        (72.5, "int"),
        ("72.5", "int"),
        (" 72", "int"),
        ("72.", "float"),
        (True, "str"),
        (DateValue.parse("2025-01-02"), "datetime"),
        (DateTimeValue.parse("2025-01-02T03:04:05"), "date"),
    ],
)
def test_undefined_conversions_fail(source: object, target: ColumnType) -> None:
    _conversion_failure(convert_value(source, target))


@pytest.mark.parametrize("target", ["str", "int", "float", "date", "datetime"])
def test_boolean_conversion_is_undefined_for_every_declared_type(
    target: ColumnType,
) -> None:
    _conversion_failure(convert_value(True, target))


@pytest.mark.parametrize("target", ["str", "int", "float", "date", "datetime"])
def test_missing_converts_to_missing_for_every_declared_type(
    target: ColumnType,
) -> None:
    assert _value(convert_value(MISSING, target)) is MISSING


def test_preserves_empty_text_as_distinct_from_missing() -> None:
    assert _value(convert_value("", "str")) == ""
    assert _value(convert_value(None, "str")) is MISSING


def test_rejects_surrogate_code_points_at_the_text_boundary() -> None:
    result = normalize_runtime_value("ok\ud800bad")

    assert isinstance(result, ConditionResult)
    assert result.condition.phase == "ingest"
    assert result.condition.condition == "invalid_text"
    assert result.condition.context == {"code_point": "U+D800", "offset": 2}


def test_large_integral_text_that_overflows_binary64_normalizes_to_missing() -> None:
    assert _value(convert_value("9" * 400, "float")) is MISSING


def test_float_to_integer_checks_integrality_and_int64_bounds() -> None:
    assert _value(convert_value(-0.0, "int")) == 0
    assert _value(convert_value(float(-(2**63)), "int")) == -(2**63)
    _conversion_failure(convert_value(float(2**63), "int"))


def test_temporal_values_preserve_collected_precision_and_canonical_text() -> None:
    date = DateValue(year=2025, month=2, day=28, collected_precision="month")
    moment = DateTimeValue.parse("2025-02-28T14:05")

    assert date.collected_precision == "month"
    assert date.to_text() == "2025-02-28"
    assert moment.collected_precision == "second"
    assert moment.to_text() == "2025-02-28T14:05:00"


@pytest.mark.parametrize(
    "text",
    ["2025-1-02", "2025-02", "2025-02-30", "0000-01-01", "2025-01-01T00:00"],
)
def test_rejects_invalid_date_text(text: str) -> None:
    with pytest.raises(ValueError):
        DateValue.parse(text)


@pytest.mark.parametrize(
    "text",
    [
        "2025-01-01",
        "2025-01-01 00:00:00",
        "2025-01-01T24:00",
        "2025-01-01T23:59:60",
        "2025-01-01T00:00:00Z",
        "2025-01-01T00:00:00.1",
    ],
)
def test_rejects_invalid_datetime_text(text: str) -> None:
    with pytest.raises(ValueError):
        DateTimeValue.parse(text)


def test_typed_table_requires_declared_polars_column_order() -> None:
    frame = pl.DataFrame({"ID": [1], "VALUE": ["A"]})
    table = TypedTable(
        columns=(
            TypedColumn(name="ID", type="int"),
            TypedColumn(name="VALUE", type="str"),
        ),
        frame=frame,
    )

    assert table.frame.columns == ["ID", "VALUE"]

    with pytest.raises(ValidationError, match="column order"):
        TypedTable(
            columns=(
                TypedColumn(name="VALUE", type="str"),
                TypedColumn(name="ID", type="int"),
            ),
            frame=frame,
        )


def test_typed_table_enforces_declared_storage_types() -> None:
    with pytest.raises(ValidationError, match="must use Float64"):
        TypedTable(
            columns=(TypedColumn(name="VALUE", type="float"),),
            frame=pl.DataFrame({"VALUE": [1]}),
        )

    with pytest.raises(ValidationError, match="must use Date"):
        TypedTable(
            columns=(TypedColumn(name="VALUE", type="date"),),
            frame=pl.DataFrame([pl.Series("VALUE", ["2025-01-02"], dtype=pl.Object)]),
        )

    with pytest.raises(ValidationError, match="whole-second precision"):
        TypedTable(
            columns=(TypedColumn(name="VALUE", type="datetime"),),
            frame=pl.DataFrame(
                {"VALUE": [dt.datetime(2025, 1, 2, 3, 4, 5, 1)]},  # noqa: DTZ001
                schema={"VALUE": pl.Datetime("us")},
            ),
        )


def test_typed_table_normalizes_nonfinite_floats_to_missing() -> None:
    table = TypedTable(
        columns=(TypedColumn(name="VALUE", type="float"),),
        frame=pl.DataFrame(
            [pl.Series("VALUE", [1.0, math.inf, -math.inf, math.nan], pl.Float64)]
        ),
    )

    assert table.frame["VALUE"].to_list() == [1.0, None, None, None]
