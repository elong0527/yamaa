"""R003 keyed joins, R007 declared-key lookups, and the helpers they share.

One relation is read once into ordered typed records, and every operation
that reaches those records -- an implicit left join, a `mapping_from`
lookup, an R013 reduction, an R015 record lookup, and the grouped row
construction R001 defines -- selects from that one reading. Partitioning and
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
    handler_value,
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
    normalize_runtime_value,
    runtime_type_name,
    values_comparable,
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


def _ordering_key(value: RuntimeValue) -> object:
    if isinstance(value, (DateValue, DateTimeValue)):
        return value.ordering_key
    return value


def compare_values(left: RuntimeValue, right: RuntimeValue) -> int:
    """Compare two non-missing values in the order their type owns.

    R007-17 gives numeric order to R010, text order to R019, and
    chronological order to R016, so one comparison serves every ordered
    operation rather than each reimplementing its type's order.
    """
    if not values_comparable(left, right):
        raise TypeError(
            f"incomparable values {runtime_type_name(left)!r} "
            f"and {runtime_type_name(right)!r}"
        )
    ordered_left = _ordering_key(left)
    ordered_right = _ordering_key(right)
    if ordered_left < ordered_right:  # type: ignore[operator]
        return -1
    if ordered_left > ordered_right:  # type: ignore[operator]
        return 1
    return 0


def partition_key(
    values: Mapping[str, RuntimeValue],
    fields: Sequence[str],
) -> PartitionKey:
    """Return the grouping key R001-7 and R007-6 partition on.

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

    R001-8 orders groups by the position of their first record and keeps
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
    R007-15 keeps `nulls` from flipping with `direction`. R007-16 makes the
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

        R003-13 keeps a right record with a missing key out of every match,
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

    R003-3 defines the applicable keys and R003-6 fixes their order, so the
    join a reviewer reads in `keys` is the join that runs.
    """
    return tuple(key for key in output_keys if relation.has(key))


class RecordResolver:
    """Expose exactly one right-side record to a predicate.

    R003-22 makes a right-side `filter` a predicate over right-side records
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
    filter: str | None = None


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
            requirement="R007-39",
        )
    return ordered[0] if keep == "first" else ordered[-1]


def join_scalar(
    relation: RelationIndex,
    keys: Sequence[str],
    key_values: Sequence[RuntimeValue],
    field: str,
    *,
    multiple_matches: Mapping[str, object] | None = None,
) -> Resolution:
    """Read one right-side field for the current row under R003's join.

    The join is many-to-one: it copies a value onto matched rows, answers
    missing where nothing matched, and refuses to choose among several
    matches unless the specification declared how.
    """
    if not relation.has(field):
        return _failed(
            "validation",
            "unknown_field",
            {"identifier": f"{relation.dataset}.{field}"},
        )
    matches = relation.matching(keys, key_values)
    if multiple_matches is None:
        if not matches:
            # R003-11 and R003-36: an absent right-side record is missing.
            return ResolvedValue(value=MISSING)
        if len(matches) == 1:
            return ResolvedValue(value=matches[0].values[field])
        # R003-8 and R003-30: right-side uniqueness holds unless R008 relaxes
        # it, because choosing by file order is not a study rule.
        return FailedResolution(
            condition=RuntimeCondition(
                phase="join",
                condition="multiple_matches",
                context={
                    "dataset": relation.dataset,
                    "match_count": len(matches),
                },
                requirement="R003-35",
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
    predicate = _parsed(selection.filter)
    if isinstance(predicate, ConditionResult):
        return FailedResolution(condition=predicate.condition)
    eligible = eligible_records(matches, predicate, relation)
    if isinstance(eligible, ConditionResult):
        return FailedResolution(condition=eligible.condition)
    if not eligible:
        # R008-14: filtering to no surviving record is an ordinary absent
        # match under R003 rather than a handled condition.
        return ResolvedValue(value=MISSING)
    if len(eligible) == 1:
        # R008-15: the handler counts only the rows where it had to choose.
        return ResolvedValue(value=eligible[0].values[field])
    chosen = select_record(eligible, terms, selection.keep)
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
            requirement="R004-31",
        )


def evaluate_mapping_from(
    payload: Mapping[str, object],
    relation: RelationIndex,
    resolver_values: Mapping[str, RuntimeValue],
) -> EvaluationResult:
    """Look one value up by the key pairs the specification declares.

    R003-14 and R003-15 make this the same equality left join as an implicit
    qualified source, differing only in where the keys come from: declared
    pairs rather than output keys, so it reaches a right side keyed on
    something else.
    """
    sources = _string_list(payload.get("source"))
    keys = _string_list(payload.get("key"))
    value_field = payload.get("value")
    if sources is None or keys is None or not isinstance(value_field, str):
        return _condition(
            "validation",
            "invalid_field_type",
            {"operation": "mapping_from", "expected": "source, key, and value"},
            requirement="R007-36",
        )
    if len(sources) != len(keys):
        return _condition(
            "validation",
            "source_key_length_mismatch",
            {
                "source": list(sources),
                "key": list(keys),
                "source_count": len(sources),
                "key_count": len(keys),
            },
            requirement="R007-48",
        )
    for field in (*keys, value_field):
        if not relation.has(field):
            return _condition(
                "validation",
                "unknown_field",
                {"identifier": f"{relation.dataset}.{field}"},
            )

    values: list[RuntimeValue] = []
    for name in sources:
        if name not in resolver_values:
            return _condition("validation", "unknown_field", {"identifier": name})
        values.append(resolver_values[name])

    missing = [
        name for name, value in zip(sources, values, strict=True) if value is MISSING
    ]
    if missing:
        # R008-9: with several inputs, `missing` fires when any one of them
        # is missing, so an incomplete key never reaches `unmapped`.
        if "missing" in payload:
            return handler_value(payload, "missing")
        return _condition(
            "mapping",
            "missing_input",
            {
                "dataset": relation.dataset,
                "source": list(sources),
                "missing_source": missing[0],
            },
            requirement="R007-49",
            applicable_handler="missing",
        )

    lookup_key = {
        field: json_value(value) for field, value in zip(keys, values, strict=True)
    }
    matches = relation.matching(keys, values)
    if len(matches) > 1:
        # R007-21's pairing identifies one record; two make the answer depend
        # on file order rather than on the study's reference data.
        return _condition(
            "mapping",
            "duplicate_lookup_key",
            {
                "dataset": relation.dataset,
                "key": list(keys),
                "lookup_key": lookup_key,
                "match_count": len(matches),
            },
            requirement="R007-37",
        )
    if not matches:
        if "unmapped" in payload:
            return handler_value(payload, "unmapped")
        return _condition(
            "mapping",
            "unmapped_key",
            {
                "dataset": relation.dataset,
                "key": list(keys),
                "lookup_key": lookup_key,
            },
            requirement="R007-49",
            applicable_handler="unmapped",
        )
    return normalize_runtime_value(matches[0].values[value_field])


def _string_list(value: object) -> tuple[str, ...] | None:
    if isinstance(value, str):
        return (value,)
    if isinstance(value, Sequence) and all(isinstance(item, str) for item in value):
        return tuple(value)  # type: ignore[arg-type]
    return None


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
