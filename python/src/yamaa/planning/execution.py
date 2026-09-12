"""Build a deterministic plan for the initial R001 execution subset."""

from __future__ import annotations

from collections.abc import Collection, Mapping, Sequence
from dataclasses import dataclass
from typing import Any

from pydantic import BaseModel, ConfigDict, Field, JsonValue

from yamaa.expressions import (
    NumericError,
    PredicateAst,
    PredicateError,
    TemplateError,
    numeric_identifiers,
    parse_numeric_cached,
    parse_predicate,
    parse_template_cached,
    template_identifiers,
)
from yamaa.io.source import LoadedDataset
from yamaa.models import ColumnType, ConditionPhase, TypedTable
from yamaa.odm import BindingFailure, BindingPlan, BoundReference, build_binding_plan
from yamaa.specification.models import (
    Expression,
    HandledExpression,
    Row,
    Specification,
)

INITIAL_OPERATIONS = ("source", "literal", "mapping")


class _FrozenModel(BaseModel):
    model_config = ConfigDict(
        strict=True,
        extra="forbid",
        frozen=True,
        arbitrary_types_allowed=True,
    )


class ExecutionDiagnostic(_FrozenModel):
    """Portable identity and context for an execution failure."""

    phase: ConditionPhase
    condition: str = Field(min_length=1)
    spec_paths: tuple[str, ...] = Field(min_length=1)
    # The committed error contracts name the requirement each failure is
    # reported against; a failure that has not been given one omits it.
    requirement: str | None = Field(default=None, pattern=r"^R[0-9]{3}-[0-9]+$")
    context: dict[str, JsonValue]


class UnsupportedFeature(_FrozenModel):
    """One valid declaration outside the initial executor subset."""

    operation: str = Field(min_length=1)
    spec_path: str = Field(min_length=1)


class PlannedDerivation(_FrozenModel):
    """One derivation with its dependencies and parsed override predicates."""

    column: str = Field(min_length=1)
    path: str = Field(min_length=1)
    expression_path: str = Field(min_length=1)
    declaration: HandledExpression
    dependencies: tuple[str, ...]
    override_predicates: tuple[dict[str, Any], ...]


class PlannedRow(_FrozenModel):
    """One explicit or implicit record-driven row template."""

    index: int | None
    declaration: Row | None
    driver: str = Field(min_length=1)
    filter_path: str | None = None
    filter_predicate: dict[str, Any] | None = None
    derivations: tuple[PlannedDerivation, ...] = ()


class ExecutionPlan(_FrozenModel):
    """Validated work in the exact order the initial executor will perform it."""

    specification: Specification
    bindings: BindingPlan
    rows: tuple[PlannedRow, ...]
    columns: tuple[PlannedDerivation, ...]
    row_derived_columns: tuple[str, ...]


class ExecutionPlanningError(ValueError):
    """Raised when a normalized specification has invalid execution semantics."""

    def __init__(self, diagnostics: Sequence[ExecutionDiagnostic]) -> None:
        if not diagnostics:
            raise ValueError("ExecutionPlanningError requires diagnostics")
        self.diagnostics = tuple(diagnostics)
        conditions = ", ".join(item.condition for item in diagnostics)
        super().__init__(f"execution planning failed: {conditions}")


class UnsupportedPlanningError(ValueError):
    """Raised when a valid plan needs operations this component does not own."""

    def __init__(self, features: Sequence[UnsupportedFeature]) -> None:
        if not features:
            raise ValueError("UnsupportedPlanningError requires features")
        self.features = tuple(features)
        operations = ", ".join(item.operation for item in features)
        super().__init__(f"execution is unsupported: {operations}")


@dataclass(frozen=True, slots=True)
class _Reference:
    name: str
    path: str
    expected_type: ColumnType | None = None
    current_value_available: bool = False
    # The R007 requirement the owning operation's input type is held to.
    requirement: str | None = None


@dataclass(frozen=True, slots=True)
class _ExpressionInfo:
    references: tuple[_Reference, ...]
    unsupported: tuple[UnsupportedFeature, ...]
    diagnostics: tuple[ExecutionDiagnostic, ...] = ()


def _diagnostic(
    condition: str,
    spec_path: str | Sequence[str],
    context: dict[str, JsonValue],
    *,
    phase: ConditionPhase = "validation",
    requirement: str | None = None,
) -> ExecutionDiagnostic:
    paths = (spec_path,) if isinstance(spec_path, str) else tuple(spec_path)
    return ExecutionDiagnostic(
        phase=phase,
        condition=condition,
        spec_paths=paths,
        requirement=requirement,
        context=context,
    )


def _expression_path(path: str, derivation: HandledExpression) -> str:
    """Recover the authored bare-expression path where normalization permits it."""
    handled = {"conversion_failure", "override"} & derivation.model_fields_set
    return f"{path}.value" if handled else path


def _deduplicate_references(references: Sequence[_Reference]) -> tuple[_Reference, ...]:
    seen: set[tuple[str, str, ColumnType | None, bool, str | None]] = set()
    ordered: list[_Reference] = []
    for reference in references:
        identity = (
            reference.name,
            reference.path,
            reference.expected_type,
            reference.current_value_available,
            reference.requirement,
        )
        if identity not in seen:
            ordered.append(reference)
            seen.add(identity)
    return tuple(ordered)


# The operations whose `source` names one variable of a stated input type.
_TYPED_SOURCES: dict[str, tuple[ColumnType | None, str]] = {
    "mapping": ("str", "R007-20"),
    "str_extract": ("str", "R007-24"),
    "str_upper": ("str", "R007-24"),
    "str_lower": ("str", "R007-24"),
    "cut": (None, "R007-22"),
}


def _expression_info(
    expression: Expression,
    path: str,
    supported_operations: Collection[str],
    *,
    column_phase: bool = True,
) -> _ExpressionInfo:
    operation = expression.operation
    operation_path = f"{path}.{operation}"
    if operation not in supported_operations:
        return _ExpressionInfo(
            references=(),
            unsupported=(
                UnsupportedFeature(operation=operation, spec_path=operation_path),
            ),
        )

    payload = expression.root[operation]
    references: list[_Reference] = []
    unsupported: list[UnsupportedFeature] = []
    diagnostics: list[ExecutionDiagnostic] = []

    def nest(nested: object, nested_path: str) -> None:
        """Collect one expression R007-3 permits this operation to nest."""
        if not isinstance(nested, Mapping) or len(nested) != 1:
            return
        info = _expression_info(
            Expression.model_validate(dict(nested)),
            nested_path,
            supported_operations,
            column_phase=column_phase,
        )
        references.extend(info.references)
        unsupported.extend(info.unsupported)
        diagnostics.extend(info.diagnostics)

    if operation == "source":
        variable = payload if isinstance(payload, str) else payload.get("variable")
        if isinstance(variable, str):
            references.append(_Reference(variable, operation_path))
    elif operation in _TYPED_SOURCES and isinstance(payload, Mapping):
        variable = payload.get("source")
        expected, requirement = _TYPED_SOURCES[operation]
        if isinstance(variable, str):
            references.append(
                _Reference(
                    variable,
                    f"{operation_path}.source",
                    expected,
                    requirement=requirement,
                )
            )
    elif operation in {"coalesce", "greatest", "least"} and isinstance(
        payload, Mapping
    ):
        sources = payload.get("sources")
        if isinstance(sources, Sequence) and not isinstance(sources, str):
            references.extend(
                _Reference(name, f"{operation_path}.sources[{index}]")
                for index, name in enumerate(sources)
                if isinstance(name, str)
            )
    elif operation == "compute" and isinstance(payload, Mapping):
        diagnostics.extend(
            _compute_references(payload, operation_path, references, column_phase)
        )
    elif operation == "str_template":
        template = payload if isinstance(payload, str) else None
        if isinstance(payload, Mapping):
            template = payload.get("template")
        if isinstance(template, str):
            diagnostics.extend(
                _template_references(template, operation_path, references)
            )
    elif operation == "str_concat" and isinstance(payload, Mapping):
        sources = payload.get("sources")
        if isinstance(sources, Sequence) and not isinstance(sources, str):
            for index, nested in enumerate(sources):
                nest(nested, f"{operation_path}.sources[{index}]")
    elif operation == "case" and isinstance(payload, Mapping):
        branches = payload.get("branches")
        if isinstance(branches, Sequence) and not isinstance(branches, str):
            for index, branch in enumerate(branches):
                if not isinstance(branch, Mapping):
                    continue
                branch_path = f"{operation_path}.branches[{index}]"
                when = branch.get("when")
                if isinstance(when, str):
                    ast = _parse_predicate_at(when, f"{branch_path}.when", diagnostics)
                    if ast is not None:
                        references.extend(
                            _Reference(name, f"{branch_path}.when")
                            for name in _predicate_identifiers(ast)
                        )
                nest(branch.get("then"), f"{branch_path}.then")
        if "otherwise" in payload:
            nest(payload["otherwise"], f"{operation_path}.otherwise")

    return _ExpressionInfo(
        references=_deduplicate_references(references),
        unsupported=tuple(unsupported),
        diagnostics=tuple(diagnostics),
    )


def _compute_references(
    payload: Mapping[str, object],
    operation_path: str,
    references: list[_Reference],
    column_phase: bool,
) -> list[ExecutionDiagnostic]:
    """Collect R010 identifiers, or report why the formula cannot be read."""
    expr = payload.get("expr")
    if not isinstance(expr, str):
        return []
    expr_path = f"{operation_path}.expr"
    try:
        ast = parse_numeric_cached(expr)
    except NumericError as error:
        return [
            ExecutionDiagnostic(
                phase="validation",
                condition=error.condition,
                spec_paths=(expr_path,),
                requirement=error.requirement,
                context={"expr": expr, **error.context},
            )
        ]
    diagnostics: list[ExecutionDiagnostic] = []
    for name in numeric_identifiers(ast):
        if column_phase and "." in name:
            # R010-3: only a declared R015 record lookup may qualify a name
            # during column derivation, and no lookup executes yet.
            diagnostics.append(
                ExecutionDiagnostic(
                    phase="validation",
                    condition="qualified_identifier",
                    spec_paths=(expr_path,),
                    requirement="R010-38",
                    context={"expr": expr, "identifier": name},
                )
            )
            continue
        references.append(_Reference(name, expr_path))
    return diagnostics


def _template_references(
    template: str,
    operation_path: str,
    references: list[_Reference],
) -> list[ExecutionDiagnostic]:
    """Collect R012 placeholders, or report a template outside the grammar."""
    try:
        parts = parse_template_cached(template)
    except TemplateError as error:
        return [
            ExecutionDiagnostic(
                phase="validation",
                condition=error.condition,
                spec_paths=(operation_path,),
                requirement=error.requirement,
                context=error.context,
            )
        ]
    references.extend(
        _Reference(name, f"{operation_path}.template", "str", requirement="R007-24")
        for name in template_identifiers(parts)
    )
    return []


def _predicate_identifiers(ast: PredicateAst) -> tuple[str, ...]:
    names: list[str] = []

    def visit(value: object) -> None:
        if isinstance(value, Mapping):
            if value.get("kind") == "identifier" and isinstance(value.get("name"), str):
                names.append(value["name"])
            for nested in value.values():
                visit(nested)
        elif isinstance(value, list):
            for nested in value:
                visit(nested)

    visit(ast)
    return tuple(dict.fromkeys(names))


def _parse_predicate_at(
    text: str,
    path: str,
    diagnostics: list[ExecutionDiagnostic],
) -> PredicateAst | None:
    try:
        return parse_predicate(text)
    except PredicateError as error:
        diagnostics.append(
            _diagnostic(
                "invalid_predicate",
                path,
                {"predicate": text, "position": error.position},
                requirement="R004-31",
            )
        )
        return None


def _plan_derivation(
    column: str,
    declaration: HandledExpression,
    path: str,
    supported_operations: Collection[str],
    diagnostics: list[ExecutionDiagnostic],
    unsupported: list[UnsupportedFeature],
    *,
    column_phase: bool = True,
) -> tuple[PlannedDerivation, tuple[_Reference, ...]]:
    value_path = _expression_path(path, declaration)
    info = _expression_info(
        declaration.value,
        value_path,
        supported_operations,
        column_phase=column_phase,
    )
    references = list(info.references)
    unsupported.extend(info.unsupported)
    diagnostics.extend(info.diagnostics)

    override_predicates: list[PredicateAst] = []
    for index, override in enumerate(declaration.override or ()):
        override_path = f"{path}.override[{index}]"
        ast = _parse_predicate_at(override.when, f"{override_path}.when", diagnostics)
        override_predicates.append(ast or {})
        if ast is not None:
            references.extend(
                _Reference(
                    name,
                    f"{override_path}.when",
                    current_value_available=True,
                )
                for name in _predicate_identifiers(ast)
            )
        override_info = _expression_info(
            override.value,
            f"{override_path}.value",
            supported_operations,
            column_phase=column_phase,
        )
        diagnostics.extend(override_info.diagnostics)
        references.extend(
            _Reference(
                reference.name,
                reference.path,
                reference.expected_type,
                current_value_available=True,
            )
            for reference in override_info.references
        )
        unsupported.extend(override_info.unsupported)

    ordered_references = _deduplicate_references(references)
    dependencies = tuple(
        dict.fromkeys(
            reference.name
            for reference in ordered_references
            if "." not in reference.name
            and not (reference.current_value_available and reference.name == column)
        )
    )
    return (
        PlannedDerivation(
            column=column,
            path=path,
            expression_path=value_path,
            declaration=declaration,
            dependencies=dependencies,
            override_predicates=tuple(override_predicates),
        ),
        ordered_references,
    )


def _bound_type(
    bound: BoundReference,
    bindings: BindingPlan,
    column_types: Mapping[str, ColumnType],
) -> ColumnType | None:
    if bound.kind == "output":
        assert bound.field is not None
        return column_types.get(bound.field)
    assert bound.dataset is not None
    dataset = bindings.datasets[bound.dataset]
    field = "Value" if bound.kind == "odm_item" else bound.field
    return next(
        (column.type for column in dataset.columns if column.name == field),
        None,
    )


def _validate_qualified_reference(
    reference: _Reference,
    drivers: Collection[str],
    bindings: BindingPlan,
    column_types: Mapping[str, ColumnType],
    diagnostics: list[ExecutionDiagnostic],
    unsupported: list[UnsupportedFeature],
    *,
    deferred_qualifiers: Collection[str] = (),
) -> None:
    qualifier = reference.name.split(".", 1)[0]
    if qualifier in deferred_qualifiers:
        return
    bound = bindings.bind(reference.name)
    if isinstance(bound, BindingFailure):
        diagnostics.append(
            _diagnostic("unknown_field", reference.path, {"identifier": reference.name})
        )
        return
    if bound.kind == "output":
        raise AssertionError("a qualified name cannot bind as an output column")
    assert bound.dataset is not None
    if any(bound.dataset != driver for driver in drivers):
        unsupported.append(
            UnsupportedFeature(
                operation="cross_dataset_source",
                spec_path=reference.path,
            )
        )
    actual = _bound_type(bound, bindings, column_types)
    if reference.expected_type is not None and actual != reference.expected_type:
        diagnostics.append(
            _diagnostic(
                "incompatible_input_type",
                reference.path,
                {
                    "source": reference.name,
                    "expected": reference.expected_type,
                    "actual": actual,
                },
                requirement=reference.requirement,
            )
        )


def _find_cycle(
    names: Sequence[str],
    dependencies: Mapping[str, Collection[str]],
) -> tuple[str, ...] | None:
    position = {name: index for index, name in enumerate(names)}
    state: dict[str, str] = {}
    stack: list[str] = []

    def visit(name: str) -> tuple[str, ...] | None:
        state[name] = "active"
        stack.append(name)
        for dependency in sorted(
            dependencies.get(name, ()),
            key=lambda item: position.get(item, len(position)),
        ):
            if dependency not in dependencies:
                continue
            if state.get(dependency) == "active":
                start = stack.index(dependency)
                return (*stack[start:], dependency)
            if dependency not in state:
                cycle = visit(dependency)
                if cycle is not None:
                    return cycle
        stack.pop()
        state[name] = "complete"
        return None

    for name in names:
        if name not in state:
            cycle = visit(name)
            if cycle is not None:
                return cycle
    return None


def _topological_row_order(
    derivations: Mapping[str, PlannedDerivation],
    column_order: Sequence[str],
) -> tuple[PlannedDerivation, ...]:
    positions = {name: index for index, name in enumerate(column_order)}
    remaining = set(derivations)
    ordered: list[PlannedDerivation] = []
    while remaining:
        ready = [
            name
            for name in remaining
            if not (set(derivations[name].dependencies) & remaining)
        ]
        if not ready:
            break
        selected = min(ready, key=lambda name: positions[name])
        ordered.append(derivations[selected])
        remaining.remove(selected)
    return tuple(ordered)


def _coverage_diagnostics(specification: Specification) -> list[ExecutionDiagnostic]:
    diagnostics: list[ExecutionDiagnostic] = []
    rows = specification.rows or ()
    declared = {column.name for column in specification.columns}

    for index, row in enumerate(rows):
        for name in row.derivations:
            if name not in declared:
                diagnostics.append(
                    _diagnostic(
                        "undeclared_column",
                        f"rows[{index}].derivations.{name}",
                        {"column": name},
                    )
                )

    for column in specification.columns:
        at_column = column.derivation is not None
        at_rows = [column.name in row.derivations for row in rows]
        path = f"columns.{column.name}.derivation"
        if at_column and any(at_rows):
            diagnostics.append(
                _diagnostic(
                    "duplicate_derivation",
                    path,
                    {"column": column.name},
                )
            )
        elif rows and not at_column and not all(at_rows):
            diagnostics.append(
                _diagnostic(
                    "missing_derivation",
                    path,
                    {
                        "column": column.name,
                        "rows": [
                            row.id for row, present in zip(rows, at_rows) if not present
                        ],
                    },
                )
            )
        elif not rows and not at_column:
            diagnostics.append(
                _diagnostic(
                    "missing_derivation",
                    path,
                    {"column": column.name},
                )
            )
    return diagnostics


def _unique_features(
    features: Sequence[UnsupportedFeature],
) -> list[UnsupportedFeature]:
    seen: set[tuple[str, str]] = set()
    ordered: list[UnsupportedFeature] = []
    for feature in features:
        identity = (feature.operation, feature.spec_path)
        if identity not in seen:
            ordered.append(feature)
            seen.add(identity)
    return ordered


def _preflight_findings(
    specification: Specification,
    supported_operations: Collection[str],
) -> tuple[list[ExecutionDiagnostic], list[UnsupportedFeature]]:
    """Find source-independent failures before any dataset is ingested."""
    diagnostics = [] if specification.parents else _coverage_diagnostics(specification)
    unsupported: list[UnsupportedFeature] = []

    if specification.domain in specification.datasets:
        diagnostics.append(
            _diagnostic(
                "duplicate_identifier",
                (f"datasets.{specification.domain}", "domain"),
                {"identifier": specification.domain},
            )
        )
    if specification.parents:
        unsupported.append(
            UnsupportedFeature(operation="inheritance", spec_path="parents")
        )
    if specification.record_lookups:
        unsupported.extend(
            UnsupportedFeature(
                operation="record_lookup",
                spec_path=f"record_lookups[{index}]",
            )
            for index, _ in enumerate(specification.record_lookups)
        )

    rows = specification.rows or ()
    if not specification.parents:
        if not rows and specification.default_driver is None:
            diagnostics.append(_diagnostic("driver_unavailable", "base", {"row": None}))
        if (
            specification.base is not None
            and specification.base not in specification.datasets
        ):
            diagnostics.append(
                _diagnostic(
                    "driver_unavailable",
                    "base",
                    {"dataset": specification.base},
                )
            )

    for index, row in enumerate(rows):
        if not specification.parents:
            driver = row.dataset or specification.default_driver
            if driver is None or driver not in specification.datasets:
                diagnostics.append(
                    _diagnostic(
                        "driver_unavailable",
                        f"rows[{index}].dataset",
                        {"row": row.id, "dataset": driver},
                    )
                )
        if row.group_by is not None:
            unsupported.append(
                UnsupportedFeature(
                    operation="grouped_rows",
                    spec_path=f"rows[{index}].group_by",
                )
            )
        for name, declaration in row.derivations.items():
            path = f"rows[{index}].derivations.{name}"
            unsupported.extend(
                _expression_info(
                    declaration.value,
                    _expression_path(path, declaration),
                    supported_operations,
                ).unsupported
            )
            for override_index, override in enumerate(declaration.override or ()):
                unsupported.extend(
                    _expression_info(
                        override.value,
                        f"{path}.override[{override_index}].value",
                        supported_operations,
                    ).unsupported
                )

    for column in specification.columns:
        declaration = column.derivation
        if declaration is None:
            continue
        path = f"columns.{column.name}.derivation"
        unsupported.extend(
            _expression_info(
                declaration.value,
                _expression_path(path, declaration),
                supported_operations,
            ).unsupported
        )
        for override_index, override in enumerate(declaration.override or ()):
            unsupported.extend(
                _expression_info(
                    override.value,
                    f"{path}.override[{override_index}].value",
                    supported_operations,
                ).unsupported
            )

    return diagnostics, _unique_features(unsupported)


def preflight_execution(
    specification: Specification,
    *,
    supported_operations: Collection[str] = INITIAL_OPERATIONS,
) -> None:
    """Reject source-independent failures before a source provider is called."""
    diagnostics, unsupported = _preflight_findings(specification, supported_operations)
    if diagnostics:
        raise ExecutionPlanningError(diagnostics)
    if unsupported:
        raise UnsupportedPlanningError(unsupported)


def plan_execution(
    specification: Specification,
    sources: Mapping[str, LoadedDataset | TypedTable],
    *,
    supported_operations: Collection[str] = INITIAL_OPERATIONS,
) -> ExecutionPlan:
    """Validate and plan the initial record-driven execution subset.

    Validation failures take precedence over unsupported status. This keeps a
    broken dependency or phase boundary distinct from a valid declaration that
    belongs to a later runtime component.
    """
    diagnostics, unsupported = _preflight_findings(specification, supported_operations)
    declared_sources = tuple(specification.datasets)
    supplied_sources = tuple(sources)
    if set(declared_sources) != set(supplied_sources):
        diagnostics.append(
            _diagnostic(
                "source_provider_mismatch",
                "datasets",
                {
                    "missing": sorted(set(declared_sources) - set(supplied_sources)),
                    "unexpected": sorted(set(supplied_sources) - set(declared_sources)),
                },
            )
        )

    rows = specification.rows or ()
    if diagnostics and set(declared_sources) != set(supplied_sources):
        raise ExecutionPlanningError(diagnostics)

    try:
        bindings = build_binding_plan(specification, sources)
    except ValueError as error:
        diagnostics.append(
            _diagnostic(
                "source_provider_mismatch",
                "datasets",
                {"reason": str(error)},
            )
        )
        raise ExecutionPlanningError(diagnostics) from error

    column_order = [column.name for column in specification.columns]
    column_positions = {name: index for index, name in enumerate(column_order)}
    column_types = {column.name: column.type for column in specification.columns}
    deferred_qualifiers = {lookup.id for lookup in specification.record_lookups or ()}
    row_plans: list[PlannedRow] = []
    row_references: dict[tuple[int, str], tuple[_Reference, ...]] = {}

    if not rows:
        if (
            specification.default_driver is not None
            and specification.default_driver in specification.datasets
        ):
            row_plans.append(
                PlannedRow(
                    index=None, declaration=None, driver=specification.default_driver
                )
            )
    else:
        for index, row in enumerate(rows):
            if row.group_by is not None:
                continue
            driver = row.dataset or specification.default_driver
            if driver is None or driver not in specification.datasets:
                continue
            filter_path = f"rows[{index}].filter" if row.filter is not None else None
            filter_ast = (
                _parse_predicate_at(row.filter, filter_path, diagnostics)
                if row.filter is not None and filter_path is not None
                else None
            )
            if filter_ast is not None:
                for identifier in _predicate_identifiers(filter_ast):
                    if "." not in identifier:
                        diagnostics.append(
                            _diagnostic(
                                "phase_boundary",
                                filter_path or f"rows[{index}].filter",
                                {
                                    "identifier": identifier,
                                    "row": row.id,
                                    "available_phase": "column_derivation",
                                    "required_phase": "row_filter",
                                },
                            )
                        )
                    else:
                        _validate_qualified_reference(
                            _Reference(
                                identifier, filter_path or f"rows[{index}].filter"
                            ),
                            {driver},
                            bindings,
                            column_types,
                            diagnostics,
                            unsupported,
                        )

            derivations: dict[str, PlannedDerivation] = {}
            for name in column_order:
                declaration = row.derivations.get(name)
                if declaration is None:
                    continue
                planned, references = _plan_derivation(
                    name,
                    declaration,
                    f"rows[{index}].derivations.{name}",
                    supported_operations,
                    diagnostics,
                    unsupported,
                    column_phase=False,
                )
                derivations[name] = planned
                row_references[(index, name)] = references

            row_names = set(derivations)
            for name, planned in derivations.items():
                for reference in row_references[(index, name)]:
                    if "." in reference.name:
                        _validate_qualified_reference(
                            reference,
                            {driver},
                            bindings,
                            column_types,
                            diagnostics,
                            unsupported,
                            deferred_qualifiers=deferred_qualifiers,
                        )
                    elif reference.name not in column_types:
                        diagnostics.append(
                            _diagnostic(
                                "unknown_field",
                                reference.path,
                                {"identifier": reference.name},
                            )
                        )
                    elif reference.name not in row_names:
                        diagnostics.append(
                            _diagnostic(
                                "phase_boundary",
                                reference.path,
                                {
                                    "identifier": reference.name,
                                    "row": row.id,
                                    "available_phase": "column_derivation",
                                    "required_phase": "row_construction",
                                },
                            )
                        )
                    elif (
                        reference.expected_type is not None
                        and column_types[reference.name] != reference.expected_type
                    ):
                        diagnostics.append(
                            _diagnostic(
                                "incompatible_input_type",
                                reference.path,
                                {
                                    "source": reference.name,
                                    "expected": reference.expected_type,
                                    "actual": column_types[reference.name],
                                },
                                requirement=reference.requirement,
                            )
                        )

            graph = {
                name: tuple(
                    dependency
                    for dependency in planned.dependencies
                    if dependency in derivations
                )
                for name, planned in derivations.items()
            }
            cycle = _find_cycle(
                [name for name in column_order if name in derivations], graph
            )
            if cycle is not None:
                paths = tuple(
                    dict.fromkeys(
                        derivations[name].expression_path for name in cycle[:-1]
                    )
                )
                diagnostics.append(
                    _diagnostic(
                        "dependency_cycle",
                        paths,
                        {"cycle": list(cycle)},
                        requirement="R001-41",
                    )
                )
            ordered = _topological_row_order(derivations, column_order)
            row_plans.append(
                PlannedRow(
                    index=index,
                    declaration=row,
                    driver=driver,
                    filter_path=filter_path,
                    filter_predicate=filter_ast,
                    derivations=ordered,
                )
            )

    column_plans: list[PlannedDerivation] = []
    drivers = {plan.driver for plan in row_plans}
    for column in specification.columns:
        if column.derivation is None:
            continue
        planned, references = _plan_derivation(
            column.name,
            column.derivation,
            f"columns.{column.name}.derivation",
            supported_operations,
            diagnostics,
            unsupported,
        )
        column_plans.append(planned)
        for reference in references:
            if "." in reference.name:
                _validate_qualified_reference(
                    reference,
                    drivers,
                    bindings,
                    column_types,
                    diagnostics,
                    unsupported,
                    deferred_qualifiers=deferred_qualifiers,
                )
            elif reference.name not in column_types:
                diagnostics.append(
                    _diagnostic(
                        "unknown_field",
                        reference.path,
                        {"identifier": reference.name},
                    )
                )
            elif (
                reference.expected_type is not None
                and column_types[reference.name] != reference.expected_type
            ):
                diagnostics.append(
                    _diagnostic(
                        "incompatible_input_type",
                        reference.path,
                        {
                            "source": reference.name,
                            "expected": reference.expected_type,
                            "actual": column_types[reference.name],
                        },
                        requirement=reference.requirement,
                    )
                )

    column_graph = {
        planned.column: tuple(
            dependency
            for dependency in planned.dependencies
            if dependency in column_types
        )
        for planned in column_plans
    }
    cycle = _find_cycle(
        [name for name in column_order if name in column_graph], column_graph
    )
    cycle_members = set(cycle[:-1]) if cycle is not None else set()
    if cycle is not None:
        by_name = {planned.column: planned for planned in column_plans}
        paths = tuple(
            dict.fromkeys(by_name[name].expression_path for name in cycle[:-1])
        )
        diagnostics.append(
            _diagnostic(
                "dependency_cycle",
                paths,
                {"cycle": list(cycle)},
                requirement="R001-41",
            )
        )

    for planned in column_plans:
        if planned.column in cycle_members:
            continue
        for dependency in planned.dependencies:
            if dependency not in column_positions:
                continue
            if column_positions[dependency] >= column_positions[planned.column]:
                diagnostics.append(
                    _diagnostic(
                        "dependency_order",
                        planned.expression_path,
                        {"column": planned.column, "dependency": dependency},
                    )
                )

    if diagnostics:
        raise ExecutionPlanningError(diagnostics)
    unique_unsupported = _unique_features(unsupported)
    if unique_unsupported:
        raise UnsupportedPlanningError(unique_unsupported)

    row_derived = tuple(
        column.name for column in specification.columns if column.derivation is None
    )
    return ExecutionPlan(
        specification=specification,
        bindings=bindings,
        rows=tuple(row_plans),
        columns=tuple(column_plans),
        row_derived_columns=row_derived,
    )
