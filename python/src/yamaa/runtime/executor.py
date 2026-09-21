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
from yamaa.runtime.intermediates import IntermediateSelector
from yamaa.runtime.joins import RelationIndex, build_relation_indexes
from yamaa.runtime.lifecycle import (
    HandlerCount,
    HandlerCounter,
    LifecycleCondition,
    LifecycleUnsupported,
    evaluate_derivation,
)
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
                implicit_joins=planned.implicit_joins,
                dispatcher=dispatcher,
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

    REQ-0038 runs it after every row derivation completes, so it corresponds to
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
    in first-appearance order (REQ-0042). Records with a missing key keep one
    entry each so the missing key still fails at the output gate.
    """
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
    groups: dict[_KeyToken, list[dict[str, object]]] = {}
    order: list[_KeyToken] = []
    key_values: dict[_KeyToken, tuple[object, ...]] = {}
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
            token: _KeyToken = ("__missing_key__", position)
        else:
            token = tuple(values)
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
    # REQ-0039 fixes where each row was appended, which is the order a
    # window falls back to when its own terms tie. Positions are assigned as
    # rows are appended, so a template's window pass (REQ-0326) sees them
    # before its grouped filter drops rows.
    position = 0
    for planned in plan.rows:
        relation = context.relations[planned.driver]
        if planned.declaration is None:
            # REQ-0042: with no template the key table is the output row set.
            new_rows = _key_grain_candidates(
                plan, planned, relation, column_types, context, dispatcher, counter
            )
            for candidate in new_rows:
                candidate.output_position = position
                position += 1
            constructed.extend(new_rows)
            continue
        if planned.grouped:
            candidates = group_candidates(planned, relation)
        else:
            candidates = _record_candidates(planned, relation, context.bindings)
        window_columns = {
            derivation.column
            for derivation in planned.derivations
            if derivation.declaration.value.operation in WINDOW_OPERATIONS
        }
        # A derivation that transitively reads a window result evaluates
        # after the window pass; planning (REQ-0326) already rejected a
        # window that reads one.
        deferred_columns = set(window_columns)
        changed = True
        while changed:
            changed = False
            for derivation in planned.derivations:
                if derivation.column not in deferred_columns and any(
                    dependency in deferred_columns
                    for dependency in derivation.dependencies
                ):
                    deferred_columns.add(derivation.column)
                    changed = True
        immediate = [
            derivation
            for derivation in planned.derivations
            if derivation.column not in deferred_columns
        ]
        window_plans = [
            derivation
            for derivation in planned.derivations
            if derivation.column in window_columns
        ]
        tail = [
            derivation
            for derivation in planned.derivations
            if derivation.column in deferred_columns
            and derivation.column not in window_columns
        ]
        staged: list[CandidateRow] = []
        for candidate in candidates:
            for derivation in immediate:
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
            staged.append(candidate)
        if window_plans:
            # REQ-0326: a template's windows partition the rows the template
            # constructs. Positions are fixed first so the REQ-0301
            # tie-break sees construction order, then the partition scope
            # exposes exactly this template's rows to the window resolver --
            # the same shape _key_space uses for windows over keys.
            for index, candidate in enumerate(staged):
                candidate.output_position = position + index
            with _partition_scope(context, staged):
                for derivation in window_plans:
                    for candidate in staged:
                        candidate.values[derivation.column] = _evaluate_one(
                            derivation,
                            column_types,
                            candidate,
                            context,
                            dispatcher,
                            counter,
                            plan.specification.keys,
                        )
            for candidate in staged:
                for derivation in tail:
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
        for candidate in staged:
            if planned.grouped and not _grouped_filter(planned, candidate):
                continue
            candidate.output_position = position
            position += 1
            constructed.append(candidate)
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
    """Build one candidate per key combination (REQ-0042).

    With no `rows` template a root `filter` scopes the feeding records
    before the distinct-keys step (REQ-1170); every retained driver record
    of a key combination feeds its single row and a direct read must resolve
    to one value (REQ-0075) or the row fails.
    """
    key_names = list(plan.specification.keys)
    driver_rows = [dict(record.values) for record in relation.records]
    if planned.filter_predicate is not None:
        driver_rows = [
            values
            for values in driver_rows
            if _evaluate_row_filter(planned, context.bindings, values)
        ]
    key_values, groups, order = _key_space(
        plan,
        driver_rows,
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
    # Key mode (no `rows` template) seeds key values from the key table;
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
        context = RelationalContext(
            bindings=BindingIndex(plan.bindings, sources),
            relations=relations,
            intermediates=IntermediateSelector(
                plan.intermediates, relations, selected_dispatcher.evaluate
            ),
            output_keys=tuple(specification.keys),
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
        sources = source_provider(specification.input)
    except ProducerSchemaUnresolved as error:
        return ExecutionUnsupported(
            features=tuple(
                UnsupportedFeature(
                    operation="workflow_schema_resolution",
                    spec_path=f"input.{dataset}.schema",
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
