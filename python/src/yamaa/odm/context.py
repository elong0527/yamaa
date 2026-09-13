"""Resolve bound names and complete long-form ODM contexts."""

from __future__ import annotations

from collections.abc import Mapping, Sequence
from dataclasses import dataclass
from functools import cmp_to_key
from typing import Literal

from pydantic import BaseModel, ConfigDict, Field, JsonValue, ValidationError

from yamaa.expressions import (
    AbsentValue,
    FailedResolution,
    PredicateError,
    PredicateValue,
    ReadOptions,
    Resolution,
    ResolvedValue,
    TruthValue,
    evaluate_predicate,
    parse_predicate,
)
from yamaa.io.polars import runtime_rows, runtime_value
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
    """The normalized R008 policy for choosing a duplicate contextual item."""

    order_by: list[OrderTerm] = Field(min_length=1)
    keep: Literal["first", "last"]
    filter: str | None = None


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

    def resolve(
        self,
        variable: str,
        read: ReadOptions | None = None,
    ) -> Resolution:
        del read
        if "." not in variable:
            return _failure("validation", "unknown_field", {"identifier": variable})
        qualifier, field = variable.split(".", 1)
        if qualifier != self._dataset or field not in self._fields:
            return _failure("validation", "unknown_field", {"identifier": variable})
        return ResolvedValue(value=self._row.values[field])


def _ordered(value: RuntimeValue) -> object:
    ordering_key = getattr(value, "ordering_key", None)
    return ordering_key if ordering_key is not None else value


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


def _keep(
    records: Sequence[_IndexedRow],
    predicate: str,
    dataset: str,
    fields: tuple[str, ...],
) -> list[_IndexedRow] | FailedResolution:
    """Return the records an R004 predicate keeps, or why it could not run."""
    try:
        ast = parse_predicate(predicate)
    except PredicateError as error:
        return _failure(
            "validation",
            "invalid_predicate",
            {"predicate": predicate, "position": error.position},
        )

    kept: list[_IndexedRow] = []
    for record in records:
        result = evaluate_predicate(ast, _CandidateResolver(dataset, fields, record))
        if isinstance(result, ConditionResult):
            return FailedResolution(condition=result.condition)
        assert isinstance(result, PredicateValue)
        if result.value is TruthValue.TRUE:
            kept.append(record)
    return kept


def _select_one(
    matches: Sequence[_IndexedRow],
    selection: MultipleMatchSelection,
    variable: str,
    dataset: str,
    fields: tuple[str, ...],
) -> tuple[_IndexedRow, bool] | Resolution:
    """Apply one R008 `multiple_matches` rule, or report why it cannot.

    The flag says whether more than one record survived the filter, which
    is the only case R008-15 counts as the handler firing.
    """
    terms: list[tuple[OrderTerm, str]] = []
    for term in selection.order_by:
        if "." not in term.variable:
            return _failure(
                "validation", "unknown_field", {"identifier": term.variable}
            )
        qualifier, field = term.variable.split(".", 1)
        if qualifier != dataset or field not in fields:
            return _failure(
                "validation", "unknown_field", {"identifier": term.variable}
            )
        terms.append((term, field))

    eligible: Sequence[_IndexedRow] = matches
    if selection.filter is not None:
        narrowed = _keep(matches, selection.filter, dataset, fields)
        if isinstance(narrowed, FailedResolution):
            return narrowed
        eligible = narrowed
    if not eligible:
        # R008-14: an empty filtered right side yields missing and does not
        # invoke either source handler.
        return ResolvedValue(value=MISSING)
    if len(eligible) == 1:
        return eligible[0], False

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
        ordered = sorted(eligible, key=cmp_to_key(compare))
    except TypeError:
        first = terms[0][0].variable if terms else variable
        return _failure("validation", "incompatible_input_type", {"source": first})
    chosen = ordered[0] if selection.keep == "first" else ordered[-1]
    return chosen, True


def _differing_columns(
    records: Sequence[_IndexedRow],
) -> dict[str, list[JsonValue]]:
    """Report the columns whose values differ across records matched together.

    R001-44 asks for this because it names the column the read did not put in
    `on`, which is the one thing the author needs in order to fix the read.
    """
    differing: dict[str, list[JsonValue]] = {}
    for column in records[0].values:
        seen: list[object] = []
        for record in records:
            value = record.values.get(column, MISSING)
            if value not in seen:
                seen.append(value)
        if len(seen) > 1:
            differing[column] = [_reported(value) for value in seen]
    return differing


def _reported(value: object) -> JsonValue:
    if value is MISSING or value is None:
        return None
    if isinstance(value, str | int | float | bool):
        return value
    return str(value)


def _typed_table(value: LoadedDataset | TypedTable) -> TypedTable:
    return value.table if isinstance(value, LoadedDataset) else value


class BindingIndex:
    """Reusable source records that create one resolver per output row."""

    def __init__(
        self,
        plan: BindingPlan,
        sources: Mapping[str, LoadedDataset | TypedTable],
    ) -> None:
        if set(plan.datasets) != set(sources):
            raise ValueError("indexed source names must exactly match the binding plan")
        self.plan = plan
        self._tables: dict[str, TypedTable] = {}
        for dataset, source in sources.items():
            table = _typed_table(source)
            if table.columns != plan.datasets[dataset].columns:
                raise ValueError(
                    f"indexed table for {dataset!r} does not match the binding plan"
                )
            self._tables[dataset] = table
        self._records: dict[str, list[_IndexedRow]] = {}
        self._selections: dict[
            tuple[str, str | None, tuple[str, ...]],
            _Selection | FailedResolution,
        ] = {}

    def fields(self, dataset: str) -> tuple[str, ...]:
        return self.plan.datasets[dataset].field_names

    def records(self, dataset: str) -> list[_IndexedRow]:
        """Read one dataset as runtime records, once per run."""
        cached = self._records.get(dataset)
        if cached is None:
            cached = [
                _IndexedRow(source_position=position, values=values)
                for position, values in enumerate(runtime_rows(self._tables[dataset]))
            ]
            self._records[dataset] = cached
        return cached

    def selection(
        self,
        dataset: str,
        predicate: str | None,
        on: tuple[str, ...],
    ) -> _Selection | FailedResolution:
        """Group the records a filtered read can reach, once per read.

        R002-20's read runs for every output row, so the filter is evaluated
        once over the dataset and its survivors are grouped by the `on`
        columns a row matches them on. Each row is then one lookup.
        """
        signature = (dataset, predicate, on)
        cached = self._selections.get(signature)
        if cached is not None:
            return cached

        fields = self.fields(dataset)
        unknown = [column for column in on if column not in fields]
        if unknown:
            selection: _Selection | FailedResolution = _failure(
                "validation",
                "unknown_field",
                {"identifier": f"{dataset}.{unknown[0]}"},
            )
        else:
            records: Sequence[_IndexedRow] = self.records(dataset)
            if predicate is not None:
                kept = _keep(records, predicate, dataset, fields)
                if isinstance(kept, FailedResolution):
                    self._selections[signature] = kept
                    return kept
                records = kept
            groups: dict[tuple[object, ...], list[_IndexedRow]] = {}
            for record in records:
                key = tuple(record.values[column] for column in on)
                groups.setdefault(key, []).append(record)
            selection = _Selection(groups=groups)
        self._selections[signature] = selection
        return selection

    def context(
        self,
        source_rows: Mapping[str, Mapping[str, object]],
        output_values: Mapping[str, object] | None = None,
        *,
        feeding_rows: Mapping[str, Sequence[Mapping[str, object]]] | None = None,
    ) -> RuntimeContext:
        """Create a row-local resolver over shared immutable records.

        ``feeding_rows`` carries every driver record of the current key
        combination, which a read without a filter resolves across: R001-12b
        counts those records, so one answers and two are two records the
        specification has not chosen between.
        """
        return RuntimeContext(self, source_rows, output_values or {}, feeding_rows)


@dataclass(frozen=True, slots=True)
class _Selection:
    """Records surviving one read's filter, grouped by its `on` values."""

    groups: dict[tuple[object, ...], list[_IndexedRow]]


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

    def resolve(
        self,
        variable: str,
        read: ReadOptions | None = None,
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
        assert bound.field is not None
        row = self._source_rows.get(bound.dataset)
        if row is None:
            # Selecting a row from another relation belongs to R003/#217. This
            # component only resolves rows explicitly supplied by its caller.
            return _failure("validation", "unknown_field", {"identifier": variable})
        if bound.field not in row:
            return _failure("validation", "unknown_field", {"identifier": variable})

        if read is not None and read.selects_records:
            matched = self._matched(bound.dataset, row, read)
            if isinstance(matched, FailedResolution):
                return matched
        else:
            matched = [
                _IndexedRow(source_position=position, values=values)
                for position, values in enumerate(
                    self._feeding_rows.get(bound.dataset, [row])
                )
            ]
        return self._one(variable, bound.dataset, bound.field, matched, read)

    def _matched(
        self,
        dataset: str,
        row: Mapping[str, object],
        read: ReadOptions,
    ) -> list[_IndexedRow] | FailedResolution:
        """Return the records this row matches under R002-22."""
        selection = self._index.selection(dataset, read.filter, read.on)
        if isinstance(selection, FailedResolution):
            return selection
        key = tuple(runtime_value(row[column]) for column in read.on)
        return selection.groups.get(key, [])

    def _one(
        self,
        variable: str,
        dataset: str,
        field: str,
        matched: Sequence[_IndexedRow],
        read: ReadOptions | None,
    ) -> Resolution:
        """Reduce the matched records to the one value R001-12b requires."""
        if read is not None and read.multiple_matches is not None:
            # R008-13 applies the declared rule to whatever matched, including
            # one record: its filter answers for that record too.
            try:
                selection = MultipleMatchSelection.model_validate(
                    dict(read.multiple_matches), strict=True
                )
            except ValidationError:
                return _failure(
                    "validation",
                    "invalid_field_type",
                    {"field": "multiple_matches"},
                )
            if not matched:
                return AbsentValue(variable=variable)
            chosen = _select_one(
                matched, selection, variable, dataset, self._index.fields(dataset)
            )
            if not isinstance(chosen, tuple):
                return chosen
            record, handled = chosen
            return ResolvedValue(
                value=runtime_value(record.values[field]),
                handled_by="multiple_matches" if handled else None,
            )
        if not matched:
            return AbsentValue(variable=variable)
        if len(matched) > 1:
            return _failure(
                "derivation",
                "multiple_rows_per_key",
                {
                    "identifier": variable,
                    "row_count": len(matched),
                    "values": [_reported(record.values[field]) for record in matched],
                    "differing_columns": _differing_columns(matched),
                },
                applicable_handler="multiple_matches",
                requirement="R001-44",
            )
        return ResolvedValue(value=runtime_value(matched[0].values[field]))
