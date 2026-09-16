"""Execute the initial record-driven R001 slice over typed source tables."""

from __future__ import annotations

from collections.abc import Callable, Mapping, Sequence
from contextlib import contextmanager
from dataclasses import dataclass
from typing import Literal, TypeAlias

from pydantic import BaseModel, ConfigDict, Field, JsonValue

from yamaa.expressions import (
    WINDOW_OPERATIONS,
    ExpressionDispatcher,
    MappingResolver,
    PredicateValue,
    TruthValue,
    evaluate_predicate,
)
from yamaa.io import Artifact, ArtifactDiagnostic, ArtifactError, build_artifact
from yamaa.io.polars import frame_from_values
from yamaa.io.source import LoadedDataset, ProducerSchemaUnresolved, SourceError
from yamaa.models import (
    MISSING,
    ConditionResult,
    DateTimeValue,
    DateValue,
    TypedColumn,
    TypedTable,
)
from yamaa.odm import BindingIndex
from yamaa.planning import (
    ExecutionDiagnostic,
    ExecutionPlanningError,
    PlannedDerivation,
    UnsupportedFeature,
    UnsupportedPlanningError,
    plan_execution,
    preflight_execution,
)
from yamaa.runtime.joins import KeyCorrelation, RelationIndex, build_relation_indexes
from yamaa.runtime.lifecycle import (
    HandlerCount,
    HandlerCounter,
    LifecycleCondition,
    LifecycleUnsupported,
    evaluate_derivation,
)
from yamaa.runtime.lookups import RecordLookupSelector
from yamaa.runtime.rows import (
    CandidateRow,
    RelationalContext,
    RowResolver,
    group_candidates,
)
from yamaa.specification.models import Column, DatasetSource, Specification
from yamaa.verification import (
    DeclarationError,
    VerificationFailure,
    build_violation_log,
    check_column,
    check_dataset,
    check_keys,
)

SourceTable: TypeAlias = LoadedDataset | TypedTable
SourceProvider: TypeAlias = Callable[
    [Mapping[str, DatasetSource]], Mapping[str, SourceTable]
]
ColumnCheck: TypeAlias = Callable[
    [TypedTable, Column, Sequence[str]], tuple[VerificationFailure, ...]
]
KeyCheck: TypeAlias = Callable[
    [TypedTable, Sequence[str]], tuple[VerificationFailure, ...]
]
DatasetCheck: TypeAlias = Callable[
    [TypedTable, Sequence[object], Sequence[str]], tuple[VerificationFailure, ...]
]
OutputBuilder: TypeAlias = Callable[[TypedTable, object, Sequence[str]], Artifact]

_KeyToken: TypeAlias = tuple[object, ...] | tuple[str, int]


class _FrozenModel(BaseModel):
    model_config = ConfigDict(
        strict=True,
        extra="forbid",
        frozen=True,
        arbitrary_types_allowed=True,
    )


class ExecutionSuccess(_FrozenModel):
    """A completed table, primary artifact, and governed warning findings."""

    status: Literal["success"] = "success"
    table: TypedTable
    artifact: Artifact
    warnings: tuple[VerificationFailure, ...] = ()
    violation_log: Artifact | None = None
    handler_counts: tuple[HandlerCount, ...]


class ExecutionFailure(_FrozenModel):
    """A semantic execution failure with any handler activity before it."""

    status: Literal["failure"] = "failure"
    diagnostics: tuple[ExecutionDiagnostic, ...] = Field(min_length=1)
    handler_counts: tuple[HandlerCount, ...]


class ExecutionUnsupported(_FrozenModel):
    """A valid specification outside this component's supported subset."""

    status: Literal["unsupported"] = "unsupported"
    features: tuple[UnsupportedFeature, ...] = Field(min_length=1)
    handler_counts: tuple[HandlerCount, ...]


ExecutionResult: TypeAlias = ExecutionSuccess | ExecutionFailure | ExecutionUnsupported


@dataclass(frozen=True, slots=True)
class ExecutionHooks:
    """The pure verification and output hooks supplied by the output component."""

    column: ColumnCheck = check_column
    keys: KeyCheck = check_keys
    dataset: DatasetCheck = check_dataset  # type: ignore[assignment]
    output: OutputBuilder = build_artifact  # type: ignore[assignment]


class _ExecutionAbort(ValueError):
    def __init__(self, diagnostics: Sequence[ExecutionDiagnostic]) -> None:
        self.diagnostics = tuple(diagnostics)
        super().__init__(", ".join(item.condition for item in diagnostics))


def _diagnostic_from_condition(error: LifecycleCondition) -> ExecutionDiagnostic:
    return error.diagnostic


def _verification_diagnostics(
    failures: Sequence[VerificationFailure],
) -> tuple[ExecutionDiagnostic, ...]:
    return tuple(
        ExecutionDiagnostic(
            phase=failure.phase,
            condition=failure.condition,
            spec_paths=failure.spec_paths,
            requirement=failure.requirement,
            context=failure.context,
        )
        for failure in failures
    )


def _artifact_diagnostics(
    diagnostics: Sequence[ArtifactDiagnostic],
) -> tuple[ExecutionDiagnostic, ...]:
    return tuple(
        ExecutionDiagnostic(
            phase=diagnostic.phase,
            condition=diagnostic.condition,
            spec_paths=diagnostic.spec_paths,
            requirement=diagnostic.requirement,
            context=diagnostic.context,
        )
        for diagnostic in diagnostics
    )


def _source_diagnostics(error: SourceError) -> tuple[ExecutionDiagnostic, ...]:
    return tuple(
        ExecutionDiagnostic(
            phase=diagnostic.phase,
            condition=diagnostic.condition,
            spec_paths=diagnostic.spec_paths,
            requirement=diagnostic.requirement,
            context=diagnostic.context,
        )
        for diagnostic in error.diagnostics
    )


def _declaration_diagnostic(error: DeclarationError) -> ExecutionDiagnostic:
    context = dict(error.context)
    if not context:
        context["reason"] = error.reason
    return ExecutionDiagnostic(
        phase="validation",
        condition=error.condition or "invalid_declaration",
        spec_paths=(error.spec_path,),
        requirement=error.requirement,
        context=context,
    )


def _table_from_candidates(
    specification: Specification,
    candidates: Sequence[CandidateRow],
    completed: set[str],
) -> TypedTable:
    columns = tuple(
        TypedColumn(name=column.name, type=column.type)
        for column in specification.columns
        if column.name in completed
    )
    rows = [
        [candidate.values[column.name] for column in columns]
        for candidate in candidates
    ]
    return frame_from_values(columns, rows)


def _evaluate_row_filter(
    planned,
    index: BindingIndex,
    source_row: dict[str, object],
) -> bool:
    if planned.filter_predicate is None:
        return True
    resolver = index.context({planned.driver: source_row})
    result = evaluate_predicate(planned.filter_predicate, resolver)
    if isinstance(result, ConditionResult):
        raise _ExecutionAbort(
            [
                ExecutionDiagnostic(
                    phase=result.condition.phase,
                    condition=result.condition.condition,
                    spec_paths=(planned.filter_path or "rows.filter",),
                    context=result.condition.context,
                )
            ]
        )
    assert isinstance(result, PredicateValue)
    return result.value is TruthValue.TRUE


# The phases whose failures name the record they happened on. A failure
# decided before any row exists reports no key.
_ROW_PHASES = frozenset(
    {"derivation", "impute", "join", "mapping", "row_construction", "convert"}
)


def _offending_keys(
    candidate: CandidateRow,
    keys: Sequence[str],
) -> list[JsonValue] | None:
    """Return the offending record's key values, when they are known.

    A column is derived in R001 order, so a key column the failing derivation
    precedes has no value; reporting a partial key would be worse than
    reporting none.
    """
    if not keys or any(name not in candidate.values for name in keys):
        return None
    return [{name: _json_key(candidate.values[name]) for name in keys}]


def _json_key(value: object) -> JsonValue:
    if value is MISSING:
        return None
    if isinstance(value, (DateValue, DateTimeValue)):
        return value.to_text()
    if isinstance(value, (str, int, float, bool)):
        return value
    return str(value)


def _with_keys(
    diagnostic: ExecutionDiagnostic,
    candidate: CandidateRow,
    keys: Sequence[str],
) -> ExecutionDiagnostic:
    if diagnostic.phase not in _ROW_PHASES or "keys" in diagnostic.context:
        return diagnostic
    offending = _offending_keys(candidate, keys)
    if offending is None:
        return diagnostic
    return diagnostic.model_copy(
        update={"context": {**diagnostic.context, "keys": offending}}
    )


def _evaluate_one(
    planned: PlannedDerivation,
    column_types: Mapping[str, str],
    candidate: CandidateRow,
    context: RelationalContext,
    dispatcher: ExpressionDispatcher,
    counter: HandlerCounter,
    keys: Sequence[str] = (),
    *,
    row_phase: bool = False,
) -> object:
    try:
        return evaluate_derivation(
            planned,
            column_types[planned.column],  # type: ignore[arg-type]
            candidate.values,
            lambda values: RowResolver(
                context,
                candidate,
                values,
                row_phase=row_phase,
                column=planned.column,
            ),
            dispatcher,
            counter,
        )
    except LifecycleCondition as error:
        raise _ExecutionAbort(
            [_with_keys(_diagnostic_from_condition(error), candidate, keys)]
        ) from error


def _register_handler_paths(plan, counter: HandlerCounter) -> None:
    for row in plan.rows:
        for derivation in row.derivations:
            counter.register_derivation(derivation)
    for derivation in plan.columns:
        counter.register_derivation(derivation)


def _record_candidates(
    planned,
    relation: RelationIndex,
    index: BindingIndex,
) -> list[CandidateRow]:
    """Build one candidate per retained driver record, in driver order."""
    return [
        CandidateRow(
            source_rows={planned.driver: values},
            values={},
            feeding_rows={planned.driver: [values]},
            row_id=planned.declaration.id if planned.declaration else None,
        )
        for values in (dict(record.values) for record in relation.records)
        if _evaluate_row_filter(planned, index, values)
    ]


def _grouped_filter(
    planned,
    candidate: CandidateRow,
) -> bool:
    """Evaluate a grouped template's filter over the completed candidate.

    R001-8 runs it after every row derivation completes, so it corresponds to
    filtering after a group reduction rather than selecting driver records.
    """
    if planned.filter_predicate is None:
        return True
    result = evaluate_predicate(
        planned.filter_predicate, MappingResolver(candidate.values)
    )
    if isinstance(result, ConditionResult):
        raise _ExecutionAbort(
            [
                ExecutionDiagnostic(
                    phase=result.condition.phase,
                    condition=result.condition.condition,
                    spec_paths=(planned.filter_path or "rows.filter",),
                    context=result.condition.context,
                )
            ]
        )
    assert isinstance(result, PredicateValue)
    return result.value is TruthValue.TRUE


@contextmanager
def _partition_scope(context: RelationalContext, rows: list[CandidateRow]):
    saved_rows = context.rows
    saved_partitions = context._partitions
    context.rows = list(rows)
    context._partitions = {}
    try:
        yield
    finally:
        context.rows = saved_rows
        context._partitions = saved_partitions


def _ordered_scalar_keys(
    key_names: Sequence[str],
    key_derivations_by_name: Mapping[str, PlannedDerivation],
) -> tuple[str, ...]:
    """Return scalar keys in an order that respects key-to-key references."""
    remaining = set(key_names)
    ordered: list[str] = []
    while remaining:
        ready = {
            name
            for name in remaining
            if all(
                dep in ordered or dep not in key_names
                for dep in key_derivations_by_name[name].dependencies
            )
        }
        if not ready:
            # A cycle among keys should have been caught during planning.
            # Fall back to declaration order to avoid hanging.
            return tuple(name for name in key_names if name in remaining)
        # Deterministic: prefer declaration order.
        for name in key_names:
            if name in ready:
                ordered.append(name)
                remaining.remove(name)
    return tuple(ordered)


def _build_key_correlation(
    plan,
    sources: Mapping[str, SourceTable],
    column_types: Mapping[str, str],
    dispatcher: ExpressionDispatcher,
    counter: HandlerCounter,
) -> KeyCorrelation | None:
    """Compile new-style key derivations into per-record key functions."""
    if plan.key_dataset is None or not plan.key_derivations:
        return None
    key_bindings = BindingIndex(plan.bindings, sources)
    key_context = RelationalContext(
        bindings=key_bindings,
        relations={},
        lookups=RecordLookupSelector((), {}),
        output_keys=tuple(plan.specification.keys),
    )
    key_names = tuple(plan.specification.keys)
    key_derivations_by_name = {
        derivation.column: derivation for derivation in plan.key_derivations
    }

    scalar_keys = [
        name
        for name in key_names
        if key_derivations_by_name[name].declaration.value.operation
        not in WINDOW_OPERATIONS
    ]
    window_derivations = [
        key_derivations_by_name[name]
        for name in key_names
        if key_derivations_by_name[name].declaration.value.operation
        in WINDOW_OPERATIONS
    ]
    ordered_scalar = _ordered_scalar_keys(key_names, key_derivations_by_name)
    ordered_scalar = [name for name in ordered_scalar if name in scalar_keys]

    def scalar_evaluator(record: Mapping[str, object]) -> dict[str, object]:
        candidate = CandidateRow(
            source_rows={plan.key_dataset: dict(record)},
            values={},
        )
        values: dict[str, object] = {}
        for name in ordered_scalar:
            derivation = key_derivations_by_name[name]
            value = _evaluate_one(
                derivation,
                column_types,
                candidate,
                key_context,
                dispatcher,
                counter,
                key_names,
            )
            values[name] = value
            candidate.values[name] = value
        return values

    def window_evaluator(
        probes: Sequence[CandidateRow], relation: RelationIndex
    ) -> Sequence[dict[str, object]]:
        del relation
        for position, probe in enumerate(probes):
            probe.output_position = position
        with _partition_scope(key_context, probes):
            for probe in probes:
                for derivation in window_derivations:
                    probe.values[derivation.column] = _evaluate_one(
                        derivation,
                        column_types,
                        probe,
                        key_context,
                        dispatcher,
                        counter,
                        key_names,
                    )
        return [dict(probe.values) for probe in probes]

    return KeyCorrelation(
        dataset=plan.key_dataset,
        key_names=key_names,
        scalar_evaluator=scalar_evaluator,
        window_evaluator=window_evaluator if window_derivations else None,
    )


def _build_project_record(
    plan,
    sources: Mapping[str, SourceTable],
    key_correlation: KeyCorrelation,
    column_types: Mapping[str, str],
    dispatcher: ExpressionDispatcher,
    counter: HandlerCounter,
) -> Callable[
    [str, Mapping[str, object], frozenset[str], Mapping[str, object] | None],
    dict[str, object],
]:
    """Return a function that projects a relation row onto output columns.

    New-style aggregates group and reduce over output-column values
    recomputed from the relation's native rows.  The projector evaluates the
    minimal column derivation graph required by the caller.
    """
    key_context = RelationalContext(
        bindings=BindingIndex(plan.bindings, sources),
        relations={},
        lookups=RecordLookupSelector((), {}),
        output_keys=tuple(plan.specification.keys),
        key_correlation=key_correlation,
    )
    column_plans_by_name = {planned.column: planned for planned in plan.columns}

    def project(
        dataset: str,
        record: Mapping[str, object],
        needed: frozenset[str],
        seed: Mapping[str, object] | None = None,
    ) -> dict[str, object]:
        values: dict[str, object] = (
            dict(seed)
            if seed is not None
            else dict(key_correlation.evaluate_scalar_keys(record))
        )
        pending = set(needed)
        expanded = True
        while expanded:
            expanded = False
            for name in list(pending):
                derivation = column_plans_by_name.get(name)
                if derivation is None:
                    continue
                for dep in derivation.dependencies:
                    if dep in column_plans_by_name and dep not in pending:
                        pending.add(dep)
                        expanded = True
        pending -= set(values)
        computed = set(values)
        while pending:
            ready = {
                name
                for name in pending
                if name in column_plans_by_name
                and all(
                    dep in computed for dep in column_plans_by_name[name].dependencies
                )
            }
            if not ready:
                break
            for name in sorted(ready):
                derivation = column_plans_by_name[name]
                candidate = CandidateRow(
                    source_rows={dataset: dict(record)},
                    values=dict(values),
                )
                values[name] = _evaluate_one(
                    derivation,
                    column_types,
                    candidate,
                    key_context,
                    dispatcher,
                    counter,
                    plan.specification.keys,
                )
                computed.add(name)
            pending -= ready
        return values

    return project


def _key_space(
    plan,
    driver_rows: Sequence[dict[str, object]],
    driver: str,
    column_types: Mapping[str, str],
    context: RelationalContext,
    dispatcher: ExpressionDispatcher,
    counter: HandlerCounter,
) -> tuple[
    dict[_KeyToken, tuple[object, ...]],
    dict[_KeyToken, list[dict[str, object]]],
    list[_KeyToken],
]:
    """Derive the standalone key table over one section's driver records.

    Keys evaluate per driver record, then collapse to distinct combinations
    in first-appearance order (R001-12). Records with a missing key keep one
    entry each so the missing key still fails at the output gate.
    """
    key_correlation = context.key_correlation
    if key_correlation is not None:
        key_names = list(key_correlation.key_names)
        groups: dict[_KeyToken, list[dict[str, object]]] = {}
        order: list[_KeyToken] = []
        key_values: dict[_KeyToken, tuple[object, ...]] = {}

        scalar_values = [
            key_correlation.scalar_evaluator(driver_row) for driver_row in driver_rows
        ]
        probes: list[CandidateRow] = []
        for driver_row, values in zip(driver_rows, scalar_values):
            probes.append(
                CandidateRow(
                    source_rows={driver: driver_row},
                    values=dict(values),
                )
            )
        if key_correlation.window_evaluator is not None:
            driver_relation = context.relations.get(driver)
            if driver_relation is not None:
                window_values = key_correlation.window_evaluator(
                    probes, driver_relation
                )
                for probe, window_value in zip(probes, window_values):
                    probe.values.update(window_value)

        for position, probe in enumerate(probes):
            values = tuple(probe.values[name] for name in key_names)
            if any(value is MISSING for value in values):
                token: _KeyToken = ("__missing_key__", position)
            else:
                token = values
            if token not in groups:
                groups[token] = []
                order.append(token)
                key_values[token] = values
            groups[token].append(driver_rows[position])
        return key_values, groups, order

    key_names = list(plan.specification.keys)
    key_plans = [planned for planned in plan.columns if planned.column in key_names]
    scalar_plans = [
        planned
        for planned in key_plans
        if planned.declaration.value.operation not in WINDOW_OPERATIONS
    ]
    window_plans = [
        planned
        for planned in key_plans
        if planned.declaration.value.operation in WINDOW_OPERATIONS
    ]
    groups = {}
    order = []
    key_values = {}
    probes: list[CandidateRow] = []
    for driver_row in driver_rows:
        probe = CandidateRow(source_rows={driver: driver_row}, values={})
        for key_plan in scalar_plans:
            probe.values[key_plan.column] = _evaluate_one(
                key_plan,
                column_types,
                probe,
                context,
                dispatcher,
                counter,
                plan.specification.keys,
            )
        probes.append(probe)
    if window_plans:
        for position, probe in enumerate(probes):
            probe.output_position = position
        with _partition_scope(context, probes):
            for probe in probes:
                for key_plan in window_plans:
                    probe.values[key_plan.column] = _evaluate_one(
                        key_plan,
                        column_types,
                        probe,
                        context,
                        dispatcher,
                        counter,
                        plan.specification.keys,
                    )
    for position, probe in enumerate(probes):
        values = tuple(probe.values[planned.column] for planned in key_plans)
        if any(value is MISSING for value in values):
            token = ("__missing_key__", position)
        else:
            token = values
        if token not in groups:
            groups[token] = []
            order.append(token)
            key_values[token] = values
        groups[token].append(driver_rows[position])
    return key_values, groups, order


def _construct_rows(
    plan,
    context: RelationalContext,
    dispatcher: ExpressionDispatcher,
    counter: HandlerCounter,
) -> list[CandidateRow]:
    """Run R001's row construction phase, in specification order."""
    column_types = {column.name: column.type for column in plan.specification.columns}
    constructed: list[CandidateRow] = []
    for planned in plan.rows:
        relation = context.relations[planned.driver]
        if planned.declaration is None:
            # R001-12: with no template the key table is the output row set.
            constructed.extend(
                _key_grain_candidates(
                    plan, planned, relation, column_types, context, dispatcher, counter
                )
            )
            continue
        if planned.grouped:
            candidates = group_candidates(planned, relation)
        else:
            candidates = _record_candidates(planned, relation, context.bindings)
        for candidate in candidates:
            for derivation in planned.derivations:
                candidate.values[derivation.column] = _evaluate_one(
                    derivation,
                    column_types,
                    candidate,
                    context,
                    dispatcher,
                    counter,
                    plan.specification.keys,
                    row_phase=True,
                )
            if planned.grouped and not _grouped_filter(planned, candidate):
                continue
            constructed.append(candidate)
    for position, candidate in enumerate(constructed):
        # R001-9 fixes where each row was appended, which is the order a
        # window falls back to when its own terms tie.
        candidate.output_position = position
    context.rows.extend(constructed)
    return constructed


def _key_grain_candidates(
    plan,
    planned,
    relation: RelationIndex,
    column_types: Mapping[str, str],
    context: RelationalContext,
    dispatcher: ExpressionDispatcher,
    counter: HandlerCounter,
) -> list[CandidateRow]:
    """Build one candidate per key combination (R001-12).

    With no `rows` template the filter cannot scope feeding records, so every
    driver record of a key combination feeds its single row and a direct read
    must resolve to one value (R001-44) or the row fails.
    """
    key_names = list(plan.specification.keys)
    key_values, groups, order = _key_space(
        plan,
        [dict(record.values) for record in relation.records],
        planned.driver,
        column_types,
        context,
        dispatcher,
        counter,
    )
    candidates: list[CandidateRow] = []
    for token in order:
        records = groups[token]
        candidate = CandidateRow(
            source_rows={planned.driver: records[0]},
            values=dict(zip(key_names, key_values[token], strict=True)),
            feeding_rows={planned.driver: records},
        )
        for derivation in planned.derivations:
            candidate.values[derivation.column] = _evaluate_one(
                derivation,
                column_types,
                candidate,
                context,
                dispatcher,
                counter,
                plan.specification.keys,
                row_phase=True,
            )
        candidates.append(candidate)
    return candidates


def _run_column_checks(
    specification: Specification,
    candidates: Sequence[CandidateRow],
    completed: set[str],
    checked: set[str],
    warnings: list[VerificationFailure],
    hooks: ExecutionHooks,
) -> None:
    if not set(specification.keys) <= completed:
        return
    table = _table_from_candidates(specification, candidates, completed)
    for column in specification.columns:
        if column.name not in completed:
            break
        if column.name in checked:
            continue
        failures = hooks.column(table, column, specification.keys)
        warnings.extend(
            failure for failure in failures if failure.severity == "warning"
        )
        errors = [failure for failure in failures if failure.severity == "error"]
        if errors:
            raise _ExecutionAbort(_verification_diagnostics(errors))
        checked.add(column.name)


def _derive_columns(
    plan,
    candidates: list[CandidateRow],
    context: RelationalContext,
    dispatcher: ExpressionDispatcher,
    counter: HandlerCounter,
    hooks: ExecutionHooks,
    warnings: list[VerificationFailure],
) -> TypedTable:
    specification = plan.specification
    column_types = {column.name: column.type for column in specification.columns}
    key_set = set(specification.keys)
    # Key-grain mode (no `rows` template) seeds key values from the key table;
    # template mode derives every column per surviving driver record as before.
    key_grain = all(planned.declaration is None for planned in plan.rows)
    completed = set(plan.row_derived_columns)
    if key_grain:
        completed |= key_set
    checked: set[str] = set()
    constructed_count = len(candidates)
    _run_column_checks(specification, candidates, completed, checked, warnings, hooks)

    for derivation in plan.columns:
        if key_grain and derivation.column in key_set:
            continue
        values = [
            _evaluate_one(
                derivation,
                column_types,
                candidate,
                context,
                dispatcher,
                counter,
                specification.keys,
            )
            for candidate in candidates
        ]
        if len(values) != constructed_count or len(candidates) != constructed_count:
            raise _ExecutionAbort(
                [
                    ExecutionDiagnostic(
                        phase="derivation",
                        condition="row_count_changed",
                        spec_paths=(derivation.path,),
                        context={
                            "column": derivation.column,
                            "expected": constructed_count,
                            "actual": len(values),
                        },
                    )
                ]
            )
        for candidate, value in zip(candidates, values, strict=True):
            candidate.values[derivation.column] = value
        completed.add(derivation.column)
        _run_column_checks(
            specification, candidates, completed, checked, warnings, hooks
        )

    declared = {column.name for column in specification.columns}
    if completed != declared or any(
        set(candidate.values) != declared for candidate in candidates
    ):
        missing = sorted(declared - completed)
        raise _ExecutionAbort(
            [
                ExecutionDiagnostic(
                    phase="derivation",
                    condition="incomplete_output_row",
                    spec_paths=("columns",),
                    context={"missing": missing},
                )
            ]
        )
    table = _table_from_candidates(specification, candidates, completed)
    _run_column_checks(specification, candidates, completed, checked, warnings, hooks)
    return table


def execute_specification(
    specification: Specification,
    sources: Mapping[str, SourceTable],
    *,
    dispatcher: ExpressionDispatcher | None = None,
    hooks: ExecutionHooks | None = None,
) -> ExecutionResult:
    """Execute one normalized specification without reading golden artifacts."""
    selected_dispatcher = dispatcher or ExpressionDispatcher()
    selected_hooks = hooks or ExecutionHooks()
    counter = HandlerCounter()
    warnings: list[VerificationFailure] = []
    try:
        plan = plan_execution(
            specification,
            sources,
            supported_operations=selected_dispatcher.supported_operations,
        )
    except ExecutionPlanningError as error:
        return ExecutionFailure(
            diagnostics=error.diagnostics,
            handler_counts=counter.snapshot(),
        )
    except UnsupportedPlanningError as error:
        return ExecutionUnsupported(
            features=error.features,
            handler_counts=counter.snapshot(),
        )

    _register_handler_paths(plan, counter)
    try:
        relations = build_relation_indexes(sources)
        column_types = {column.name: column.type for column in specification.columns}
        key_correlation = _build_key_correlation(
            plan, sources, column_types, selected_dispatcher, counter
        )
        project_record = (
            _build_project_record(
                plan,
                sources,
                key_correlation,
                column_types,
                selected_dispatcher,
                counter,
            )
            if key_correlation is not None
            else None
        )
        context = RelationalContext(
            bindings=BindingIndex(plan.bindings, sources),
            relations=relations,
            lookups=RecordLookupSelector(plan.record_lookups, relations),
            output_keys=tuple(specification.keys),
            key_correlation=key_correlation,
            project_record=project_record,
        )
        candidates = _construct_rows(
            plan,
            context,
            selected_dispatcher,
            counter,
        )
        table = _derive_columns(
            plan,
            candidates,
            context,
            selected_dispatcher,
            counter,
            selected_hooks,
            warnings,
        )

        key_failures = selected_hooks.keys(table, specification.keys)
        if key_failures:
            raise _ExecutionAbort(_verification_diagnostics(key_failures))
        dataset_failures = selected_hooks.dataset(
            table,
            specification.verifications or (),
            specification.keys,
        )
        warnings.extend(
            failure for failure in dataset_failures if failure.severity == "warning"
        )
        dataset_errors = [
            failure for failure in dataset_failures if failure.severity == "error"
        ]
        if dataset_errors:
            raise _ExecutionAbort(_verification_diagnostics(dataset_errors))
        artifact = selected_hooks.output(
            table, specification.output, specification.keys
        )
        violation_log = build_violation_log(warnings, specification.output)
    except _ExecutionAbort as error:
        return ExecutionFailure(
            diagnostics=error.diagnostics,
            handler_counts=counter.snapshot(),
        )
    except LifecycleUnsupported as error:
        return ExecutionUnsupported(
            features=(error.feature,),
            handler_counts=counter.snapshot(),
        )
    except DeclarationError as error:
        return ExecutionFailure(
            diagnostics=(_declaration_diagnostic(error),),
            handler_counts=counter.snapshot(),
        )
    except ArtifactError as error:
        return ExecutionFailure(
            diagnostics=_artifact_diagnostics(error.diagnostics),
            handler_counts=counter.snapshot(),
        )

    return ExecutionSuccess(
        table=table,
        artifact=artifact,
        warnings=tuple(warnings),
        violation_log=violation_log,
        handler_counts=counter.snapshot(),
    )


def execute_with_source_provider(
    specification: Specification,
    source_provider: SourceProvider,
    *,
    dispatcher: ExpressionDispatcher | None = None,
    hooks: ExecutionHooks | None = None,
) -> ExecutionResult:
    """Preflight one specification, then obtain and execute its source tables."""
    selected_dispatcher = dispatcher or ExpressionDispatcher()
    try:
        preflight_execution(
            specification,
            supported_operations=selected_dispatcher.supported_operations,
        )
    except ExecutionPlanningError as error:
        return ExecutionFailure(diagnostics=error.diagnostics, handler_counts=())
    except UnsupportedPlanningError as error:
        return ExecutionUnsupported(features=error.features, handler_counts=())

    try:
        sources = source_provider(specification.datasets)
    except ProducerSchemaUnresolved as error:
        return ExecutionUnsupported(
            features=tuple(
                UnsupportedFeature(
                    operation="workflow_schema_resolution",
                    spec_path=f"datasets.{dataset}.schema",
                )
                for dataset in error.datasets
            ),
            handler_counts=(),
        )
    except SourceError as error:
        return ExecutionFailure(
            diagnostics=_source_diagnostics(error),
            handler_counts=(),
        )
    return execute_specification(
        specification,
        sources,
        dispatcher=selected_dispatcher,
        hooks=hooks,
    )
