"""The R016 temporal operations: what a date operation reads and returns.

R016 owns both temporal types completely, so this module computes rather than
decides: the two value spaces, the one lexical form each admits, the collected
precision every value carries, and the calendar-unit counting all come from
that rule. Nothing here consults a host locale, widens a parse, or lets a
`datetime` into a date operation.
"""

from __future__ import annotations

import calendar
import datetime as dt
import re
from collections.abc import Mapping
from typing import Literal, TypeAlias

from pydantic import JsonValue

from yamaa.expressions.core import (
    AbsentValue,
    ExpressionHandler,
    FailedResolution,
    Resolver,
    expression_condition,
    handler_value,
)
from yamaa.models import (
    MISSING,
    ConditionResult,
    DateTimeValue,
    DateValue,
    EvaluationResult,
    RuntimeValue,
    ValueResult,
    normalize_runtime_value,
    runtime_type_name,
)

Precision: TypeAlias = Literal["year", "month", "day"]

# R016-44: one precision ladder, spelled twice. A policy names a level and
# `date_precision` returns that level's code; they are not two vocabularies.
_LADDER: tuple[Precision, ...] = ("year", "month", "day")
_CODES: dict[Precision, str] = {"year": "Y", "month": "M", "day": "D"}

# R016-42: the collected text a truncated date is carried as, which is prefix
# truncation only. A day known without its month cannot be collected, so the
# ladder has no rung for one.
_YEAR = re.compile(r"[0-9]{4}")
_YEAR_MONTH = re.compile(r"[0-9]{4}-[0-9]{2}")
_DATE_TEXT = re.compile(r"[0-9]{4}-[0-9]{2}-[0-9]{2}")


def _condition(
    condition: str,
    context: dict[str, JsonValue],
    *,
    phase: Literal["validation", "impute", "derivation"] = "validation",
    requirement: str | None = None,
    handler: Literal["missing", "invalid"] | None = None,
    field: str | None = None,
) -> ConditionResult:
    return expression_condition(
        phase,
        condition,
        context,
        handler,
        requirement,
        field,
    )


def _incompatible(
    operation: str,
    source: str,
    expected: str,
    value: RuntimeValue,
) -> ConditionResult:
    return _condition(
        "incompatible_input_type",
        {
            "operation": operation,
            "source": source,
            "expected": expected,
            "actual": runtime_type_name(value),
        },
        requirement="R016-65",
    )


def _read(
    payload: Mapping[str, object],
    field: str,
    resolver: Resolver,
    operation: str,
) -> RuntimeValue | ConditionResult:
    """Resolve one named variable of a temporal operation."""
    variable = payload.get(field)
    if not isinstance(variable, str):
        return _condition(
            "invalid_field_type",
            {"operation": operation, "expected": f"a variable for {field!r}"},
            requirement="R007-36",
        )
    resolved = resolver.resolve(variable)
    if isinstance(resolved, FailedResolution):
        return ConditionResult(condition=resolved.condition)
    if isinstance(resolved, AbsentValue):
        return _condition("unknown_field", {"identifier": variable})
    normalized = normalize_runtime_value(resolved.value)
    if isinstance(normalized, ValueResult):
        return normalized.value
    assert isinstance(normalized, ConditionResult)
    return normalized


def _date_operand(
    value: RuntimeValue,
    operation: str,
    source: str,
) -> DateValue | None | ConditionResult:
    """Return a `date` operand, or say why the value is not one.

    R016-55 keeps every operation but `to_date` on `date`, and R016-65 makes a
    `datetime` reaching one an error rather than a widening.
    """
    if value is MISSING:
        return None
    if isinstance(value, DateValue):
        return value
    return _incompatible(operation, source, "date", value)


def collected_precision(text: str) -> Precision | None:
    """Return how much of a date the collected text carries, or None.

    R016-42 admits prefix truncation only, so the answer is `day`, `month`,
    `year`, or that the text is not a date prefix at all.
    """
    if _DATE_TEXT.fullmatch(text) is not None:
        try:
            dt.date.fromisoformat(text)
        except ValueError:
            return None
        return "day"
    if _YEAR_MONTH.fullmatch(text) is not None:
        return "month" if 1 <= int(text[5:7]) <= 12 else None
    if _YEAR.fullmatch(text) is not None:
        return "year" if int(text) >= 1 else None
    return None


def _prefix_fields(text: str, precision: Precision) -> tuple[int, int | None]:
    year = int(text[:4])
    month = int(text[5:7]) if precision in {"month", "day"} else None
    return year, month


def _month_length(year: int, month: int) -> int:
    return calendar.monthrange(year, month)[1]


def _interval(year: int, month: int | None) -> tuple[dt.date, dt.date]:
    """Return the days a truncated source still admits (R016-49)."""
    if month is None:
        return dt.date(year, 1, 1), dt.date(year, 12, 31)
    return dt.date(year, month, 1), dt.date(year, month, _month_length(year, month))


def _resolved_day(day: object, year: int, month: int) -> int | ConditionResult:
    """Resolve the declared day against the month the date lands in (R016-48)."""
    if day == "first":
        return 1
    if day == "last":
        return _month_length(year, month)
    if type(day) is not int:
        # R016-68: rejected where the specification is read; reaching here at
        # all means the declaration escaped that check.
        return _condition(
            "value_not_permitted",
            {"field": "day", "value": str(day), "permitted": ["first", "last"]},
            requirement="R016-68",
        )
    if not 1 <= day <= 31:
        return _condition(
            "value_not_permitted",
            {"field": "day", "value": day, "permitted": "1 to 31"},
            requirement="R016-67",
        )
    return day


def _date_impute(payload: object, resolver: Resolver) -> EvaluationResult:
    if not isinstance(payload, Mapping):
        return _condition(
            "invalid_field_type",
            {"operation": "date_impute", "expected": "a mapping"},
            requirement="R007-36",
        )
    source = _read(payload, "source", resolver, "date_impute")
    if isinstance(source, ConditionResult):
        return source

    month = payload.get("month")
    if type(month) is not int or not 1 <= month <= 12:
        # R016-56: the range checks apply even when the component is unused,
        # so a policy cannot hide an invalid literal.
        return _condition(
            "month_out_of_range",
            {"month": month if isinstance(month, int) else str(month)},
            requirement="R016-67",
            field="month",
        )
    declared_day = payload.get("day")
    if type(declared_day) is int and not 1 <= declared_day <= 31:
        return _condition(
            "value_not_permitted",
            {"field": "day", "value": declared_day, "permitted": "1 to 31"},
            requirement="R016-67",
        )

    if source is MISSING:
        if "missing" in payload:
            return handler_value(payload, "missing")
        return _condition(
            "missing_input",
            {"operation": "date_impute", "source": str(payload.get("source"))},
            phase="impute",
            handler="missing",
            requirement="R016-52",
        )
    if isinstance(source, DateTimeValue):
        return _incompatible("date_impute", "source", "str", source)
    if isinstance(source, DateValue):
        # R016-54 types the source `str`; a value is already complete.
        return ValueResult(value=source)
    if not isinstance(source, str):
        return _incompatible("date_impute", "source", "str", source)

    precision = collected_precision(source)
    if precision is None:
        # R016-52: text that is not a date prefix is a different defect from
        # an uncollected value, and a specification may answer them apart.
        if "invalid" in payload:
            return handler_value(payload, "invalid")
        return _condition(
            "invalid_date_text",
            {
                "operation": "date_impute",
                "source": str(payload.get("source")),
                "value": source,
            },
            phase="impute",
            handler="invalid",
            requirement="R016-52",
        )

    minimum = payload.get("minimum_source_precision", "year")
    if minimum not in {"year", "month"}:
        return _condition(
            "value_not_permitted",
            {"field": "minimum_source_precision", "value": str(minimum)},
            requirement="R016-47",
        )
    if _LADDER.index(precision) < _LADDER.index(minimum):  # type: ignore[arg-type]
        # R016-47: below the declared minimum is neither a missing source nor
        # invalid text, so neither handler answers it.
        return ValueResult(value=MISSING)

    bound = None
    if "not_before" in payload:
        read = _read(payload, "not_before", resolver, "date_impute")
        if isinstance(read, ConditionResult):
            return read
        operand = _date_operand(read, "date_impute", "not_before")
        if isinstance(operand, ConditionResult):
            return operand
        bound = operand

    if precision == "day":
        # R016-50: a complete source supplied nothing for the bound to move.
        return ValueResult(value=DateValue.parse(source))

    year, source_month = _prefix_fields(source, precision)
    completed_month = source_month if source_month is not None else month
    day = _resolved_day(declared_day, year, completed_month)
    if isinstance(day, ConditionResult):
        return day
    if day > _month_length(year, completed_month):
        # R016-56: the completed value must be a real calendar date.
        return _condition(
            "invalid_calendar_date",
            {
                "value": source,
                "completed": f"{year:04d}-{completed_month:02d}-{day:02d}",
            },
            phase="impute",
            requirement="R016-67",
        )

    completed = dt.date(year, completed_month, day)
    bounded = _apply_bound(completed, bound, year, source_month)
    if bounded is None:
        # R016-69: the interval admits no day satisfying the bound.
        return ValueResult(value=MISSING)
    return ValueResult(
        value=DateValue(
            year=bounded.year,
            month=bounded.month,
            day=bounded.day,
            collected_precision=precision,
        )
    )


def _apply_bound(
    completed: dt.date,
    bound: DateValue | None,
    year: int,
    source_month: int | None,
) -> dt.date | None:
    """Move only what imputation supplied, and only inside the interval.

    R016-49 lets the bound move the result within the days the collected
    components still admit, and no further.
    """
    if bound is None:
        return completed
    lower = dt.date(bound.year, bound.month, bound.day)
    if completed >= lower:
        return completed
    first, last = _interval(year, source_month)
    earliest = max(first, lower)
    return earliest if earliest <= last else None


def _date_precision(payload: object, resolver: Resolver) -> EvaluationResult:
    if not isinstance(payload, Mapping):
        return _condition(
            "invalid_field_type",
            {"operation": "date_precision", "expected": "a mapping"},
            requirement="R007-36",
        )
    source = _read(payload, "source", resolver, "date_precision")
    if isinstance(source, ConditionResult):
        return source
    if source is MISSING:
        if "missing" in payload:
            return handler_value(payload, "missing")
        return _condition(
            "missing_input",
            {"operation": "date_precision", "source": str(payload.get("source"))},
            phase="impute",
            handler="missing",
            requirement="R016-52",
        )
    if isinstance(source, DateValue):
        # R016-45: a value reports the precision it carries, which binds the
        # flag to the date it describes rather than to the text beside it.
        return ValueResult(value=_CODES[source.collected_precision])
    if isinstance(source, DateTimeValue):
        return _incompatible("date_precision", "source", "str or date", source)
    if not isinstance(source, str):
        return _incompatible("date_precision", "source", "str or date", source)

    precision = collected_precision(source)
    if precision is None:
        if "invalid" in payload:
            return handler_value(payload, "invalid")
        return _condition(
            "invalid_date_text",
            {
                "operation": "date_precision",
                "source": str(payload.get("source")),
                "value": source,
            },
            phase="impute",
            handler="invalid",
            requirement="R016-52",
        )
    return ValueResult(value=_CODES[precision])


def _to_date(payload: object, resolver: Resolver) -> EvaluationResult:
    payload = {"source": payload} if isinstance(payload, str) else payload
    if not isinstance(payload, Mapping):
        return _condition(
            "invalid_field_type",
            {"operation": "to_date", "expected": "a mapping"},
            requirement="R007-36",
        )
    source = _read(payload, "source", resolver, "to_date")
    if isinstance(source, ConditionResult):
        return source
    if source is MISSING:
        # R016-57: a missing source returns a missing date.
        return ValueResult(value=MISSING)
    if not isinstance(source, DateTimeValue):
        # R016-66: in particular a `date` is not an identity spelling.
        return _incompatible("to_date", "source", "datetime", source)
    return ValueResult(
        value=DateValue(year=source.year, month=source.month, day=source.day)
    )


def _study_day(payload: object, resolver: Resolver) -> EvaluationResult:
    if not isinstance(payload, Mapping):
        return _condition(
            "invalid_field_type",
            {"operation": "study_day", "expected": "a mapping"},
            requirement="R007-36",
        )
    operands = []
    for field in ("date", "reference"):
        read = _read(payload, field, resolver, "study_day")
        if isinstance(read, ConditionResult):
            return read
        operand = _date_operand(read, "study_day", field)
        if isinstance(operand, ConditionResult):
            return operand
        operands.append(operand)
    day, reference = operands
    if day is None or reference is None:
        return ValueResult(value=MISSING)
    difference = _ordinal(day) - _ordinal(reference)
    # The reference is day 1 and there is no day zero, so an earlier date
    # counts back from -1.
    return ValueResult(value=difference + 1 if difference >= 0 else difference)


def _ordinal(value: DateValue) -> int:
    return dt.date(value.year, value.month, value.day).toordinal()


_UNITS = ("day", "week", "month", "year")


def _date_diff(payload: object, resolver: Resolver) -> EvaluationResult:
    if not isinstance(payload, Mapping):
        return _condition(
            "invalid_field_type",
            {"operation": "date_diff", "expected": "a mapping"},
            requirement="R007-36",
        )
    unit = payload.get("unit")
    if unit not in _UNITS:
        return _condition(
            "value_not_permitted",
            {"field": "unit", "value": str(unit), "permitted": list(_UNITS)},
            requirement="R007-37",
        )
    bounds = payload.get("bounds", "exclusive")
    if bounds not in {"exclusive", "inclusive", "between"}:
        return _condition(
            "value_not_permitted",
            {
                "field": "bounds",
                "value": str(bounds),
                "permitted": ["exclusive", "inclusive", "between"],
            },
            requirement="R007-37",
        )
    if unit != "day" and bounds != "exclusive":
        # R016-76 and R016-77: bounds counts endpoints of a day range, and an
        # age of 35 does not become 36.
        return _condition(
            "value_not_permitted",
            {"value": bounds, "unit": unit, "permitted": ["exclusive"]},
            requirement="R016-77",
            field="bounds",
        )

    operands = []
    for field in ("start", "end"):
        read = _read(payload, field, resolver, "date_diff")
        if isinstance(read, ConditionResult):
            return read
        operand = _date_operand(read, "date_diff", field)
        if isinstance(operand, ConditionResult):
            return operand
        operands.append(operand)
    start, end = operands
    if start is None or end is None:
        return ValueResult(value=MISSING)
    return ValueResult(value=whole_units(start, end, unit, bounds))


def whole_units(
    start: DateValue,
    end: DateValue,
    unit: str,
    bounds: str = "exclusive",
) -> int:
    """Count whole calendar units from `start` to `end` (R016-72 to R016-76)."""
    if unit == "day":
        days = _ordinal(end) - _ordinal(start)
        if bounds == "inclusive":
            return days + 1
        if bounds == "between":
            return days - 1
        return days
    if unit == "week":
        # R016-72: whole seven-day blocks, with any remainder discarded.
        days = _ordinal(end) - _ordinal(start)
        quotient = abs(days) // 7
        return -quotient if days < 0 else quotient
    # R016-75: an earlier end negates the count with the operands exchanged.
    if _ordinal(end) < _ordinal(start):
        return -_anniversaries(end, start, unit)
    return _anniversaries(start, end, unit)


def _anniversaries(start: DateValue, end: DateValue, unit: str) -> int:
    """Count anniversaries of `start` falling on or before `end`.

    R016-73 clamps the k-th anniversary's day to the length of the month it
    lands in, which is what puts a February 29 anniversary on February 28 of a
    common year (R016-74).
    """
    months = (end.year - start.year) * 12 + (end.month - start.month)
    if months > 0 and _clamped(start, months) > _ordinal(end):
        months -= 1
    if unit == "month":
        return max(months, 0)
    return max(months // 12, 0)


def _clamped(start: DateValue, months: int) -> int:
    total = start.month - 1 + months
    year = start.year + total // 12
    month = total % 12 + 1
    return dt.date(year, month, min(start.day, _month_length(year, month))).toordinal()


def date_handlers() -> dict[str, ExpressionHandler]:
    """Return the R016 temporal operations this component registers."""
    return {
        "date_diff": _date_diff,
        "date_impute": _date_impute,
        "date_precision": _date_precision,
        "study_day": _study_day,
        "to_date": _to_date,
    }
