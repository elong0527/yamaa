from __future__ import annotations

import pytest

from yamaa.expressions import Partition, evaluate_window
from yamaa.expressions.windows import (
    baseline_flag,
    baseline_value,
    previous_non_missing,
    rank,
    row_number,
    row_value,
)
from yamaa.models import MISSING, ConditionResult, DateValue, ValueResult

RowValues = dict[str, object]


def partition(
    rows: list[RowValues],
    current: int,
    eligible: list[bool] | None = None,
) -> Partition:
    return Partition(
        rows=tuple(rows),
        current=current,
        eligible=tuple(eligible if eligible is not None else [True] * len(rows)),
    )


def _value(result: object) -> object:
    assert isinstance(result, ValueResult), result
    return result.value


def _condition(result: object) -> ConditionResult:
    assert isinstance(result, ConditionResult), result
    return result


def visits(*values: object) -> list[RowValues]:
    return [{"AVAL": value} for value in values]


# --- the partition contract ----------------------------------------------


def test_a_partition_locates_the_current_row_inside_itself() -> None:
    with pytest.raises(ValueError, match="lie in its own partition"):
        partition(visits(1, 2), current=2)
    with pytest.raises(ValueError, match="states whether it is eligible"):
        Partition(rows=({"A": 1},), current=0, eligible=())


# --- row_number ----------------------------------------------------------


@pytest.mark.parametrize("current", [0, 1, 2])
def test_row_number_counts_from_one_along_the_declared_order(current: int) -> None:
    assert _value(row_number(partition(visits(5, 6, 7), current))) == current + 1


def test_an_excluded_row_receives_missing_and_consumes_no_number() -> None:
    # R007-7: a window that declares a filter still preserves row count, so
    # an excluded row is answered rather than dropped.
    rows = visits(5, 6, 7)
    eligible = [True, False, True]

    assert _value(row_number(partition(rows, 0, eligible))) == 1
    assert _value(row_number(partition(rows, 1, eligible))) is MISSING
    assert _value(row_number(partition(rows, 2, eligible))) == 2


# --- rank ----------------------------------------------------------------


@pytest.mark.parametrize(
    ("method", "expected"),
    [
        # R007-18: competition leaves the positions a tie occupied out of the
        # numbers that follow; dense numbers distinct values consecutively.
        ("competition", [1, 2, 2, 4]),
        ("dense", [1, 2, 2, 3]),
    ],
)
def test_rows_equal_on_every_term_take_one_number(
    method: str, expected: list[int]
) -> None:
    rows = visits("A", "B", "B", "C")

    numbers = [
        _value(rank(partition(rows, index), ["AVAL"], method))  # type: ignore[arg-type]
        for index in range(len(rows))
    ]

    assert numbers == expected


def test_two_missing_values_are_equal_for_the_purpose_of_a_tie() -> None:
    rows = visits(MISSING, MISSING, "C")

    numbers = [_value(rank(partition(rows, index), ["AVAL"])) for index in range(3)]

    assert numbers == [1, 1, 3]


def test_rank_compares_only_the_declared_terms() -> None:
    # R007-18: a specification that wants a tie broken declares the term that
    # breaks it, so an undeclared column cannot separate two rows.
    rows = [{"AVAL": "A", "SEQ": 1}, {"AVAL": "A", "SEQ": 2}]

    assert [_value(rank(partition(rows, i), ["AVAL"])) for i in (0, 1)] == [1, 1]
    assert [_value(rank(partition(rows, i), ["AVAL", "SEQ"])) for i in (0, 1)] == [1, 2]


def test_an_excluded_row_is_neither_numbered_nor_ranked() -> None:
    rows = visits("A", "B")

    assert _value(rank(partition(rows, 0, [False, True]), ["AVAL"])) is MISSING
    assert _value(rank(partition(rows, 1, [False, True]), ["AVAL"])) == 1


# --- row_value -----------------------------------------------------------


@pytest.mark.parametrize(
    ("current", "offset", "expected"),
    [
        (1, -1, 5),
        (1, 1, 7),
        (0, -1, MISSING),
        (2, 1, MISSING),
        (0, 2, 7),
    ],
)
def test_an_offset_moves_along_the_declared_order(
    current: int, offset: int, expected: object
) -> None:
    # A row with fewer than |offset| rows on that side yields missing, which
    # does not distinguish an absent row from a present missing value.
    assert _value(row_value(partition(visits(5, 6, 7), current), "AVAL", offset)) == (
        expected
    )


def test_a_zero_offset_is_refused_rather_than_read_as_the_current_row() -> None:
    # R007-43: the current row's own value is `source`.
    condition = _condition(row_value(partition(visits(5), 0), "AVAL", 0))

    assert condition.condition.condition == "zero_offset"
    assert condition.condition.requirement == "R007-43"


# --- previous_non_missing ------------------------------------------------


def test_one_result_crosses_any_number_of_consecutive_gaps() -> None:
    rows = visits(5, MISSING, MISSING, 8)

    assert _value(previous_non_missing(partition(rows, 3), "AVAL")) == 5


def test_the_current_row_is_never_its_own_candidate() -> None:
    # R001-29: it searches a separate completed source column, so nothing
    # here reads the column being derived.
    rows = visits(5, 6)

    assert _value(previous_non_missing(partition(rows, 1), "AVAL")) == 5
    assert _value(previous_non_missing(partition(rows, 0), "AVAL")) is MISSING


def test_a_row_with_no_earlier_non_missing_value_yields_missing() -> None:
    rows = visits(MISSING, MISSING, 8)

    assert _value(previous_non_missing(partition(rows, 1), "AVAL")) is MISSING


# --- baseline_flag and baseline_value ------------------------------------


def dated(*pairs: tuple[str | object, str]) -> list[RowValues]:
    return [
        {
            "ADT": DateValue.parse(day) if isinstance(day, str) else day,
            "TRTSDT": DateValue.parse(reference),
        }
        for day, reference in pairs
    ]


def test_the_latest_eligible_row_is_the_only_one_flagged() -> None:
    rows = dated(
        ("2025-01-02", "2025-01-10"),
        ("2025-01-06", "2025-01-10"),
        ("2025-01-20", "2025-01-10"),
    )

    flags = [
        _value(baseline_flag(partition(rows, index), "ADT", "TRTSDT"))
        for index in range(3)
    ]

    # The third row's date is after the reference, so it is not eligible.
    assert flags == [MISSING, "Y", MISSING]


def test_a_tie_for_the_latest_eligible_date_is_refused() -> None:
    rows = dated(("2025-01-06", "2025-01-10"), ("2025-01-06", "2025-01-10"))

    condition = _condition(baseline_flag(partition(rows, 0), "ADT", "TRTSDT"))

    assert condition.condition.condition == "ambiguous_baseline"
    assert condition.condition.context["date"] == "2025-01-06"
    assert condition.condition.context["match_count"] == 2


def test_a_partition_with_no_eligible_row_flags_nothing() -> None:
    rows = dated(("2025-01-20", "2025-01-10"), (MISSING, "2025-01-10"))

    assert _value(baseline_flag(partition(rows, 0), "ADT", "TRTSDT")) is MISSING
    assert _value(baseline_flag(partition(rows, 1), "ADT", "TRTSDT")) is MISSING


def test_the_flagged_row_broadcasts_its_value_to_the_partition() -> None:
    rows = [
        {"AVAL": 5.1, "ABLFL": "Y"},
        {"AVAL": 5.4, "ABLFL": MISSING},
        {"AVAL": 5.2, "ABLFL": MISSING},
    ]

    values = [
        _value(baseline_value(partition(rows, index), "AVAL", "ABLFL"))
        for index in range(3)
    ]

    assert values == [5.1, 5.1, 5.1]


def test_no_flagged_row_broadcasts_missing_and_two_are_refused() -> None:
    none_flagged = [{"AVAL": 1.0, "ABLFL": MISSING}]
    two_flagged = [{"AVAL": 1.0, "ABLFL": "Y"}, {"AVAL": 2.0, "ABLFL": "Y"}]

    assert (
        _value(baseline_value(partition(none_flagged, 0), "AVAL", "ABLFL")) is MISSING
    )
    condition = _condition(baseline_value(partition(two_flagged, 0), "AVAL", "ABLFL"))
    assert condition.condition.condition == "ambiguous_baseline"
    assert condition.condition.context["flag_count"] == 2


# --- dispatch ------------------------------------------------------------


def test_every_registered_window_dispatches_to_its_own_operation() -> None:
    rows = [{"AVAL": 5, "ABLFL": "Y"}, {"AVAL": 6, "ABLFL": MISSING}]
    located = partition(rows, 1)

    assert _value(evaluate_window("row_number", {}, located)) == 2
    assert _value(evaluate_window("rank", {"order_by": ["AVAL"]}, located)) == 2
    assert (
        _value(evaluate_window("row_value", {"source": "AVAL", "offset": -1}, located))
        == 5
    )
    assert (
        _value(evaluate_window("previous_non_missing", {"source": "AVAL"}, located))
        == 5
    )
    assert (
        _value(
            evaluate_window(
                "baseline_value", {"value": "AVAL", "flag": "ABLFL"}, located
            )
        )
        == 5
    )


def test_a_method_outside_the_two_r007_names_is_refused() -> None:
    condition = _condition(
        evaluate_window(
            "rank", {"order_by": ["AVAL"], "method": "olympic"}, partition(visits(1), 0)
        )
    )

    assert condition.condition.condition == "value_not_permitted"
