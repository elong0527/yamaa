"""Resolve bound names and complete long-form ODM contexts."""

from __future__ import annotations

from collections.abc import Mapping
from dataclasses import dataclass
from functools import cmp_to_key
from typing import Literal

from pydantic import BaseModel, ConfigDict, Field, JsonValue, ValidationError

from yamaa.expressions import (
    AbsentValue,
    FailedResolution,
    PredicateError,
    PredicateValue,
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
    ConditionResult,
    RuntimeCondition,
    RuntimeValue,
    TypedTable,
    runtime_type_name,
    values_comparable,
)
from yamaa.planning import ODM_CONTEXT_COLUMNS, BindingFailure, BindingPlan
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
    phase: Literal["validation", "join"],
    condition: str,
    context: dict[str, JsonValue],
    *,
    applicable_handler: Literal["multiple_matches"] | None = None,
) -> FailedResolution:
    return FailedResolution(
        condition=RuntimeCondition(
            phase=phase,
            condition=condition,
            context=context,
            applicable_handler=applicable_handler,
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


class OdmItemIndex:
    """A source-ordered index over complete available ODM context plus ItemOID."""

    def __init__(
        self,
        dataset: str,
        table: TypedTable,
        *,
        batch_size: int | None = None,
    ) -> None:
        if batch_size is not None and batch_size < 1:
            raise ValueError("batch_size must be positive")
        self.dataset = dataset
        self.fields = tuple(column.name for column in table.columns)
        self.context_columns = tuple(
            column for column in ODM_CONTEXT_COLUMNS if column in self.fields
        )
        self._records: dict[tuple[object, ...], list[_IndexedRow]] = {}
        if "ItemOID" not in self.fields or "Value" not in self.fields:
            return

        rows = runtime_rows(table)
        size = batch_size or max(len(rows), 1)
        for start in range(0, len(rows), size):
            for offset, row in enumerate(rows[start : start + size], start=start):
                item_oid = row["ItemOID"]
                if not isinstance(item_oid, str):
                    continue
                key = tuple(row[column] for column in self.context_columns) + (
                    item_oid,
                )
                self._records.setdefault(key, []).append(
                    _IndexedRow(source_position=offset, values=row)
                )

    def _filter(
        self,
        matches: list[_IndexedRow],
        predicate: str | None,
    ) -> list[_IndexedRow] | FailedResolution:
        if predicate is None:
            return matches
        try:
            ast = parse_predicate(predicate)
        except PredicateError as error:
            return _failure(
                "validation",
                "invalid_predicate",
                {"predicate": predicate, "position": error.position},
            )

        kept: list[_IndexedRow] = []
        for row in matches:
            result = evaluate_predicate(
                ast,
                _CandidateResolver(self.dataset, self.fields, row),
            )
            if isinstance(result, ConditionResult):
                return FailedResolution(condition=result.condition)
            assert isinstance(result, PredicateValue)
            if result.value is TruthValue.TRUE:
                kept.append(row)
        return kept

    def _select(
        self,
        matches: list[_IndexedRow],
        selection: MultipleMatchSelection,
        variable: str,
    ) -> Resolution:
        eligible = self._filter(matches, selection.filter)
        if isinstance(eligible, FailedResolution):
            return eligible
        if not eligible:
            # R008-14: an empty filtered right side yields missing and does not
            # invoke either source handler.
            return ResolvedValue(value=MISSING)
        if len(eligible) == 1:
            return ResolvedValue(value=eligible[0].values["Value"])

        terms: list[tuple[OrderTerm, str]] = []
        for term in selection.order_by:
            if "." not in term.variable:
                return _failure(
                    "validation",
                    "unknown_field",
                    {"identifier": term.variable},
                )
            qualifier, field = term.variable.split(".", 1)
            if qualifier != self.dataset or field not in self.fields:
                return _failure(
                    "validation",
                    "unknown_field",
                    {"identifier": term.variable},
                )
            terms.append((term, field))

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
            return _failure(
                "validation",
                "incompatible_input_type",
                {"source": first},
            )
        chosen = ordered[0] if selection.keep == "first" else ordered[-1]
        return ResolvedValue(
            value=chosen.values["Value"],
            handled_by="multiple_matches",
        )

    def resolve(
        self,
        item_oid: str,
        context_row: Mapping[str, object],
        *,
        multiple_matches: Mapping[str, object] | None = None,
    ) -> Resolution:
        variable = f"{self.dataset}.{item_oid}"
        if not self.context_columns:
            return _failure("validation", "unknown_field", {"identifier": variable})
        if any(column not in context_row for column in self.context_columns):
            return _failure("validation", "unknown_field", {"identifier": variable})

        key = tuple(
            runtime_value(context_row[column]) for column in self.context_columns
        ) + (item_oid,)
        matches = list(self._records.get(key, ()))
        if not matches:
            return AbsentValue(variable=variable)
        if multiple_matches is None:
            if len(matches) == 1:
                return ResolvedValue(value=matches[0].values["Value"])
            return _failure(
                "join",
                "multiple_matches",
                {"variable": variable, "matches": len(matches)},
                applicable_handler="multiple_matches",
            )
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
        return self._select(matches, selection, variable)


def _typed_table(value: LoadedDataset | TypedTable) -> TypedTable:
    return value.table if isinstance(value, LoadedDataset) else value


class BindingIndex:
    """Reusable source indexes that create one resolver per output row."""

    CONTEXT_COLUMNS = ODM_CONTEXT_COLUMNS

    def __init__(
        self,
        plan: BindingPlan,
        sources: Mapping[str, LoadedDataset | TypedTable],
        *,
        batch_size: int | None = None,
    ) -> None:
        if set(plan.datasets) != set(sources):
            raise ValueError("indexed source names must exactly match the binding plan")
        self.plan = plan
        for dataset, source in sources.items():
            table = _typed_table(source)
            if table.columns != plan.datasets[dataset].columns:
                raise ValueError(
                    f"indexed table for {dataset!r} does not match the binding plan"
                )
        self._odm = {
            dataset: OdmItemIndex(
                dataset,
                _typed_table(source),
                batch_size=batch_size,
            )
            for dataset, source in sources.items()
            if plan.datasets[dataset].is_long_form_odm
        }

    def context(
        self,
        source_rows: Mapping[str, Mapping[str, object]],
        output_values: Mapping[str, object] | None = None,
    ) -> RuntimeContext:
        """Create a row-local resolver over shared immutable indexes."""
        return RuntimeContext(self, source_rows, output_values or {})


class RuntimeContext:
    """R002 resolver for one constructed row and its completed outputs."""

    def __init__(
        self,
        index: BindingIndex,
        source_rows: Mapping[str, Mapping[str, object]],
        output_values: Mapping[str, object],
    ) -> None:
        self._index = index
        self._source_rows = {dataset: dict(row) for dataset, row in source_rows.items()}
        self._output_values = dict(output_values)

    def resolve(self, variable: str) -> Resolution:
        return self._resolve(variable, multiple_matches=None)

    def resolve_with_multiple_matches(
        self,
        variable: str,
        multiple_matches: Mapping[str, object],
    ) -> Resolution:
        return self._resolve(variable, multiple_matches=multiple_matches)

    def _resolve(
        self,
        variable: str,
        *,
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
        if bound.kind == "dataset":
            assert bound.field is not None
            if bound.field not in row:
                return _failure("validation", "unknown_field", {"identifier": variable})
            return ResolvedValue(value=runtime_value(row[bound.field]))

        assert bound.item_oid is not None
        return self._index._odm[bound.dataset].resolve(
            bound.item_oid,
            row,
            multiple_matches=multiple_matches,
        )
