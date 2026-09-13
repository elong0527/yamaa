"""The R007 window operations over one already-ordered partition.

A window reads the constructed output rows of its partition and answers for
one of them, so everything here takes an ordered sequence of rows and the
position of the current row in it. Which rows are in the partition and what
order they are in belong to the runtime that owns the constructed rows; this
module decides only what each operation reports once they are.

Row count is preserved by construction: every operation answers for the row
it was asked about, and a row its `filter` excluded receives missing rather
than disappearing (R007-7).
"""

from __future__ import annotations

from collections.abc import Mapping, Sequence
from dataclasses import dataclass
from typing import Literal, TypeAlias

from pydantic import JsonValue

from yamaa.expressions.core import (
    ExpressionHandler,
    expression_condition,
    relational_handler,
)
from yamaa.models import (
    MISSING,
    ConditionResult,
    EvaluationResult,
    RuntimeValue,
    ValueResult,
    compare_values,
)

RowValues: TypeAlias = Mapping[str, object]
WindowResult: TypeAlias = ValueResult | ConditionResult


@dataclass(frozen=True, slots=True)
class Partition:
    """One window partition, ordered, with the current row located in it.

    `rows` are the partition's rows in the order the window's `order_by`
    terms put them. `current` is the current row's index in that order, and
    `eligible` says which of them the window's `filter` retained: R007-7
    keeps an excluded row in the output and gives it missing rather than
    dropping it, so the excluded rows are absent from the numbering and
    present in the row set.
    """

    rows: tuple[RowValues, ...]
    current: int
    eligible: tuple[bool, ...]

    def __post_init__(self) -> None:
        if len(self.rows) != len(self.eligible):
            raise ValueError("every partition row states whether it is eligible")
        if not 0 <= self.current < len(self.rows):
            raise ValueError("the current row must lie in its own partition")

    @property
    def current_row(self) -> RowValues:
        return self.rows[self.current]

    def numbered(self) -> tuple[int, ...]:
        """Return the indexes the window numbers, in order."""
        return tuple(index for index, keep in enumerate(self.eligible) if keep)


def _condition(
    condition: str,
    context: dict[str, JsonValue],
    *,
    requirement: str | None = None,
) -> ConditionResult:
    return expression_condition("derivation", condition, context, None, requirement)


def _missing() -> ValueResult:
    return ValueResult(value=MISSING)


def row_number(partition: Partition) -> WindowResult:
    """Number the eligible rows from 1 along the declared order."""
    if not partition.eligible[partition.current]:
        return _missing()
    return ValueResult(value=partition.numbered().index(partition.current) + 1)


def _tied(left: RowValues, right: RowValues, terms: Sequence[str]) -> bool:
    """Return whether two rows are equal on every order term.

    Two missing values are equal for this purpose whatever `nulls` does with
    them, because `nulls` places a row and does not distinguish it.
    """
    return all(left.get(term, MISSING) == right.get(term, MISSING) for term in terms)


def rank(
    partition: Partition,
    terms: Sequence[str],
    method: Literal["competition", "dense"] = "competition",
) -> WindowResult:
    """Number the eligible rows, sharing one number across a tie.

    R007-18 makes `rank` compare only the declared terms, so records equal on
    every one of them receive a single number rather than the distinct
    numbers their positions would give.
    """
    if not partition.eligible[partition.current]:
        return _missing()
    numbered = partition.numbered()
    value = 0
    previous: RowValues | None = None
    for position, index in enumerate(numbered):
        row = partition.rows[index]
        if previous is None or not _tied(previous, row, terms):
            # `competition` leaves the positions a tie occupied out of the
            # numbers that follow; `dense` numbers distinct values in turn.
            value = position + 1 if method == "competition" else value + 1
            previous = row
        if index == partition.current:
            return ValueResult(value=value)
    raise AssertionError("an eligible current row is always numbered")


def row_value(partition: Partition, source: str, offset: int) -> WindowResult:
    """Read one source from the row `offset` places along the declared order."""
    if offset == 0:
        # R007-43: the current row's own value is `source`, and a window must
        # not be a second spelling of it.
        return _condition("zero_offset", {"offset": offset}, requirement="R007-43")
    target = partition.current + offset
    if not 0 <= target < len(partition.rows):
        # R007-7: a row that does not exist reads the same as a present row
        # whose value is missing.
        return _missing()
    return ValueResult(value=_read(partition.rows[target], source))


def previous_non_missing(partition: Partition, source: str) -> WindowResult:
    """Read the closest strictly earlier row whose source is not missing.

    R001-29 makes this cross any number of missing rows by searching a
    separate completed source column; the current row is never a candidate,
    so nothing here reads the column being derived.
    """
    for index in range(partition.current - 1, -1, -1):
        value = _read(partition.rows[index], source)
        if value is not MISSING:
            return ValueResult(value=value)
    return _missing()


def baseline_flag(
    partition: Partition,
    date: str,
    reference_date: str,
) -> WindowResult:
    """Flag the one latest row whose date is at or before the reference."""
    latest: int | None = None
    for index, row in enumerate(partition.rows):
        candidate = _read(row, date)
        reference = _read(row, reference_date)
        if candidate is MISSING or reference is MISSING:
            continue
        try:
            if compare_values(candidate, reference) > 0:
                continue
            if latest is None:
                latest = index
                continue
            order = compare_values(candidate, _read(partition.rows[latest], date))
        except TypeError:
            return _condition(
                "incompatible_input_type",
                {"operation": "baseline_flag", "source": date},
                requirement="R007-38",
            )
        if order > 0:
            latest = index
    if latest is None:
        return _missing()
    tied = [
        index
        for index, row in enumerate(partition.rows)
        if index != latest
        and _read(row, date) is not MISSING
        and _read(row, date) == _read(partition.rows[latest], date)
        and _read(row, reference_date) is not MISSING
        and compare_values(_read(row, date), _read(row, reference_date)) <= 0
    ]
    if tied:
        # A tie for the latest eligible date leaves no unique baseline, and
        # choosing by position would make the answer depend on file order.
        return _condition(
            "ambiguous_baseline",
            {
                "operation": "baseline_flag",
                "date": _rendered(_read(partition.rows[latest], date)),
                "match_count": len(tied) + 1,
            },
            requirement="R007-37",
        )
    return ValueResult(value="Y" if partition.current == latest else MISSING)


def baseline_value(
    partition: Partition,
    value: str,
    flag: str,
) -> WindowResult:
    """Broadcast the value from the one row the flag marks."""
    flagged = [
        index for index, row in enumerate(partition.rows) if _read(row, flag) == "Y"
    ]
    if not flagged:
        return _missing()
    if len(flagged) > 1:
        return _condition(
            "ambiguous_baseline",
            {
                "operation": "baseline_value",
                "flag": flag,
                "flag_count": len(flagged),
            },
            requirement="R007-37",
        )
    return ValueResult(value=_read(partition.rows[flagged[0]], value))


def _rendered(value: RuntimeValue) -> JsonValue:
    """Render one value for a structured diagnostic context."""
    text = getattr(value, "to_text", None)
    if callable(text):
        return str(text())
    if value is MISSING:
        return None
    return value if isinstance(value, (str, int, float, bool)) else str(value)


def _read(row: RowValues, name: str) -> RuntimeValue:
    value = row.get(name, MISSING)
    return value if value is not None else MISSING  # type: ignore[return-value]


WINDOW_OPERATIONS: tuple[str, ...] = (
    "row_number",
    "rank",
    "row_value",
    "previous_non_missing",
    "baseline_flag",
    "baseline_value",
)


def evaluate_window(
    operation: str,
    payload: Mapping[str, object],
    partition: Partition,
) -> EvaluationResult:
    """Dispatch one window operation over its located partition."""
    if operation == "row_number":
        return row_number(partition)
    if operation == "rank":
        method = payload.get("method", "competition")
        if method not in {"competition", "dense"}:
            return _condition(
                "value_not_permitted",
                {"field": "method", "value": str(method)},
                requirement="R007-37",
            )
        return rank(partition, _order_variables(payload), method)  # type: ignore[arg-type]
    if operation == "row_value":
        offset = payload.get("offset")
        if type(offset) is not int:
            return _condition(
                "invalid_field_type",
                {"operation": operation, "expected": "an integer offset"},
                requirement="R007-36",
            )
        return row_value(partition, str(payload.get("source")), offset)
    if operation == "previous_non_missing":
        return previous_non_missing(partition, str(payload.get("source")))
    if operation == "baseline_flag":
        return baseline_flag(
            partition,
            str(payload.get("date")),
            str(payload.get("reference_date")),
        )
    return baseline_value(
        partition, str(payload.get("value")), str(payload.get("flag"))
    )


def _order_variables(payload: Mapping[str, object]) -> tuple[str, ...]:
    terms = payload.get("order_by")
    if not isinstance(terms, Sequence) or isinstance(terms, str):
        return ()
    names: list[str] = []
    for term in terms:
        if isinstance(term, str):
            names.append(term)
        elif isinstance(term, Mapping) and isinstance(term.get("variable"), str):
            names.append(str(term["variable"]))
    return tuple(names)


def window_handlers() -> dict[str, ExpressionHandler]:
    """Return the R007 window operations this component registers.

    Each reads the constructed output rows of its partition, so each is
    dispatched to the resolver that owns them.
    """
    return {operation: relational_handler(operation) for operation in WINDOW_OPERATIONS}
