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
from typing import Any, Literal

from pydantic import JsonValue, ValidationError

from yamaa.expressions import (
    AggregateError,
    ExpressionDispatcher,
    FailedResolution,
    MappingResolver,
    NumericError,
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
    numeric_identifiers,
    parse_aggregate_cached,
    parse_numeric_cached,
    parse_predicate_cached,
)
from yamaa.expressions.core import CallableResolver
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
from yamaa.models.values import convert_value
from yamaa.odm import BindingIndex
from yamaa.odm.items import (
    ODM_HIERARCHY_FIELDS,
    ODM_IDENTIFYING_FIELDS,
    parse_odm_read,
)
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
from yamaa.specification.models import Expression, OrderTerm, key_pairs


@dataclass(slots=True)
class CandidateRow:
    """One constructed row, with whatever R001 gave it to be constructed from."""

    source_rows: dict[str, dict[str, object]]
    values: dict[str, object]
    # Every driver record feeding this row under REQ-0042, which a direct
    # dataset read collects one value across.
    feeding_rows: dict[str, list[dict[str, object]]] = field(default_factory=dict)
    row_id: str | None = None
    # Where REQ-0039 appended this row, which is the order a window falls back
    # to when its terms tie.
    output_position: int = -1
    group_driver: str | None = None
    group_records: tuple[IndexedRecord, ...] = ()
    group_values: dict[str, RuntimeValue] = field(default_factory=dict)
    intermediates: dict[str, IntermediateOutcome] = field(default_factory=dict)
    # How R001 built the row, which fixes the ODM scope an `odm` read takes
    # (REQ-1269): a key combination, one driver record, or one driver group.
    built_by: Literal["key", "record", "group"] | None = None

    @property
    def reported_group(self) -> dict[str, JsonValue]:
        """Return the group a failure names, keyed by its bare column names."""
        return {
            name.split(".", 1)[-1]: json_value(value)
            for name, value in self.group_values.items()
        }


@dataclass(frozen=True, slots=True)
class _OrderedPartition:
    """One window partition in declared order, read once for a window pass."""

    rows: tuple[dict[str, object], ...]
    eligible: tuple[bool, ...]
    # Each member row's index in `rows`, by row identity.
    positions: dict[int, int]


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
    _odm_scopes: dict[
        tuple[str, tuple[str, ...]], dict[tuple[object, ...], tuple[IndexedRecord, ...]]
    ] = field(default_factory=dict)
    # One ordered window partition per derived column, partition, and window
    # ordering; see `RowResolver._ordered`.
    _windows: dict[tuple[object, ...], _OrderedPartition] = field(default_factory=dict)

    def odm_records(
        self,
        dataset: str,
        fields: tuple[str, ...],
        values: tuple[object, ...],
    ) -> tuple[IndexedRecord, ...]:
        """Return the ODM records equal on `fields`, missing equal to missing.

        REQ-1269 takes a scope the way REQ-0037 groups, so a repeat key that
        was never collected matches another that was not; the index is built
        once per dataset and set of fields, and every row shares it.
        """
        key = (dataset, fields)
        index = self._odm_scopes.get(key)
        if index is None:
            index = partition_records(self.relations[dataset].records, fields)
            self._odm_scopes[key] = index
        return index.get(values, ())

    def partition(
        self, fields: tuple[str, ...]
    ) -> dict[tuple[object, ...], list[CandidateRow]]:
        """Group the constructed rows by these columns, once per key combination.

        REQ-0467 broadcasts an output-row reduction back to each row of its
        partition, so every row of one partition asks the same question. The
        columns a partition is taken on are complete before the reduction
        reads them and never change afterwards, so one grouping answers all
        of them.
        """
        grouped = self._partitions.get(fields)
        if grouped is None:
            grouped = {}
            for row in self.rows:
                # REQ-0297 lets a window field name a qualified source
                # variable as well as a current-output column, so the key
                # reads each row through the same combined view a window
                # field resolves against.
                readable = _readable(row)
                key = tuple(readable.get(name, MISSING) for name in fields)
                grouped.setdefault(key, []).append(row)
            self._partitions[fields] = grouped
        return grouped


def driver_groups(
    relation: RelationIndex,
    fields: Sequence[str],
) -> list[tuple[tuple[RuntimeValue, ...], tuple[IndexedRecord, ...]]]:
    """Partition a driver relation into the candidates a grouped template makes.

    REQ-0037 partitions the complete driver relation by the equality each
    value's type owns, with missing equal to missing, and REQ-0038 orders the
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
            requirement="REQ-0321",
        )
    )


def _derive_variable_names(derive: object) -> set[str]:
    """Collect the qualified variable names a derive step's derivations name.

    REQ-1189 pins a derived aggregate to one relation. The engine re-derives
    that relation from the payload the way it re-derives the reducer's
    relation; the planner validates it strictly. REQ-1242 lets bindings read
    keep-declared named intermediates, so callers split these names into the
    driving relation's fields and per-row intermediate reads.
    """
    names: set[str] = set()

    def add(name: str) -> None:
        head, dot, _ = name.partition(".")
        if dot and head:
            names.add(name)

    def visit(node: object) -> None:
        if isinstance(node, str):
            # A bare-string derivation is one source read.
            add(node)
        elif isinstance(node, Mapping):
            if len(node) == 1:
                operation, payload = next(iter(node.items()))
                if operation == "source":
                    variable = payload
                    if isinstance(payload, Mapping):
                        variable = payload.get("variable")
                    if isinstance(variable, str):
                        add(variable)
                    return
                if operation == "compute":
                    expr = (
                        payload.get("expr") if isinstance(payload, Mapping) else payload
                    )
                    if isinstance(expr, str):
                        try:
                            identifiers = numeric_identifiers(
                                parse_numeric_cached(expr)
                            )
                        except NumericError:
                            identifiers = ()
                        for name in identifiers:
                            add(name)
                    return
            for value in node.values():
                visit(value)
        elif isinstance(node, list):
            for item in node:
                visit(item)

    visit(derive)
    return names


def _normalize_derive_derivation(
    derivation: object,
) -> tuple[dict[str, object], object | None]:
    """Split a derive binding's derivation into an expression and handler.

    Returns the one-operation expression mapping and the declared
    `unconvertible` handler, or None when the binding is unhandled.
    """
    handler: object | None = None
    if isinstance(derivation, Mapping) and "value" in derivation:
        # The loader wraps a derivation as a handled expression:
        # {"value": <expression>, "unconvertible"?: ...}.
        handler = derivation.get("unconvertible")
        derivation = derivation["value"]
    if isinstance(derivation, str):
        # REQ-0290: a bare string reads one variable.
        return {"source": derivation}, handler
    if isinstance(derivation, Mapping) and len(derivation) == 1:
        return dict(derivation), handler
    raise ValueError("a derive binding derivation must be one expression")


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
        dispatcher: ExpressionDispatcher | None = None,
    ) -> None:
        self._context = context
        self._candidate = candidate
        self._values: dict[str, Any] = dict(values)
        self._row_phase = row_phase
        self._column = column
        self._implicit_joins = {join.dataset: join for join in implicit_joins}
        # The derive step evaluates binding derivations with the same
        # configured dispatcher as every other derivation (REQ-1189), so
        # project-registered extensions work inside bindings too.
        self._dispatcher = dispatcher
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
        if qualifier in self._candidate.source_rows:
            return self._base.resolve(variable)
        if self._context.intermediates.declares(qualifier):
            return self._lookup_read(qualifier, variable.split(".", 1)[1])
        implicit = self._implicit_joins.get(qualifier)
        if implicit is not None:
            # REQ-0150: a plain cross-dataset scalar source joins on the
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
        if qualifier in self._candidate.source_rows:
            return self._base.resolve_selected(
                variable, selector=selector, multiple_matches=multiple_matches
            )
        if qualifier is not None and self._context.intermediates.declares(qualifier):
            # R003 already chose the record; the source reads a column of it.
            return self._lookup_read(qualifier, variable.split(".", 1)[1])
        if qualifier is not None:
            implicit = self._implicit_joins.get(qualifier)
            if implicit is not None:
                # REQ-0150: a structured cross-dataset source joins on the
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
                    requirement="REQ-0125",
                )
            )
        # REQ-0156/REQ-0157: a row-phase join states which current-row
        # variables it matches; otherwise the keys match themselves.
        match_variables = (
            join.match_variables if join.match_variables is not None else join.keys
        )
        payload: dict[str, object] = {
            "dataset": join.dataset,
            # An unpaired match reaches evaluate_intermediate as no key, which
            # answers it as a condition rather than a truncated match.
            "key": (
                dict(zip(join.keys, match_variables, strict=True))
                if len(join.keys) == len(match_variables)
                else None
            ),
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
        result = evaluate_intermediate(
            payload,
            relation,
            self.resolve,
            evaluate=self._dispatcher.evaluate
            if self._dispatcher is not None
            else None,
            resolver=self,
        )
        if isinstance(result, ValueResult):
            return ResolvedValue(value=result.value, handled_by=result.handled_by)
        return FailedResolution(condition=result.condition)

    def _lookup_read(self, identifier: str, field_name: str) -> Resolution:
        plan = self._context.intermediates.plans[identifier]
        derived = {name for name, _ in plan.derived}
        readable = (
            field_name in plan.self_fields
            if plan.dataset == "SELF"
            else self._context.relations[plan.dataset].has(field_name)
        )
        if not readable and field_name not in derived:
            return _failed(
                _condition(
                    "unknown_field",
                    {"identifier": f"{identifier}.{field_name}"},
                    requirement="REQ-0125",
                )
            )
        outcome = self._candidate.intermediates.get(identifier)
        if outcome is None:
            outcome = self._context.intermediates.select(
                identifier, self._lookup_current(plan), resolver=self
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
            # REQ-0124: an intermediate that yields nothing answers its decided
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
        names = plan.dependencies
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
                        # The diagnostic names the intermediate whose match
                        # the row could not supply, never an empty path.
                        spec_paths=(plan.path,),
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
        if operation in WINDOW_OPERATIONS:
            return self._window(operation, payload)
        if operation == "odm":
            return self._odm(payload)
        return self._aggregate(payload)

    def _odm_scope(
        self, dataset: str, item_oid: str
    ) -> tuple[tuple[IndexedRecord, ...], dict[str, JsonValue]] | ConditionResult:
        """Return the row's scope records carrying the item, and the scope.

        REQ-1269: a key combination reads the records it was derived from, a
        grouped row the records equal to it on its group's hierarchy fields,
        and a record-driven row its driver record's item group occurrence.
        """
        candidate = self._candidate
        if candidate.built_by == "key" and dataset in candidate.feeding_rows:
            records = tuple(
                IndexedRecord(position=position, values=values)
                for position, values in enumerate(candidate.feeding_rows[dataset])
                if values.get("ItemOID") == item_oid
            )
            scope = {
                key: json_value(candidate.values.get(key, MISSING))
                for key in self._context.output_keys
            }
            return records, scope
        if candidate.built_by == "group" and candidate.group_driver == dataset:
            fields = tuple(
                name
                for name in ODM_HIERARCHY_FIELDS
                if f"{dataset}.{name}" in candidate.group_values
            )
            values = tuple(
                candidate.group_values[f"{dataset}.{name}"] for name in fields
            )
        elif candidate.built_by == "record" and dataset in candidate.source_rows:
            driver = candidate.source_rows[dataset]
            fields = ODM_HIERARCHY_FIELDS
            values = tuple(driver[name] for name in fields)
        else:
            return ConditionResult(
                condition=_condition(
                    "invalid_odm_context",
                    {"dataset": dataset, "row": candidate.row_id},
                    requirement="REQ-1277",
                )
            )
        records = self._context.odm_records(
            dataset, (*fields, "ItemOID"), (*values, item_oid)
        )
        scope = {
            name: json_value(value) for name, value in zip(fields, values, strict=True)
        }
        return records, scope

    def _odm(self, payload: Mapping[str, object]) -> EvaluationResult:
        """Read the one record an `odm` expression identifies (REQ-1271)."""
        read = parse_odm_read(payload)
        if read is None:
            return _invalid("odm", "an item written as DATASET.ItemOID")
        if read.dataset not in self._context.relations:
            return ConditionResult(
                condition=_condition(
                    "unknown_field", {"identifier": read.item}, requirement="REQ-0103"
                )
            )
        found = self._odm_scope(read.dataset, read.item_oid)
        if isinstance(found, ConditionResult):
            return found
        records, scope = found
        for level, wanted in (
            ("StudyEventOID", read.events),
            ("FormOID", read.forms),
            ("ItemGroupOID", read.item_groups),
        ):
            if wanted is not None:
                records = tuple(
                    record for record in records if record.values[level] in wanted
                )
        predicate = self._predicate(read.filter)
        if isinstance(predicate, ConditionResult):
            return predicate
        if predicate is not None:
            eligible = eligible_records(
                records, predicate, self._context.relations[read.dataset]
            )
            if isinstance(eligible, ConditionResult):
                return eligible
            records = tuple(eligible)
        # REQ-1272: none gives missing, one its Value, and two or more fail
        # whatever their values, since two records that agree are still two.
        if not records:
            return ValueResult(value=MISSING)
        if len(records) == 1:
            return ValueResult(value=records[0].values["Value"])
        differ: dict[str, JsonValue] = {}
        for name in ODM_IDENTIFYING_FIELDS:
            seen: list[JsonValue] = []
            for record in records:
                value = json_value(record.values[name])
                if value not in seen:
                    seen.append(value)
            if len(seen) > 1:
                differ[name] = seen
        return ConditionResult(
            condition=_condition(
                "odm_not_unique",
                {
                    "item": read.item,
                    "row": self._candidate.row_id,
                    "scope": scope,
                    "records": len(records),
                    "differ": differ,
                    "repeated": not differ,
                },
                phase=self._phase,
                requirement="REQ-1278",
            )
        )

    def _window(
        self,
        operation: str,
        payload: Mapping[str, object],
    ) -> EvaluationResult:
        """Locate this row in its ordered partition and ask the window.

        REQ-0293 partitions the constructed output rows by the window's own
        `group_by` and preserves row count. REQ-0326 scopes a row-construction
        window to the rows its enclosing row template constructs: the caller
        exposes exactly those rows, with every dependency of this window
        already completed in that scope.
        """
        if self._row_phase:
            return _invalid(operation, "a window has no row-construction context")
        located = self._locate(payload)
        if isinstance(located, ConditionResult):
            return located
        partition, partition_keys = located
        result = evaluate_window(operation, payload, partition)
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
    ) -> tuple[Partition, dict[str, JsonValue]] | ConditionResult:
        """Return this row's partition in declared order, and its place in it."""
        window = window_spec(payload)
        fields = _names(window.get("group_by"))
        # REQ-0297: a window field may name a qualified source variable of
        # the row's driver, so availability and the partition key read the
        # row through its completed columns and driver record together.
        readable = _readable(self._candidate)
        unavailable = [name for name in fields if name not in readable]
        if unavailable:
            return ConditionResult(
                condition=_condition("unknown_field", {"identifier": unavailable[0]})
            )
        key = tuple(readable[name] for name in fields)
        ordered = self._ordered(window, fields, key)
        if isinstance(ordered, ConditionResult):
            return ordered
        partition_keys = {
            name: json_value(readable[name])  # type: ignore[arg-type]
            for name in fields
        }
        index = ordered.positions.get(id(self._candidate))
        if index is None:
            return ConditionResult(
                condition=_condition(
                    "unknown_field",
                    {"identifier": "the current row is absent from its partition"},
                )
            )
        partition = Partition(
            rows=ordered.rows, current=index, eligible=ordered.eligible
        )
        return partition, partition_keys

    def _ordered(
        self,
        window: Mapping[str, object],
        fields: tuple[str, ...],
        key: tuple[object, ...],
    ) -> _OrderedPartition | ConditionResult:
        """Order one partition and apply the window's filter, once per window pass.

        The executor completes a window's column across every row of its
        scope before the next column, and REQ-0326 completes every
        dependency of the window in that scope first, so each row of one
        partition would order and filter the same rows by the same values.
        The pass is identified by the column being derived, so a later
        window over the same partition reads the columns completed since;
        without the cache every row re-sorts its whole partition.
        """
        terms = _order_terms(window.get("order_by"))
        declared_filter = window.get("filter")
        identity: tuple[object, ...] | None = None
        if self._column is not None and (
            declared_filter is None or isinstance(declared_filter, str)
        ):
            identity = (
                self._column,
                fields,
                key,
                tuple((term.variable, term.direction, term.nulls) for term, _ in terms),
                declared_filter,
            )
            cached = self._context._windows.get(identity)
            if cached is not None:
                return cached
        members = [
            (row, _readable(row))
            for row in self._context.partition(fields).get(key, ())
        ]
        if terms:
            indexed = [
                IndexedRecord(position=row.output_position, values=values)
                for row, values in members
            ]
            try:
                # REQ-0301 falls back to construction order, which is what the
                # shared ordering helper breaks a remaining tie by.
                ordered = order_records(indexed, terms)
            except OrderError as error:
                return ConditionResult(
                    condition=_condition(
                        "incompatible_input_type",
                        {"source": error.variable, "types": sorted(set(error.types))},
                        requirement="REQ-0324",
                    )
                )
            by_position = {
                row.output_position: entry for entry in members for row in (entry[0],)
            }
            members = [by_position[record.position] for record in ordered]
        eligible = self._eligible(declared_filter, members)
        if isinstance(eligible, ConditionResult):
            return eligible
        located = _OrderedPartition(
            rows=tuple(values for _, values in members),
            eligible=eligible,
            positions={id(row): index for index, (row, _) in enumerate(members)},
        )
        if identity is not None:
            self._context._windows[identity] = located
        return located

    def _eligible(
        self,
        declared_filter: object,
        members: Sequence[tuple[CandidateRow, dict[str, object]]],
    ) -> tuple[bool, ...] | ConditionResult:
        """Say which partition rows the window's filter retained (REQ-0294)."""
        predicate = self._predicate(declared_filter)
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

        derive = payload.get("derive")
        if relation_name is None and derive is not None:
            # REQ-1189: a derived aggregate names its relation through the
            # derive step when the reducer only names bound variables.
            # REQ-1242: qualifiers naming declared intermediates read the
            # intermediate's row, never the reduced relation.
            declares = self._context.intermediates.declares
            derived = {
                name.split(".", 1)[0]
                for name in _derive_variable_names(derive)
                if not declares(name.split(".", 1)[0])
            }
            relation_name = next(iter(sorted(derived)), None)

        if relation_name is None:
            selected = self._output_rows(identifiers, group_by, predicate)
        elif self._row_phase and relation_name == self._candidate.group_driver:
            selected = self._driver_group(relation_name, identifiers, predicate)
        else:
            pairs = key_pairs(payload.get("key"))
            if pairs is None or not pairs[0]:
                # REQ-0140: the planner requires the declared pairs, so this
                # is only reachable on an unplanned path.
                return _invalid("aggregate", "declared key pairs")
            key_fields, key_entries = pairs
            key_values: list[RuntimeValue] = []
            key_resolver = CallableResolver(self.resolve)
            # REQ-1189: the expression match values evaluate through the
            # configured dispatcher, like the derive bindings below.
            dispatcher = self._dispatcher or ExpressionDispatcher()
            for entry in key_entries:
                if isinstance(entry, str):
                    resolved = self.resolve(entry)
                    if isinstance(resolved, ResolvedValue):
                        key_values.append(resolved.value)
                    elif isinstance(resolved, FailedResolution):
                        return ConditionResult(condition=resolved.condition)
                    else:
                        key_values.append(MISSING)
                    continue
                # REQ-1259: an expression match value evaluates against the
                # current row. A missing result matches nothing.
                expression = (
                    entry
                    if isinstance(entry, Expression)
                    else Expression.model_validate(dict(entry))  # type: ignore[call-overload]
                )
                evaluated = dispatcher.evaluate(expression, key_resolver)
                if isinstance(evaluated, ConditionResult):
                    return evaluated
                if not isinstance(evaluated, ValueResult):
                    return _invalid(
                        "aggregate",
                        "a key match value that did not evaluate to a value",
                    )
                key_values.append(evaluated.value)
            selected = self._right_side(
                relation_name,
                identifiers,
                group_by,
                predicate,
                payload.get("between"),
                key_fields,
                key_values,
                derive,
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
                    requirement="REQ-0188",
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
        key_values: Sequence[RuntimeValue],
        derive: object = None,
    ) -> tuple[list[dict[str, object]], dict[str, object]] | ConditionResult:
        """Reduce the partition REQ-0140 selects for the current row."""
        relation = self._context.relations[dataset]
        values: list[RuntimeValue] = list(key_values)
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
        if derive is not None:
            enriched = self._derive_records(derive, dataset, narrowed)
            if isinstance(enriched, ConditionResult):
                return enriched
            return enriched, grouped
        return [_record_values(record, identifiers) for record in narrowed], grouped

    def _derive_records(
        self,
        derive: object,
        dataset: str,
        records: list[IndexedRecord],
    ) -> list[dict[str, object]] | ConditionResult:
        """Bind each record's derive variables before reduction.

        REQ-1189 evaluates one binding per record in declaration order;
        REQ-1190 converts each value to its declared type through R011.
        """
        if not isinstance(derive, list) or not derive:
            return _invalid("aggregate", "a derive that is not a binding list")
        bindings: list[tuple[str, str, dict[str, object], object | None]] = []
        for index, binding in enumerate(derive):
            if not isinstance(binding, Mapping):
                return _invalid(
                    "aggregate",
                    f"a derive binding at index {index} that is not a mapping",
                )
            name = binding.get("name")
            target_type = binding.get("type")
            derivation = binding.get("derivation")
            if (
                not isinstance(name, str)
                or not name
                or target_type not in ("str", "int", "float", "date", "datetime")
                or derivation is None
            ):
                return _invalid(
                    "aggregate",
                    f"a derive binding at index {index} without a name, type, and derivation",
                )
            try:
                expression, handler = _normalize_derive_derivation(derivation)
            except ValueError:
                return _invalid(
                    "aggregate",
                    f"a derive binding '{name}' whose derivation is not one expression",
                )
            bindings.append((name, target_type, expression, handler))
        names = [name for name, _, _, _ in bindings]
        if len(set(names)) != len(names):
            return _invalid("aggregate", "derive binding names that are not unique")

        # REQ-1242: a binding may read a keep-declared named intermediate.
        # The read resolves once per output row - the value is the same for
        # every record the bindings evaluate - through the row's normal
        # intermediate lookup, which caches the selected record on the
        # candidate row. A selection failure raises LifecycleCondition like
        # any other intermediate read.
        declares = self._context.intermediates.declares
        intermediate_scope: dict[str, object] = {}
        for variable in sorted(_derive_variable_names(derive)):
            head, _, field = variable.partition(".")
            if not declares(head) or variable in intermediate_scope:
                continue
            resolved = self._lookup_read(head, field)
            if isinstance(resolved, FailedResolution):
                return ConditionResult(condition=resolved.condition)
            intermediate_scope[variable] = (
                resolved.value if isinstance(resolved, ResolvedValue) else MISSING
            )

        dispatcher = self._dispatcher or ExpressionDispatcher()
        enriched: list[dict[str, object]] = []
        for record in records:
            # The binding sees the record's fields qualified, the row's
            # intermediate reads, and every earlier binding by its
            # unqualified name (REQ-1189).
            scope: dict[str, object] = dict(intermediate_scope)
            scope.update(
                {f"{dataset}.{field}": value for field, value in record.values.items()}
            )
            for name, target_type, expression, handler in bindings:
                operation = next(iter(expression))
                try:
                    parsed = Expression.model_validate(
                        {operation: expression[operation]}
                    )
                except ValidationError:
                    return _invalid(
                        "aggregate",
                        f"a derive binding '{name}' whose derivation is not one expression",
                    )
                evaluated = dispatcher.evaluate(parsed, MappingResolver(scope))
                if isinstance(evaluated, ConditionResult):
                    return evaluated
                if not isinstance(evaluated, ValueResult):
                    return _invalid(
                        "aggregate",
                        f"a derive binding '{name}' that did not evaluate to a value",
                    )
                converted = convert_value(evaluated.value, target_type)  # type: ignore[arg-type]
                if isinstance(converted, ConditionResult):
                    if handler is None:
                        return converted
                    handled = convert_value(handler, target_type)  # type: ignore[arg-type]
                    if isinstance(handled, ConditionResult):
                        return handled
                    scope[name] = handled.value  # type: ignore[union-attr]
                elif isinstance(converted, ValueResult):
                    scope[name] = converted.value
                else:
                    return _invalid(
                        "aggregate",
                        f"a derive binding '{name}' that did not convert to {target_type}",
                    )
            enriched.append(scope)
        return enriched

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
            # REQ-0473: a missing cutoff admits no record rather
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
                            requirement="REQ-0509",
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
                            requirement="REQ-0509",
                        )
                    )
                # Every stated endpoint is inclusive under REQ-0136.
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
        """Reduce the records of the current driver group (REQ-0467)."""
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
        """Reduce the constructed output rows of this row's partition (REQ-0467)."""
        fields = tuple(group_by)
        # REQ-0297: a window field may name a qualified source variable of
        # the row's driver, so availability and the partition key read the
        # row through its completed columns and driver record together.
        readable = _readable(self._candidate)
        unavailable = [name for name in fields if name not in readable]
        if unavailable:
            return ConditionResult(
                condition=_condition("unknown_field", {"identifier": unavailable[0]})
            )
        key = tuple(readable[name] for name in fields)
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
        return records, {name: readable[name] for name in fields}


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

    REQ-0297 lets a window field name a qualified source variable as well as a
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
                # REQ-0047: only the grouped variables are scalars of the
                # candidate; every other driver field varies within the group
                # and is read through an aggregate or not at all.
                source_rows={
                    planned.driver: dict(zip(fields, key, strict=True)),
                },
                values={},
                # REQ-0044 collects a direct read across the records feeding
                # one key combination. A grouped candidate has no such read:
                # its scalars are the keys, and everything else reduces.
                feeding_rows={},
                row_id=planned.declaration.id if planned.declaration else None,
                group_driver=planned.driver,
                group_records=records,
                group_values=values,
                built_by="group",
            )
        )
    return candidates
