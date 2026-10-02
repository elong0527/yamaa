"""Resolve bound names against the records a row reaches."""

from __future__ import annotations

from collections.abc import Collection, Iterable, Mapping, Sequence
from dataclasses import dataclass
from functools import cmp_to_key
from typing import Literal

from pydantic import BaseModel, ConfigDict, Field, JsonValue, ValidationError

from yamaa.expressions import (
    FailedResolution,
    PredicateError,
    PredicateValue,
    Resolution,
    ResolvedValue,
    TruthValue,
    evaluate_predicate,
    parse_predicate,
)
from yamaa.io.polars import runtime_value
from yamaa.io.source import LoadedDataset
from yamaa.models import (
    MISSING,
    ConditionPhase,
    ConditionResult,
    RuntimeCondition,
    RuntimeValue,
    TypedTable,
    runtime_type_name,
    values_comparable,
)
from yamaa.odm.bindings import BindingFailure, BindingPlan
from yamaa.specification.models import OrderTerm


class _FrozenModel(BaseModel):
    model_config = ConfigDict(strict=True, extra="forbid", frozen=True)


class MultipleMatchSelection(_FrozenModel):
    """The normalized R008 policy for keeping one of several matching records."""

    order_by: list[OrderTerm] = Field(min_length=1)
    keep: Literal["first", "last"]


@dataclass(frozen=True, slots=True)
class _IndexedRow:
    source_position: int
    values: dict[str, object]


def _failure(
    phase: ConditionPhase,
    condition: str,
    context: dict[str, JsonValue],
    *,
    applicable_handler: Literal["multiple_matches"] | None = None,
    requirement: str | None = None,
) -> FailedResolution:
    return FailedResolution(
        condition=RuntimeCondition(
            phase=phase,
            condition=condition,
            context=context,
            applicable_handler=applicable_handler,
            requirement=requirement,
        )
    )


class _CandidateResolver:
    """Expose only one right-side record to an R008 predicate."""

    def __init__(self, dataset: str, fields: tuple[str, ...], row: _IndexedRow) -> None:
        self._dataset = dataset
        self._fields = fields
        self._row = row

    def resolve(self, variable: str) -> Resolution:
        if "." not in variable:
            return _failure("validation", "unknown_field", {"identifier": variable})
        qualifier, field = variable.split(".", 1)
        if qualifier != self._dataset or field not in self._fields:
            return _failure("validation", "unknown_field", {"identifier": variable})
        return ResolvedValue(value=self._row.values[field])


def _ordered(value: RuntimeValue) -> object:
    ordering_key = getattr(value, "ordering_key", None)
    return ordering_key if ordering_key is not None else value


def _distinct_values(readings: Iterable[object]) -> list[RuntimeValue]:
    """Collapse repeated readings of one value, in first-appearance order.

    REQ-0044 counts the values a derivation yields for one key combination,
    not the records carrying them, so a field constant over a subject's
    records reads as that single value. Each runtime type owns that identity
    and hashes it -- REQ-0573 keeps collected precision out of a date's -- so
    a value is held beside its type name: that separates a number from a flag
    carrying it, and keeps a key combination covering the whole input from
    comparing every reading against every other one.
    """
    distinct: list[RuntimeValue] = []
    seen: set[tuple[str | None, RuntimeValue]] = set()
    for reading in readings:
        value: RuntimeValue = runtime_value(reading)  # type: ignore[assignment]
        token = (runtime_type_name(value), value)
        if token in seen:
            continue
        seen.add(token)
        distinct.append(value)
    return distinct


def _compare_runtime(left: RuntimeValue, right: RuntimeValue) -> int:
    if not values_comparable(left, right):
        raise TypeError(
            f"incomparable order values {runtime_type_name(left)!r} and "
            f"{runtime_type_name(right)!r}"
        )
    ordered_left = _ordered(left)
    ordered_right = _ordered(right)
    if ordered_left < ordered_right:  # type: ignore[operator]
        return -1
    if ordered_left > ordered_right:  # type: ignore[operator]
        return 1
    return 0


def select_one(
    dataset: str,
    fields: tuple[str, ...],
    matches: list[_IndexedRow],
    selection: MultipleMatchSelection,
    variable: str,
    field_name: str,
) -> Resolution:
    """Order the eligible records and keep the one REQ-0354 declares.

    The records reach one value, so the selection is over records the
    specification already narrowed: a single one is that value and fires no
    handler, and ties fall back to the order the source carries them in.
    """
    terms: list[tuple[OrderTerm, str]] = []
    for term in selection.order_by:
        if "." not in term.variable:
            return _failure(
                "validation",
                "unknown_field",
                {"identifier": term.variable},
            )
        qualifier, field = term.variable.split(".", 1)
        if qualifier != dataset or field not in fields:
            return _failure(
                "validation",
                "unknown_field",
                {"identifier": term.variable},
            )
        terms.append((term, field))

    if len(matches) == 1:
        return ResolvedValue(value=matches[0].values[field_name])

    def compare(left: _IndexedRow, right: _IndexedRow) -> int:
        for term, field in terms:
            left_value = left.values[field]
            right_value = right.values[field]
            left_missing = left_value is MISSING
            right_missing = right_value is MISSING
            if left_missing or right_missing:
                if left_missing and right_missing:
                    continue
                missing_first = term.nulls == "first"
                result = -1 if left_missing == missing_first else 1
            else:
                result = _compare_runtime(  # type: ignore[arg-type]
                    left_value, right_value
                )
                if term.direction == "desc":
                    result = -result
            if result:
                return result
        return left.source_position - right.source_position

    try:
        ordered = sorted(matches, key=cmp_to_key(compare))
    except TypeError:
        first = terms[0][0].variable if terms else variable
        return _failure(
            "validation",
            "incompatible_input_type",
            {"source": first},
        )
    chosen = ordered[0] if selection.keep == "first" else ordered[-1]
    return ResolvedValue(
        value=chosen.values[field_name],
        handled_by="multiple_matches",
    )


def _typed_table(value: LoadedDataset | TypedTable) -> TypedTable:
    return value.table if isinstance(value, LoadedDataset) else value


class BindingIndex:
    """A checked binding plan that creates one resolver per output row."""

    def __init__(
        self,
        plan: BindingPlan,
        sources: Mapping[str, LoadedDataset | TypedTable],
        *,
        virtual_datasets: Collection[str] = (),
    ) -> None:
        if set(plan.datasets) - set(virtual_datasets) != set(sources):
            raise ValueError("indexed source names must exactly match the binding plan")
        self.plan = plan
        for dataset, source in sources.items():
            table = _typed_table(source)
            if table.columns != plan.datasets[dataset].columns:
                raise ValueError(
                    f"indexed table for {dataset!r} does not match the binding plan"
                )

    def context(
        self,
        source_rows: Mapping[str, Mapping[str, object]],
        output_values: Mapping[str, object] | None = None,
        *,
        feeding_rows: Mapping[str, Sequence[Mapping[str, object]]] | None = None,
    ) -> RuntimeContext:
        """Create a row-local resolver over shared immutable indexes.

        ``feeding_rows`` carries every driver record of the current key
        combination and section. A plain dataset field read collects one
        value across them under REQ-0044: no value is missing, repeated
        readings of one value are that value, and two values disagreeing
        fail.
        """
        return RuntimeContext(self, source_rows, output_values or {}, feeding_rows)


class RuntimeContext:
    """R002 resolver for one constructed row and its completed outputs."""

    def __init__(
        self,
        index: BindingIndex,
        source_rows: Mapping[str, Mapping[str, object]],
        output_values: Mapping[str, object],
        feeding_rows: Mapping[str, Sequence[Mapping[str, object]]] | None = None,
    ) -> None:
        self._index = index
        self._source_rows = {dataset: dict(row) for dataset, row in source_rows.items()}
        self._output_values = dict(output_values)
        self._feeding_rows = {
            dataset: [dict(row) for row in rows]
            for dataset, rows in (feeding_rows or {}).items()
        }

    def resolve(self, variable: str) -> Resolution:
        return self._resolve(variable, selector=None, multiple_matches=None)

    def resolve_selected(
        self,
        variable: str,
        *,
        selector: str | None,
        multiple_matches: Mapping[str, object] | None,
    ) -> Resolution:
        return self._resolve(
            variable, selector=selector, multiple_matches=multiple_matches
        )

    def _resolve(
        self,
        variable: str,
        *,
        selector: str | None,
        multiple_matches: Mapping[str, object] | None,
    ) -> Resolution:
        bound = self._index.plan.bind(variable)
        if isinstance(bound, BindingFailure):
            return FailedResolution(condition=bound.condition)

        if bound.kind == "output":
            assert bound.field is not None
            if bound.field not in self._output_values:
                return _failure("validation", "unknown_field", {"identifier": variable})
            return ResolvedValue(value=runtime_value(self._output_values[bound.field]))

        assert bound.dataset is not None
        row = self._source_rows.get(bound.dataset)
        if row is None:
            # Selecting a row from another relation belongs to R003/#217. This
            # component only resolves rows explicitly supplied by its caller.
            return _failure("validation", "unknown_field", {"identifier": variable})
        assert bound.field is not None
        if bound.field not in row:
            return _failure("validation", "unknown_field", {"identifier": variable})
        feeding = self._feeding_rows.get(bound.dataset, [row])
        if selector is not None:
            # REQ-0131: the filter states which of the records this row
            # reaches the source may read, before REQ-0044 counts values.
            eligible = self._eligible(bound.dataset, selector, feeding)
            if isinstance(eligible, FailedResolution):
                return eligible
            feeding = eligible
        if not feeding:
            return ResolvedValue(value=MISSING)
        carrying = [
            feeding_row
            for feeding_row in feeding
            if bound.field in feeding_row
            and feeding_row[bound.field] is not MISSING
            and feeding_row[bound.field] is not None
        ]
        present = _distinct_values(feeding_row[bound.field] for feeding_row in carrying)
        if not present:
            # The records read carry no value, and that is the answer: the
            # row's own record may sit outside a filter, so it is no fallback.
            return ResolvedValue(value=MISSING)
        if len(present) > 1:
            if multiple_matches is not None:
                # REQ-0353: the specification says which of the records it
                # keeps, so the disagreement is answered rather than fatal.
                return self._select(
                    bound.dataset, bound.field, carrying, multiple_matches
                )
            # REQ-0044 counts values, not the records carrying them: a
            # field constant over a subject's records is one value, two
            # records disagreeing are two.
            return _failure(
                "derivation",
                "multiple_values_per_key",
                {"identifier": variable, "value_count": len(present)},
                requirement="REQ-0075",
            )
        return ResolvedValue(value=present[0])

    def _select(
        self,
        dataset: str,
        field_name: str,
        records: Sequence[Mapping[str, object]],
        multiple_matches: Mapping[str, object],
    ) -> Resolution:
        """Keep one of the records this key combination carries (REQ-0354)."""
        try:
            selection = MultipleMatchSelection.model_validate(
                dict(multiple_matches), strict=True
            )
        except ValidationError:
            return _failure(
                "validation",
                "invalid_field_type",
                {"field": "multiple_matches"},
            )
        return select_one(
            dataset,
            self._fields(dataset),
            [
                _IndexedRow(source_position=position, values=dict(record))
                for position, record in enumerate(records)
            ],
            selection,
            f"{dataset}.{field_name}",
            field_name,
        )

    def _fields(self, dataset: str) -> tuple[str, ...]:
        return tuple(
            column.name for column in self._index.plan.datasets[dataset].columns
        )

    def _eligible(
        self,
        dataset: str,
        selector: str,
        records: Sequence[Mapping[str, object]],
    ) -> list[dict[str, object]] | FailedResolution:
        """Keep the records a source filter selects, in their own order."""
        try:
            ast = parse_predicate(selector)
        except PredicateError as error:
            return _failure(
                "validation",
                "invalid_predicate",
                {"predicate": selector, "position": error.position},
                requirement=error.requirement,
            )
        fields = self._fields(dataset)
        kept: list[dict[str, object]] = []
        for position, record in enumerate(records):
            values = dict(record)
            result = evaluate_predicate(
                ast,
                _CandidateResolver(
                    dataset,
                    fields,
                    _IndexedRow(source_position=position, values=values),
                ),
            )
            if isinstance(result, ConditionResult):
                return FailedResolution(condition=result.condition)
            assert isinstance(result, PredicateValue)
            if result.value is TruthValue.TRUE:
                kept.append(values)
        return kept
