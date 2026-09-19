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
from yamaa.expressions.windows import (
    WINDOW_OPERATIONS,
    Partition,
    evaluate_window,
    window_spec,
)
from yamaa.models import (
    MISSING,
    ConditionPhase,
    ConditionResult,
    EvaluationResult,
    RuntimeCondition,
    RuntimeValue,
    ValueResult,
)
from yamaa.odm import BindingIndex
from yamaa.planning import (
    ExecutionDiagnostic,
    ImplicitJoin,
    PlannedIntermediate,
    PlannedRow,
)
from yamaa.runtime.intermediates import (
    IntermediateOutcome,
    IntermediateSelector,
    absent_value,
    evaluate_intermediate,
)
from yamaa.runtime.joins import (
    IndexedRecord,
    OrderError,
    RelationIndex,
    compare_values,
    eligible_records,
    json_value,
    order_records,
    partition_records,
)
from yamaa.runtime.lifecycle import LifecycleCondition
from yamaa.specification.models import OrderTerm


@dataclass(slots=True)
class CandidateRow:
    """One constructed row, with whatever R001 gave it to be constructed from."""

    source_rows: dict[str, dict[str, object]]
    values: dict[str, object]
    # Every driver record feeding this row under R001-12, which a direct
    # dataset read collects one value across.
    feeding_rows: dict[str, list[dict[str, object]]] = field(default_factory=dict)
    row_id: str | None = None
    # Where R001-9 appended this row, which is the order a window falls back
    # to when its terms tie.
    output_position: int = -1
    group_driver: str | None = None
    group_records: tuple[IndexedRecord, ...] = ()
    group_values: dict[str, RuntimeValue] = field(default_factory=dict)
    intermediates: dict[str, IntermediateOutcome] = field(default_factory=dict)

    @property
    def reported_group(self) -> dict[str, JsonValue]:
        """Return the group a failure names, keyed by its bare column names."""
        return {
            name.split(".", 1)[-1]: json_value(value)
            for name, value in self.group_values.items()
        }


@dataclass(slots=True)
class RelationalContext:
    """The relations, intermediates, and constructed rows one run shares."""

    bindings: BindingIndex
    relations: dict[str, RelationIndex]
    intermediates: IntermediateSelector
    output_keys: tuple[str, ...]
    rows: list[CandidateRow] = field(default_factory=list)
    _partitions: dict[tuple[str, ...], dict[tuple[object, ...], list[CandidateRow]]] = (
        field(default_factory=dict)
    )

    def partition(
        self, fields: tuple[str, ...]
    ) -> dict[tuple[object, ...], list[CandidateRow]]:
        """Group the constructed rows by these columns, once per key combination.

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
        column: str | None = None,
        implicit_joins: Sequence[ImplicitJoin] = (),
    ) -> None:
        self._context = context
        self._candidate = candidate
        self._values: dict[str, Any] = dict(values)
        self._row_phase = row_phase
        self._column = column
        self._implicit_joins = {join.dataset: join for join in implicit_joins}
        self._base = context.bindings.context(
            candidate.source_rows,
            self._values,
            feeding_rows=candidate.feeding_rows,
        )

    @property
    def _phase(self) -> ConditionPhase:
        return "row_construction" if self._row_phase else "derivation"

    def resolve(self, variable: str) -> Resolution:
        qualifier = variable.split(".", 1)[0] if "." in variable else None
        if qualifier is None:
            return self._base.resolve(variable)
        if self._context.intermediates.declares(qualifier):
            return self._lookup_read(qualifier, variable.split(".", 1)[1])
        implicit = self._implicit_joins.get(qualifier)
        if implicit is not None:
            # R003-40: a plain cross-dataset scalar source joins on the
            # applicable keys the planner inferred.
            return self._implicit_read(implicit, variable.split(".", 1)[1])
        # A row driver needs no join: the read is the current driver record.
        return self._base.resolve(variable)

    def resolve_selected(
        self,
        variable: str,
        *,
        selector: str | None,
        multiple_matches: Mapping[str, object] | None,
    ) -> Resolution:
        qualifier = variable.split(".", 1)[0] if "." in variable else None
        if qualifier is not None and self._context.intermediates.declares(qualifier):
            # R003 already chose the record; the source reads a column of it.
            return self._lookup_read(qualifier, variable.split(".", 1)[1])
        if qualifier is not None:
            implicit = self._implicit_joins.get(qualifier)
            if implicit is not None:
                # R003-40: a structured cross-dataset source joins on the
                # applicable keys, then selects among the matched records.
                return self._implicit_read(
                    implicit,
                    variable.split(".", 1)[1],
                    selector=selector,
                    multiple_matches=multiple_matches,
                )
        return self._base.resolve_selected(
            variable, selector=selector, multiple_matches=multiple_matches
        )

    def _implicit_read(
        self,
        join: ImplicitJoin,
        field_name: str,
        *,
        selector: str | None = None,
        multiple_matches: Mapping[str, object] | None = None,
    ) -> Resolution:
        relation = self._context.relations[join.dataset]
        if not relation.has(field_name):
            return _failed(
                _condition(
                    "unknown_field",
                    {"identifier": f"{join.dataset}.{field_name}"},
                    requirement="R003-15",
                )
            )
        payload: dict[str, object] = {
            "dataset": join.dataset,
            "key_base": list(join.keys),
            "key": list(join.keys),
            "value": field_name,
        }
        if selector is not None:
            payload["filter"] = selector
        if multiple_matches is not None:
            order_by = multiple_matches.get("order_by")
            if order_by is not None:
                payload["order_by"] = list(order_by)  # type: ignore[arg-type]
            keep = multiple_matches.get("keep")
            if keep is not None:
                payload["keep"] = keep
        result = evaluate_intermediate(payload, relation, self.resolve)
        if isinstance(result, ValueResult):
            return ResolvedValue(value=result.value, handled_by=result.handled_by)
        return FailedResolution(condition=result.condition)

    def _lookup_read(self, identifier: str, field_name: str) -> Resolution:
        plan = self._context.intermediates.plans[identifier]
        relation = self._context.relations[plan.dataset]
        if not relation.has(field_name):
            return _failed(
                _condition(
                    "unknown_field",
                    {"identifier": f"{identifier}.{field_name}"},
                    requirement="R003-15",
                )
            )
        outcome = self._candidate.intermediates.get(identifier)
        if outcome is None:
            outcome = self._context.intermediates.select(
                identifier, self._lookup_current(plan)
            )
            self._candidate.intermediates[identifier] = outcome
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
        if outcome.record is None:
            # R003-14: an intermediate that yields nothing answers its decided
            # absence, which stays distinct from a record whose value is
            # missing.
            return ResolvedValue(
                value=absent_value(outcome.absent), handled_by=outcome.handled_by
            )
        return ResolvedValue(
            value=outcome.record.values[field_name], handled_by=outcome.handled_by
        )

    def _lookup_current(self, plan: PlannedIntermediate) -> dict[str, RuntimeValue]:
        """Resolve this row's match values under their declared names."""
        names = list(plan.match_variables)
        if plan.between_value is not None:
            names.append(plan.between_value)
        current: dict[str, RuntimeValue] = {}
        for name in names:
            resolved = self.resolve(name)
            if isinstance(resolved, ResolvedValue):
                current[name] = resolved.value
            elif isinstance(resolved, FailedResolution):
                raise LifecycleCondition(
                    ExecutionDiagnostic(
                        phase=resolved.condition.phase,
                        condition=resolved.condition.condition,
                        spec_paths=(),
                        requirement=resolved.condition.requirement,
                        context=resolved.condition.context,
                    )
                )
            else:
                current[name] = MISSING
        return current

    def resolve_relation(
        self,
        operation: str,
        payload: Mapping[str, object],
    ) -> EvaluationResult:
        """Answer the operations that read a relation rather than one value."""
        if operation == "lookup":
            return self._inline_lookup(payload)
        if operation in WINDOW_OPERATIONS:
            return self._window(operation, payload)
        return self._aggregate(payload)

    def _window(
        self,
        operation: str,
        payload: Mapping[str, object],
    ) -> EvaluationResult:
        """Locate this row in its ordered partition and ask the window.

        R007-6 partitions the constructed output rows by the window's own
        `group_by` and preserves row count, and R007-41 keeps a window out of
        row construction, so the rows are always the completed ones.
        """
        if self._row_phase:
            return _invalid(operation, "a window has no row-construction context")
        located = self._locate(payload)
        if isinstance(located, ConditionResult):
            return located
        members, current, partition_keys = located
        eligible = self._eligible(payload, members)
        if isinstance(eligible, ConditionResult):
            return eligible
        result = evaluate_window(
            operation,
            payload,
            Partition(
                rows=tuple(values for _, values in members),
                current=current,
                eligible=eligible,
            ),
        )
        if isinstance(result, ConditionResult):
            # A window failure is a property of the partition rather than of
            # one row, so it names the partition it could not answer for.
            named: dict[str, JsonValue] = {}
            if self._column is not None:
                named["column"] = self._column
            named.update(result.condition.context)
            named.setdefault("keys", [partition_keys])
            return ConditionResult(
                condition=result.condition.model_copy(update={"context": named})
            )
        return result

    def _locate(
        self,
        payload: Mapping[str, object],
    ) -> (
        tuple[list[tuple[CandidateRow, dict[str, object]]], int, dict[str, JsonValue]]
        | ConditionResult
    ):
        """Return this row's partition in declared order, and its place in it."""
        window = window_spec(payload)
        fields = _names(window.get("group_by"))
        unavailable = [name for name in fields if name not in self._values]
        if unavailable:
            return ConditionResult(
                condition=_condition("unknown_field", {"identifier": unavailable[0]})
            )
        key = tuple(self._values[name] for name in fields)
        members = [
            (row, _readable(row))
            for row in self._context.partition(fields).get(key, ())
        ]
        terms = _order_terms(window.get("order_by"))
        if terms:
            indexed = [
                IndexedRecord(position=row.output_position, values=values)
                for row, values in members
            ]
            try:
                # R007-16 falls back to construction order, which is what the
                # shared ordering helper breaks a remaining tie by.
                ordered = order_records(indexed, terms)
            except OrderError as error:
                return ConditionResult(
                    condition=_condition(
                        "incompatible_input_type",
                        {"source": error.variable, "types": sorted(set(error.types))},
                        requirement="R007-39",
                    )
                )
            by_position = {
                row.output_position: entry for entry in members for row in (entry[0],)
            }
            members = [by_position[record.position] for record in ordered]
        partition_keys = {
            name: json_value(self._values[name])  # type: ignore[arg-type]
            for name in fields
        }
        for index, (row, _) in enumerate(members):
            if row is self._candidate:
                return members, index, partition_keys
        return ConditionResult(
            condition=_condition(
                "unknown_field",
                {"identifier": "the current row is absent from its partition"},
            )
        )

    def _eligible(
        self,
        payload: Mapping[str, object],
        members: Sequence[tuple[CandidateRow, dict[str, object]]],
    ) -> tuple[bool, ...] | ConditionResult:
        """Say which partition rows the window's filter retained (R007-7)."""
        predicate = self._predicate(window_spec(payload).get("filter"))
        if isinstance(predicate, ConditionResult):
            return predicate
        if predicate is None:
            return tuple(True for _ in members)
        kept: list[bool] = []
        for _, values in members:
            result = evaluate_predicate(predicate, MappingResolver(values))
            if isinstance(result, ConditionResult):
                return result
            assert isinstance(result, PredicateValue)
            kept.append(result.value is TruthValue.TRUE)
        return tuple(kept)

    def _inline_lookup(self, payload: Mapping[str, object]) -> EvaluationResult:
        dataset = payload.get("dataset")
        if not isinstance(dataset, str) or dataset not in self._context.relations:
            return _invalid("lookup", "an undeclared dataset")
        return evaluate_intermediate(
            payload, self._context.relations[dataset], self.resolve
        )

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
            key_fields = _names(payload.get("key"))
            key_variables = _names(payload.get("key_base"))
            if (
                not key_fields
                or not key_variables
                or len(key_fields) != len(key_variables)
            ):
                # R003-30: the planner requires the declared pairs, so this
                # is only reachable on an unplanned path.
                return _invalid("aggregate", "declared key and source pairs")
            selected = self._right_side(
                relation_name,
                identifiers,
                group_by,
                predicate,
                payload.get("between"),
                key_fields,
                key_variables,
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
        key_fields: Sequence[str],
        key_variables: Sequence[str],
    ) -> tuple[list[dict[str, object]], dict[str, object]] | ConditionResult:
        """Reduce the partition R003-30 selects for the current row."""
        relation = self._context.relations[dataset]
        values: list[RuntimeValue] = []
        for name in key_variables:
            resolved = self.resolve(name)
            if isinstance(resolved, ResolvedValue):
                values.append(resolved.value)
            elif isinstance(resolved, FailedResolution):
                return ConditionResult(condition=resolved.condition)
            else:
                values.append(MISSING)
        # `matching` keeps right records with a missing key out of every
        # match, and a current row carrying a missing key reaches nothing
        # for the same reason: an uncollected identifier is not an identity
        # two rows share.
        matched = relation.matching(key_fields, values)
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


def _readable(row: CandidateRow) -> dict[str, object]:
    """Return every name one partition row answers to.

    R007-12 lets a window field name a qualified source variable as well as a
    current-output column, so a row is read through its completed columns and
    the driver record it was constructed from together.
    """
    values: dict[str, object] = dict(row.values)
    for dataset, record in row.source_rows.items():
        for field_name, value in record.items():
            values[f"{dataset}.{field_name}"] = value
    return values


def _order_terms(declared: object) -> list[tuple[OrderTerm, str]]:
    """Bind each declared order term to the output column it reads."""
    if not isinstance(declared, Sequence) or isinstance(declared, str):
        return []
    terms: list[tuple[OrderTerm, str]] = []
    for entry in declared:
        term = (
            OrderTerm(variable=entry)
            if isinstance(entry, str)
            else OrderTerm.model_validate(dict(entry), strict=True)
            if isinstance(entry, Mapping)
            else None
        )
        if term is not None:
            terms.append((term, term.variable))
    return terms


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
                # R001-12b collects a direct read across the records feeding
                # one key combination. A grouped candidate has no such read:
                # its scalars are the keys, and everything else reduces.
                feeding_rows={},
                row_id=planned.declaration.id if planned.declaration else None,
                group_driver=planned.driver,
                group_records=records,
                group_values=values,
            )
        )
    return candidates
