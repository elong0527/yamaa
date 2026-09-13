"""Execute the initial record-driven R001 slice over typed source tables."""

from __future__ import annotations

from collections.abc import Callable, Mapping, Sequence
from dataclasses import dataclass
from typing import Literal, TypeAlias

from pydantic import BaseModel, ConfigDict, Field, JsonValue

from yamaa.expressions import (
    ExpressionDispatcher,
    PredicateValue,
    TruthValue,
    evaluate_predicate,
)
from yamaa.io import Artifact, ArtifactDiagnostic, ArtifactError, build_artifact
from yamaa.io.polars import frame_from_values, runtime_rows
from yamaa.io.source import LoadedDataset, SourceError
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
from yamaa.runtime.lifecycle import (
    HandlerCount,
    HandlerCounter,
    LifecycleCondition,
    LifecycleUnsupported,
    evaluate_derivation,
)
from yamaa.specification.models import Column, DatasetSource, Specification
from yamaa.verification import (
    DeclarationError,
    VerificationFailure,
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


class _FrozenModel(BaseModel):
    model_config = ConfigDict(
        strict=True,
        extra="forbid",
        frozen=True,
        arbitrary_types_allowed=True,
    )


class ExecutionSuccess(_FrozenModel):
    """A completed typed table and its ordered, selected output artifact."""

    status: Literal["success"] = "success"
    table: TypedTable
    artifact: Artifact
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


@dataclass(slots=True)
class _CandidateRow:
    source_rows: dict[str, dict[str, object]]
    values: dict[str, object]


class _ExecutionAbort(ValueError):
    def __init__(self, diagnostics: Sequence[ExecutionDiagnostic]) -> None:
        self.diagnostics = tuple(diagnostics)
        super().__init__(", ".join(item.condition for item in diagnostics))


def _typed_table(value: SourceTable) -> TypedTable:
    return value.table if isinstance(value, LoadedDataset) else value


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
        context=context,
    )


def _table_from_candidates(
    specification: Specification,
    candidates: Sequence[_CandidateRow],
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
_ROW_PHASES = frozenset({"derivation", "mapping", "row_construction", "convert"})


def _offending_keys(
    candidate: _CandidateRow,
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
    candidate: _CandidateRow,
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
    candidate: _CandidateRow,
    index: BindingIndex,
    dispatcher: ExpressionDispatcher,
    counter: HandlerCounter,
    keys: Sequence[str] = (),
) -> object:
    try:
        return evaluate_derivation(
            planned,
            column_types[planned.column],  # type: ignore[arg-type]
            candidate.values,
            lambda values: index.context(candidate.source_rows, values),
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


def _construct_rows(
    plan,
    sources: Mapping[str, SourceTable],
    index: BindingIndex,
    dispatcher: ExpressionDispatcher,
    counter: HandlerCounter,
) -> list[_CandidateRow]:
    column_types = {column.name: column.type for column in plan.specification.columns}
    source_rows = {
        dataset: runtime_rows(_typed_table(source))
        for dataset, source in sources.items()
    }
    candidates: list[_CandidateRow] = []
    for planned in plan.rows:
        for driver_row in source_rows[planned.driver]:
            if not _evaluate_row_filter(planned, index, driver_row):
                continue
            candidate = _CandidateRow(
                source_rows={planned.driver: driver_row},
                values={},
            )
            for derivation in planned.derivations:
                candidate.values[derivation.column] = _evaluate_one(
                    derivation,
                    column_types,
                    candidate,
                    index,
                    dispatcher,
                    counter,
                    plan.specification.keys,
                )
            candidates.append(candidate)
    return candidates


def _run_column_checks(
    specification: Specification,
    candidates: Sequence[_CandidateRow],
    completed: set[str],
    checked: set[str],
    hooks: ExecutionHooks,
) -> None:
    if not set(specification.keys) <= completed:
        return
    table = _table_from_candidates(specification, candidates, completed)
    for column in specification.columns:
        if column.name not in completed or column.name in checked:
            continue
        failures = hooks.column(table, column, specification.keys)
        if failures:
            raise _ExecutionAbort(_verification_diagnostics(failures))
        checked.add(column.name)


def _derive_columns(
    plan,
    candidates: list[_CandidateRow],
    index: BindingIndex,
    dispatcher: ExpressionDispatcher,
    counter: HandlerCounter,
    hooks: ExecutionHooks,
) -> TypedTable:
    specification = plan.specification
    column_types = {column.name: column.type for column in specification.columns}
    completed = set(plan.row_derived_columns)
    checked: set[str] = set()
    constructed_count = len(candidates)
    _run_column_checks(specification, candidates, completed, checked, hooks)

    for derivation in plan.columns:
        values = [
            _evaluate_one(
                derivation,
                column_types,
                candidate,
                index,
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
        _run_column_checks(specification, candidates, completed, checked, hooks)

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
    _run_column_checks(specification, candidates, completed, checked, hooks)
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
        index = BindingIndex(plan.bindings, sources)
        candidates = _construct_rows(
            plan,
            sources,
            index,
            selected_dispatcher,
            counter,
        )
        table = _derive_columns(
            plan,
            candidates,
            index,
            selected_dispatcher,
            counter,
            selected_hooks,
        )

        key_failures = selected_hooks.keys(table, specification.keys)
        if key_failures:
            raise _ExecutionAbort(_verification_diagnostics(key_failures))
        dataset_failures = selected_hooks.dataset(
            table,
            specification.verifications or (),
            specification.keys,
        )
        if dataset_failures:
            raise _ExecutionAbort(_verification_diagnostics(dataset_failures))
        artifact = selected_hooks.output(
            table, specification.output, specification.keys
        )
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
