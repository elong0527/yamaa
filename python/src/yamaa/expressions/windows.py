"""The R007 window operations over one already-ordered partition.

A window reads the constructed output rows of its partition and answers for
one of them, so everything here takes an ordered sequence of rows and the
position of the current row in it. Which rows are in the partition and what
order they are in belong to the runtime that owns the constructed rows; this
module decides only what each operation reports once they are.

Row count is preserved by construction: every operation answers for the row
it was asked about, and a row its `filter` excluded receives missing rather
than disappearing (REQ-0294).
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
    `eligible` says which of them the window's `filter` retained: REQ-0294
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

    REQ-0303 makes `rank` compare only the declared terms, so records equal on
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
        # REQ-0328: the current row's own value is `source`, and a window must
        # not be a second spelling of it.
        return _condition("zero_offset", {"offset": offset}, requirement="REQ-0328")
    target = partition.current + offset
    if not 0 <= target < len(partition.rows):
        # REQ-0294: a row that does not exist reads the same as a present row
        # whose value is missing.
        return _missing()
    return ValueResult(value=_read(partition.rows[target], source))


def previous_non_missing(partition: Partition, source: str) -> WindowResult:
    """Read the closest strictly earlier row whose source is not missing.

    REQ-0061 makes this cross any number of missing rows by searching a
    separate completed source column; the current row is never a candidate,
    so nothing here reads the column being derived.
    """
    if not partition.eligible[partition.current]:
        return _missing()
    for index in range(partition.current - 1, -1, -1):
        if not partition.eligible[index]:
            continue
        value = _read(partition.rows[index], source)
        if value is not MISSING:
            return ValueResult(value=value)
    return _missing()


def locf(partition: Partition, source: str) -> WindowResult:
    """Keep the current observed value or the closest earlier present value."""
    if not partition.eligible[partition.current]:
        return _missing()
    value = _read(partition.rows[partition.current], source)
    if value is not MISSING:
        return ValueResult(value=value)
    return previous_non_missing(partition, source)


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
                requirement="REQ-0323",
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
            requirement="REQ-0322",
        )
    return ValueResult(value="Y" if partition.current == latest else MISSING)


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
    "locf",
    "baseline_flag",
)


def window_spec(payload: Mapping[str, object]) -> Mapping[str, object]:
    """Return the window_spec the payload declares; absent means an empty one.

    R007 nests partitioning, ordering, and filtering under `window:` so the
    window expressions share one definition instead of repeating the
    same three fields. Every reader of those fields goes through here.
    """
    window = payload.get("window")
    return window if isinstance(window, Mapping) else {}


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
                requirement="REQ-0322",
            )
        return rank(partition, _order_variables(payload), method)  # type: ignore[arg-type]
    if operation == "row_value":
        offset = payload.get("offset")
        if type(offset) is not int:
            return _condition(
                "invalid_field_type",
                {"operation": operation, "expected": "an integer offset"},
                requirement="REQ-0321",
            )
        return row_value(partition, str(payload.get("source")), offset)
    if operation == "previous_non_missing":
        return previous_non_missing(partition, str(payload.get("source")))
    if operation == "locf":
        return locf(partition, str(payload.get("source")))
    if operation == "baseline_flag":
        return baseline_flag(
            partition,
            str(payload.get("date")),
            str(payload.get("reference_date")),
        )
    raise ValueError(f"unknown window operation: {operation}")


def _order_variables(payload: Mapping[str, object]) -> tuple[str, ...]:
    terms = window_spec(payload).get("order_by")
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
