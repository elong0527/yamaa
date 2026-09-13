"""Row-local resolution and the grouped row construction R001 defines.

One constructed row reaches four things: the driver record or group R001
gives it, the output values completed so far, the relations R003 joins to it,
and the records R013 reduces for it. This module is where those meet, so the
expression layer keeps resolving one name at a time and the join engine keeps
knowing nothing about rows.
"""

from __future__ import annotations

from collections.abc import Mapping, Sequence
from dataclasses import dataclass, field
from typing import Any

from pydantic import JsonValue

from yamaa.expressions import (
    AggregateError,
    FailedResolution,
    MappingResolver,
    PredicateAst,
    PredicateError,
    PredicateValue,
    Resolution,
    ResolvedValue,
    TruthValue,
    aggregate_identifiers,
    aggregate_star_datasets,
    evaluate_aggregate,
    evaluate_predicate,
    parse_aggregate_cached,
    parse_predicate_cached,
)
from yamaa.models import (
    MISSING,
    ConditionPhase,
    ConditionResult,
    EvaluationResult,
    RuntimeCondition,
    RuntimeValue,
)
from yamaa.odm import BindingIndex
from yamaa.planning import ExecutionDiagnostic, PlannedRow
from yamaa.runtime.joins import (
    IndexedRecord,
    RelationIndex,
    applicable_keys,
    compare_values,
    eligible_records,
    evaluate_mapping_from,
    join_scalar,
    json_value,
    partition_records,
    resolution_result,
)
from yamaa.runtime.lifecycle import LifecycleCondition
from yamaa.runtime.lookups import LookupOutcome, RecordLookupSelector


@dataclass(slots=True)
class CandidateRow:
    """One constructed row, with whatever R001 gave it to be constructed from."""

    source_rows: dict[str, dict[str, object]]
    values: dict[str, object]
    row_id: str | None = None
    group_driver: str | None = None
    group_records: tuple[IndexedRecord, ...] = ()
    group_values: dict[str, RuntimeValue] = field(default_factory=dict)
    lookups: dict[str, LookupOutcome] = field(default_factory=dict)

    @property
    def reported_group(self) -> dict[str, JsonValue]:
        """Return the group a failure names, keyed by its bare column names."""
        return {
            name.split(".", 1)[-1]: json_value(value)
            for name, value in self.group_values.items()
        }


@dataclass(slots=True)
class RelationalContext:
    """The relations, lookups, and constructed rows one run shares."""

    bindings: BindingIndex
    relations: dict[str, RelationIndex]
    lookups: RecordLookupSelector
    output_keys: tuple[str, ...]
    rows: list[CandidateRow] = field(default_factory=list)
    _partitions: dict[tuple[str, ...], dict[tuple[object, ...], list[CandidateRow]]] = (
        field(default_factory=dict)
    )

    def partition(
        self, fields: tuple[str, ...]
    ) -> dict[tuple[object, ...], list[CandidateRow]]:
        """Group the constructed rows by these columns, once per grain.

        R007-9 broadcasts an output-row reduction back to each row of its
        partition, so every row of one partition asks the same question. The
        columns a partition is taken on are complete before the reduction
        reads them and never change afterwards, so one grouping answers all
        of them.
        """
        grouped = self._partitions.get(fields)
        if grouped is None:
            grouped = {}
            for row in self.rows:
                key = tuple(row.values.get(name, MISSING) for name in fields)
                grouped.setdefault(key, []).append(row)
            self._partitions[fields] = grouped
        return grouped


def driver_groups(
    relation: RelationIndex,
    fields: Sequence[str],
) -> list[tuple[tuple[RuntimeValue, ...], tuple[IndexedRecord, ...]]]:
    """Partition a driver relation into the candidates a grouped template makes.

    R001-7 partitions the complete driver relation by the equality each
    value's type owns, with missing equal to missing, and R001-8 orders the
    groups by the position of their first record and keeps driver order
    inside each one.
    """
    return list(partition_records(relation.records, fields).items())


def _failed(condition: RuntimeCondition) -> FailedResolution:
    return FailedResolution(condition=condition)


def _condition(
    condition: str,
    context: Mapping[str, JsonValue],
    *,
    phase: ConditionPhase = "validation",
    requirement: str | None = None,
) -> RuntimeCondition:
    return RuntimeCondition(
        phase=phase,
        condition=condition,
        context=dict(context),
        requirement=requirement,
    )


def _invalid(operation: str, reason: str) -> ConditionResult:
    return ConditionResult(
        condition=_condition(
            "invalid_field_type",
            {"operation": operation, "reason": reason},
            requirement="R007-36",
        )
    )


class RowResolver:
    """Resolve every name one derivation of one constructed row can read."""

    def __init__(
        self,
        context: RelationalContext,
        candidate: CandidateRow,
        values: Mapping[str, object],
        *,
        row_phase: bool = False,
    ) -> None:
        self._context = context
        self._candidate = candidate
        self._values: dict[str, Any] = dict(values)
        self._row_phase = row_phase
        self._base = context.bindings.context(candidate.source_rows, self._values)

    @property
    def _phase(self) -> ConditionPhase:
        return "row_construction" if self._row_phase else "derivation"

    def resolve(self, variable: str) -> Resolution:
        qualifier = variable.split(".", 1)[0] if "." in variable else None
        if qualifier is None:
            return self._base.resolve(variable)
        if self._context.lookups.declares(qualifier):
            return self._record_lookup(qualifier, variable.split(".", 1)[1])
        if self._joins(qualifier):
            return self._join(qualifier, variable.split(".", 1)[1], None)
        return self._base.resolve(variable)

    def resolve_with_multiple_matches(
        self,
        variable: str,
        multiple_matches: Mapping[str, object],
    ) -> Resolution:
        qualifier = variable.split(".", 1)[0] if "." in variable else None
        if qualifier is not None and self._joins(qualifier):
            return self._join(qualifier, variable.split(".", 1)[1], multiple_matches)
        if qualifier is not None and self._context.lookups.declares(qualifier):
            # R015 already chose the record; the source reads a column of it.
            return self._record_lookup(qualifier, variable.split(".", 1)[1])
        return self._base.resolve_with_multiple_matches(variable, multiple_matches)

    def _joins(self, qualifier: str) -> bool:
        """Return whether reaching this relation needs the R003 join.

        R003-18 lets the qualifier equal the current row driver, and a scalar
        source then reads the driver record the row was constructed from
        rather than joining back to its relation.
        """
        return (
            qualifier in self._context.relations
            and qualifier not in self._candidate.source_rows
        )

    def _join(
        self,
        dataset: str,
        field_name: str,
        multiple_matches: Mapping[str, object] | None,
    ) -> Resolution:
        relation = self._context.relations[dataset]
        keys = applicable_keys(self._context.output_keys, relation)
        if not keys:
            # R003-7 and R003-33: without an applicable key the join has no
            # stated identity to match on.
            return _failed(
                _condition(
                    "no_applicable_keys",
                    {
                        "dataset": dataset,
                        "keys": list(self._context.output_keys),
                    },
                    requirement="R003-33",
                )
            )
        unavailable = [key for key in keys if key not in self._values]
        if unavailable:
            # R003-34: the left key must already be complete, because a join
            # cannot match on a value this row has not derived yet.
            return _failed(
                _condition(
                    "key_unavailable",
                    {"dataset": dataset, "keys": unavailable},
                    requirement="R003-34",
                )
            )
        return join_scalar(
            relation,
            keys,
            [self._values[key] for key in keys],
            field_name,
            multiple_matches=multiple_matches,
        )

    def _record_lookup(self, identifier: str, field_name: str) -> Resolution:
        outcome = self._candidate.lookups.get(identifier)
        if outcome is None:
            outcome = self._context.lookups.select(identifier, self._values)
            self._candidate.lookups[identifier] = outcome
        if outcome.condition is not None:
            assert outcome.spec_path is not None
            raise LifecycleCondition(
                ExecutionDiagnostic(
                    phase=outcome.condition.condition.phase,
                    condition=outcome.condition.condition.condition,
                    spec_paths=(outcome.spec_path,),
                    requirement=outcome.condition.condition.requirement,
                    context=outcome.condition.condition.context,
                )
            )
        plan = self._context.lookups.plans[identifier]
        relation = self._context.relations[plan.dataset]
        if not relation.has(field_name):
            return _failed(
                _condition(
                    "unknown_field",
                    {"identifier": f"{identifier}.{field_name}"},
                    requirement="R015-31",
                )
            )
        if outcome.record is None:
            # R015-18 and R015-22: an absent record is missing everywhere it
            # is read, and stays distinct from a record whose value is blank.
            return ResolvedValue(value=MISSING)
        return ResolvedValue(value=outcome.record.values[field_name])

    def resolve_relation(
        self,
        operation: str,
        payload: Mapping[str, object],
    ) -> EvaluationResult:
        """Answer the operations that read a relation rather than one value."""
        if operation == "mapping_from":
            return self._mapping_from(payload)
        return self._aggregate(payload)

    def _mapping_from(self, payload: Mapping[str, object]) -> EvaluationResult:
        dataset = payload.get("dataset")
        if not isinstance(dataset, str) or dataset not in self._context.relations:
            return _invalid("mapping_from", "an undeclared dataset")
        names = _names(payload.get("source"))
        values: dict[str, RuntimeValue] = {}
        for name in names:
            resolved = self.resolve(name)
            if not isinstance(resolved, ResolvedValue):
                return resolution_result(resolved)
            values[name] = resolved.value  # type: ignore[assignment]
        return evaluate_mapping_from(payload, self._context.relations[dataset], values)

    def _aggregate(self, payload: Mapping[str, object]) -> EvaluationResult:
        expr = payload.get("expr")
        if not isinstance(expr, str):
            return _invalid("aggregate", "a reducer expression")
        try:
            ast = parse_aggregate_cached(expr)
        except AggregateError as error:
            return ConditionResult(
                condition=_condition(
                    error.condition,
                    {"expr": expr, **error.context},
                    requirement=error.requirement,
                )
            )
        predicate = self._predicate(payload.get("filter"))
        if isinstance(predicate, ConditionResult):
            return predicate

        identifiers = aggregate_identifiers(ast)
        named = {name.split(".", 1)[0] for name in identifiers if "." in name}
        named |= set(aggregate_star_datasets(ast))
        relation_name = next(iter(sorted(named)), None)
        group_by = _names(payload.get("group_by"))

        if relation_name is None:
            selected = self._output_rows(identifiers, group_by, predicate)
        elif self._row_phase and relation_name == self._candidate.group_driver:
            selected = self._driver_group(relation_name, identifiers, predicate)
        else:
            selected = self._right_side(
                relation_name, identifiers, group_by, predicate, payload.get("between")
            )
        if isinstance(selected, ConditionResult):
            return selected
        records, grouped = selected
        return evaluate_aggregate(
            ast,
            expr,
            records,
            grouped,
            phase=self._phase,
            context={
                "row": self._candidate.row_id,
                "group": self._candidate.reported_group,
            }
            if self._candidate.group_driver is not None
            else {},
        )

    def _predicate(self, declared: object) -> PredicateAst | None | ConditionResult:
        if declared is None:
            return None
        if not isinstance(declared, str):
            return _invalid("aggregate", "a filter that is not a predicate")
        try:
            return parse_predicate_cached(declared)
        except PredicateError as error:
            return ConditionResult(
                condition=_condition(
                    "invalid_predicate",
                    {"predicate": declared, "position": error.position},
                    requirement="R004-31",
                )
            )

    def _right_side(
        self,
        dataset: str,
        identifiers: Sequence[str],
        group_by: Sequence[str],
        predicate: PredicateAst | None,
        between: object,
    ) -> tuple[list[dict[str, object]], dict[str, object]] | ConditionResult:
        """Reduce the partition R003-17 selects for the current row."""
        relation = self._context.relations[dataset]
        if group_by:
            # R003-20: a coarser declared grain is what the join matches on.
            fields = tuple(name.split(".", 1)[-1] for name in group_by)
        else:
            fields = applicable_keys(self._context.output_keys, relation)
        if not fields:
            return ConditionResult(
                condition=_condition(
                    "no_applicable_keys",
                    {"dataset": dataset, "keys": list(self._context.output_keys)},
                    requirement="R003-33",
                )
            )
        unavailable = [name for name in fields if name not in self._values]
        if unavailable:
            return ConditionResult(
                condition=_condition(
                    "key_unavailable",
                    {"dataset": dataset, "keys": unavailable},
                    requirement="R003-34",
                )
            )
        matched = relation.matching(fields, [self._values[name] for name in fields])
        grouped: dict[str, object] = {
            name: matched[0].values[name.split(".", 1)[-1]] if matched else MISSING
            for name in group_by
        }
        eligible = eligible_records(matched, predicate, relation)
        if isinstance(eligible, ConditionResult):
            return eligible
        narrowed = self._between(eligible, relation, between)
        if isinstance(narrowed, ConditionResult):
            return narrowed
        return [_record_values(record, identifiers) for record in narrowed], grouped

    def _between(
        self,
        records: Sequence[IndexedRecord],
        relation: RelationIndex,
        between: object,
    ) -> list[IndexedRecord] | ConditionResult:
        """Narrow a right side by the closed range the current row reads."""
        if between is None:
            return list(records)
        if not isinstance(between, Mapping):
            return _invalid("aggregate", "a between that is not a declaration")
        name = between.get("value")
        if not isinstance(name, str):
            return _invalid("aggregate", "a between without a value")
        resolved = self.resolve(name)
        if not isinstance(resolved, ResolvedValue):
            return ConditionResult(
                condition=_condition("unknown_field", {"identifier": name})
            )
        value = resolved.value
        if value is MISSING:
            # R003-27 and R013-8: a missing cutoff admits no record rather
            # than silently reducing the unrestricted right side.
            return []
        bounds = [
            (side, str(between[side]).split(".", 1)[-1])
            for side in ("lower", "upper")
            if isinstance(between.get(side), str)
        ]
        kept: list[IndexedRecord] = []
        for record in records:
            admitted = True
            for side, bound_field in bounds:
                if not relation.has(bound_field):
                    return ConditionResult(
                        condition=_condition(
                            "unknown_field",
                            {"identifier": f"{relation.dataset}.{bound_field}"},
                            requirement="R013-44",
                        )
                    )
                bound = record.values[bound_field]
                if bound is MISSING:
                    admitted = False
                    break
                try:
                    order = compare_values(bound, value)  # type: ignore[arg-type]
                except TypeError:
                    return ConditionResult(
                        condition=_condition(
                            "incompatible_input_type",
                            {
                                "source": f"{relation.dataset}.{bound_field}",
                                "expected": "a comparable bound",
                            },
                            requirement="R013-44",
                        )
                    )
                # Every stated endpoint is inclusive under R003-26.
                if (side == "lower" and order > 0) or (side == "upper" and order < 0):
                    admitted = False
                    break
            if admitted:
                kept.append(record)
        return kept

    def _driver_group(
        self,
        dataset: str,
        identifiers: Sequence[str],
        predicate: PredicateAst | None,
    ) -> tuple[list[dict[str, object]], dict[str, object]] | ConditionResult:
        """Reduce the records of the current driver group (R007-10)."""
        relation = self._context.relations[dataset]
        eligible = eligible_records(self._candidate.group_records, predicate, relation)
        if isinstance(eligible, ConditionResult):
            return eligible
        grouped = dict(self._candidate.group_values)
        return [_record_values(record, identifiers) for record in eligible], grouped

    def _output_rows(
        self,
        identifiers: Sequence[str],
        group_by: Sequence[str],
        predicate: PredicateAst | None,
    ) -> tuple[list[dict[str, object]], dict[str, object]] | ConditionResult:
        """Reduce the constructed output rows of this row's partition (R007-9)."""
        fields = tuple(group_by)
        unavailable = [name for name in fields if name not in self._values]
        if unavailable:
            return ConditionResult(
                condition=_condition("unknown_field", {"identifier": unavailable[0]})
            )
        key = tuple(self._values[name] for name in fields)
        records: list[dict[str, object]] = []
        for row in self._context.partition(fields).get(key, ()):
            if predicate is not None:
                result = evaluate_predicate(predicate, MappingResolver(row.values))
                if isinstance(result, ConditionResult):
                    return result
                assert isinstance(result, PredicateValue)
                if result.value is not TruthValue.TRUE:
                    continue
            records.append(
                {name: row.values.get(name, MISSING) for name in identifiers}
            )
        return records, {name: self._values[name] for name in fields}


def _record_values(
    record: IndexedRecord,
    identifiers: Sequence[str],
) -> dict[str, object]:
    """Map each written identifier to this record's value for it."""
    return {
        name: record.values.get(name.split(".", 1)[-1], MISSING) for name in identifiers
    }


def _names(value: object) -> tuple[str, ...]:
    if isinstance(value, str):
        return (value,)
    if isinstance(value, Sequence):
        return tuple(item for item in value if isinstance(item, str))
    return ()


def group_candidates(
    planned: PlannedRow,
    relation: RelationIndex,
) -> list[CandidateRow]:
    """Build one empty candidate per driver group, in first-occurrence order."""
    fields = planned.group_fields
    candidates: list[CandidateRow] = []
    for key, records in driver_groups(relation, fields):
        values = dict(zip(planned.group_variables, key, strict=True))
        candidates.append(
            CandidateRow(
                # R001-15: only the grouped variables are scalars of the
                # candidate; every other driver field varies within the group
                # and is read through an aggregate or not at all.
                source_rows={
                    planned.driver: dict(zip(fields, key, strict=True)),
                },
                values={},
                row_id=planned.declaration.id if planned.declaration else None,
                group_driver=planned.driver,
                group_records=records,
                group_values=values,
            )
        )
    return candidates
