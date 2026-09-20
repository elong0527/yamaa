"""R003 keyed joins and the helpers they share.

One relation is read once into ordered typed records, and every operation
that reaches those records -- a named or inline `lookup`, an R013
reduction, and the grouped row construction R001 defines -- selects from
that one reading. Partitioning and
ordering live here rather than beside each caller so that two operations
cannot disagree about which records a key reaches or which record an order
puts first.
"""

from __future__ import annotations

from collections.abc import Iterable, Mapping, Sequence
from dataclasses import dataclass
from functools import cmp_to_key
from typing import Literal, TypeAlias

from pydantic import BaseModel, ConfigDict, Field, JsonValue, ValidationError

from yamaa.expressions import (
    AbsentValue,
    FailedResolution,
    PredicateAst,
    PredicateError,
    PredicateValue,
    Resolution,
    ResolvedValue,
    TruthValue,
    evaluate_predicate,
    parse_predicate_cached,
)
from yamaa.io.polars import runtime_rows
from yamaa.io.source import LoadedDataset
from yamaa.models import (
    MISSING,
    ColumnType,
    ConditionPhase,
    ConditionResult,
    DateTimeValue,
    DateValue,
    EvaluationResult,
    HandlerName,
    RuntimeCondition,
    RuntimeValue,
    TypedTable,
    ValueResult,
    compare_values,
    normalize_runtime_value,
    runtime_type_name,
)
from yamaa.specification.models import OrderTerm

PartitionKey: TypeAlias = tuple[RuntimeValue, ...]
SourceTable: TypeAlias = LoadedDataset | TypedTable


@dataclass(frozen=True, slots=True)
class IndexedRecord:
    """One relation record with the position that fixes its record order."""

    position: int
    values: Mapping[str, RuntimeValue]


class OrderError(ValueError):
    """One order term compares values whose types are not mutually comparable."""

    def __init__(self, variable: str, types: Sequence[str]) -> None:
        self.variable = variable
        self.types = tuple(types)
        super().__init__(f"order term {variable!r} compares {sorted(set(types))}")


def partition_key(
    values: Mapping[str, RuntimeValue],
    fields: Sequence[str],
) -> PartitionKey:
    """Return the grouping key REQ-0037 and REQ-0293 partition on.

    Missing equals missing for grouping, so a record whose grouping value was
    never collected joins the other records that share that absence rather
    than forming a partition of its own.
    """
    return tuple(values[field] for field in fields)


def partition_records(
    records: Iterable[IndexedRecord],
    fields: Sequence[str],
) -> dict[PartitionKey, tuple[IndexedRecord, ...]]:
    """Partition records by equality on `fields`, in first-occurrence order.

    REQ-0038 orders groups by the position of their first record and keeps
    driver order inside a group, which is what an insertion-ordered mapping
    of appended records is.
    """
    grouped: dict[PartitionKey, list[IndexedRecord]] = {}
    for record in records:
        grouped.setdefault(partition_key(record.values, fields), []).append(record)
    return {key: tuple(members) for key, members in grouped.items()}


def order_records(
    records: Sequence[IndexedRecord],
    terms: Sequence[tuple[OrderTerm, str]],
) -> list[IndexedRecord]:
    """Order records by R007's terms, breaking every remaining tie by position.

    Each term carries its own direction and its own missing placement, and
    REQ-0300 keeps `nulls` from flipping with `direction`. REQ-0301 makes the
    result total by falling back to record order, so no ordered selection
    has an undefined case.
    """

    def compare(left: IndexedRecord, right: IndexedRecord) -> int:
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
                try:
                    result = compare_values(left_value, right_value)
                except TypeError as error:
                    raise OrderError(
                        term.variable,
                        [
                            name
                            for name in (
                                runtime_type_name(left_value),
                                runtime_type_name(right_value),
                            )
                            if name is not None
                        ],
                    ) from error
                if term.direction == "desc":
                    result = -result
            if result:
                return result
        return left.position - right.position

    return sorted(records, key=cmp_to_key(compare))


class RelationIndex:
    """One typed source relation read once as ordered runtime records."""

    def __init__(self, dataset: str, table: TypedTable) -> None:
        self.dataset = dataset
        self.types: dict[str, ColumnType] = {
            column.name: column.type for column in table.columns
        }
        self.records: tuple[IndexedRecord, ...] = tuple(
            IndexedRecord(position=position, values=values)
            for position, values in enumerate(runtime_rows(table))
        )
        self._matched: dict[
            tuple[str, ...], dict[PartitionKey, tuple[IndexedRecord, ...]]
        ] = {}

    @property
    def fields(self) -> tuple[str, ...]:
        return tuple(self.types)

    def has(self, field: str) -> bool:
        return field in self.types

    def matching(
        self,
        fields: Sequence[str],
        values: Sequence[RuntimeValue],
    ) -> tuple[IndexedRecord, ...]:
        """Return the records equal on every field, in record order.

        REQ-0123 keeps a right record with a missing key out of every match,
        and a left row carrying a missing key reaches nothing for the same
        reason: an uncollected identifier is not an identity two rows share.
        """
        identity = tuple(fields)
        index = self._matched.get(identity)
        if index is None:
            index = partition_records(
                (
                    record
                    for record in self.records
                    if not any(record.values[field] is MISSING for field in identity)
                ),
                identity,
            )
            self._matched[identity] = index
        if any(value is MISSING for value in values):
            return ()
        return index.get(tuple(values), ())


def build_relation_indexes(
    sources: Mapping[str, SourceTable],
) -> dict[str, RelationIndex]:
    """Read every declared source once into its ordered typed records."""
    return {
        dataset: RelationIndex(
            dataset,
            source.table if isinstance(source, LoadedDataset) else source,
        )
        for dataset, source in sources.items()
    }


def applicable_keys(
    output_keys: Sequence[str],
    relation: RelationIndex,
) -> tuple[str, ...]:
    """Return the output keys the right side also carries, in `keys` order.

    REQ-0113 defines the applicable keys and REQ-0116 fixes their order, so the
    join a reviewer reads in `keys` is the join that runs.
    """
    return tuple(key for key in output_keys if relation.has(key))


class RecordResolver:
    """Expose exactly one right-side record to a predicate.

    REQ-0132 makes a right-side `filter` a predicate over right-side records
    only, so a resolver that could also reach the current row would let one
    silently correlate.
    """

    def __init__(self, dataset: str, fields: Sequence[str], record: IndexedRecord):
        self._dataset = dataset
        self._fields = frozenset(fields)
        self._record = record

    def resolve(self, variable: str) -> Resolution:
        if "." not in variable:
            return _failed("validation", "unknown_field", {"identifier": variable})
        qualifier, field = variable.split(".", 1)
        if qualifier != self._dataset or field not in self._fields:
            return _failed("validation", "unknown_field", {"identifier": variable})
        return ResolvedValue(value=self._record.values[field])


def _condition(
    phase: ConditionPhase,
    condition: str,
    context: Mapping[str, JsonValue],
    *,
    requirement: str | None = None,
    applicable_handler: HandlerName | None = None,
) -> ConditionResult:
    return ConditionResult(
        condition=RuntimeCondition(
            phase=phase,
            condition=condition,
            context=dict(context),
            requirement=requirement,
            applicable_handler=applicable_handler,
        )
    )


def _failed(
    phase: ConditionPhase,
    condition: str,
    context: Mapping[str, JsonValue],
) -> FailedResolution:
    return FailedResolution(
        condition=RuntimeCondition(
            phase=phase, condition=condition, context=dict(context)
        )
    )


def json_value(value: object) -> JsonValue:
    """Render one runtime value for a structured diagnostic context."""
    if value is MISSING or value is None:
        return None
    if isinstance(value, (DateValue, DateTimeValue)):
        return value.to_text()
    if isinstance(value, (str, int, float, bool)):
        return value
    return str(value)


def eligible_records(
    records: Sequence[IndexedRecord],
    predicate: PredicateAst | None,
    relation: RelationIndex,
) -> list[IndexedRecord] | ConditionResult:
    """Keep the records a right-side predicate answers TRUE for."""
    if predicate is None:
        return list(records)
    kept: list[IndexedRecord] = []
    for record in records:
        result = evaluate_predicate(
            predicate,
            RecordResolver(relation.dataset, relation.fields, record),
        )
        if isinstance(result, ConditionResult):
            return result
        assert isinstance(result, PredicateValue)
        if result.value is TruthValue.TRUE:
            kept.append(record)
    return kept


class MultipleMatchSelection(BaseModel):
    """The normalized R008 policy for choosing one of several matches."""

    model_config = ConfigDict(strict=True, extra="forbid", frozen=True)

    order_by: list[OrderTerm] = Field(min_length=1)
    keep: Literal["first", "last"]


def resolve_order_terms(
    terms: Sequence[OrderTerm],
    relation: RelationIndex,
) -> list[tuple[OrderTerm, str]] | ConditionResult:
    """Bind each order term to a field of the relation it orders."""
    resolved: list[tuple[OrderTerm, str]] = []
    for term in terms:
        qualifier, _, field = term.variable.partition(".")
        if qualifier != relation.dataset or not relation.has(field):
            return _condition(
                "validation",
                "unknown_field",
                {"identifier": term.variable},
            )
        resolved.append((term, field))
    return resolved


def select_record(
    records: Sequence[IndexedRecord],
    terms: Sequence[tuple[OrderTerm, str]],
    keep: Literal["first", "last"],
) -> IndexedRecord | ConditionResult:
    """Order the eligible records and retain the one `keep` names."""
    try:
        ordered = order_records(records, terms)
    except OrderError as error:
        return _condition(
            "validation",
            "incompatible_input_type",
            {"source": error.variable, "types": sorted(set(error.types))},
            requirement="REQ-0324",
        )
    return ordered[0] if keep == "first" else ordered[-1]


def join_scalar(
    relation: RelationIndex,
    keys: Sequence[str],
    key_values: Sequence[RuntimeValue],
    field: str,
    *,
    selector: str | None = None,
    multiple_matches: Mapping[str, object] | None = None,
) -> Resolution:
    """Read one right-side field for the current row under R003's join.

    The join is many-to-one: it copies a value onto matched rows, answers
    missing where nothing matched, and refuses to choose among several
    matches unless the specification declared how. REQ-0131 lets the source
    say which of the matched records it may read at all.
    """
    if not relation.has(field):
        return _failed(
            "validation",
            "unknown_field",
            {"identifier": f"{relation.dataset}.{field}"},
        )
    matches = relation.matching(keys, key_values)
    if selector is not None:
        predicate = _parsed(selector)
        if isinstance(predicate, ConditionResult):
            return FailedResolution(condition=predicate.condition)
        selected = eligible_records(matches, predicate, relation)
        if isinstance(selected, ConditionResult):
            return FailedResolution(condition=selected.condition)
        matches = selected
    if multiple_matches is None:
        if not matches:
            # REQ-0121 and REQ-0146: an absent right-side record is missing.
            return ResolvedValue(value=MISSING)
        if len(matches) == 1:
            return ResolvedValue(value=matches[0].values[field])
        # REQ-0118 and REQ-0140: right-side uniqueness holds unless the Local
        # handlers contract relaxes it, because choosing by file order is not
        # a study rule.
        return FailedResolution(
            condition=RuntimeCondition(
                phase="join",
                condition="multiple_matches",
                context={
                    "dataset": relation.dataset,
                    "match_count": len(matches),
                },
                requirement="REQ-0145",
                applicable_handler="multiple_matches",
            )
        )

    try:
        selection = MultipleMatchSelection.model_validate(
            dict(multiple_matches), strict=True
        )
    except ValidationError:
        return _failed(
            "validation",
            "invalid_field_type",
            {"field": "multiple_matches"},
        )
    terms = resolve_order_terms(selection.order_by, relation)
    if isinstance(terms, ConditionResult):
        return FailedResolution(condition=terms.condition)
    if not matches:
        # REQ-0355: filtering to no surviving record is an ordinary absent
        # match under R003 rather than a handled condition.
        return ResolvedValue(value=MISSING)
    if len(matches) == 1:
        # REQ-0356: the handler counts only the rows where it had to choose.
        return ResolvedValue(value=matches[0].values[field])
    chosen = select_record(matches, terms, selection.keep)
    if isinstance(chosen, ConditionResult):
        return FailedResolution(condition=chosen.condition)
    return ResolvedValue(value=chosen.values[field], handled_by="multiple_matches")


def _parsed(predicate: str | None) -> PredicateAst | None | ConditionResult:
    if predicate is None:
        return None
    try:
        return parse_predicate_cached(predicate)
    except PredicateError as error:
        return _condition(
            "validation",
            "invalid_predicate",
            {"predicate": predicate, "position": error.position},
            requirement="REQ-0188",
        )


def resolution_result(resolution: Resolution) -> EvaluationResult:
    """Return one resolution as the evaluation result an expression yields."""
    if isinstance(resolution, ResolvedValue):
        normalized = normalize_runtime_value(resolution.value)
        if isinstance(normalized, ValueResult) and resolution.handled_by is not None:
            return ValueResult(value=normalized.value, handled_by=resolution.handled_by)
        return normalized
    if isinstance(resolution, FailedResolution):
        return ConditionResult(condition=resolution.condition)
    assert isinstance(resolution, AbsentValue)
    return _condition(
        "validation", "unknown_field", {"identifier": resolution.variable}
    )
