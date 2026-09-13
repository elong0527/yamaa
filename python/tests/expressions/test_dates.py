from __future__ import annotations

import pytest

from yamaa.expressions import MappingResolver, evaluate_expression
from yamaa.expressions.dates import collected_precision, whole_units
from yamaa.models import (
    MISSING,
    ConditionResult,
    DateTimeValue,
    DateValue,
    ValueResult,
)


def date(text: str, precision: str = "day") -> DateValue:
    value = DateValue.parse(text)
    return value.model_copy(update={"collected_precision": precision})


def _evaluate(
    operation: str,
    payload: dict[str, object],
    values: dict[str, object] | None = None,
) -> object:
    return evaluate_expression({operation: payload}, MappingResolver(values or {}))


def _value(
    operation: str,
    payload: dict[str, object],
    values: dict[str, object] | None = None,
) -> object:
    result = _evaluate(operation, payload, values)
    assert isinstance(result, ValueResult), result
    return result.value


def _condition(
    operation: str,
    payload: dict[str, object],
    values: dict[str, object] | None = None,
) -> ConditionResult:
    result = _evaluate(operation, payload, values)
    assert isinstance(result, ConditionResult), result
    return result


# --- collected precision -------------------------------------------------


@pytest.mark.parametrize(
    ("text", "expected"),
    [
        ("2025-01-15", "day"),
        ("2025-01", "month"),
        ("2025", "year"),
        ("UNKNOWN", None),
        ("2025-13", None),
        ("2025-02-30", None),
        ("2025-1-2", None),
        ("20250102", None),
        ("2025-01-15T00:00:00", None),
        ("", None),
    ],
)
def test_only_a_date_or_a_date_prefix_carries_a_precision(
    text: str, expected: str | None
) -> None:
    # R016-42: the collected text is prefix truncation only, and R016-46
    # gives a day without its month no representation to describe.
    assert collected_precision(text) == expected


def test_precision_is_read_from_a_value_or_from_the_text_it_came_from() -> None:
    # R016-45: reading the date binds a flag to the value it describes.
    assert (
        _value("date_precision", {"source": "S"}, {"S": date("2025-02-28", "month")})
        == "M"
    )
    assert _value("date_precision", {"source": "S"}, {"S": date("2025-02-28")}) == "D"
    assert _value("date_precision", {"source": "S"}, {"S": "2025"}) == "Y"


def test_a_datetime_is_not_a_date_precision_source() -> None:
    # R016-65: a datetime reaching a date operation is an error, not a widening.
    condition = _condition(
        "date_precision",
        {"source": "S"},
        {"S": DateTimeValue.parse("2025-01-01T00:00")},
    )

    assert condition.condition.condition == "incompatible_input_type"


def test_the_two_absences_stay_apart_and_each_may_be_answered() -> None:
    # R016-52: text that is not a date is a different defect from an
    # uncollected value, and a specification may answer them differently.
    missing = _condition("date_precision", {"source": "S"}, {"S": MISSING})
    invalid = _condition("date_precision", {"source": "S"}, {"S": "UNKNOWN"})

    assert missing.condition.condition == "missing_input"
    assert missing.condition.applicable_handler == "missing"
    assert invalid.condition.condition == "invalid_date_text"
    assert invalid.condition.applicable_handler == "invalid"
    assert (
        _value("date_precision", {"source": "S", "missing": "?"}, {"S": MISSING}) == "?"
    )
    assert (
        _value("date_precision", {"source": "S", "invalid": "!"}, {"S": "UNKNOWN"})
        == "!"
    )


# --- date_impute ---------------------------------------------------------


def impute(source: object, **extra: object) -> dict[str, object]:
    return {"source": "S", "month": 6, "day": 15, **extra}


def test_a_complete_source_is_returned_unchanged_whatever_the_bound_says() -> None:
    # R016-50: it supplied nothing for the bound to move.
    value = _value(
        "date_impute",
        impute(None, not_before="B"),
        {"S": "2025-01-05", "B": date("2025-03-20")},
    )

    assert value == date("2025-01-05")
    assert value.collected_precision == "day"


def test_a_truncated_source_is_completed_and_keeps_its_collected_precision() -> None:
    # R016-43: which components were supplied is a property of the value.
    month = _value("date_impute", impute(None), {"S": "2025-01"})
    year = _value("date_impute", impute(None), {"S": "2025"})

    assert month == date("2025-01-15")
    assert month.collected_precision == "month"
    assert year == date("2025-06-15")
    assert year.collected_precision == "year"


def test_a_source_below_the_declared_minimum_is_missing_and_fires_no_handler() -> None:
    # R016-47: neither a missing source nor invalid text.
    value = _value(
        "date_impute",
        impute(None, minimum_source_precision="month", missing="X", invalid="X"),
        {"S": "2025"},
    )

    assert value is MISSING


@pytest.mark.parametrize(
    ("year", "expected"),
    [(2024, "2024-02-29"), (2025, "2025-02-28")],
)
def test_last_names_the_day_the_month_actually_ends_on(
    year: int, expected: str
) -> None:
    # R016-48: `last` resolves against the month the completed date lands in,
    # so a February is 28 or 29 according to the year.
    value = _value(
        "date_impute", {"source": "S", "month": 2, "day": "last"}, {"S": f"{year}-02"}
    )

    assert value == date(expected)


def test_first_names_the_day_the_month_begins_with() -> None:
    assert _value(
        "date_impute", {"source": "S", "month": 2, "day": "first"}, {"S": "2025-02"}
    ) == date("2025-02-01")


def test_a_completed_value_that_is_not_a_calendar_date_fails() -> None:
    # R016-56: the completed value must be a real calendar date.
    condition = _condition(
        "date_impute", {"source": "S", "month": 2, "day": 30}, {"S": "2025-02"}
    )

    assert condition.condition.condition == "invalid_calendar_date"


def test_a_month_outside_the_calendar_fails_even_when_it_is_unused() -> None:
    # R016-56: the range checks still apply when a component is not used, so
    # a specification cannot hide an invalid literal behind a policy.
    condition = _condition(
        "date_impute", {"source": "S", "month": 13, "day": 15}, {"S": "2025-01-05"}
    )

    assert condition.condition.condition == "month_out_of_range"


def test_the_bound_moves_the_result_only_inside_the_interval_it_admits() -> None:
    # R016-49: the bound invents no more than it must, and a year-only source
    # admits the whole of its year.
    inside = _value(
        "date_impute",
        impute(None, not_before="B"),
        {"S": "2025-03", "B": date("2025-03-20")},
    )
    across_the_year = _value(
        "date_impute",
        impute(None, not_before="B"),
        {"S": "2025", "B": date("2025-09-09")},
    )

    assert inside == date("2025-03-20")
    assert across_the_year == date("2025-09-09")


def test_an_interval_admitting_no_day_on_or_after_the_bound_is_missing() -> None:
    # R016-69: missing, not a failure, and no handler answers it.
    value = _value(
        "date_impute",
        impute(None, not_before="B", missing="X", invalid="X"),
        {"S": "2025-02", "B": date("2025-03-20")},
    )

    assert value is MISSING


# --- study_day -----------------------------------------------------------


@pytest.mark.parametrize(
    ("day", "reference", "expected"),
    [
        ("2025-01-10", "2025-01-10", 1),
        ("2025-01-11", "2025-01-10", 2),
        ("2025-01-09", "2025-01-10", -1),
        ("2025-01-01", "2025-01-10", -9),
    ],
)
def test_the_reference_is_day_one_and_there_is_no_day_zero(
    day: str, reference: str, expected: int
) -> None:
    value = _value(
        "study_day",
        {"date": "D", "reference": "R"},
        {"D": date(day), "R": date(reference)},
    )

    assert value == expected
    assert value != 0


def test_a_missing_operand_yields_a_missing_study_day() -> None:
    assert (
        _value(
            "study_day",
            {"date": "D", "reference": "R"},
            {"D": MISSING, "R": date("2025-01-10")},
        )
        is MISSING
    )


# --- date_diff -----------------------------------------------------------


@pytest.mark.parametrize(
    ("start", "end", "unit", "bounds", "expected"),
    [
        ("2025-01-01", "2025-01-11", "day", "exclusive", 10),
        ("2025-01-01", "2025-01-11", "day", "inclusive", 11),
        ("2025-01-01", "2025-01-11", "day", "between", 9),
        ("2025-01-01", "2025-01-15", "week", "exclusive", 2),
        ("2025-01-01", "2025-01-07", "week", "exclusive", 0),
        # R016-73's three pinned boundary cases.
        ("2025-01-31", "2025-02-28", "month", "exclusive", 1),
        ("2024-02-29", "2025-02-28", "month", "exclusive", 12),
        ("2024-02-29", "2025-02-28", "year", "exclusive", 1),
        ("2025-01-31", "2025-03-01", "month", "exclusive", 1),
    ],
)
def test_whole_calendar_units_count_what_r016_pins(
    start: str, end: str, unit: str, bounds: str, expected: int
) -> None:
    value = _value(
        "date_diff",
        {"start": "S", "end": "E", "unit": unit, "bounds": bounds},
        {"S": date(start), "E": date(end)},
    )

    assert value == expected


@pytest.mark.parametrize("unit", ["day", "week", "month", "year"])
def test_an_earlier_end_negates_the_count_with_the_operands_exchanged(
    unit: str,
) -> None:
    # R016-75: no unit rounds toward negative infinity.
    forward = whole_units(date("2024-02-29"), date("2025-04-30"), unit)
    backward = whole_units(date("2025-04-30"), date("2024-02-29"), unit)

    assert backward == -forward
    assert forward > 0


def test_a_february_29_anniversary_falls_on_february_28_in_a_common_year() -> None:
    # R016-74: the case an age computation meets every leap year.
    assert whole_units(date("2024-02-29"), date("2025-02-28"), "year") == 1
    assert whole_units(date("2024-02-29"), date("2025-02-27"), "year") == 0


@pytest.mark.parametrize("bounds", ["inclusive", "between"])
@pytest.mark.parametrize("unit", ["week", "month", "year"])
def test_bounds_beyond_days_is_rejected_rather_than_reinterpreted(
    unit: str, bounds: str
) -> None:
    # R016-76 and R016-77: an age of 35 does not become 36.
    condition = _condition(
        "date_diff",
        {"start": "S", "end": "E", "unit": unit, "bounds": bounds},
        {"S": date("2025-01-01"), "E": date("2025-01-11")},
    )

    assert condition.condition.condition == "value_not_permitted"
    assert condition.condition.requirement == "R016-77"
    assert condition.condition.context["value"] == bounds
    assert condition.condition.context["permitted"] == ["exclusive"]


def test_a_missing_endpoint_needs_no_guarding_predicate() -> None:
    assert (
        _value(
            "date_diff",
            {"start": "S", "end": "E", "unit": "day"},
            {"S": MISSING, "E": date("2025-01-11")},
        )
        is MISSING
    )


def test_a_datetime_endpoint_fails_rather_than_widening_the_operation() -> None:
    # R016-55: `unit: day` between two moments could defend 1 or 0.
    condition = _condition(
        "date_diff",
        {"start": "S", "end": "E", "unit": "day"},
        {"S": DateTimeValue.parse("2025-01-01T23:00"), "E": date("2025-01-02")},
    )

    assert condition.condition.condition == "incompatible_input_type"


# --- to_date -------------------------------------------------------------


def test_to_date_copies_the_calendar_fields_and_drops_the_time() -> None:
    value = _value(
        "to_date", {"source": "S"}, {"S": DateTimeValue.parse("2025-01-12T14:30:05")}
    )

    assert value == date("2025-01-12")
    # R016-8: `to_date` produces a value collected to the day.
    assert value.collected_precision == "day"


def test_to_date_refuses_a_date_as_an_identity_spelling() -> None:
    # R016-66.
    condition = _condition("to_date", {"source": "S"}, {"S": date("2025-01-12")})

    assert condition.condition.condition == "incompatible_input_type"


def test_a_missing_datetime_yields_a_missing_date() -> None:
    assert _value("to_date", {"source": "S"}, {"S": MISSING}) is MISSING


def test_collected_precision_takes_no_part_in_equality_or_grouping() -> None:
    # R016-35: two values compare by their fields, and precision is not one
    # of them. A join matches and a partition groups by that equality, so a
    # completed date and a collected one naming the same day are one key.
    collected = date("2025-01-15")
    imputed = date("2025-01-15", "month")

    assert collected == imputed
    assert hash(collected) == hash(imputed)
    assert {collected: "one"}[imputed] == "one"
    # R016-10: the property is still read off the value that carries it.
    assert imputed.collected_precision == "month"
    assert collected != date("2025-01-16")
