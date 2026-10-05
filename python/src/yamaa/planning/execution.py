"""Build a deterministic plan for the initial R001 execution subset."""

from __future__ import annotations

from collections.abc import Callable, Collection, Mapping, Sequence
from dataclasses import dataclass, replace
from typing import Any, Literal

from pydantic import BaseModel, ConfigDict, Field, JsonValue, ValidationError

from yamaa.expressions import (
    WINDOW_OPERATIONS,
    AggregateError,
    NumericError,
    PredicateAst,
    PredicateError,
    TemplateError,
    aggregate_identifiers,
    aggregate_star_datasets,
    numeric_identifiers,
    parse_aggregate_cached,
    parse_numeric_cached,
    parse_predicate,
    parse_template_cached,
    predicate_identifiers,
    source_operand,
    template_identifiers,
    ungrouped_identifiers,
    window_spec,
)
from yamaa.io.artifact import profile_of
from yamaa.io.source import LoadedDataset
from yamaa.models import ColumnType, ConditionPhase, TypedColumn, TypedTable
from yamaa.odm import (
    BindingFailure,
    BindingPlan,
    BoundReference,
    DatasetBinding,
    build_binding_plan,
)
from yamaa.odm.items import ODM_SCHEMA_FIELDS, odm_read_sites
from yamaa.planning.dependencies import ColumnDependencyAnalyzer, DependencyAnalyzer
from yamaa.planning.references import (
    IntermediateReadScope,
    IntermediateScope,
    KeyInference,
    QualifiedFinding,
    QualifiedScope,
    ReferenceCompiler,
    ReferenceCompilerFactory,
    ReferenceFinding,
)
from yamaa.specification.models import (
    Expression,
    HandledExpression,
    Intermediate,
    OrderTerm,
    RecordBetween,
    Row,
    Specification,
    key_pairs,
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
    requirement: str | None = Field(
        default=None, pattern=r"^(?:REQ-[0-9]{4,}|R[0-9]{3}-[1-9][0-9]*[a-z]?)$"
    )
    context: dict[str, JsonValue]


class UnsupportedFeature(_FrozenModel):
    """One valid declaration outside the initial executor subset."""

    operation: str = Field(min_length=1)
    spec_path: str = Field(min_length=1)


class PlannedDerivation(_FrozenModel):
    """One derivation with its dependencies."""

    column: str = Field(min_length=1)
    path: str = Field(min_length=1)
    expression_path: str = Field(min_length=1)
    declaration: HandledExpression
    dependencies: tuple[str, ...]
    implicit_joins: tuple[ImplicitJoin, ...] = ()

    @property
    def operation_path(self) -> str:
        """Return the authored operation that owns this graph node."""
        return f"{self.expression_path}.{self.declaration.value.operation}"


class MatchValueExpression(_FrozenModel):
    """One `key` match value REQ-1259 plans as an expression.

    `column` is the key column the value is paired with; `name` is the
    synthetic match name (`key[<column>]`) the value takes in
    `match_variables`; `expression` is evaluated against the current row at
    match time; `variables` are the current-row identifiers the expression
    reads, which `dependencies` reports.
    """

    column: str = Field(min_length=1)
    name: str = Field(min_length=1)
    expression: Expression
    variables: tuple[str, ...] = ()


def _match_name(column: str) -> str:
    """Return the synthetic match name of an expression-valued `key` pair.

    The brackets keep the name from ever colliding with a real variable.
    """
    return f"key[{column}]"


class PlannedIntermediateUnique(_FrozenModel):
    columns: tuple[str, ...]
    spec_path: str
    verification_id: str | None = None


class PlannedIntermediate(_FrozenModel):
    """One validated `intermediates` entry, ready to select a record.

    `match_variables` and `match_fields` pair by position: the first names
    what the current row reads, the second the right-side column it must
    equal. Both come from the entry's `key` (REQ-0115), with REQ-0153
    inferring an omitted key from the applicable output keys. A match value
    that is an expression (REQ-1259) takes the synthetic name `key[<column>]`
    in `match_variables`, and its `MatchValueExpression` lives in
    `match_expressions`.
    """

    identifier: str = Field(min_length=1)
    dataset: str = Field(min_length=1)
    self_fields: tuple[str, ...] = ()
    path: str = Field(min_length=1)
    match_variables: tuple[str, ...]
    match_fields: tuple[str, ...]
    match_expressions: tuple[MatchValueExpression, ...] = ()
    filter_predicate: dict[str, Any] | None = None
    order_terms: tuple[tuple[OrderTerm, str], ...] = ()
    keep: Literal["first", "last"] | None = None
    between_value: str | None = None
    between_lower: str | None = None
    between_upper: str | None = None
    readable_columns: tuple[str, ...] = ()
    # REQ-0129: the declared `no_match` literal answers absence; without it
    # absence fails as unmatched_key, except on an implicit join (REQ-0111),
    # which registers no handler and answers absence as missing.
    no_match: Any = None
    no_match_declared: bool = False
    implicit_join: bool = False
    # REQ-1185: derivations are computed per record before matching; the map
    # is empty when the author declared none.
    derived: tuple[tuple[str, HandledExpression], ...] = ()
    # REQ-1185: qualified driver reads, with their expression paths.
    derivation_references: tuple[tuple[str, str], ...] = ()
    # REQ-1245: checks over the source-only filtered donor records.
    unique_checks: tuple[PlannedIntermediateUnique, ...] = ()

    @property
    def filter_variables(self) -> tuple[str, ...]:
        """Qualified current-driver fields used by a correlated filter."""
        if self.filter_predicate is None:
            return ()
        return tuple(
            name
            for name in predicate_identifiers(self.filter_predicate)
            if name.split(".", 1)[0] != self.dataset
            and not (self.dataset == "SELF" and "." not in name)
        )

    @property
    def dependencies(self) -> tuple[str, ...]:
        """Return the current-row variables REQ-0050 makes this intermediate need.

        Donor fields contribute no current-row dependency. Matching values,
        range values, and correlated filter and derivation fields must be
        available before selecting the shared record. REQ-1259: an expression match value
        contributes the identifiers its expression reads, not its synthetic
        match name.
        """
        synthetic = {keyed.name for keyed in self.match_expressions}
        names = [name for name in self.match_variables if name not in synthetic]
        names.extend(self.filter_variables)
        names.extend(name for name, _ in self.derivation_references)
        for keyed in self.match_expressions:
            names.extend(keyed.variables)
        if self.between_value is not None:
            names.append(self.between_value)
        return tuple(dict.fromkeys(names))


class ResolvedJoin(_FrozenModel):
    """The key pairs one intermediate-like resolution matches on.

    R003 makes validation report the pairs for every named intermediate,
    dataset-qualified aggregate, and implicit join, so a reviewer sees which
    columns the resolution matches on rather than having to infer them from
    two schemas. `inferred` marks the pairs REQ-0150 infers
    from the applicable keys; the rest are declared by the author.
    """

    spec_path: str = Field(min_length=1)
    dataset: str = Field(min_length=1)
    source: tuple[str, ...]
    key: tuple[str, ...]
    inferred: bool = False


@dataclass(frozen=True, slots=True)
class ImplicitJoin:
    """One restored R003 implicit join: the dataset and its inferred keys.

    The runtime matches on `keys` (as both the source variables and the
    right-side fields) whenever a derivation reads a column of `dataset`.
    """

    dataset: str
    keys: tuple[str, ...]
    # REQ-0130 lets a reduction declare keys coarser than the applicable
    # keys, and the join then matches on that instead.
    declared_grain: bool = False
    # REQ-0156/REQ-0157: a row-phase join matches the driver record's (or
    # group keys') fields rather than completed output columns. `None`
    # keeps the column-phase meaning: match on `keys` themselves.
    match_variables: tuple[str, ...] | None = None


class PlannedRow(_FrozenModel):
    """One record-driven or group-driven row template.

    REQ-0035 gives a template one of two modes, and `group_variables` is what
    tells them apart: empty for a record-driven template, and the driver
    variables the group-by keys partition on for a group-driven one. A grouped
    template's filter selects completed candidates rather than driver
    records, which REQ-0038 evaluates after the whole derivation graph.
    """

    index: int | None
    declaration: Row | None
    driver: str = Field(min_length=1)
    group_variables: tuple[str, ...] = ()
    filter_path: str | None = None
    filter_predicate: dict[str, Any] | None = None
    derivations: tuple[PlannedDerivation, ...] = ()

    @property
    def grouped(self) -> bool:
        return bool(self.group_variables)

    @property
    def group_fields(self) -> tuple[str, ...]:
        """Return the driver columns the group-by keys partition on."""
        return tuple(name.split(".", 1)[1] for name in self.group_variables)


class ExecutionPlan(_FrozenModel):
    """Validated work in the exact order the initial executor will perform it."""

    specification: Specification
    bindings: BindingPlan
    rows: tuple[PlannedRow, ...]
    columns: tuple[PlannedDerivation, ...]
    row_derived_columns: tuple[str, ...]
    intermediates: tuple[PlannedIntermediate, ...] = ()
    resolved_joins: tuple[ResolvedJoin, ...] = ()


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
    current_driver: bool = False
    # The R007 requirement the owning operation's input type is held to.
    requirement: str | None = None
    # The relation this reference reaches through a declared intermediate pairing,
    # whose key variables the reading derivation therefore depends on.
    join_relation: str | None = None
    # The declared `key` columns of that pairing, in order.
    join_key: tuple[str, ...] | None = None
    # The current-row match values of that pairing, in `join_key` order.
    join_match_values: tuple[str, ...] | None = None
    # The columns that join replaces the applicable keys with, when REQ-0130
    # lets a reduction declare keys coarser than they are.
    join_group_by: tuple[str, ...] | None = None
    # How the reference reaches its relation: as one scalar of the current
    # row driver, as the records R013 reduces, as a right-side column R003
    # pairs with a declared key rather than a key of its own, or as a stored
    # field of the records a predicate reads. REQ-0047 and REQ-0111 each turn
    # on the difference.
    reach: Literal["scalar", "relation", "declared", "record"] = "scalar"
    # The other name this reference's runtime type must be comparable with,
    # which is how REQ-0305 pairs a `intermediate` source with its key column.
    same_type_as: str | None = None
    # REQ-1259: an expression match value's planned expression. `name` is
    # the synthetic match name (`key[<column>]`), which is not a real variable:
    # ordinary name validation and dependency recording skip it, and only the
    # paired-type check applies (against `same_type_as`).
    key_expression: MatchValueExpression | None = None
    # REQ-1259: parallel to `join_match_values`; the planned expression for each
    # match value, or None when the value is a bare variable.
    # Lets _validate_aggregate_keys type-check expression results instead of
    # resolving synthetic names.
    join_key_expressions: tuple[MatchValueExpression | None, ...] | None = None


@dataclass(frozen=True, slots=True)
class _Scope:
    """Where an expression sits, which is what fixes the contexts it may use.

    REQ-0295 through REQ-0296 make an aggregate valid in exactly three places,
    and each is told apart by the phase it evaluates in and by whether the
    enclosing row template groups its driver.
    """

    column_phase: bool = True
    grouped_driver: str | None = None
    group_variables: tuple[str, ...] = ()
    intermediates: frozenset[str] = frozenset()
    # REQ-1242: named intermediates that declare `keep` select exactly one
    # record per row, so a derive step may read them alongside its one
    # driving relation.
    keep_intermediates: frozenset[str] = frozenset()


_COLUMN_SCOPE = _Scope()


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


def expression_path(path: str, derivation: HandledExpression) -> str:
    """Recover the authored bare-expression path where normalization permits it."""
    handled = {"unconvertible"} & derivation.model_fields_set
    return f"{path}.value" if handled else path


def _deduplicate_references(references: Sequence[_Reference]) -> tuple[_Reference, ...]:
    seen: set[tuple[object, ...]] = set()
    ordered: list[_Reference] = []
    for reference in references:
        identity = (
            reference.name,
            reference.path,
            reference.expected_type,
            reference.current_value_available,
            reference.requirement,
            reference.join_relation,
            reference.join_key,
            reference.join_match_values,
            reference.reach,
            reference.same_type_as,
        )
        if identity not in seen:
            ordered.append(reference)
            seen.add(identity)
    return tuple(ordered)


# The operations whose `source` names one variable of a stated input type.
_TYPED_SOURCES: dict[str, tuple[ColumnType | None, str]] = {
    "mapping": ("str", "REQ-0304"),
    "str_extract": ("str", "REQ-0308"),
    "str_case": ("str", "REQ-0308"),
    "cut": (None, "REQ-0306"),
}

# REQ-0590 types every temporal operand. `date_precision` and `to_date` read
# more than one source kind, so they state no expected type and answer at
# evaluation.
_TEMPORAL_VARIABLES: dict[str, tuple[tuple[str, ColumnType | None, str], ...]] = {
    "date_diff": (("start", "date", "REQ-0606"), ("end", "date", "REQ-0606")),
    "study_day": (("date", "date", "REQ-0606"), ("reference", "date", "REQ-0606")),
    "date_impute": (
        ("source", "str", "REQ-0590"),
        ("not_before", "date", "REQ-0585"),
    ),
    "date_precision": (("source", None, "REQ-0581"),),
    "datetime_impute": (("source", "str", "REQ-1182"),),
    "datetime_precision": (("source", None, "REQ-1183"),),
    "to_date": (("source", None, "REQ-0607"),),
}

# The operations whose `source` names one variable their handler types at
# evaluation. The read is still a dependency, and REQ-1259 evaluates a named
# intermediate's expression match values over exactly the reads collected here.
_EVALUATED_SOURCES: tuple[str, ...] = (
    "round_half_away_from_zero",
    "str_contains",
    "str_pad",
    "to_epoch_day",
)


def _expression_info(
    expression: Expression,
    path: str,
    supported_operations: Collection[str],
    *,
    scope: _Scope = _COLUMN_SCOPE,
    dataset_fields: Mapping[str, Collection[str]] | None = None,
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
        """Collect one expression REQ-0290 permits this operation to nest."""
        if not isinstance(nested, Mapping) or len(nested) != 1:
            return
        info = _expression_info(
            Expression.model_validate(dict(nested)),
            nested_path,
            supported_operations,
            scope=scope,
            dataset_fields=dataset_fields,
        )
        references.extend(info.references)
        unsupported.extend(info.unsupported)
        diagnostics.extend(info.diagnostics)

    if operation == "source":
        variable = payload if isinstance(payload, str) else payload.get("variable")
        if isinstance(variable, str):
            references.append(_Reference(variable, operation_path))
        if isinstance(payload, Mapping) and payload.get("filter") is not None:
            diagnostics.extend(
                _filtered_source_references(
                    variable,
                    payload.get("filter"),
                    f"{operation_path}.filter",
                    references,
                    scope,
                )
            )
    elif operation in _TYPED_SOURCES and isinstance(payload, Mapping):
        operand = source_operand(payload.get("source"))
        expected, requirement = _TYPED_SOURCES[operation]
        if operand is not None:
            variable, selector = operand
            references.append(
                _Reference(
                    variable,
                    f"{operation_path}.source",
                    expected,
                    requirement=requirement,
                )
            )
            diagnostics.extend(
                _filtered_source_references(
                    variable,
                    selector,
                    f"{operation_path}.source.filter",
                    references,
                    scope,
                )
            )
    elif operation in WINDOW_OPERATIONS and isinstance(payload, Mapping):
        diagnostics.extend(
            _window_references(operation, payload, operation_path, references, scope)
        )
    elif operation in _TEMPORAL_VARIABLES and isinstance(payload, Mapping):
        references.extend(
            _Reference(
                payload[field],
                f"{operation_path}.{field}",
                expected,
                requirement=requirement,
            )
            for field, expected, requirement in _TEMPORAL_VARIABLES[operation]
            if isinstance(payload.get(field), str)
        )
    elif operation in _EVALUATED_SOURCES and isinstance(payload, Mapping):
        variable = payload.get("source")
        if isinstance(variable, str):
            references.append(_Reference(variable, f"{operation_path}.source"))
        if operation == "str_pad" and (
            type(payload.get("width")) is not int or payload["width"] < 1
        ):
            diagnostics.append(
                _diagnostic(
                    "invalid_field_type",
                    f"{operation_path}.width",
                    {"expected": "a positive integer width"},
                    requirement="REQ-1261",
                )
            )
    elif operation == "function" and isinstance(payload, Mapping):
        arguments = payload.get("args")
        if isinstance(arguments, Mapping):
            # REQ-0679 writes a named variable as a plain string; every other
            # argument leaf is a literal and depends on nothing.
            references.extend(
                _Reference(value, f"{operation_path}.args.{name}")
                for name, value in arguments.items()
                if isinstance(value, str)
            )
    elif operation in {"first_available", "greatest", "least"} and isinstance(
        payload, Mapping
    ):
        sources = payload.get("sources")
        if isinstance(sources, Sequence) and not isinstance(sources, str):
            for index, entry in enumerate(sources):
                operand = (
                    source_operand(entry)
                    if operation == "first_available"
                    else (entry, None)
                    if isinstance(entry, str)
                    else None
                )
                if operand is None:
                    continue
                name, selector = operand
                source_path = f"{operation_path}.sources[{index}]"
                references.append(_Reference(name, source_path))
                diagnostics.extend(
                    _filtered_source_references(
                        name,
                        selector,
                        f"{source_path}.filter",
                        references,
                        scope,
                    )
                )
    elif operation == "compute" and isinstance(payload, Mapping):
        diagnostics.extend(
            _compute_references(payload, operation_path, references, scope)
        )
    elif operation == "aggregate":
        diagnostics.extend(
            _aggregate_references(
                payload,
                operation_path,
                references,
                scope,
                supported_operations,
                unsupported,
            )
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
    elif (
        operation == "case"
        and isinstance(payload, Sequence)
        and not isinstance(payload, str)
    ):
        for index, item in enumerate(payload):
            if not isinstance(item, Mapping):
                continue
            item_path = f"{operation_path}[{index}]"
            if "otherwise" in item:
                nest(item["otherwise"], f"{item_path}.otherwise")
                continue
            when = item.get("when")
            if isinstance(when, str):
                ast = _parse_predicate_at(when, f"{item_path}.when", diagnostics)
                if ast is not None:
                    references.extend(
                        _Reference(
                            name,
                            f"{item_path}.when",
                            requirement="REQ-0189",
                        )
                        for name in predicate_identifiers(ast)
                    )
            nest(item.get("then"), f"{item_path}.then")
    elif operation == "flag" and isinstance(payload, (Mapping, str)):
        # REQ-1256: a bare predicate string is the condition. R006 expands
        # it to the mapping form at load, so both spellings report at the
        # canonical condition path, as the handler does.
        condition = payload if isinstance(payload, str) else payload.get("condition")
        condition_path = f"{operation_path}.condition"
        if isinstance(condition, str):
            ast = _parse_predicate_at(condition, condition_path, diagnostics)
            if ast is not None:
                references.extend(
                    _Reference(
                        name,
                        condition_path,
                        requirement="REQ-0189",
                    )
                    for name in predicate_identifiers(ast)
                )
        if (
            isinstance(payload, Mapping)
            and "false_value" in payload
            and "missing" not in payload
        ):
            # REQ-1258: a flag that names its false value names its unknown one.
            diagnostics.append(
                _diagnostic(
                    "missing_value_required",
                    f"{operation_path}.missing",
                    {"false_value": payload["false_value"]},
                    requirement="REQ-1258",
                )
            )

    return _ExpressionInfo(
        references=_deduplicate_references(references),
        unsupported=tuple(unsupported),
        diagnostics=tuple(diagnostics),
    )


def _compute_references(
    payload: Mapping[str, object],
    operation_path: str,
    references: list[_Reference],
    scope: _Scope,
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
        qualifier = name.split(".", 1)[0] if "." in name else None
        if (
            scope.column_phase
            and qualifier is not None
            and qualifier not in scope.intermediates
        ):
            # REQ-0409 and REQ-0125: a qualified identifier is admitted only for
            # a record already selected by a declared intermediate, so every
            # other join stays under R003 rather than inside the formula.
            diagnostics.append(
                ExecutionDiagnostic(
                    phase="validation",
                    condition="qualified_identifier",
                    spec_paths=(expr_path,),
                    requirement="REQ-0442",
                    context={"expr": expr, "identifier": name},
                )
            )
            continue
        references.append(_Reference(name, expr_path))
    return diagnostics


def _filtered_source_references(
    variable: object,
    selector: object,
    filter_path: str,
    references: list[_Reference],
    scope: _Scope,
) -> list[ExecutionDiagnostic]:
    """Collect what a source `filter` reads, which is its own right side.

    REQ-0131 lets a source state which records it may read, and REQ-0132 keeps
    that predicate over those records alone, so every identifier names a
    field of the dataset the source reads and none of them makes the reading
    column depend on an output value.
    """
    if not isinstance(selector, str):
        return []
    diagnostics: list[ExecutionDiagnostic] = []
    dataset = (
        variable.split(".", 1)[0]
        if isinstance(variable, str) and "." in variable
        else None
    )
    if (
        dataset is None
        or dataset in scope.intermediates
        or (dataset == scope.grouped_driver)
    ):
        # REQ-0148: an output column, a chosen intermediate record, and a group key
        # are each one value, so a filter has no records to select among.
        return [
            _diagnostic(
                "prohibited_construct",
                filter_path,
                {"identifier": variable if isinstance(variable, str) else None},
                requirement="REQ-0148",
            )
        ]
    ast = _parse_predicate_at(selector, filter_path, diagnostics)
    if ast is None:
        return diagnostics
    for name in predicate_identifiers(ast):
        if name.split(".", 1)[0] != dataset or "." not in name:
            diagnostics.append(
                _diagnostic(
                    "unknown_field",
                    filter_path,
                    {"identifier": name, "dataset": dataset},
                    requirement="REQ-0132",
                )
            )
            continue
        references.append(_Reference(name, filter_path, reach="record"))
    return diagnostics


def _as_names(value: object) -> tuple[str, ...] | None:
    if isinstance(value, str):
        return (value,)
    if isinstance(value, Sequence) and all(isinstance(item, str) for item in value):
        return tuple(value)  # type: ignore[arg-type]
    return None


# REQ-0115: how a diagnostic describes the shape a `key` must take.
_KEY_EXPECTED = (
    "key as a column, a non-empty list of columns, or a non-empty mapping of "
    "columns to match values"
)


def _match_value_entries(values: Sequence[object]) -> tuple[str | Expression, ...]:
    """Return match values with every expression as an Expression model.

    REQ-1259: expression match values arrive as raw mappings on the payload
    paths; validating each once lets every use see an Expression model.
    """
    return tuple(
        value
        if isinstance(value, (str, Expression))
        else Expression.model_validate(dict(value))  # type: ignore[call-overload]
        for value in values
    )


# REQ-1259: fixed-result operations in the rule's closed static-type table,
# for the REQ-0118 comparability check on expression match values. `source` and
# `literal` take their types from their payloads below. Operations absent
# here defer the check to runtime comparison of values.
_MATCH_VALUE_RESULT_TYPES: dict[str, ColumnType] = {
    "baseline_flag": "str",
    "cut": "str",
    "date_diff": "int",
    "date_impute": "date",
    "date_precision": "str",
    "datetime_impute": "datetime",
    "datetime_precision": "str",
    "rank": "int",
    "row_number": "int",
    "str_case": "str",
    "str_concat": "str",
    "str_contains": "bool",
    "str_extract": "str",
    "str_pad": "str",
    "str_template": "str",
    "study_day": "int",
    "to_date": "date",
    "to_epoch_day": "int",
    # compute and round_half_away_from_zero are numeric; int and float compare
    # mutually under REQ-0005, so "float" covers both. The greatest and least
    # registry expressions return the extreme of any comparable type
    # (REQ-0425), so their result type depends on their inputs.
    "compute": "float",
    "round_half_away_from_zero": "float",
}


def _match_value_result_type(
    expression: Expression,
    bindings: BindingPlan,
    column_types: Mapping[str, ColumnType],
) -> ColumnType | None:
    """Infer an expression match value's static result type, if it is known.

    `source` takes its variable's type, `literal` takes its value's type,
    and the fixed-result operations above take their table type. Anything
    else returns None, deferring comparability to the runtime (REQ-1259).
    """
    operation = expression.operation
    payload = expression.root[operation]
    if operation == "source":
        variable: object = payload
        if isinstance(payload, Mapping):
            variable = payload.get("variable")
        if isinstance(variable, str):
            return _reference_type(variable, bindings, column_types)
        return None
    if operation == "literal":
        value = payload.get("value") if isinstance(payload, Mapping) else payload
        if isinstance(value, bool):
            return "bool"
        if isinstance(value, int):
            return "int"
        if isinstance(value, float):
            return "float"
        if isinstance(value, str):
            return "str"
        return None
    return _MATCH_VALUE_RESULT_TYPES.get(operation)


_WINDOW_VARIABLES: dict[str, tuple[str, ...]] = {
    "row_number": (),
    "rank": (),
    "row_value": ("source",),
    "previous_non_missing": ("source",),
    "locf": ("source",),
    "baseline_flag": ("date", "reference_date"),
}

# REQ-0340: these windows number or move along declared positions, so they
# require window.order_by. REQ-0341: the baseline window locates its row
# by date instead, so a declared order_by is rejected.
_WINDOW_ORDER_BY_REQUIRED: tuple[str, ...] = (
    "row_number",
    "rank",
    "row_value",
    "previous_non_missing",
    "locf",
)
_WINDOW_ORDER_BY_FORBIDDEN: tuple[str, ...] = ("baseline_flag",)


def _window_references(
    operation: str,
    payload: Mapping[str, object],
    operation_path: str,
    references: list[_Reference],
    scope: _Scope,
) -> list[ExecutionDiagnostic]:
    """Collect what one window reads, and reject the contexts R007 refuses.

    REQ-0326 scopes a row-construction window to the rows its enclosing row
    template constructs. The reference checks below already confine every
    window read to that template's constructed columns and its driver,
    which is exactly that relation, so no blanket rejection is needed here.
    """
    diagnostics: list[ExecutionDiagnostic] = []
    if operation == "row_value" and payload.get("offset") == 0:
        # REQ-0328: the current row's own value is `source`, and a window must
        # not be a second spelling of it.
        diagnostics.append(
            _diagnostic(
                "zero_offset",
                f"{operation_path}.offset",
                {"offset": 0},
                requirement="REQ-0328",
            )
        )
    window = window_spec(payload)
    if operation in _WINDOW_ORDER_BY_REQUIRED and not window.get("order_by"):
        # REQ-0340: without a declared order the window has no positions to
        # number or to move along.
        diagnostics.append(
            _diagnostic(
                "window_order_by_required",
                f"{operation_path}.window",
                {"operation": operation},
                requirement="REQ-0340",
            )
        )
    if operation in _WINDOW_ORDER_BY_FORBIDDEN and window.get("order_by"):
        # REQ-0341: the baseline row is located by date and flag, not by a
        # declared order, so a declared order would be silently ignored.
        diagnostics.append(
            _diagnostic(
                "window_order_by_forbidden",
                f"{operation_path}.window.order_by",
                {"operation": operation},
                requirement="REQ-0341",
            )
        )
    for field in _WINDOW_VARIABLES[operation]:
        name = payload.get(field)
        if isinstance(name, str):
            references.append(_Reference(name, f"{operation_path}.{field}"))
    for index, name in enumerate(_as_names(window.get("group_by")) or ()):
        references.append(
            _Reference(name, f"{operation_path}.window.group_by[{index}]")
        )
    for index, term in enumerate(window.get("order_by") or ()):
        variable = term if isinstance(term, str) else None
        if isinstance(term, Mapping) and isinstance(term.get("variable"), str):
            variable = str(term["variable"])
        if isinstance(variable, str):
            references.append(
                _Reference(variable, f"{operation_path}.window.order_by[{index}]")
            )
    predicate = window.get("filter")
    if isinstance(predicate, str):
        filter_path = f"{operation_path}.window.filter"
        ast = _parse_predicate_at(predicate, filter_path, diagnostics)
        if ast is not None:
            references.extend(
                _Reference(name, filter_path) for name in predicate_identifiers(ast)
            )
    return diagnostics


# REQ-1189: payload argument names that name record fields for each
# derivation operation, mirroring the schema declarations
# (yaml/schema_expression_*.yaml). A plain string in any other argument
# position is a literal, an enum value, or an identifier -- never a
# variable reference. Window operations additionally read their
# window_spec (see _DERIVE_WINDOW_OPERATIONS below).
_DERIVE_VARIABLE_FIELDS: dict[str, tuple[str, ...]] = {
    "baseline_flag": ("date", "reference_date"),
    "cut": ("source",),
    "date_diff": ("start", "end"),
    "date_impute": ("source", "not_before"),
    "date_precision": ("source",),
    "datetime_impute": ("source",),
    "datetime_precision": ("source",),
    "greatest": ("sources",),
    "least": ("sources",),
    "mapping": ("source",),
    "previous_non_missing": ("source",),
    "locf": ("source",),
    "round_half_away_from_zero": ("source",),
    "row_value": ("source",),
    "str_case": ("source",),
    "str_extract": ("source",),
    "str_pad": ("source",),
    "study_day": ("date", "reference"),
    "to_date": ("source",),
    "to_epoch_day": ("source",),
}

# Window operations name their window_spec's fields in a derive binding
# derivation, alongside the operation's own variable fields above.
_DERIVE_WINDOW_OPERATIONS: tuple[str, ...] = (
    "row_number",
    "rank",
    "row_value",
    "previous_non_missing",
    "locf",
    "baseline_flag",
)


def _derive_reference_names(derivation: object) -> list[str]:
    """Collect every variable name a derive binding derivation reads.

    Only argument positions the schema declares as variable references
    contribute names; literals, enum values, and identifiers never do.
    Nested derivations, expressions, and predicates recurse (REQ-1189).
    """
    names: list[str] = []

    def add_variable_field(value: object) -> None:
        # A variable field is a bare variable or a filtered source naming
        # one variable with an optional record filter.
        operand = source_operand(value)
        if operand is not None:
            names.append(operand[0])
            add_predicate_names(operand[1])

    def add_predicate_names(text: object) -> None:
        if not isinstance(text, str):
            return
        try:
            ast = parse_predicate(text)
        except PredicateError:
            # The predicate's own validation reports the error; the scan
            # only collects names from predicates that parse.
            return
        names.extend(predicate_identifiers(ast))

    def add_order_by_names(order_by: object) -> None:
        if isinstance(order_by, Sequence) and not isinstance(order_by, str):
            for term in order_by:
                variable = term
                if isinstance(term, Mapping):
                    variable = term.get("variable")
                if isinstance(variable, str):
                    names.append(variable)

    def add_window_names(window: object) -> None:
        if not isinstance(window, Mapping):
            return
        group_by = window.get("group_by")
        if isinstance(group_by, Sequence) and not isinstance(group_by, str):
            names.extend(entry for entry in group_by if isinstance(entry, str))
        add_order_by_names(window.get("order_by"))
        add_predicate_names(window.get("filter"))

    def visit(node: object) -> None:
        if isinstance(node, str):
            # A bare-string derivation is one source read.
            names.append(node)
        elif isinstance(node, Mapping):
            if len(node) == 1:
                operation, payload = next(iter(node.items()))
                if operation == "source":
                    variable = payload
                    if isinstance(payload, Mapping):
                        variable = payload.get("variable")
                    if isinstance(variable, str):
                        names.append(variable)
                    return
                if operation == "compute":
                    expr = (
                        payload.get("expr") if isinstance(payload, Mapping) else payload
                    )
                    if isinstance(expr, str):
                        try:
                            parsed = parse_numeric_cached(expr)
                        except NumericError:
                            parsed = None
                        if parsed is not None:
                            names.extend(numeric_identifiers(parsed))
                    return
                if operation in ("literal", "aggregate"):
                    # A literal is a fixed value; a nested reduction names
                    # its own relation, checked by _aggregate_references.
                    return
                if operation == "function" and isinstance(payload, Mapping):
                    # REQ-0679 writes a named variable as a plain string;
                    # every other argument leaf is a literal class mapping
                    # and depends on nothing.
                    args = payload.get("args")
                    if isinstance(args, Mapping):
                        names.extend(
                            value for value in args.values() if isinstance(value, str)
                        )
                    return
                if operation == "str_template":
                    template = payload
                    if isinstance(payload, Mapping):
                        template = payload.get("template")
                    if isinstance(template, str):
                        try:
                            parts = parse_template_cached(template)
                        except TemplateError:
                            parts = None
                        if parts is not None:
                            names.extend(template_identifiers(parts))
                    return
                if (
                    operation == "case"
                    and isinstance(payload, Sequence)
                    and not isinstance(payload, str)
                ):
                    for item in payload:
                        if not isinstance(item, Mapping):
                            continue
                        if "otherwise" in item:
                            visit(item["otherwise"])
                        else:
                            add_predicate_names(item.get("when"))
                            visit(item.get("then"))
                    return
                if operation == "flag" and isinstance(payload, (Mapping, str)):
                    # The values are literals and name nothing; only the
                    # predicate names variables. A bare string is the
                    # condition (REQ-1256).
                    condition = (
                        payload
                        if isinstance(payload, str)
                        else payload.get("condition")
                    )
                    add_predicate_names(condition)
                    return
                if operation == "first_available" and isinstance(payload, Mapping):
                    sources = payload.get("sources")
                    if isinstance(sources, Sequence) and not isinstance(sources, str):
                        for entry in sources:
                            add_variable_field(entry)
                    return
                if operation == "str_concat" and isinstance(payload, Mapping):
                    sources = payload.get("sources")
                    if isinstance(sources, Sequence) and not isinstance(sources, str):
                        for entry in sources:
                            visit(entry)
                    return
                if operation in ("greatest", "least") and isinstance(payload, Mapping):
                    sources = payload.get("sources")
                    if isinstance(sources, Sequence) and not isinstance(sources, str):
                        names.extend(
                            entry for entry in sources if isinstance(entry, str)
                        )
                    return
                if operation in _DERIVE_VARIABLE_FIELDS and isinstance(
                    payload, Mapping
                ):
                    for field in _DERIVE_VARIABLE_FIELDS[operation]:
                        value = payload.get(field)
                        values = value if isinstance(value, list) else [value]
                        for entry in values:
                            add_variable_field(entry)
                    if operation in _DERIVE_WINDOW_OPERATIONS:
                        add_window_names(payload.get("window"))
                    return
                if operation in ("row_number", "rank") and isinstance(payload, Mapping):
                    # No variable fields of their own; only the window_spec
                    # names record fields (the rank method is an enum).
                    add_window_names(payload.get("window"))
                    return
            elif set(node) <= {"value", "unconvertible"} and "value" in node:
                # A handled expression wraps one derivation; unconvertible
                # is a literal and names nothing.
                visit(node["value"])
                return
            for value in node.values():
                visit(value)
        elif isinstance(node, list):
            for item in node:
                visit(item)

    visit(derivation)
    return names


def _aggregate_references(
    payload: object,
    operation_path: str,
    references: list[_Reference],
    scope: _Scope,
    supported_operations: Collection[str],
    unsupported: list[UnsupportedFeature],
) -> list[ExecutionDiagnostic]:
    """Read one R013 reduction and report the context R007 does not permit."""
    if isinstance(payload, str):
        payload = {"expr": payload}
    if not isinstance(payload, Mapping):
        return [
            _diagnostic(
                "invalid_field_type",
                operation_path,
                {"operation": "aggregate", "expected": "a mapping"},
                requirement="REQ-0321",
            )
        ]
    expr = payload.get("expr")
    if not isinstance(expr, str):
        return [
            _diagnostic(
                "invalid_field_type",
                operation_path,
                {"operation": "aggregate", "expected": "a reducer expression"},
                requirement="REQ-0321",
            )
        ]
    # A one-field aggregate is R006's canonical form of the scalar shorthand,
    # so the operation is the narrowest location both spellings share.
    expr_path = operation_path if set(payload) == {"expr"} else f"{operation_path}.expr"
    try:
        ast = parse_aggregate_cached(expr)
    except AggregateError as error:
        return [
            ExecutionDiagnostic(
                phase="validation",
                condition=error.condition,
                spec_paths=(expr_path,),
                requirement=error.requirement,
                context={"expr": expr, **error.context},
            )
        ]

    identifiers = aggregate_identifiers(ast)
    qualifiers = {name.split(".", 1)[0] for name in identifiers if "." in name}
    unqualified = [name for name in identifiers if "." not in name]
    relations = qualifiers | set(aggregate_star_datasets(ast))
    if len(relations) > 1 or (relations and unqualified):
        # REQ-0468: a reduction is not a join, so one expression names one
        # relation and two datasets are composed through two columns.
        named = sorted(relations) + (["<output>"] if unqualified else [])
        return [
            _diagnostic(
                "mixed_relations",
                expr_path,
                {"expr": expr, "relations": named},
                requirement="REQ-0504",
            )
        ]

    relation = next(iter(sorted(relations)), None)
    group_by = _as_names(payload.get("group_by")) or ()
    between = payload.get("between")

    derive = payload.get("derive")
    derive_names: tuple[str, ...] = ()
    derive_refs: list[tuple[str, str]] = []
    if derive is not None:
        # REQ-1189: a derive step binds per-record variables; the reducer
        # names them unqualified and the step names the reduced relation.
        if not isinstance(derive, list) or not derive:
            return [
                _diagnostic(
                    "invalid_derive_step",
                    f"{operation_path}.derive",
                    {"reason": "a derive step is a non-empty binding list"},
                    requirement="REQ-1189",
                )
            ]
        seen: set[str] = set()
        qualifiers: set[str] = set()
        binding_names: list[str] = []
        for index, binding in enumerate(derive):
            binding_path = f"{operation_path}.derive[{index}]"
            if not isinstance(binding, Mapping):
                return [
                    _diagnostic(
                        "invalid_derive_step",
                        binding_path,
                        {"reason": "a derive binding is a mapping"},
                        requirement="REQ-1189",
                    )
                ]
            name = binding.get("name")
            if not isinstance(name, str) or not name or name in seen:
                return [
                    _diagnostic(
                        "invalid_derive_step",
                        binding_path,
                        {"reason": "derive binding names are unique identifiers"},
                        requirement="REQ-1189",
                    )
                ]
            seen.add(name)
            binding_names.append(name)
            for ref in _derive_reference_names(binding.get("derivation")):
                head, dot, _ = ref.partition(".")
                if dot:
                    qualifiers.add(head)
                    derive_refs.append((ref, binding_path))
                elif ref not in binding_names[:-1]:
                    return [
                        _diagnostic(
                            "unknown_derive_variable",
                            binding_path,
                            {"variable": ref, "binding": name},
                            requirement="REQ-1189",
                        )
                    ]
        # The filter also names the relation when the bindings do not. A
        # throwaway parse only reads qualifiers; the real parse below
        # reports any predicate error.
        filter_declared = payload.get("filter")
        if isinstance(filter_declared, str):
            probe_filter = _parse_predicate_at(
                filter_declared, f"{operation_path}.filter", []
            )
            if probe_filter is not None:
                for ref in predicate_identifiers(probe_filter):
                    head, dot, _ = ref.partition(".")
                    if dot:
                        qualifiers.add(head)
        # REQ-1242: bindings may read keep-declared named intermediates: with
        # `keep`, the intermediate selects exactly one record per row, so it
        # is a row-scoped value, not another reduced relation. Every other
        # qualifier must be the step's one driving relation.
        relation_qualifiers = {
            qualifier
            for qualifier in qualifiers
            if qualifier not in scope.intermediates
        }
        if len(relation_qualifiers) != 1:
            return [
                _diagnostic(
                    "invalid_derive_step",
                    f"{operation_path}.derive",
                    {
                        "reason": "a derive step names exactly one relation",
                        "relations": sorted(relation_qualifiers),
                    },
                    requirement="REQ-1191",
                )
            ]
        for qualifier in sorted(qualifiers - relation_qualifiers):
            if qualifier not in scope.keep_intermediates:
                return [
                    _diagnostic(
                        "invalid_derive_step",
                        f"{operation_path}.derive",
                        {
                            "reason": "a derive step reads only keep-declared named intermediates",
                            "intermediate": qualifier,
                        },
                        requirement="REQ-1242",
                    )
                ]
        derive_names = tuple(binding_names)
        relation = next(iter(relation_qualifiers))
        for name in unqualified:
            if name not in derive_names:
                return [
                    _diagnostic(
                        "unknown_derive_variable",
                        expr_path,
                        {"variable": name, "expr": expr},
                        requirement="REQ-1189",
                    )
                ]
        # A derived aggregate is a qualified reduction; the bound variables
        # are not output columns, so they add no scalar references.
        unqualified = [name for name in unqualified if name not in derive_names]

    diagnostics = _aggregate_context(
        relation,
        group_by,
        between,
        ("key",) if payload.get("key") is not None else (),
        expr,
        operation_path,
        scope,
    )
    if diagnostics:
        return diagnostics

    grain = scope.group_variables if scope.grouped_driver is not None else group_by
    diagnostics.extend(
        _diagnostic(
            "aggregate_identifier_not_grouped",
            expr_path,
            {"expr": expr, "dataset": relation, "identifier": name},
            requirement="REQ-0503",
        )
        for name in ungrouped_identifiers(ast)
        if name not in grain
    )

    # REQ-0140: a right-side reduction over a qualified relation declares the
    # key pairs it matches on. A grouped-row reduction reads its own driver
    # group and declares none.
    joined = relation if scope.grouped_driver is None and relation else None
    key_fields: tuple[str, ...] | None = None
    key_variables: tuple[str, ...] | None = None
    key_expressions: tuple[MatchValueExpression | None, ...] | None = None
    if joined is not None:
        written = payload.get("key")
        pairs = key_pairs(written) if written is not None else None
        if written is None:
            # REQ-0153: the planner fills an omitted key before reference
            # collection, or records no_applicable_keys when no key applies.
            # Either way there is nothing left to check here.
            pass
        elif pairs is None or not pairs[0]:
            # REQ-0140: a key that names no column and match value is
            # reported here rather than reducing over the whole relation
            # unkeyed.
            diagnostics.append(
                _diagnostic(
                    "missing_aggregate_keys",
                    operation_path,
                    {"dataset": joined, "key": written},
                    requirement="REQ-0140",
                )
            )
        else:
            keys, raw_entries = pairs
            entries = _match_value_entries(raw_entries)
            key_fields = tuple(keys)
            keyed: list[MatchValueExpression | None] = []
            variables: list[str] = []
            for key, entry in zip(keys, entries, strict=True):
                if isinstance(entry, str):
                    keyed.append(None)
                    variables.append(entry)
                    continue
                # REQ-1259: an expression match value evaluates against the
                # current row.
                info = _expression_info(
                    entry,
                    f"{operation_path}.key.{key}",
                    supported_operations,
                    scope=scope,
                )
                planned = MatchValueExpression(
                    column=key,
                    name=_match_name(key),
                    expression=entry,
                    # The expression's current-row dependencies are exactly
                    # the identifiers its references read -- not every
                    # string leaf.
                    variables=tuple(dict.fromkeys(ref.name for ref in info.references)),
                )
                keyed.append(planned)
                variables.append(planned.name)
                references.extend(info.references)
                unsupported.extend(info.unsupported)
                diagnostics.extend(info.diagnostics)
            key_variables = tuple(variables)
            key_expressions = tuple(keyed)

    def relational(name: str, path: str) -> _Reference:
        qualified = "." in name
        return _Reference(
            name,
            path,
            join_relation=joined if qualified else None,
            join_key=key_fields if qualified and joined else None,
            join_match_values=key_variables if qualified and joined else None,
            join_key_expressions=key_expressions if qualified and joined else None,
            reach="relation" if qualified else "scalar",
        )

    references.extend(
        relational(name, expr_path) for name in identifiers if name not in derive_names
    )
    # REQ-1242: a binding reads the driving relation's fields or a
    # keep-declared intermediate's readable columns. Routing the qualified
    # binding references through the ordinary checks validates both against
    # the relation and the intermediate's plan (REQ-0103/REQ-0125).
    references.extend(relational(name, path) for name, path in derive_refs)
    references.extend(
        relational(name, f"{operation_path}.group_by[{index}]")
        for index, name in enumerate(group_by)
    )
    predicate = payload.get("filter")
    if isinstance(predicate, str):
        filter_path = f"{operation_path}.filter"
        ast_filter = _parse_predicate_at(predicate, filter_path, diagnostics)
        if ast_filter is not None:
            references.extend(
                relational(name, filter_path)
                for name in predicate_identifiers(ast_filter)
            )
    if isinstance(between, Mapping):
        value = between.get("value")
        if isinstance(value, str):
            references.append(_Reference(value, f"{operation_path}.between.value"))
        for bound in ("lower", "upper"):
            name = between.get(bound)
            if isinstance(name, str):
                # REQ-0472: bounds are bare donor-record fields; qualify
                # them with the aggregate's one relation for the join.
                references.append(
                    _Reference(
                        f"{relation}.{name}",
                        f"{operation_path}.between.{bound}",
                        join_relation=joined,
                        join_key=key_fields if joined else None,
                        join_match_values=key_variables if joined else None,
                        join_key_expressions=key_expressions if joined else None,
                        reach="relation",
                    )
                )
    return diagnostics


def _aggregate_context(
    relation: str | None,
    group_by: Sequence[str],
    between: object,
    key_fields: Sequence[str],
    expr: str,
    operation_path: str,
    scope: _Scope,
) -> list[ExecutionDiagnostic]:
    """Reject every aggregate context outside the three REQ-0295 permits."""

    def reject(reason: str, path: str = operation_path) -> list[ExecutionDiagnostic]:
        return [
            _diagnostic(
                "invalid_aggregate_context",
                path,
                {"expr": expr, "reason": reason},
                requirement="REQ-0329",
            )
        ]

    if scope.grouped_driver is not None:
        # Context 3: the enclosing `row.group_by` owns the keys, so the
        # aggregate declares none of its own and narrows nothing per row.
        if relation is not None and relation != scope.grouped_driver:
            return reject(
                f"a grouped row aggregate reads {scope.grouped_driver!r}, "
                f"not {relation!r}"
            )
        if relation is None:
            return reject("a grouped row aggregate reads its row driver")
        if group_by:
            return reject(
                "the enclosing row.group_by owns the keys",
                f"{operation_path}.group_by",
            )
        if between is not None:
            return reject(
                "a grouped row aggregate has no separate right side",
                f"{operation_path}.between",
            )
        if key_fields:
            # REQ-0142: the group is the match. A key pair here would not
            # widen the read to the scope it names; the aggregate would still
            # reduce the current group, so the declaration is refused rather
            # than silently ignored.
            return [
                _diagnostic(
                    "invalid_aggregate_context",
                    [f"{operation_path}.{field}" for field in key_fields],
                    {
                        "expr": expr,
                        "reason": "a grouped row aggregate reads its own group "
                        "and declares no key pairs",
                    },
                    requirement="REQ-0142",
                )
            ]
        return []

    if not scope.column_phase:
        return reject("an ungrouped row template has no aggregate context")

    if relation is None:
        # Context 2: constructed output rows, partitioned by `group_by`.
        if not group_by:
            return [
                _diagnostic(
                    "invalid_aggregate_context",
                    operation_path,
                    {
                        "expr": expr,
                        "reason": "an output-row reduction declares group_by",
                    },
                    requirement="REQ-0507",
                )
            ]
        if any("." in name for name in group_by):
            return reject(
                "an output-row reduction groups on current-output columns",
                f"{operation_path}.group_by",
            )
        if between is not None:
            return reject(
                "between narrows a qualified right side only",
                f"{operation_path}.between",
            )
        return []

    # Context 1: a declared dataset relation reduced before the R003 join.
    if any(not name.startswith(f"{relation}.") for name in group_by):
        return reject(
            f"a qualified reduction groups on columns of {relation!r}",
            f"{operation_path}.group_by",
        )
    if isinstance(between, Mapping):
        bounds = [
            between[side]
            for side in ("lower", "upper")
            if isinstance(between.get(side), str)
        ]
        if not bounds:
            # REQ-0472 and REQ-0135: at least one bound, or the narrowing states
            # nothing and silently reduces the unrestricted right side.
            return reject(
                "between declares at least one bound", f"{operation_path}.between"
            )
        if any("." in str(bound) for bound in bounds):
            # REQ-0472: bounds are bare donor-record fields of the
            # expression's one relation, not qualified references.
            return reject(
                f"a between bound must name a bare column of {relation!r}",
                f"{operation_path}.between",
            )
    return []


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
        _Reference(name, f"{operation_path}.template", "str", requirement="REQ-0308")
        for name in template_identifiers(parts)
    )
    return []


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
                requirement=error.requirement,
            )
        )
        return None


def _fill_omitted_lookup_keys(
    declaration: HandledExpression,
    value_path: str,
    scope: _Scope,
    infer: Callable[[str, str], tuple[str, ...] | None],
) -> tuple[HandledExpression, frozenset[str]]:
    """Fill omitted intermediate/aggregate key pairs from the applicable keys.

    REQ-0153 lets a named intermediate or a qualified aggregate omit `key`,
    inferring the applicable output keys, and REQ-0115 lets a key name its
    columns alone, matching same-named current-row values. The planner and
    the runtime downstream only understand a complete mapping of columns to
    match values, so both are resolved here, before reference collection.
    Returns the rewritten declaration and the operation paths where a key
    was inferred.
    """
    inferred: set[str] = set()

    def fill_pairs(
        payload: Mapping[str, object],
        dataset: str,
        operation_path: str,
    ) -> Mapping[str, object] | None:
        """Return the payload with its key as a full mapping, or None to skip."""
        written = payload.get("key")
        if isinstance(written, Mapping):
            return None
        if written is None:
            keys = infer(dataset, operation_path)
            if keys is None:
                # The diagnostic is recorded; leave the payload for the
                # operation's own validation to report.
                return None
            inferred.add(operation_path)
        else:
            keys = _as_names(written)
            if keys is None:
                # Present but malformed; downstream validation reports it.
                return None
        return {**payload, "key": {key: key for key in keys}}

    def qualified_relation(payload: Mapping[str, object]) -> str | None:
        """Mirror _aggregate_references' join detection for the fill."""
        expr = payload.get("expr")
        if not isinstance(expr, str):
            return None
        try:
            ast = parse_aggregate_cached(expr)
        except AggregateError:
            return None
        identifiers = aggregate_identifiers(ast)
        qualifiers = {name.split(".", 1)[0] for name in identifiers if "." in name}
        unqualified = [name for name in identifiers if "." not in name]
        relations = qualifiers | set(aggregate_star_datasets(ast))
        if len(relations) > 1 or (relations and unqualified):
            return None
        relation = next(iter(sorted(relations)), None)
        if relation is None and payload.get("derive") is not None:
            # REQ-1189: a derived aggregate names its relation through the
            # derive step when the reducer only names bound variables.
            # REQ-1242: qualifiers naming declared intermediates read the
            # intermediate's row, never the reduced relation.
            derived: set[str] = set()
            derive_declared = payload.get("derive")
            if isinstance(derive_declared, list):
                for binding in derive_declared:
                    if isinstance(binding, Mapping):
                        for name in _derive_reference_names(binding.get("derivation")):
                            if "." in name:
                                head = name.split(".", 1)[0]
                                if head not in scope.intermediates:
                                    derived.add(head)
            relation = next(iter(sorted(derived)), None)
        # A grouped-row reduction reads its own driver group and declares
        # no key pairs (REQ-0142).
        return relation if scope.grouped_driver is None and relation else None

    def fill_aggregate(node: object, operation_path: str) -> object:
        payload = node if isinstance(node, Mapping) else {"expr": node}
        if not isinstance(payload, Mapping):
            return node
        relation = qualified_relation(payload)
        if relation is None:
            return node
        filled = fill_pairs(payload, relation, operation_path)
        if filled is None:
            return node
        return dict(filled)

    def walk(node: object, operation_path: str) -> object:
        if isinstance(node, Mapping):
            if set(node) == {"aggregate"}:
                return {
                    "aggregate": fill_aggregate(
                        node["aggregate"], f"{operation_path}.aggregate"
                    )
                }
            return {
                key: walk(value, f"{operation_path}.{key}")
                for key, value in node.items()
            }
        if isinstance(node, Sequence) and not isinstance(node, str):
            return [
                walk(value, f"{operation_path}[{index}]")
                for index, value in enumerate(node)
            ]
        return node

    root = walk(declaration.value.root, value_path)
    if root == declaration.value.root:
        return declaration, frozenset()
    rewritten = declaration.value.model_copy(update={"root": root})
    return declaration.model_copy(update={"value": rewritten}), frozenset(inferred)


def _plan_derivation(
    column: str,
    declaration: HandledExpression,
    path: str,
    supported_operations: Collection[str],
    diagnostics: list[ExecutionDiagnostic],
    unsupported: list[UnsupportedFeature],
    *,
    scope: _Scope = _COLUMN_SCOPE,
    infer_keys: (
        Callable[[str, str, list[ExecutionDiagnostic]], tuple[str, ...] | None] | None
    ) = None,
    dataset_fields: Mapping[str, Collection[str]] | None = None,
) -> tuple[PlannedDerivation, tuple[_Reference, ...], frozenset[str]]:
    value_path = expression_path(path, declaration)
    inferred_paths: frozenset[str] = frozenset()
    deferred: list[ExecutionDiagnostic] = []
    if infer_keys is not None:
        declaration, inferred_paths = _fill_omitted_lookup_keys(
            declaration,
            value_path,
            scope,
            lambda dataset, path: infer_keys(dataset, path, deferred),
        )
    info = _expression_info(
        declaration.value,
        value_path,
        supported_operations,
        scope=scope,
        dataset_fields=dataset_fields,
    )
    references = list(info.references)
    unsupported.extend(info.unsupported)
    diagnostics.extend(info.diagnostics)
    # A failed key inference is reported after the operation's own
    # validation, so a more fundamental problem (say, an aggregate in a
    # context R007 forbids) is named first.
    diagnostics.extend(deferred)

    ordered_references = _deduplicate_references(references)
    dependencies = tuple(
        dict.fromkeys(
            reference.name
            for reference in ordered_references
            if "." not in reference.name
            # REQ-1259: an expression match value's synthetic match name is not a
            # real variable; its inner references already carry the true deps.
            and reference.key_expression is None
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
        ),
        ordered_references,
        inferred_paths,
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
    return next(
        (column.type for column in dataset.columns if column.name == bound.field),
        None,
    )


def _validate_qualified_reference(
    reference: _Reference,
    drivers: Collection[str],
    bindings: BindingPlan,
    column_types: Mapping[str, ColumnType],
    diagnostics: list[ExecutionDiagnostic],
    *,
    intermediates: Mapping[str, PlannedIntermediate] = {},
    row: Row | None = None,
    grouped_by_driver: Mapping[str, Sequence[tuple[str, ...]]] | None = None,
    reference_compiler: ReferenceCompiler | None = None,
) -> None:
    """Check one qualified name against the relation it reaches.

    REQ-0150: a name qualified with a dataset reaches that dataset through
    the implicit join on the applicable keys; when the keys are unclear the
    author wraps the read in an explicit `intermediate` (REQ-0152). The current
    row's own datasets need no join: a scalar source qualified with a row
    driver reads the current driver record. During row construction REQ-0047
    lets a row derivation read its driver, group keys, earlier row columns,
    or an intermediate; REQ-0156/REQ-0157 additionally let it read another
    dataset through a planned join, whose match variables name
    driver-record fields or group keys, and name relation-internal fields
    of lookups and source filters directly.
    """
    qualifier = reference.name.split(".", 1)[0]
    if qualifier in intermediates and qualifier not in drivers:
        intermediate = intermediates[qualifier]
        _validate_intermediate_reference(
            reference, intermediate, bindings, diagnostics, reference_compiler
        )
        driver_references = [
            (name, f"{intermediate.path}.filter", "REQ-0120")
            for name in intermediate.filter_variables
        ]
        driver_references.extend(
            (name, path, "REQ-1185")
            for name, path in intermediate.derivation_references
        )
        for name, path, requirement in driver_references:
            _validate_qualified_reference(
                _Reference(name, path, current_driver=True, requirement=requirement),
                drivers,
                bindings,
                column_types,
                diagnostics,
                row=row,
                grouped_by_driver=grouped_by_driver,
                reference_compiler=reference_compiler,
            )
        return
    if reference_compiler is not None:
        scope = QualifiedScope(
            drivers=tuple(drivers),
            current_driver=reference.current_driver,
            reach=reference.reach,
            joined=reference.join_relation is not None,
            phase="row" if row is not None else "column",
            group_by=None
            if row is None or row.group_by is None
            else tuple(row.group_by),
            groups=tuple((grouped_by_driver or {}).get(qualifier, ())),
        )
        for finding in reference_compiler.validate_qualified(
            reference.name, reference.expected_type, scope
        ):
            _append_qualified_finding(finding, reference, drivers, diagnostics, row=row)
        return
    _validate_direct_qualified_reference(
        reference,
        drivers,
        bindings,
        column_types,
        diagnostics,
        intermediates=intermediates,
        row=row,
        grouped_by_driver=grouped_by_driver,
    )


def _append_qualified_finding(
    finding: QualifiedFinding,
    reference: _Reference,
    drivers: Collection[str],
    diagnostics: list[ExecutionDiagnostic],
    *,
    row: Row | None,
) -> None:
    """Attach authored names, paths and requirements without re-evaluating native scope rules."""
    requirement = reference.requirement
    context: dict[str, JsonValue] = {"identifier": reference.name}
    if finding.kind == "driver_mismatch":
        condition = "unknown_field"
        context["drivers"] = sorted(drivers)
    elif finding.kind == "unknown_field":
        condition, requirement = "unknown_field", "REQ-0103"
    elif finding.kind in ("row_phase", "row_group"):
        if row is None:
            raise ValueError("native row scope finding outside row construction")
        context["row"] = row.id
        if finding.kind == "row_phase":
            condition, requirement = "phase_boundary", None
            context.update(
                available_phase="column_derivation", required_phase="row_construction"
            )
        else:
            condition, requirement = "ungrouped_driver_field", "REQ-0067"
            context["dataset"] = reference.name.split(".", 1)[0]
    elif finding.kind == "column_group":
        condition, requirement = "ungrouped_driver_field", "REQ-0107"
        context["dataset"] = reference.name.split(".", 1)[0]
    elif finding.kind == "incompatible_input_type":
        condition = "incompatible_input_type"
        context = {
            "source": reference.name,
            "expected": finding.expected,
            "actual": finding.actual,
        }
    else:
        raise ValueError("unknown native qualified reference finding")
    diagnostics.append(
        _diagnostic(condition, reference.path, context, requirement=requirement)
    )


def _validate_direct_qualified_reference(
    reference: _Reference,
    drivers: Collection[str],
    bindings: BindingPlan,
    column_types: Mapping[str, ColumnType],
    diagnostics: list[ExecutionDiagnostic],
    *,
    intermediates: Mapping[str, PlannedIntermediate],
    row: Row | None,
    grouped_by_driver: Mapping[str, Sequence[tuple[str, ...]]] | None,
) -> None:
    """Unchanged reference rules; native planning selects the shared compiler above."""
    qualifier = reference.name.split(".", 1)[0]
    if reference.current_driver and set(drivers) != {qualifier}:
        diagnostics.append(
            _diagnostic(
                "unknown_field",
                reference.path,
                {"identifier": reference.name, "drivers": sorted(drivers)},
                requirement=reference.requirement,
            )
        )
        return
    bound = bindings.bind(reference.name)
    if isinstance(bound, BindingFailure):
        diagnostics.append(
            _diagnostic(
                "unknown_field",
                reference.path,
                {"identifier": reference.name},
                requirement="REQ-0103",
            )
        )
        return
    if bound.kind == "output":
        raise AssertionError("a qualified name cannot bind as an output column")
    if reference.reach == "record" and bound.kind != "dataset":
        # REQ-0132: a predicate reads stored fields of the records it selects
        # among, and an ODM item is resolved from a record rather than
        # carried by one.
        diagnostics.append(
            _diagnostic(
                "unknown_field",
                reference.path,
                {"identifier": reference.name},
                requirement="REQ-0132",
            )
        )
        return
    assert bound.dataset is not None
    if (
        bound.kind == "dataset"
        and reference.reach not in ("declared", "relation")
        and reference.join_relation is None
        and bound.dataset not in drivers
        and row is None
    ):
        # REQ-0152: the implicit-join pre-pass already recorded why the
        # applicable keys are unclear; nothing more to add here. Row
        # derivations fall through to the row-phase check below, which
        # REQ-0156/REQ-0157 extend to planned joins.
        return
    if row is not None and not _row_join_reference(
        reference, bound, bindings, drivers, intermediates
    ):
        _validate_row_phase_reference(
            reference, bound.dataset, next(iter(drivers), None), row, diagnostics
        )
    if row is None and grouped_by_driver is not None:
        # REQ-0107: a column-level derivation reads every constructed row, so
        # a scalar source of a grouped template's driver must name a variable
        # in the group_by of every grouped template that dataset drives.
        _validate_column_phase_group_key(
            reference, bound, grouped_by_driver, diagnostics
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


def _row_join_reference(
    reference: _Reference,
    bound: BoundReference,
    bindings: BindingPlan,
    drivers: Collection[str],
    intermediates: Mapping[str, PlannedIntermediate],
) -> bool:
    """Tell whether a row-phase read reaches another dataset legitimately.

    REQ-0156/REQ-0157 let a row derivation read a non-driver dataset through
    a planned implicit join, and name relation-internal fields of lookups
    and source filters directly. Such references skip the row-phase
    gate; existence and types are still checked. A scalar read the
    key-inference pre-pass left unannotated already carries its own
    REQ-0151/REQ-0152 diagnostic, so it reports no knock-on phase error;
    anything else unresolvable (for example an ODM item) still does.
    """
    if reference.join_relation is not None:
        return True
    if reference.reach in ("declared", "record"):
        return True
    qualifier = reference.name.split(".", 1)[0] if "." in reference.name else None
    return (
        reference.reach == "scalar"
        and bound.kind == "dataset"
        and qualifier is not None
        and qualifier in bindings.datasets
        and qualifier not in drivers
        and qualifier not in intermediates
    )


def _validate_row_phase_reference(
    reference: _Reference,
    dataset: str,
    driver: str | None,
    row: Row,
    diagnostics: list[ExecutionDiagnostic],
) -> None:
    if dataset != driver:
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
        return
    if (
        row.group_by is not None
        and reference.reach == "scalar"
        and reference.name not in row.group_by
    ):
        # REQ-0067: a driver field that varies within the group has no single
        # value for the candidate, so it is read through an aggregate or not
        # at all.
        diagnostics.append(
            _diagnostic(
                "ungrouped_driver_field",
                reference.path,
                {"identifier": reference.name, "row": row.id, "dataset": dataset},
                requirement="REQ-0067",
            )
        )


def _validate_column_phase_group_key(
    reference: _Reference,
    bound: BoundReference,
    grouped_by_driver: Mapping[str, Sequence[tuple[str, ...]]],
    diagnostics: list[ExecutionDiagnostic],
) -> None:
    """Fail a column-level scalar source that is not a group key (REQ-0107).

    A column-level derivation is evaluated for every constructed row, so a
    scalar read of a grouped template's driver dataset is only meaningful
    when the variable is a group key of every grouped template that dataset
    drives. Anything else varies within the group and resolves to nothing at
    runtime, which previously failed silently.
    """
    if bound.kind != "dataset" or reference.reach != "scalar":
        return
    templates = grouped_by_driver.get(bound.dataset or "")
    if not templates:
        return
    for group_by in templates:
        if reference.name not in group_by:
            diagnostics.append(
                _diagnostic(
                    "ungrouped_driver_field",
                    reference.path,
                    {"identifier": reference.name, "dataset": bound.dataset},
                    requirement="REQ-0107",
                )
            )
            return


def _intermediate_scope(intermediate: PlannedIntermediate) -> IntermediateScope:
    """Project normalized names into the trusted metadata port without visibility decisions."""
    return IntermediateScope(
        dataset=intermediate.dataset,
        self_fields=intermediate.self_fields,
        derived=tuple(name for name, _ in intermediate.derived),
        readable=intermediate.readable_columns,
        dependencies=intermediate.dependencies,
    )


def _validate_intermediate_reference(
    reference: _Reference,
    intermediate: PlannedIntermediate,
    bindings: BindingPlan,
    diagnostics: list[ExecutionDiagnostic],
    reference_compiler: ReferenceCompiler | None = None,
) -> None:
    """Select shared field visibility while retaining the default reference implementation."""
    if reference_compiler is None:
        _reference_intermediate_visibility(
            reference, intermediate, bindings, diagnostics
        )
        return
    for finding in reference_compiler.validate_intermediate(
        reference.name.split(".", 1)[1], _intermediate_scope(intermediate)
    ):
        if finding.kind != "unknown_field":
            raise ValueError("unexpected native intermediate field finding")
        diagnostics.append(
            _diagnostic(
                "unknown_field",
                reference.path,
                {"identifier": reference.name},
                requirement="REQ-0125",
            )
        )


def _reference_intermediate_visibility(
    reference: _Reference,
    intermediate: PlannedIntermediate,
    bindings: BindingPlan,
    diagnostics: list[ExecutionDiagnostic],
) -> None:
    """Check that an intermediate id qualifies a column its dataset has."""
    field = reference.name.split(".", 1)[1]
    dataset = bindings.datasets.get(intermediate.dataset)
    readable = intermediate.readable_columns
    derived = {name for name, _ in intermediate.derived}
    available = (
        set(intermediate.self_fields)
        if intermediate.dataset == "SELF"
        else set(dataset.field_names)
        if dataset is not None
        else set()
    )
    if (dataset is not None or intermediate.dataset == "SELF") and (
        (field not in available and field not in derived)
        or (readable and field not in readable)
    ):
        # REQ-0125: the named column must exist in the intermediate's dataset and,
        # when the intermediate declares `columns`, be one of them.
        diagnostics.append(
            _diagnostic(
                "unknown_field",
                reference.path,
                {"identifier": reference.name},
                requirement="REQ-0125",
            )
        )


def _dataset_types(bindings: BindingPlan, dataset: str) -> dict[str, ColumnType]:
    binding = bindings.datasets.get(dataset)
    if binding is None:
        return {}
    return {column.name: column.type for column in binding.columns}


def _reference_type(
    name: str,
    bindings: BindingPlan,
    column_types: Mapping[str, ColumnType],
) -> ColumnType | None:
    if "." not in name:
        return column_types.get(name)
    bound = bindings.bind(name)
    if isinstance(bound, BindingFailure):
        return None
    return _bound_type(bound, bindings, column_types)


def _validate_paired_type(
    reference: _Reference,
    bindings: BindingPlan,
    column_types: Mapping[str, ColumnType],
    diagnostics: list[ExecutionDiagnostic],
    reference_compiler: ReferenceCompiler | None = None,
) -> None:
    """Check the two halves of a declared key pair against REQ-0305."""
    if reference.same_type_as is None:
        return
    if reference.key_expression is not None:
        # REQ-1259: an expression match value compares its statically known
        # result type; an unknown static type defers the check to the runtime.
        actual = _match_value_result_type(
            reference.key_expression.expression, bindings, column_types
        )
        if actual is None:
            return
    else:
        actual = _reference_type(reference.name, bindings, column_types)
    expected = _reference_type(reference.same_type_as, bindings, column_types)
    if (
        actual is None
        or expected is None
        or _comparable_types(actual, expected, reference_compiler=reference_compiler)
    ):
        return
    diagnostics.append(
        _diagnostic(
            "incompatible_input_type",
            reference.path,
            {"source": reference.name, "expected": expected, "actual": actual},
            requirement=reference.requirement or "REQ-0305",
        )
    )


def _reference_comparable_types(left: ColumnType, right: ColumnType) -> bool:
    """Retain the default backend's declared-type rule independently of Rust."""
    return left == right or {left, right} <= {"int", "float"}


def _comparable_types(
    left: ColumnType,
    right: ColumnType,
    reference_compiler: ReferenceCompiler | None = None,
) -> bool:
    """Select REQ-0005 comparison through the captured backend compiler port."""
    if reference_compiler is not None:
        return reference_compiler.comparable_types(left, right)
    return _reference_comparable_types(left, right)


def _reference_applicable_keys(
    keys: tuple[str, ...],
    column_types: Mapping[str, ColumnType],
    fields: Mapping[str, ColumnType],
) -> KeyInference:
    """Retain default backend inference with explicit invalid-identity admission."""
    for key in keys:
        if key not in column_types:
            return KeyInference("undeclared_output", key=key)
    applicable = tuple(key for key in keys if key in fields)
    if not applicable:
        return KeyInference("no_applicable_keys")
    for key in applicable:
        if not _reference_comparable_types(column_types[key], fields[key]):
            return KeyInference(
                "incompatible", key=key, expected=column_types[key], actual=fields[key]
            )
    return KeyInference("keys", keys=applicable)


def _infer_applicable_keys(
    specification: Specification,
    bindings: BindingPlan,
    column_types: Mapping[str, ColumnType],
    dataset: str,
    path: str,
    diagnostics: list[ExecutionDiagnostic],
    *,
    hint: str = "declare an explicit `intermediate:` with `source`/`key` pairs",
    requirement: str = "REQ-0152",
    fields: Mapping[str, ColumnType] | None = None,
    reference_compiler: ReferenceCompiler | None = None,
) -> tuple[str, ...] | None:
    """Infer the applicable keys REQ-0150 defines for an implicit join.

    Returns the output keys, in output-key order, that also exist on the
    right-side dataset, or None after recording why the key is unclear.
    """
    fields = fields if fields is not None else _dataset_types(bindings, dataset)
    keys = tuple(specification.keys)
    inference = (
        _reference_applicable_keys(keys, column_types, fields)
        if reference_compiler is None
        else reference_compiler.infer_keys(keys, fields)
    )
    if inference.kind == "undeclared_output":
        # REQ-0220 was already diagnosed during source-independent preflight.
        return None
    if inference.kind == "no_applicable_keys":
        # REQ-0152: with no applicable key the intended match is unclear,
        # so the author must state it explicitly.
        diagnostics.append(
            _diagnostic(
                "no_applicable_keys",
                path,
                {
                    "dataset": dataset,
                    "keys": list(specification.keys),
                    "hint": hint,
                },
                requirement=requirement,
            )
        )
        return None
    if inference.kind == "incompatible":
        # REQ-0151: preserve the first mismatch selected in output-key order.
        diagnostics.append(
            _diagnostic(
                "incompatible_input_type",
                path,
                {
                    "source": inference.key,
                    "expected": inference.expected,
                    "actual": inference.actual,
                },
                requirement="REQ-0151",
            )
        )
        return None
    if inference.kind != "keys":
        raise ValueError("unknown key inference outcome")
    return inference.keys


def _resolve_implicit_joins(
    references: Sequence[_Reference],
    specification: Specification,
    bindings: BindingPlan,
    drivers: Collection[str],
    intermediates: Mapping[str, PlannedIntermediate],
    column_types: Mapping[str, ColumnType],
    diagnostics: list[ExecutionDiagnostic],
    reference_compiler: ReferenceCompiler | None = None,
) -> tuple[_Reference, ...]:
    """Annotate plain cross-dataset scalar sources with their implicit join.

    REQ-0150: a scalar `source` qualified with a dataset joins that dataset
    on the applicable keys whenever those keys are clear. The annotation
    marks the reference so relation-dependency recording, reference
    validation, and the runtime all see the same inferred pairing. Only
    plain dataset columns join: an ODM item resolves from the current
    row's context, never through an inferred join.
    """
    annotated: list[_Reference] = []
    for reference in references:
        if "." not in reference.name:
            annotated.append(reference)
            continue
        qualifier = reference.name.split(".", 1)[0]
        if (
            qualifier in intermediates
            or qualifier not in bindings.datasets
            or qualifier in drivers
            or reference.reach != "scalar"
            or reference.current_driver
            or reference.join_relation is not None
        ):
            annotated.append(reference)
            continue
        bound = bindings.bind(reference.name)
        if isinstance(bound, BindingFailure) or bound.kind != "dataset":
            # REQ-0150 joins dataset columns only: an ODM item resolves from
            # the current row's context, never through an inferred join.
            annotated.append(reference)
            continue
        keys = _infer_applicable_keys(
            specification,
            bindings,
            column_types,
            qualifier,
            reference.path,
            diagnostics,
            reference_compiler=reference_compiler,
        )
        if keys is None:
            annotated.append(reference)
            continue
        annotated.append(
            replace(
                reference,
                join_relation=qualifier,
                join_key=keys,
                join_match_values=keys,
            )
        )
    return tuple(annotated)


def _row_join_match_variables(
    references: Sequence[_Reference],
    *,
    row: Row,
    driver: str,
    bindings: BindingPlan,
    diagnostics: list[ExecutionDiagnostic],
    reference_compiler: ReferenceCompiler | None = None,
) -> tuple[_Reference, ...]:
    """Point a row-phase implicit join's match at the driver record.

    REQ-0156/REQ-0157: the candidate holds single values only for its driver
    record (ungrouped) or its group keys (grouped), so the inferred
    applicable keys match the same-named driver fields rather than
    not-yet-derived output columns.
    """
    fields = _dataset_types(bindings, driver)
    group_fields = (
        set()
        if row.group_by is None
        else {
            name.split(".", 1)[1]
            for name in row.group_by
            if name.startswith(f"{driver}.") and name.count(".") == 1
        }
    )
    rewritten: list[_Reference] = []
    for reference in references:
        if (
            reference.join_relation is None
            or reference.join_key is None
            or reference.join_match_values is None
            or reference.join_relation == driver
        ):
            rewritten.append(reference)
            continue
        match: list[str] = []
        for key in reference.join_key:
            if row.group_by is not None and key not in group_fields:
                # REQ-0157: a key the group does not carry varies within it.
                diagnostics.append(
                    _diagnostic(
                        "ungrouped_driver_field",
                        reference.path,
                        {
                            "identifier": f"{driver}.{key}",
                            "row": row.id,
                            "dataset": driver,
                        },
                        requirement="REQ-0067",
                    )
                )
                continue
            if key not in fields:
                diagnostics.append(
                    _diagnostic(
                        "unknown_field",
                        reference.path,
                        {"identifier": f"{driver}.{key}"},
                        requirement="REQ-0103",
                    )
                )
                continue
            right = _dataset_types(bindings, reference.join_relation).get(key)
            if right is not None and not _comparable_types(
                fields[key], right, reference_compiler=reference_compiler
            ):
                # REQ-0151: an inferred key must compare equal on both sides.
                diagnostics.append(
                    _diagnostic(
                        "incompatible_input_type",
                        reference.path,
                        {
                            "source": f"{driver}.{key}",
                            "expected": fields[key],
                            "actual": right,
                        },
                        requirement="REQ-0151",
                    )
                )
                continue
            match.append(f"{driver}.{key}")
        rewritten.append(replace(reference, join_match_values=tuple(match)))
    return tuple(rewritten)


def _with_relation_dependencies(
    planned: PlannedDerivation,
    references: Sequence[_Reference],
    specification: Specification,
    bindings: BindingPlan,
    intermediates: Mapping[str, PlannedIntermediate],
    drivers: Collection[str],
    diagnostics: list[ExecutionDiagnostic],
    column_types: Mapping[str, ColumnType],
    resolved: list[ResolvedJoin],
    *,
    inferred_paths: frozenset[str] = frozenset(),
    reference_compiler: ReferenceCompiler | None = None,
) -> PlannedDerivation:
    """Add the current-row values a derivation needs to reach another relation.

    REQ-0050 makes an intermediate's match values dependencies of every column that
    reads it, and REQ-0140 makes an aggregate's declared `source` values
    dependencies the same way. Recording them as ordinary dependencies is
    what puts the keys' inputs before the read in R001's declaration order.
    REQ-0150 adds the inferred keys of an implicit join as dependencies too,
    and records the join so the runtime can match on it.
    """
    extra: list[str] = []
    implicit: list[ImplicitJoin] = []
    # One reading per declared pairing: an aggregate names the same right
    # side from its `expr` and its `filter`, and REQ-0143 asks for the keys
    # the resolution matches on, not for one line per mention.
    checked: set[tuple[str, tuple[str, ...], tuple[str, ...]]] = set()
    for reference in references:
        if "." not in reference.name:
            continue
        qualifier = reference.name.split(".", 1)[0]
        if qualifier in intermediates and qualifier not in drivers:
            extra.extend(intermediates[qualifier].dependencies)
            continue
        if reference.reach in {"declared", "record"}:
            # REQ-0130: `intermediate` declares its own pairs and never consults
            # output keys, so it depends on no key beyond its sources, and
            # REQ-0132 keeps a source filter inside the right side it reads.
            continue
        if (
            reference.join_relation is not None
            and reference.join_key is not None
            and reference.join_match_values is not None
        ):
            if reference.reach == "scalar":
                # REQ-0150: an implicit join the pre-pass inferred from the
                # applicable keys. The keys were validated there.
                # REQ-0156/REQ-0157: a row-phase join states its driver-side
                # match variables separately; elsewhere they are the keys.
                match_variables = reference.join_match_values
                implicit.append(
                    ImplicitJoin(
                        dataset=reference.join_relation,
                        keys=reference.join_key,
                        match_variables=(
                            None
                            if match_variables == reference.join_key
                            else match_variables
                        ),
                    )
                )
            else:
                _validate_aggregate_keys(
                    reference,
                    bindings,
                    column_types,
                    diagnostics,
                    reference_compiler=reference_compiler,
                )
            extra.extend(reference.join_match_values)
            pairing = (
                reference.join_relation,
                reference.join_match_values,
                reference.join_key,
            )
            if pairing in checked:
                continue
            checked.add(pairing)
            resolved.append(
                ResolvedJoin(
                    spec_path=reference.path,
                    dataset=reference.join_relation,
                    source=reference.join_match_values,
                    key=reference.join_key,
                    inferred=reference.reach == "scalar"
                    or any(
                        reference.path == path or reference.path.startswith(f"{path}.")
                        for path in inferred_paths
                    ),
                )
            )
    if not extra and not implicit:
        return planned
    dependencies = tuple(
        dict.fromkeys(
            (*planned.dependencies, *(name for name in extra if name != planned.column))
        )
    )
    return planned.model_copy(
        update={
            "dependencies": dependencies,
            "implicit_joins": tuple(dict.fromkeys(implicit)),
        }
    )


def _lookup_dependencies(
    reference: _Reference,
    intermediates: Mapping[str, PlannedIntermediate],
) -> tuple[str, ...]:
    """Return the current-row values an intermediate-qualified reference needs."""
    intermediate = intermediates.get(reference.name.split(".", 1)[0])
    return intermediate.dependencies if intermediate is not None else ()


def _lookup_match_available_at_row_construction(
    match: str,
    *,
    row: Row,
    driver: str | None,
    bindings: BindingPlan,
) -> bool:
    """Tell whether a lookup match variable is known while rows are built.

    REQ-0126 (issue #711): a grouped template knows its group keys during
    row construction, and an ungrouped template reads its driver record
    1:1, so driver fields need no template derivation either. This is the
    same boundary REQ-0156/REQ-0157 draw for implicit joins.
    """
    if match in (row.group_by or ()):
        return True
    return (
        driver is not None
        and row.group_by is None
        and match.startswith(f"{driver}.")
        and match.count(".") == 1
        and match.split(".", 1)[1] in _dataset_types(bindings, driver)
    )


def _validate_aggregate_keys(
    reference: _Reference,
    bindings: BindingPlan,
    column_types: Mapping[str, ColumnType],
    diagnostics: list[ExecutionDiagnostic],
    reference_compiler: ReferenceCompiler | None = None,
) -> None:
    """Check the declared pairs one aggregate matches on.

    REQ-0141 names the key a column of the relation and the source key a
    resolvable current-row variable; REQ-0004 performs no implicit conversion
    between operation inputs, so the two sides of each pair must already
    agree. Reporting the disagreement is what keeps a resolution from
    quietly matching nothing and presenting an unasked question as an
    absent record.
    """
    assert reference.join_relation is not None
    assert reference.join_key is not None
    assert reference.join_match_values is not None
    dataset = reference.join_relation
    fields = _dataset_types(bindings, dataset)
    expressions = reference.join_key_expressions or ()
    for index, (variable, field) in enumerate(
        zip(reference.join_match_values, reference.join_key, strict=True)
    ):
        if field not in fields:
            diagnostics.append(
                _diagnostic(
                    "unknown_field",
                    reference.path,
                    {"identifier": f"{dataset}.{field}"},
                    requirement="REQ-0141",
                )
            )
            continue
        keyed = expressions[index] if index < len(expressions) else None
        if keyed is not None:
            # REQ-1259: an expression match value compares its statically
            # known result type; an unknown static type defers to the runtime.
            # The inner references were validated separately.
            left = _match_value_result_type(keyed.expression, bindings, column_types)
            if left is None:
                continue
        else:
            left = _reference_type(variable, bindings, column_types)
            if left is None:
                diagnostics.append(
                    _diagnostic(
                        "unknown_field",
                        reference.path,
                        {"identifier": variable},
                        requirement="REQ-0141",
                    )
                )
                continue
        right = fields[field]
        if _comparable_types(left, right, reference_compiler=reference_compiler):
            continue
        diagnostics.append(
            _diagnostic(
                "incompatible_input_type",
                reference.path,
                {"source": variable, "expected": left, "actual": right},
                requirement="REQ-0004",
            )
        )


def _qualification_suggestion(
    identifier: str, dataset: str, fields: Collection[str]
) -> str | None:
    """Suggest the qualified spelling for an unqualified or wrongly-qualified
    lookup field (REQ-0120). Returns None when the bare field name is not a
    column of the dataset: a genuinely unknown field gets no suggestion."""
    qualifier = identifier.split(".", 1)[0]
    bare = identifier.split(".", 1)[-1]
    if qualifier != dataset and bare in fields:
        return f"{dataset}.{bare}"
    return None


def _match_value_suggestion(
    identifier: str, column_types: Mapping[str, ColumnType]
) -> str | None:
    """Suggest the bare current-row spelling for a `SELF`-qualified match
    value (REQ-0117). `SELF` names donor rows and is never a current-row
    scope, so `SELF.AVISIT` reads no match value; when the current row
    carries `AVISIT`, the bare name is the value meant. Returns None for any
    other identifier."""
    qualifier, separator, bare = identifier.partition(".")
    if qualifier == "SELF" and separator and bare in column_types:
        return bare
    return None


def _append_reference_finding(
    finding: ReferenceFinding | None,
    reference: _Reference,
    diagnostics: list[ExecutionDiagnostic],
    *,
    row: Row | None = None,
) -> None:
    """Attach the authored path and row identity to a core-selected reference condition."""
    if finding is None:
        return
    context = dict(finding.context)
    requirement = reference.requirement
    if finding.condition == "phase_boundary":
        if row is None:
            raise ValueError("native output phase finding outside row construction")
        context = {
            "identifier": context["identifier"],
            "row": row.id,
            "available_phase": context["available_phase"],
            "required_phase": context["required_phase"],
        }
        requirement = None
    diagnostics.append(
        _diagnostic(finding.condition, reference.path, context, requirement=requirement)
    )


def _unresolvable_reference_diagnostic(
    reference: _Reference,
    candidate_datasets: Collection[str],
    dataset_fields: Mapping[str, Collection[str]],
) -> ExecutionDiagnostic:
    """Name an unqualified reference no output column resolves.

    REQ-0106: an unqualified name only ever binds to an output column, so a
    name no output column carries is unresolvable as written. When the bare
    name is a field of an in-scope dataset the author almost certainly meant
    the qualified source read, so the diagnostic says so and suggests the
    qualified spelling; a name no dataset carries stays `unknown_field`.
    """
    candidates = sorted(
        dataset
        for dataset in candidate_datasets
        if reference.name in (dataset_fields.get(dataset) or ())
    )
    if not candidates:
        return _diagnostic(
            "unknown_field",
            reference.path,
            {"identifier": reference.name},
            requirement=reference.requirement,
        )
    return _diagnostic(
        "unresolvable_name",
        reference.path,
        {
            "identifier": reference.name,
            "suggestion": f"{candidates[0]}.{reference.name}",
        },
        requirement=reference.requirement,
    )


def _plan_lookups(
    specification: Specification,
    bindings: BindingPlan,
    column_types: Mapping[str, ColumnType],
    diagnostics: list[ExecutionDiagnostic],
    resolved: list[ResolvedJoin],
    supported_operations: Collection[str],
    unsupported: list[UnsupportedFeature],
    default_columns: frozenset[str],
    dependency_analyzer: DependencyAnalyzer | None = None,
    reference_compiler: ReferenceCompiler | None = None,
) -> dict[str, PlannedIntermediate]:
    """Validate each declared intermediate against its loaded dataset."""
    planned: dict[str, PlannedIntermediate] = {}
    # REQ-0120: a named intermediate's filter/order_by suggests the qualified
    # spelling, so the lookup datasets' columns ride along for suggestions.
    dataset_fields = {
        name: _dataset_types(bindings, name) for name in bindings.datasets
    }
    self_fields = {
        column.name: column.type
        for column in specification.columns
        if column.derivation is None or column.name in default_columns
    }
    if specification.rows:
        dataset_fields["SELF"] = self_fields
    row_drivers = {row.dataset for row in specification.rows or ()}
    intermediate_ids = frozenset(item.id for item in specification.intermediates or ())
    reads: dict[str, tuple[_IntermediateRead, ...]] = {}
    for index, intermediate in enumerate(specification.intermediates or ()):
        path = f"intermediates[{index}]"
        if intermediate.dataset not in dataset_fields:
            continue
        if (
            intermediate.key is None
            and not any(
                getattr(intermediate, field)
                for field in (
                    "between",
                    "filter",
                    "order_by",
                    "keep",
                    "columns",
                    "derivations",
                    "verifications",
                )
            )
            and "no_match" in intermediate.model_fields_set
            and intermediate.no_match is None
        ):
            # REQ-1248: an intermediate declaring nothing beyond id, dataset,
            # and `no_match: null` reads exactly what the implicit join
            # reads, so the author should read the input dataset directly
            # instead. Without `no_match` the intermediate requires a match,
            # which the implicit join cannot state. A written key has no
            # default, so even an empty one is declared and REQ-0115
            # reports it.
            diagnostics.append(
                _diagnostic(
                    "rename_only_intermediate",
                    path,
                    {
                        "intermediate": intermediate.id,
                        "dataset": intermediate.dataset,
                    },
                    requirement="REQ-1248",
                )
            )
            continue
        fields = dataset_fields[intermediate.dataset]
        key_inferred = False
        # REQ-0115: a key of column names matches same-named current-row
        # values; only a mapping states each match value.
        source_defaulted = not isinstance(intermediate.key, Mapping)
        written_values: tuple[object, ...]
        if intermediate.key is None and intermediate.id in row_drivers:
            # A row driver exposes every surviving record. It does not match
            # on output keys, which may not even be present on its source.
            # Retain any applicable keys for ordinary lookup reads elsewhere.
            match_fields = tuple(key for key in specification.keys if key in fields)
            written_values = match_fields
        elif intermediate.key is None:
            # REQ-0153: an omitted key is inferred from the applicable keys.
            inferred = _infer_applicable_keys(
                specification,
                bindings,
                column_types,
                intermediate.dataset,
                path,
                diagnostics,
                hint="declare the `key` explicitly",
                requirement="REQ-0153",
                fields=fields,
                reference_compiler=reference_compiler,
            )
            if inferred is None:
                continue
            match_fields = inferred
            written_values = inferred
            key_inferred = True
        else:
            pairs = key_pairs(intermediate.key)
            if pairs is None or not pairs[0]:
                # REQ-0115: a written key names at least one pair.
                diagnostics.append(
                    _diagnostic(
                        "invalid_field_type",
                        f"{path}.key",
                        {"intermediate": intermediate.id, "expected": _KEY_EXPECTED},
                        requirement="REQ-0115" if pairs is not None else "REQ-0321",
                    )
                )
                continue
            match_fields, written_values = pairs
        # REQ-1259: a match value is a bare variable or an ordinary
        # expression; an expression evaluates against the current row and
        # supplies its column's match value.
        variables_list: list[str] = []
        keyed_list: list[MatchValueExpression] = []
        keyed_infos: dict[str, _ExpressionInfo] = {}
        for field, entry in zip(match_fields, written_values, strict=True):
            if isinstance(entry, str):
                variables_list.append(entry)
                continue
            assert isinstance(entry, Expression)
            info = _expression_info(
                entry,
                f"{path}.key.{field}",
                supported_operations,
                dataset_fields=dataset_fields,
            )
            keyed = MatchValueExpression(
                column=field,
                name=_match_name(field),
                expression=entry,
                # The expression's current-row dependencies are exactly the
                # identifiers its references read -- not every string leaf.
                variables=tuple(dict.fromkeys(ref.name for ref in info.references)),
            )
            variables_list.append(keyed.name)
            keyed_list.append(keyed)
            keyed_infos[keyed.name] = info
        variables = tuple(variables_list)
        match_expressions = tuple(keyed_list)

        failed = False
        intermediate_reads: list[_IntermediateRead] = []
        driver_reads: list[_Reference] = []
        derived = _validate_intermediate_derivations(
            intermediate,
            path,
            fields,
            supported_operations,
            diagnostics,
            unsupported,
            dataset_fields=dataset_fields,
            intermediate_ids=intermediate_ids,
            reads=intermediate_reads,
            driver_reads=driver_reads,
            row_drivers=row_drivers,
        )
        # REQ-1185: a name whose derivation failed validation is already
        # reported at its derivation path; the key below must not repeat the
        # failure, but a key naming such a name cannot be satisfied.
        failed_derivations = set(intermediate.derivations or {}) - set(derived)
        available_fields = fields.keys() | derived.keys()
        keyed_by_name = {keyed.name: keyed for keyed in match_expressions}
        for variable, field in zip(variables, match_fields, strict=True):
            keyed = keyed_by_name.get(variable)
            if keyed is not None:
                # REQ-1259: an expression match value compares its
                # statically known result type; an unknown static type defers
                # the check to the runtime.
                left = _match_value_result_type(
                    keyed.expression, bindings, column_types
                )
                if left is None:
                    continue
            else:
                left = _reference_type(variable, bindings, column_types)
            right = fields.get(field)
            if right is None and field in derived:
                right = _DERIVED_RESULT_TYPES.get(derived[field].value.operation)
            if (
                left is None
                or right is None
                or _comparable_types(left, right, reference_compiler=reference_compiler)
            ):
                continue
            diagnostics.append(
                _diagnostic(
                    "incompatible_input_type",
                    f"{path}.key",
                    {
                        "intermediate": intermediate.id,
                        "source": variable,
                        "expected": left,
                        "actual": right,
                    },
                    requirement="REQ-0323",
                )
            )
            failed = True
        for field in match_fields:
            if field in derived:
                continue
            if field in failed_derivations:
                # REQ-1185: the derivation's own diagnostic names the
                # problem; the key just cannot be satisfied.
                failed = True
                continue
            if field not in fields:
                diagnostics.append(
                    _diagnostic(
                        "unknown_field",
                        f"{path}.key",
                        {
                            "intermediate": intermediate.id,
                            "identifier": f"{intermediate.dataset}.{field}",
                        },
                        requirement="REQ-0116",
                    )
                )
                failed = True
        for field, variable in zip(match_fields, variables, strict=True):
            keyed = keyed_by_name.get(variable)
            if keyed is not None:
                # REQ-1259: every identifier an expression match value reads must
                # be known, and must have the input type its operation
                # states, exactly as the same expression inline in a
                # derivation would be checked.
                info = keyed_infos[keyed.name]
                unsupported.extend(info.unsupported)
                diagnostics.extend(info.diagnostics)
                for reference in info.references:
                    actual = _reference_type(reference.name, bindings, column_types)
                    if actual is None:
                        context = {
                            "intermediate": intermediate.id,
                            "identifier": reference.name,
                        }
                        suggestion = _match_value_suggestion(
                            reference.name, column_types
                        )
                        if suggestion is not None:
                            context["suggestion"] = suggestion
                        diagnostics.append(
                            _diagnostic(
                                "unknown_field",
                                f"{path}.key.{field}",
                                context,
                                requirement="REQ-0117",
                            )
                        )
                        failed = True
                    elif (
                        reference.expected_type is not None
                        and actual != reference.expected_type
                    ):
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
                        failed = True
                continue
            if _reference_type(variable, bindings, column_types) is None:
                context = {"intermediate": intermediate.id, "identifier": variable}
                suggestion = _match_value_suggestion(variable, column_types)
                if suggestion is not None:
                    context["suggestion"] = suggestion
                diagnostics.append(
                    _diagnostic(
                        "unknown_field",
                        f"{path}.key",
                        context,
                        requirement="REQ-0117",
                    )
                )
                failed = True

        predicate = None
        if intermediate.filter is not None:
            predicate = _parse_predicate_at(
                intermediate.filter, f"{path}.filter", diagnostics
            )
            if predicate is not None:
                for identifier in predicate_identifiers(predicate):
                    qualifier, separator, field = identifier.partition(".")
                    donor_field = (
                        qualifier == intermediate.dataset and field in available_fields
                    ) or (
                        intermediate.dataset == "SELF"
                        and not separator
                        and identifier in available_fields
                    )
                    correlated_field = (
                        qualifier != intermediate.dataset
                        and field in dataset_fields.get(qualifier, ())
                    )
                    if not (donor_field or (separator and correlated_field)):
                        context: dict[str, JsonValue] = {
                            "intermediate": intermediate.id,
                            "identifier": identifier,
                        }
                        suggestion = _qualification_suggestion(
                            identifier, intermediate.dataset, available_fields
                        )
                        if suggestion is not None:
                            context["suggestion"] = suggestion
                        diagnostics.append(
                            _diagnostic(
                                "unknown_field",
                                f"{path}.filter",
                                context,
                                requirement="REQ-0120",
                            )
                        )
                        failed = True

        terms: list[tuple[OrderTerm, str]] = []
        for term_index, term in enumerate(intermediate.order_by or ()):
            qualifier, separator, field = term.variable.partition(".")
            bare_self = intermediate.dataset == "SELF" and not separator
            donor_term = qualifier == intermediate.dataset or bare_self
            if bare_self:
                field = qualifier
            if donor_term and field in failed_derivations:
                # The derivation already reports why this ordering field is invalid.
                failed = True
                continue
            if not donor_term or field not in available_fields:
                context = {
                    "intermediate": intermediate.id,
                    "identifier": term.variable,
                }
                suggestion = _qualification_suggestion(
                    term.variable, intermediate.dataset, available_fields
                )
                if suggestion is not None:
                    context["suggestion"] = suggestion
                diagnostics.append(
                    _diagnostic(
                        "unknown_field",
                        f"{path}.order_by[{term_index}]",
                        context,
                        requirement="REQ-0120",
                    )
                )
                failed = True
                continue
            terms.append((term, field))

        if intermediate.columns is not None:
            for field in intermediate.columns:
                if field not in available_fields:
                    diagnostics.append(
                        _diagnostic(
                            "unknown_field",
                            f"{path}.columns",
                            {
                                "intermediate": intermediate.id,
                                "identifier": f"{intermediate.dataset}.{field}",
                            },
                            requirement="REQ-0122",
                        )
                    )
                    failed = True

        unique_checks: list[PlannedIntermediateUnique] = []
        verification_ids: set[str] = set()
        for verification_index, verification in enumerate(
            intermediate.verifications or ()
        ):
            unique = verification.unique
            columns = unique if isinstance(unique, list) else unique.columns
            verification_id = None if isinstance(unique, list) else unique.id
            check_path = f"{path}.verifications[{verification_index}].unique"
            if verification_id is not None:
                if verification_id in verification_ids:
                    diagnostics.append(
                        _diagnostic(
                            "duplicate_identifier",
                            f"{check_path}.id",
                            {"identifier": verification_id},
                            requirement="REQ-0398",
                        )
                    )
                    failed = True
                verification_ids.add(verification_id)
            for unique_index, field in enumerate(columns):
                if field in derived or field in fields:
                    continue
                field_path = (
                    f"{check_path}[{unique_index}]"
                    if isinstance(unique, list)
                    else f"{check_path}.columns[{unique_index}]"
                )
                diagnostics.append(
                    _diagnostic(
                        "unknown_field",
                        field_path,
                        {
                            "intermediate": intermediate.id,
                            "identifier": f"{intermediate.dataset}.{field}",
                        },
                        requirement="REQ-1245",
                    )
                )
                failed = True
            if predicate is not None and any(
                identifier.split(".", 1)[0] != intermediate.dataset
                for identifier in predicate_identifiers(predicate)
            ):
                diagnostics.append(
                    _diagnostic(
                        "correlated_filter_with_unique_verification",
                        check_path,
                        {"intermediate": intermediate.id},
                        requirement="REQ-1245",
                    )
                )
                failed = True
            if driver_reads:
                diagnostics.append(
                    _diagnostic(
                        "correlated_derivation_with_unique_verification",
                        check_path,
                        {"intermediate": intermediate.id},
                        requirement="REQ-1245",
                    )
                )
                failed = True
            unique_checks.append(
                PlannedIntermediateUnique(
                    columns=tuple(columns),
                    spec_path=check_path,
                    verification_id=verification_id,
                )
            )

        between = intermediate.between
        if between is not None:
            failed = (
                _validate_intermediate_between(
                    intermediate.id,
                    between,
                    path,
                    fields,
                    bindings,
                    column_types,
                    diagnostics,
                    reference_compiler=reference_compiler,
                )
                or failed
            )
        if failed:
            continue

        if match_fields:
            resolved.append(
                ResolvedJoin(
                    spec_path=path,
                    dataset=intermediate.dataset,
                    source=variables,
                    key=match_fields,
                    inferred=key_inferred or source_defaulted,
                )
            )
        planned[intermediate.id] = PlannedIntermediate(
            identifier=intermediate.id,
            dataset=intermediate.dataset,
            self_fields=tuple(self_fields) if intermediate.dataset == "SELF" else (),
            path=path,
            match_variables=variables,
            match_expressions=match_expressions,
            match_fields=match_fields,
            filter_predicate=predicate,
            order_terms=tuple(terms),
            keep=intermediate.keep if intermediate.order_by is not None else None,
            between_value=between.value if between is not None else None,
            between_lower=between.lower if between is not None else None,
            between_upper=between.upper if between is not None else None,
            readable_columns=tuple(intermediate.columns)
            if intermediate.columns
            else (),
            no_match=intermediate.no_match,
            no_match_declared="no_match" in intermediate.model_fields_set,
            derived=tuple(derived.items()),
            derivation_references=tuple((ref.name, ref.path) for ref in driver_reads),
            unique_checks=tuple(unique_checks),
        )
        if intermediate_reads:
            reads[intermediate.id] = tuple(intermediate_reads)
    _validate_intermediate_reads(
        planned,
        reads,
        dataset_fields,
        diagnostics,
        dependency_analyzer,
        reference_compiler,
    )
    return planned


def _validate_intermediate_reads(
    planned: Mapping[str, PlannedIntermediate],
    reads: Mapping[str, Sequence[_IntermediateRead]],
    dataset_fields: Mapping[str, Mapping[str, ColumnType]],
    diagnostics: list[ExecutionDiagnostic],
    dependency_analyzer: DependencyAnalyzer | None = None,
    reference_compiler: ReferenceCompiler | None = None,
) -> None:
    """Check each REQ-1263 read once every intermediate is planned.

    The read intermediate must be input-backed, expose the read column, and
    take every match value, range value, and correlated filter field from the
    reading donor record. Intermediates that read each other form a
    `dependency_cycle`. Any diagnostic fails planning, so the reader keeps
    its plan and a column reading it reports nothing further.
    """
    if reference_compiler is None:
        _reference_intermediate_reads(planned, reads, dataset_fields, diagnostics)
    else:
        for reader, entries in reads.items():
            for read in entries:
                target = planned.get(read.target)
                scope = IntermediateReadScope(
                    reader=reader,
                    target_name=read.target,
                    target=_intermediate_scope(target) if target is not None else None,
                    field=read.field,
                    donor_dataset=read.dataset,
                    visible=tuple(sorted(read.visible)),
                )
                for finding in reference_compiler.validate_intermediate_read(scope):
                    context: dict[str, JsonValue] = {
                        "intermediate": reader,
                        "identifier": f"{read.target}.{read.field}",
                    }
                    if finding.kind == "self_phase":
                        condition, requirement = "phase_boundary", "REQ-1263"
                    elif finding.kind == "unknown_field":
                        condition, requirement = "unknown_field", "REQ-0125"
                    elif finding.kind == "unavailable_dependency":
                        if scope.target is None or finding.dependency is None:
                            raise ValueError(
                                "native dependency finding without a target index"
                            )
                        condition, requirement = "unknown_field", "REQ-1263"
                        context["identifier"] = scope.target.dependencies[
                            finding.dependency
                        ]
                        context["read"] = read.target
                    else:
                        raise ValueError("unknown native intermediate read finding")
                    diagnostics.append(
                        _diagnostic(
                            condition, read.path, context, requirement=requirement
                        )
                    )
    order = [identifier for identifier in planned if identifier in reads]
    graph = {
        reader: {read.target for read in entries} for reader, entries in reads.items()
    }
    cycle = (
        _find_cycle(order, graph)
        if dependency_analyzer is None
        else dependency_analyzer(order, graph).cycle
    )
    if cycle is not None:
        members = cycle[:-1]
        paths = tuple(
            dict.fromkeys(
                next(
                    read.path
                    for read in reads[member]
                    if read.target == cycle[position + 1]
                )
                for position, member in enumerate(members)
            )
        )
        diagnostics.append(
            _diagnostic(
                "dependency_cycle",
                paths,
                {"cycle": list(cycle)},
                requirement="REQ-1263",
            )
        )


def _reference_intermediate_reads(
    planned: Mapping[str, PlannedIntermediate],
    reads: Mapping[str, Sequence[_IntermediateRead]],
    dataset_fields: Mapping[str, Mapping[str, ColumnType]],
    diagnostics: list[ExecutionDiagnostic],
) -> None:
    """Unchanged donor visibility rules for the default Python reference planner."""
    for reader, entries in reads.items():
        for read in entries:
            if read.target == reader:
                # A read of the reader itself is the one-member cycle below.
                continue
            target = planned.get(read.target)
            if target is None:
                # The read intermediate's own validation already reported
                # why it has no plan.
                continue
            identifier = f"{read.target}.{read.field}"
            if target.dataset == "SELF":
                # REQ-1263: completed rows do not exist while donor records
                # are augmented.
                diagnostics.append(
                    _diagnostic(
                        "phase_boundary",
                        read.path,
                        {"intermediate": reader, "identifier": identifier},
                        requirement="REQ-1263",
                    )
                )
                continue
            readable = set(dataset_fields.get(target.dataset, ())) | {
                name for name, _ in target.derived
            }
            if read.field not in readable or (
                target.readable_columns and read.field not in target.readable_columns
            ):
                diagnostics.append(
                    _diagnostic(
                        "unknown_field",
                        read.path,
                        {"intermediate": reader, "identifier": identifier},
                        requirement="REQ-0125",
                    )
                )
            for name in target.dependencies:
                qualifier, separator, field = name.partition(".")
                supplied = (
                    qualifier == read.dataset and field in read.visible
                    if separator
                    else name in read.visible
                )
                if supplied:
                    continue
                diagnostics.append(
                    _diagnostic(
                        "unknown_field",
                        read.path,
                        {
                            "intermediate": reader,
                            "identifier": name,
                            "read": read.target,
                        },
                        requirement="REQ-1263",
                    )
                )


# REQ-1185: the declared result type of each operation an intermediate
# derivation may use, so a derived target-side key field type-checks
# against its driver-side match value.
_DERIVED_RESULT_TYPES: dict[str, ColumnType] = {
    "str_case": "str",
}


@dataclass(frozen=True, slots=True)
class _IntermediateRead:
    """One REQ-1263 read of another intermediate by an intermediate derivation.

    `visible` is what the donor record supplies at the reading derivation:
    the reader dataset's stored fields plus the derivations declared before
    it. The read intermediate's match must be answerable from exactly that.
    """

    reader: str
    dataset: str
    target: str
    field: str
    path: str
    visible: frozenset[str]


def _in_window(reference_path: str, derivation_path: str) -> bool:
    """Say whether a derivation reference sits inside a window operation.

    REQ-1263: a window's fields read the partitioned donor records only,
    so a read of another intermediate there is out of scope.
    """
    remainder = reference_path[len(derivation_path) :].split(".")
    return any(segment.split("[", 1)[0] in WINDOW_OPERATIONS for segment in remainder)


def _validate_intermediate_derivations(
    intermediate: Intermediate,
    path: str,
    fields: Mapping[str, ColumnType],
    supported_operations: Collection[str],
    diagnostics: list[ExecutionDiagnostic],
    unsupported: list[UnsupportedFeature],
    dataset_fields: Mapping[str, Collection[str]] | None = None,
    intermediate_ids: Collection[str] = (),
    reads: list[_IntermediateRead] | None = None,
    driver_reads: list[_Reference] | None = None,
    row_drivers: Collection[str | None] = (),
) -> dict[str, HandledExpression]:
    """Validate one intermediate's REQ-1185 derivations.

    A derivation reads the intermediate's own stored dataset fields plus the
    derivations declared before it in the same map: a bare name means the
    dataset's field or an earlier derived name, and a qualified name must
    name the dataset or the current driver. Driver reads are checked against
    the actual driver at each use, just like a correlated filter. A name
    qualified by another declared intermediate is a REQ-1263 read, collected
    into `reads` for `_validate_intermediate_reads` once every intermediate
    is planned. Anything else - a later derivation in the same map, a read of
    the driver or another intermediate inside a window, or an unknown field -
    fails as `unknown_field`. A derived name must not shadow a stored column. Returns
    the valid declarations in author order.
    """
    dataset = intermediate.dataset
    derived: dict[str, HandledExpression] = {}
    for name, declaration in (intermediate.derivations or {}).items():
        derivation_path = f"{path}.derivations.{name}"
        if name in fields:
            # REQ-1185: the stored column would be unreachable under the
            # derived name, so the declaration is rejected, not merged.
            diagnostics.append(
                _diagnostic(
                    "duplicate_derivation",
                    derivation_path,
                    {
                        "intermediate": intermediate.id,
                        "derivation": name,
                        "identifier": f"{dataset}.{name}",
                    },
                    requirement="REQ-1185",
                )
            )
            continue
        info = _expression_info(
            declaration.value,
            expression_path(derivation_path, declaration),
            supported_operations,
            dataset_fields=dataset_fields,
            scope=_Scope(column_phase=False),
        )
        unsupported.extend(info.unsupported)
        diagnostics.extend(info.diagnostics)
        ok = True
        name_reads: list[_IntermediateRead] = []
        name_driver_reads: list[_Reference] = []
        for reference in info.references:
            identifier = reference.name
            if "." in identifier:
                qualifier, _, field = identifier.partition(".")
                # REQ-1185: derivations evaluate in declaration order, so a
                # reference may read an earlier derived name as well as a
                # stored field; a later one is not in scope yet.
                allowed = qualifier == dataset and (field in fields or field in derived)
                if (
                    not allowed
                    and qualifier != dataset
                    and (qualifier not in intermediate_ids or qualifier in row_drivers)
                    and field in (dataset_fields or {}).get(qualifier, ())
                    and not _in_window(reference.path, derivation_path)
                ):
                    name_driver_reads.append(reference)
                    continue
                if (
                    not allowed
                    and qualifier in intermediate_ids
                    and not _in_window(reference.path, derivation_path)
                ):
                    # REQ-1263: another intermediate's column, matched for
                    # this donor record; checked once every plan exists.
                    name_reads.append(
                        _IntermediateRead(
                            reader=intermediate.id,
                            dataset=dataset,
                            target=qualifier,
                            field=field,
                            path=reference.path,
                            visible=frozenset(fields) | frozenset(derived),
                        )
                    )
                    continue
            else:
                allowed = identifier in fields or identifier in derived
            if allowed:
                continue
            windowed_read = identifier.partition(".")[0] in intermediate_ids
            diagnostics.append(
                _diagnostic(
                    "unknown_field",
                    reference.path,
                    {"intermediate": intermediate.id, "identifier": identifier},
                    # REQ-1263 keeps another intermediate out of a window.
                    requirement="REQ-1263" if windowed_read else "REQ-1185",
                )
            )
            ok = False
        if ok:
            derived[name] = declaration
            if reads is not None:
                reads.extend(name_reads)
            if driver_reads is not None:
                driver_reads.extend(name_driver_reads)
    return derived


def _validate_intermediate_between(
    identifier: str,
    between: RecordBetween,
    path: str,
    fields: Mapping[str, ColumnType],
    bindings: BindingPlan,
    column_types: Mapping[str, ColumnType],
    diagnostics: list[ExecutionDiagnostic],
    reference_compiler: ReferenceCompiler | None = None,
) -> bool:
    """Check the closed range an intermediate matches by, before any data is read."""
    if between.lower is None and between.upper is None:
        # REQ-1049: a between with no bound narrows by nothing; the
        # declaration is refused rather than silently ignored.
        diagnostics.append(
            _diagnostic(
                "invalid_field_type",
                f"{path}.between",
                {
                    "intermediate": identifier,
                    "expected": "at least one of lower, upper",
                },
                requirement="REQ-1049",
            )
        )
        return True
    return _check_between(
        identifier,
        between.value,
        between.lower,
        between.upper,
        path,
        fields,
        bindings,
        column_types,
        diagnostics,
        reference_compiler=reference_compiler,
    )


def _check_between(
    identifier: str | None,
    value: str,
    lower: str | None,
    upper: str | None,
    path: str,
    fields: Mapping[str, ColumnType],
    bindings: BindingPlan,
    column_types: Mapping[str, ColumnType],
    diagnostics: list[ExecutionDiagnostic],
    reference_compiler: ReferenceCompiler | None = None,
) -> bool:
    """Check the closed range an intermediate matches by, before any data is read."""
    context = {"intermediate": identifier} if identifier is not None else {}
    stated = [
        (name, side)
        for side, name in (("lower", lower), ("upper", upper))
        if name is not None
    ]
    missing = [name for name, _ in stated if name not in fields]
    if missing:
        # REQ-0121: a bound naming a column the dataset does not have.
        diagnostics.extend(
            _diagnostic(
                "unknown_field",
                f"{path}.between",
                {**context, "identifier": name},
                requirement="REQ-0121",
            )
            for name in missing
        )
        return True
    value_type = _reference_type(value, bindings, column_types)
    if value_type is None:
        diagnostics.append(
            _diagnostic(
                "unknown_field",
                f"{path}.between.value",
                {**context, "identifier": value},
                requirement="REQ-0121",
            )
        )
        return True
    types = {side: fields[name] for name, side in stated}
    if all(
        _comparable_types(value_type, bound_type, reference_compiler=reference_compiler)
        for bound_type in types.values()
    ):
        return False
    # REQ-0121: report the runtime types before any record is compared.
    diagnostics.append(
        _diagnostic(
            "incomparable_range_types",
            f"{path}.between",
            {
                **context,
                "value_type": value_type,
                **{f"{side}_type": bound_type for side, bound_type in types.items()},
            },
            requirement="REQ-0121",
        )
    )
    return True


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
                return _rooted_at_first_declared(cycle, position)
    return None


def _rooted_at_first_declared(
    cycle: tuple[str, ...],
    position: Mapping[str, int],
) -> tuple[str, ...]:
    """Rotate a cycle so it starts at its earliest declared member.

    REQ-0072 asks for the cycle path, and a traversal reports whichever
    member it happened to enter from. Two runtimes would then name one
    cycle two ways, so the reported path is rotated to one spelling: the
    member declared first, which is also the order its `spec_paths` take.
    """
    members = cycle[:-1]
    start = min(range(len(members)), key=lambda index: position[members[index]])
    rotated = members[start:] + members[:start]
    return (*rotated, rotated[0])


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


def _coverage_diagnostics(
    specification: Specification,
    supported_operations: Collection[str],
) -> list[ExecutionDiagnostic]:
    diagnostics: list[ExecutionDiagnostic] = []
    rows = specification.rows or ()
    declared = {column.name for column in specification.columns}
    overridden = [
        column.name
        for column in specification.columns
        if column.derivation is not None
        and any(column.name in row.derivations for row in rows)
    ]
    column_phase_only = (
        _column_level_reads(specification, supported_operations)[1]
        if overridden
        else frozenset()
    )

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
        # REQ-0199: a column-level derivation is the column's default; a row
        # template naming the column overrides it for that template only, so
        # the two placements together are covered, not duplicated. REQ-1260:
        # a derivation that is not row-local cannot be a default, so pairing
        # it with a template's derivation still derives the column twice.
        if at_column and any(at_rows) and column.name in column_phase_only:
            diagnostics.append(
                _diagnostic(
                    "duplicate_derivation",
                    path,
                    {
                        "column": column.name,
                        "rows": [
                            row.id for row, present in zip(rows, at_rows) if present
                        ],
                    },
                    requirement="REQ-1260",
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
                    requirement="REQ-0200",
                )
            )
        elif not rows and not at_column:
            diagnostics.append(
                _diagnostic(
                    "missing_derivation",
                    path,
                    {"column": column.name},
                    requirement="REQ-0198",
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


def _intermediate_ids(specification: Specification) -> frozenset[str]:
    return frozenset(
        intermediate.id for intermediate in specification.intermediates or ()
    )


def _keep_intermediate_ids(specification: Specification) -> frozenset[str]:
    """Return the ids of intermediates that declare `keep`.

    The planned selection only honors `keep` when `order_by` is present
    (REQ-0119), so the static single-record-per-row promise mirrors it.
    """
    return frozenset(
        intermediate.id
        for intermediate in specification.intermediates or ()
        if intermediate.keep is not None and intermediate.order_by is not None
    )


def _row_scope(
    specification: Specification,
    row: Row,
    driver: str | None,
) -> _Scope:
    """Return where this template's row derivations sit."""
    intermediates = _intermediate_ids(specification)
    keep_intermediates = _keep_intermediate_ids(specification)
    if row.group_by is None or driver is None:
        return _Scope(
            column_phase=False,
            intermediates=intermediates,
            keep_intermediates=keep_intermediates,
        )
    return _Scope(
        column_phase=False,
        grouped_driver=driver,
        group_variables=tuple(row.group_by),
        intermediates=intermediates,
        keep_intermediates=keep_intermediates,
    )


def _group_by_declaration(
    row: Row,
    index: int,
    driver: str | None,
) -> list[ExecutionDiagnostic]:
    """Check the keys a grouped template declares, before any record is read."""
    if row.group_by is None:
        return []
    path = f"rows[{index}].group_by"
    names = tuple(row.group_by)
    if not names or len(set(names)) != len(names):
        # REQ-0065: empty keys name no partition and a repeated column
        # states the same one twice.
        return [
            _diagnostic(
                "invalid_field_type",
                path,
                {"row": row.id, "group_by": list(names)},
                requirement="REQ-0065",
            )
        ]
    if driver is None:
        return []
    return [
        _diagnostic(
            "unknown_field",
            path,
            {"row": row.id, "identifier": name, "dataset": driver},
            requirement="REQ-0066",
        )
        for name in names
        if not name.startswith(f"{driver}.") or name.count(".") != 1
    ]


def _lookup_declarations(
    specification: Specification,
) -> list[ExecutionDiagnostic]:
    """Check every `intermediates` entry that needs no source data.

    REQ-0113 through REQ-0115 and REQ-0119 are decided by the declaration alone,
    so they are answered before a dataset is read rather than on the first
    row that reaches the intermediate.
    """
    diagnostics: list[ExecutionDiagnostic] = []
    seen: dict[str, str] = {}
    for index, intermediate in enumerate(specification.intermediates or ()):
        path = f"intermediates[{index}]"
        collision = None
        if intermediate.id in specification.input:
            collision = f"input.{intermediate.id}"
        elif intermediate.id == specification.domain:
            collision = "domain"
        elif intermediate.id in seen:
            collision = seen[intermediate.id]
        if collision is not None:
            # REQ-0113: the id shares one namespace with dataset identifiers,
            # so a qualified name would otherwise reach two relations.
            diagnostics.append(
                _diagnostic(
                    "duplicate_identifier",
                    (f"{path}.id", collision),
                    {"identifier": intermediate.id},
                    requirement="REQ-0113",
                )
            )
        seen.setdefault(intermediate.id, f"{path}.id")
        if intermediate.dataset not in specification.input and not (
            intermediate.dataset == "SELF" and specification.rows
        ):
            diagnostics.append(
                _diagnostic(
                    "unknown_field",
                    f"{path}.dataset",
                    {
                        "intermediate": intermediate.id,
                        "identifier": intermediate.dataset,
                    },
                    requirement="REQ-0120",
                )
            )
        if intermediate.dataset == "SELF" and "SELF" in specification.input:
            diagnostics.append(
                _diagnostic(
                    "duplicate_identifier",
                    (f"{path}.dataset", "input.SELF"),
                    {"identifier": "SELF"},
                    requirement="REQ-0113",
                )
            )
        for declared, missing in (
            ("order_by", "keep"),
            ("keep", "order_by"),
        ):
            if (
                getattr(intermediate, declared) is not None
                and getattr(intermediate, missing) is None
            ):
                diagnostics.append(
                    _diagnostic(
                        "unpaired_fields",
                        path,
                        {
                            "intermediate": intermediate.id,
                            "declared": [declared],
                            "missing": [missing],
                        },
                        requirement="REQ-0119",
                    )
                )
    return diagnostics


def _warning_verification_paths(specification: Specification) -> tuple[str, ...]:
    """Return every declaration whose normalized severity is ``warning``."""
    paths: list[str] = []
    for column in specification.columns:
        for index, verification in enumerate(column.verifications or ()):
            operation = verification.operation
            payload = verification.root[operation]
            if isinstance(payload, Mapping) and payload.get("severity") == "warning":
                paths.append(
                    f"columns.{column.name}.verifications[{index}].{operation}.severity"
                )
    for index, verification in enumerate(specification.verifications or ()):
        operation = verification.operation
        payload = verification.root[operation]
        if isinstance(payload, Mapping) and payload.get("severity") == "warning":
            paths.append(f"verifications[{index}].{operation}.severity")
    return tuple(paths)


def _sidecar_declarations(
    specification: Specification,
) -> list[ExecutionDiagnostic]:
    """Validate warning, warning-log, and verification-log declarations."""
    diagnostics: list[ExecutionDiagnostic] = []
    warning_paths = _warning_verification_paths(specification)
    output = specification.output
    log, report = output.warning_log, output.verification_log
    if warning_paths and log is None:
        diagnostics.append(
            _diagnostic(
                "missing_warning_log",
                "output.warning_log",
                {"warnings": list(warning_paths)},
                requirement="REQ-0391",
            )
        )
    for field, path in (
        ("output.warning_log", log),
        ("output.verification_log", report),
    ):
        if path is not None and profile_of(path) is None:
            diagnostics.append(
                _diagnostic(
                    "unknown_artifact_profile",
                    field,
                    {"path": path, "permitted": [".csv", ".parquet"]},
                    requirement="REQ-0760",
                )
            )
    # REQ-0756 and REQ-1180: the primary artifact and the two sidecars name
    # three different files.
    for (left_field, left_path), (right_field, right_path), requirement in (
        (("output.path", output.path), ("output.warning_log", log), "REQ-0756"),
        (
            ("output.path", output.path),
            ("output.verification_log", report),
            "REQ-1180",
        ),
        (
            ("output.warning_log", log),
            ("output.verification_log", report),
            "REQ-1180",
        ),
    ):
        if left_path is None or right_path is None or left_path != right_path:
            continue
        diagnostics.append(
            _diagnostic(
                "artifact_path_collision",
                (left_field, right_field),
                {"path": left_path},
                requirement=requirement,
            )
        )
    return diagnostics


def _odm_read_diagnostics(specification: Specification) -> list[ExecutionDiagnostic]:
    """Check every `odm` read against the rows it can be read for.

    REQ-1270 gives an `odm` read a scope only in a row built from its ODM
    input, so the check needs nothing but the specification and runs before
    any source is read. REQ-1271 confines `filter` to schema fields.
    """
    diagnostics: list[ExecutionDiagnostic] = []
    rows = specification.rows or ()

    def driver(row: Row) -> str | None:
        if row.dataset is not None:
            return row.dataset
        if len(specification.input) == 1:
            return next(iter(specification.input))
        return None

    for site in odm_read_sites(specification):
        dataset = site.read.dataset
        if dataset not in specification.input:
            diagnostics.append(
                _diagnostic(
                    "unknown_field",
                    f"{site.path}.item",
                    {"identifier": site.read.item},
                    requirement="REQ-0103",
                )
            )
            continue
        if site.location in ("intermediate", "derive"):
            scoped = False
        elif site.location == "row":
            assert site.row_index is not None
            scoped = driver(rows[site.row_index]) == dataset
        elif rows:
            scoped = all(
                driver(row) == dataset
                for row in rows
                if site.column_name not in row.derivations
            )
        else:
            scoped = specification.default_driver == dataset
        if not scoped:
            diagnostics.append(
                _diagnostic(
                    "invalid_odm_context",
                    site.path,
                    {"dataset": dataset, "location": site.location},
                    requirement="REQ-1277",
                )
            )
        if site.read.filter is None:
            continue
        filter_path = f"{site.path}.filter"
        ast = _parse_predicate_at(site.read.filter, filter_path, diagnostics)
        if ast is None:
            continue
        for name in predicate_identifiers(ast):
            qualifier, _, field = name.partition(".")
            if qualifier != dataset or field not in ODM_SCHEMA_FIELDS:
                diagnostics.append(
                    _diagnostic(
                        "unknown_field",
                        filter_path,
                        {"identifier": name, "dataset": dataset},
                        requirement="REQ-1271",
                    )
                )
    return diagnostics


def _preflight_findings(
    specification: Specification,
    supported_operations: Collection[str],
) -> tuple[list[ExecutionDiagnostic], list[UnsupportedFeature]]:
    """Find source-independent failures before any dataset is ingested."""
    diagnostics = (
        []
        if specification.parents
        else _coverage_diagnostics(specification, supported_operations)
    )
    unsupported: list[UnsupportedFeature] = []

    if specification.domain in specification.input:
        diagnostics.append(
            _diagnostic(
                "duplicate_identifier",
                (f"input.{specification.domain}", "domain"),
                {"identifier": specification.domain},
                requirement="REQ-0080",
            )
        )
    if specification.parents:
        unsupported.append(
            UnsupportedFeature(operation="inheritance", spec_path="parents")
        )
    diagnostics.extend(_lookup_declarations(specification))
    diagnostics.extend(_sidecar_declarations(specification))
    if not specification.parents:
        diagnostics.extend(_odm_read_diagnostics(specification))
        declared_columns = {column.name for column in specification.columns}
        for index, key in enumerate(specification.keys):
            if key not in declared_columns:
                diagnostics.append(
                    _diagnostic(
                        "undeclared_column",
                        f"keys[{index}]",
                        {"column": key},
                        requirement="REQ-0220",
                    )
                )

    rows = specification.rows or ()
    if not specification.parents:
        if not rows and specification.default_driver is None:
            diagnostics.append(_diagnostic("driver_unavailable", "base", {"row": None}))
        if (
            specification.base is not None
            and specification.base not in specification.input
        ):
            diagnostics.append(
                _diagnostic(
                    "driver_unavailable",
                    "base",
                    {"dataset": specification.base},
                )
            )
        if specification.filter is not None and rows:
            diagnostics.append(
                _diagnostic(
                    "conflicting_row_construction",
                    ("filter", "rows"),
                    {},
                    requirement="REQ-1171",
                )
            )

    for index, row in enumerate(rows):
        if not specification.parents:
            driver = row.dataset
            if driver is None and len(specification.input) == 1:
                driver = next(iter(specification.input))
            if driver not in specification.input and driver not in _intermediate_ids(
                specification
            ):
                diagnostics.append(
                    _diagnostic(
                        "driver_unavailable",
                        f"rows[{index}].dataset",
                        {"row": row.id, "dataset": driver},
                    )
                )
        driver = row.dataset
        if driver is None and len(specification.input) == 1:
            driver = next(iter(specification.input))
        diagnostics.extend(_group_by_declaration(row, index, driver))
        scope = _row_scope(specification, row, driver)
        for name, declaration in row.derivations.items():
            path = f"rows[{index}].derivations.{name}"
            unsupported.extend(
                _expression_info(
                    declaration.value,
                    expression_path(path, declaration),
                    supported_operations,
                    scope=scope,
                ).unsupported
            )

    column_scope = _Scope(
        intermediates=_intermediate_ids(specification),
        keep_intermediates=_keep_intermediate_ids(specification),
    )
    for column in specification.columns:
        declaration = column.derivation
        if declaration is None:
            continue
        path = f"columns.{column.name}.derivation"
        unsupported.extend(
            _expression_info(
                declaration.value,
                expression_path(path, declaration),
                supported_operations,
                scope=column_scope,
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


# REQ-1260: operations whose column derivations cannot become row-template
# defaults. An ODM item read can use a row template's scope, so it is omitted.
_DATASET_LEVEL_OPERATIONS = frozenset({"aggregate", *WINDOW_OPERATIONS})


def _column_level_reads(
    specification: Specification,
    supported_operations: Collection[str],
) -> tuple[dict[str, frozenset[str]], frozenset[str]]:
    """Return each column-level derivation's bare reads and the non-row-local names.

    REQ-1260: a column-level derivation is row-local unless it uses a
    dataset-level operation, reads a named intermediate, or reads another
    column-level column that is not row-local.
    """
    intermediates = _intermediate_ids(specification)
    # With the dataset-level operations withheld, the planner's own walk
    # reports each one it meets, however deeply nested, as unsupported.
    row_local_operations = tuple(
        operation
        for operation in supported_operations
        if operation not in _DATASET_LEVEL_OPERATIONS
    )
    reads: dict[str, frozenset[str]] = {}
    blocked: set[str] = set()
    for column in specification.columns:
        if column.derivation is None:
            continue
        info = _expression_info(
            column.derivation.value,
            f"columns.{column.name}.derivation",
            row_local_operations,
            scope=_Scope(intermediates=intermediates),
        )
        if any(
            feature.operation in _DATASET_LEVEL_OPERATIONS
            for feature in info.unsupported
        ) or any(
            reference.name.partition(".")[0] in intermediates
            for reference in info.references
            if "." in reference.name
        ):
            blocked.add(column.name)
        reads[column.name] = frozenset(
            reference.name for reference in info.references if "." not in reference.name
        )
    changed = True
    while changed:
        changed = False
        for name, names in reads.items():
            if name not in blocked and names & blocked:
                blocked.add(name)
                changed = True
    return reads, frozenset(blocked)


def _row_phase_default_columns(
    specification: Specification,
    supported_operations: Collection[str],
    dataset_fields: Mapping[str, Mapping[str, ColumnType]],
) -> frozenset[str]:
    """Column names whose column-level derivations are row-phase defaults.

    REQ-1260: a row-local column-level derivation is a default when a row
    template names its column, or when a row-phase context reads the column:
    a template derivation (windows included), a grouped template filter, a
    `SELF` intermediate's donor field, or a match value of a named
    intermediate a template derivation reads. A default's own reads of
    column-level columns are defaults in turn. The references come from the
    planner's own expression walk, so a qualified field or a string literal
    that happens to spell a column name promotes nothing.
    """
    rows = specification.rows or ()
    if not rows:
        # Without row templates there is no row phase; column-level
        # derivations keep their column-phase meaning.
        return frozenset()
    reads, blocked = _column_level_reads(specification, supported_operations)
    candidates = reads.keys() - blocked
    if not candidates:
        return frozenset()

    intermediates = specification.intermediates or ()
    intermediate_ids = _intermediate_ids(specification)
    self_ids = {
        intermediate.id
        for intermediate in intermediates
        if intermediate.dataset == "SELF"
    }
    referenced: set[str] = set()

    def references(
        declaration: HandledExpression, path: str, scope: _Scope = _COLUMN_SCOPE
    ) -> tuple[_Reference, ...]:
        return _expression_info(
            declaration.value, path, supported_operations, scope=scope
        ).references

    def donor_reads(found: Sequence[_Reference]) -> set[str]:
        # REQ-0120: a read through a SELF intermediate names a donor field,
        # and donor fields are row-derived output columns.
        return {
            field
            for qualifier, separator, field in (
                reference.name.partition(".") for reference in found
            )
            if separator and qualifier in self_ids
        }

    def match_values(intermediate: Intermediate, index: int) -> tuple[str, ...]:
        # REQ-0112: the match values are the key's (REQ-0115), which an
        # omitted key infers from the applicable keys (REQ-0153). SELF's
        # applicable keys are its donor fields, which this promotion decides,
        # so an omitted SELF key promotes nothing. A qualified driver field
        # names no column.
        key: object = intermediate.key
        if key is None and intermediate.dataset != "SELF":
            fields = dataset_fields.get(intermediate.dataset, {})
            key = [name for name in specification.keys if name in fields]
        pairs = key_pairs(key) if key is not None else None
        names: list[str] = []
        for field, entry in zip(*(pairs or ((), ())), strict=True):
            if isinstance(entry, str):
                names.append(entry)
            elif isinstance(entry, Expression):
                info = _expression_info(
                    entry,
                    f"intermediates[{index}].key.{field}",
                    supported_operations,
                )
                names.extend(reference.name for reference in info.references)
        if intermediate.between is not None:
            names.append(intermediate.between.value)
        return tuple(name for name in names if "." not in name)

    matches = {
        intermediate.id: match_values(intermediate, index)
        for index, intermediate in enumerate(intermediates)
    }

    def match_reads(found: Sequence[_Reference]) -> set[str]:
        # REQ-0126: a row-phase read of a named intermediate needs every
        # value it matches on from the reading template.
        return {
            name
            for qualifier, separator, _ in (
                reference.name.partition(".") for reference in found
            )
            if separator
            for name in matches.get(qualifier, ())
        }

    for index, row in enumerate(rows):
        driver = row.dataset
        if driver is None and len(specification.input) == 1:
            driver = next(iter(specification.input))
        scope = _row_scope(specification, row, driver)
        for name, declaration in row.derivations.items():
            found = references(declaration, f"rows[{index}].derivations.{name}", scope)
            referenced.update(
                reference.name for reference in found if "." not in reference.name
            )
            referenced.update(donor_reads(found))
            referenced.update(match_reads(found))
        if row.filter is not None:
            # REQ-0068: a row template filter reads the candidate's columns:
            # completed columns for a grouped template, derived columns and
            # lookup state for an ungrouped one. A lookup read also promotes
            # the column-level values it matches on into the row phase.
            try:
                filter_ast = parse_predicate(row.filter)
            except PredicateError:
                pass
            else:
                filter_names = predicate_identifiers(filter_ast)
                referenced.update(name for name in filter_names if "." not in name)
                if row.group_by is None:
                    referenced.update(
                        match
                        for name in filter_names
                        for qualifier, separator, _ in (name.partition("."),)
                        if separator and qualifier != driver
                        for match in matches.get(qualifier, ())
                    )
    for column in specification.columns:
        if column.derivation is not None:
            referenced.update(
                donor_reads(
                    references(
                        column.derivation,
                        f"columns.{column.name}.derivation",
                        _Scope(intermediates=intermediate_ids),
                    )
                )
            )
    for index, intermediate in enumerate(intermediates):
        if intermediate.dataset != "SELF":
            continue
        # REQ-0120: SELF's own fields are donor fields, named bare or as
        # `SELF.field`.
        names = [*(intermediate.key or ()), *(intermediate.columns or ())]
        names.extend(term.variable for term in intermediate.order_by or ())
        if intermediate.between is not None:
            # REQ-0121: the bounds are donor fields; the value is the current
            # row's, promoted only by a row-phase read (see `match_reads`).
            # REQ-1049: one-sided between leaves the absent bound as None.
            names.extend(
                bound
                for bound in (
                    intermediate.between.lower,
                    intermediate.between.upper,
                )
                if bound is not None
            )
        if intermediate.verifications:
            # REQ-1245: the asserted-unique columns are donor fields.
            for verification in intermediate.verifications:
                unique = verification.unique
                names.extend(unique if isinstance(unique, list) else unique.columns)
        if intermediate.filter is not None:
            try:
                names.extend(
                    predicate_identifiers(parse_predicate(intermediate.filter))
                )
            except PredicateError:
                pass
        for name, declaration in (intermediate.derivations or {}).items():
            names.extend(
                reference.name
                for reference in references(
                    declaration, f"intermediates[{index}].derivations.{name}"
                )
            )
        for name in names:
            qualifier, separator, field = name.partition(".")
            if not separator:
                referenced.add(name)
            elif qualifier == "SELF":
                referenced.add(field)

    promoted: set[str] = set()
    pending = [
        name
        for name in sorted(candidates)
        if name in referenced or any(name in row.derivations for row in rows)
    ]
    while pending:
        name = pending.pop()
        if name not in promoted:
            promoted.add(name)
            pending.extend(sorted(reads[name] & candidates))
    return frozenset(promoted)


def _intermediate_driver_type(
    expression: Expression,
    fields: Mapping[str, ColumnType],
    intermediates: Mapping[str, Mapping[str, ColumnType]] | None = None,
) -> ColumnType | None:
    """Find a stable type for a projected intermediate derivation.

    `intermediates` holds the known field types of every declared
    intermediate, so a REQ-1263 read keeps the read column's type.
    """
    operation = expression.operation
    value = expression.root[operation]
    if operation in ("row_number", "rank"):
        return "int"
    if operation in _DERIVED_RESULT_TYPES:
        return _DERIVED_RESULT_TYPES[operation]
    if operation == "source":
        variable = value.get("variable") if isinstance(value, Mapping) else value
        if isinstance(variable, str):
            if variable in fields:
                return fields[variable]
            qualifier, _, field = variable.partition(".")
            if intermediates is not None and qualifier in intermediates:
                return intermediates[qualifier].get(field)
            return fields.get(field)
    if operation == "literal":
        if isinstance(value, str):
            return "str"
        if isinstance(value, int) and not isinstance(value, bool):
            return "int"
        if isinstance(value, float):
            return "float"
    if (
        operation == "case"
        and isinstance(value, Sequence)
        and not isinstance(value, str)
    ):
        branch_types: set[ColumnType] = set()
        for branch in value:
            if not isinstance(branch, Mapping):
                return None
            result = branch.get("then", branch.get("otherwise"))
            if not isinstance(result, Mapping):
                return None
            try:
                nested = Expression.model_validate(result)
            except ValidationError:
                return None
            branch_type = _intermediate_driver_type(nested, fields, intermediates)
            if branch_type is None:
                if result != {"literal": None}:
                    return None
                continue
            branch_types.add(branch_type)
        if len(branch_types) == 1:
            return next(iter(branch_types))
    return None


def _intermediate_field_types(
    declared: Mapping[str, Intermediate],
    bindings: BindingPlan,
) -> dict[str, dict[str, ColumnType]]:
    """Return the stored and typed derived fields of each input intermediate.

    REQ-1263 reads may chain through several intermediates in any
    declaration order, so derived types settle over repeated passes until
    no pass types another name.
    """
    types = {
        identifier: {
            column.name: column.type
            for column in bindings.datasets[item.dataset].columns
        }
        for identifier, item in declared.items()
        if item.dataset in bindings.datasets
    }
    changed = True
    while changed:
        changed = False
        for identifier, fields in types.items():
            for name, derivation in (declared[identifier].derivations or {}).items():
                if name in fields:
                    continue
                inferred = _intermediate_driver_type(derivation.value, fields, types)
                if inferred is not None:
                    fields[name] = inferred
                    changed = True
    return types


def _bind_intermediate_drivers(
    specification: Specification,
    bindings: BindingPlan,
    diagnostics: list[ExecutionDiagnostic],
    supported_operations: Collection[str],
) -> BindingPlan:
    """Expose source-only intermediate records as typed row drivers."""
    declared = {item.id: item for item in specification.intermediates or ()}
    intermediate_types = _intermediate_field_types(declared, bindings)
    datasets = dict(bindings.datasets)
    for index, row in enumerate(specification.rows or ()):
        identifier = row.dataset
        if (
            identifier is None
            or identifier not in declared
            or identifier in specification.input
        ):
            continue
        item = declared[identifier]
        prohibited = [
            field
            for field in ("key", "between", "order_by", "keep")
            if getattr(item, field) is not None
        ]
        if item.dataset not in specification.input:
            prohibited.append("dataset")
        if item.filter is not None:
            try:
                filter_names = predicate_identifiers(parse_predicate(item.filter))
            except PredicateError:
                filter_names = ()
            if any(name.partition(".")[0] != item.dataset for name in filter_names):
                prohibited.append("filter")
        for name, declaration in (item.derivations or {}).items():
            info = _expression_info(
                declaration.value,
                f"intermediates.{identifier}.derivations.{name}",
                supported_operations,
                scope=_Scope(column_phase=False),
            )
            if any(
                ref.name.partition(".")[0] in specification.input
                and ref.name.partition(".")[0] != item.dataset
                for ref in info.references
                if "." in ref.name
            ):
                prohibited.append("derivations")
                break
        if "no_match" in item.model_fields_set:
            prohibited.append("no_match")
        if prohibited:
            diagnostics.append(
                _diagnostic(
                    "invalid_intermediate_driver",
                    f"rows[{index}].dataset",
                    {"intermediate": identifier, "fields": prohibited},
                    requirement="REQ-1262",
                )
            )
            continue
        if item.dataset not in bindings.datasets:
            continue
        source = bindings.datasets[item.dataset]
        types: dict[str, ColumnType] = {
            column.name: column.type for column in source.columns
        }
        for name, derivation in (item.derivations or {}).items():
            inferred = _intermediate_driver_type(
                derivation.value, types, intermediate_types
            )
            if inferred is not None:
                types[name] = inferred
        visible = (
            item.columns
            if item.columns is not None
            else [
                *source.field_names,
                *(item.derivations or {}),
            ]
        )
        unknown = [name for name in visible if name not in types]
        if unknown:
            diagnostics.append(
                _diagnostic(
                    "unknown_intermediate_driver_type",
                    f"rows[{index}].dataset",
                    {"intermediate": identifier, "columns": unknown},
                    requirement="REQ-1262",
                )
            )
            continue
        datasets[identifier] = DatasetBinding(
            dataset=identifier,
            columns=tuple(TypedColumn(name=name, type=types[name]) for name in visible),
        )
    return bindings.model_copy(update={"datasets": datasets})


def plan_execution(
    specification: Specification,
    sources: Mapping[str, LoadedDataset | TypedTable],
    *,
    supported_operations: Collection[str] = INITIAL_OPERATIONS,
    dependency_analyzer: DependencyAnalyzer | None = None,
    column_dependency_analyzer: ColumnDependencyAnalyzer | None = None,
    reference_compiler_factory: ReferenceCompilerFactory | None = None,
) -> ExecutionPlan:
    """Validate and plan the initial record-driven execution subset.

    Validation failures take precedence over unsupported status. This keeps a
    broken dependency or phase boundary distinct from a valid declaration that
    belongs to a later runtime component.

    A trusted backend may supply shared bound-graph analysis. Reference planning
    retains its existing algorithms when the implementation port is absent.
    """
    diagnostics, unsupported = _preflight_findings(specification, supported_operations)
    declared_sources = tuple(specification.input)
    supplied_sources = tuple(sources)
    if set(declared_sources) != set(supplied_sources):
        diagnostics.append(
            _diagnostic(
                "source_provider_mismatch",
                "input",
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
                "input",
                {"reason": str(error)},
            )
        )
        raise ExecutionPlanningError(diagnostics) from error
    bindings = _bind_intermediate_drivers(
        specification, bindings, diagnostics, supported_operations
    )

    column_order = [column.name for column in specification.columns]
    column_positions = {name: index for index, name in enumerate(column_order)}
    column_types = {column.name: column.type for column in specification.columns}
    reference_compiler = (
        reference_compiler_factory(bindings, column_types)
        if reference_compiler_factory is not None
        else None
    )
    if reference_compiler is not None:
        bindings = bindings.with_reference_binder(reference_compiler.bind)
    # REQ-0120: a named intermediate's filter/order_by suggests the qualified
    # spelling, so the lookup datasets' columns ride along for suggestions.
    dataset_fields = {
        name: _dataset_types(bindings, name) for name in bindings.datasets
    }
    # REQ-1260: templates naming a default's column override it; the rest
    # inherit it, planned in each template's own row scope. Computed before
    # the intermediates, since a default is a SELF donor field.
    default_columns = _row_phase_default_columns(
        specification, supported_operations, dataset_fields
    )
    default_derivations = {
        column.name: column.derivation
        for column in specification.columns
        if column.name in default_columns
    }
    resolved_joins: list[ResolvedJoin] = []

    def infer_lookup_keys(
        dataset: str, path: str, deferred: list[ExecutionDiagnostic]
    ) -> tuple[str, ...] | None:
        """Infer omitted intermediate/aggregate keys (REQ-0153)."""
        return _infer_applicable_keys(
            specification,
            bindings,
            column_types,
            dataset,
            path,
            deferred,
            hint="declare the `source`/`key` pairs explicitly",
            requirement="REQ-0153",
            reference_compiler=reference_compiler,
        )

    intermediates = _plan_lookups(
        specification,
        bindings,
        column_types,
        diagnostics,
        resolved_joins,
        supported_operations,
        unsupported,
        default_columns,
        dependency_analyzer,
        reference_compiler,
    )
    row_plans: list[PlannedRow] = []
    row_references: dict[tuple[int, str], tuple[_Reference, ...]] = {}

    if not rows:
        if (
            specification.default_driver is not None
            and specification.default_driver in specification.input
        ):
            driver = specification.default_driver
            # REQ-1170: a root filter reads the base driver before derivation.
            filter_ast = None
            if specification.filter is not None:
                filter_ast = _parse_predicate_at(
                    specification.filter, "filter", diagnostics
                )
            if filter_ast is not None:
                for identifier in predicate_identifiers(filter_ast):
                    if "." in identifier:
                        _validate_qualified_reference(
                            _Reference(identifier, "filter"),
                            {driver},
                            bindings,
                            column_types,
                            diagnostics,
                            intermediates=intermediates,
                            reference_compiler=reference_compiler,
                        )
                    else:
                        diagnostics.append(
                            _diagnostic(
                                "phase_boundary",
                                "filter",
                                {
                                    "identifier": identifier,
                                    "available_phase": "column_derivation",
                                    "required_phase": "row_filter",
                                },
                            )
                        )
            row_plans.append(
                PlannedRow(
                    index=None,
                    declaration=None,
                    driver=driver,
                    filter_path="filter" if filter_ast is not None else None,
                    filter_predicate=filter_ast,
                )
            )
    else:
        for index, row in enumerate(rows):
            driver = row.dataset
            if driver is None and len(specification.input) == 1:
                driver = next(iter(specification.input))
            if driver is None or driver not in bindings.datasets:
                continue
            row_scope = _row_scope(specification, row, driver)
            grouped = row.group_by is not None
            filter_path = f"rows[{index}].filter" if row.filter is not None else None
            filter_ast = (
                _parse_predicate_at(row.filter, filter_path, diagnostics)
                if row.filter is not None and filter_path is not None
                else None
            )
            filter_names: tuple[str, ...] = ()
            if filter_ast is not None:
                path = filter_path or f"rows[{index}].filter"
                filter_names = predicate_identifiers(filter_ast)
                for identifier in filter_names:
                    if "." in identifier:
                        if grouped:
                            # REQ-0068: a grouped filter reads the candidate's
                            # completed columns, never a driver record.
                            diagnostics.append(
                                _diagnostic(
                                    "phase_boundary",
                                    path,
                                    {
                                        "identifier": identifier,
                                        "row": row.id,
                                        "available_phase": "row_construction",
                                        "required_phase": "grouped_row_filter",
                                    },
                                )
                            )
                        else:
                            _validate_qualified_reference(
                                _Reference(identifier, path),
                                {driver} if driver is not None else frozenset(),
                                bindings,
                                column_types,
                                diagnostics,
                                intermediates=intermediates,
                                reference_compiler=reference_compiler,
                            )

            derivations: dict[str, PlannedDerivation] = {}
            for name in column_order:
                declaration = row.derivations.get(name)
                # REQ-1260: a template without its own derivation inherits
                # the column-level default, planned in this template's scope.
                inherited = declaration is None and name in default_derivations
                if inherited:
                    declaration = default_derivations[name]
                if declaration is None:
                    continue
                planned, references, inferred_paths = _plan_derivation(
                    name,
                    declaration,
                    f"columns.{name}.derivation"
                    if inherited
                    else f"rows[{index}].derivations.{name}",
                    supported_operations,
                    diagnostics,
                    unsupported,
                    scope=row_scope,
                    infer_keys=infer_lookup_keys,
                    dataset_fields=dataset_fields,
                )
                annotated = _resolve_implicit_joins(
                    references,
                    specification,
                    bindings,
                    {driver},
                    intermediates,
                    column_types,
                    diagnostics,
                    reference_compiler=reference_compiler,
                )
                # REQ-0156/REQ-0157: a row-phase join matches the driver
                # record (or the group keys), not output columns.
                annotated = _row_join_match_variables(
                    annotated,
                    row=row,
                    driver=driver,
                    bindings=bindings,
                    diagnostics=diagnostics,
                    reference_compiler=reference_compiler,
                )
                derivations[name] = _with_relation_dependencies(
                    planned,
                    annotated,
                    specification,
                    bindings,
                    intermediates,
                    {driver},
                    diagnostics,
                    column_types,
                    resolved_joins,
                    inferred_paths=inferred_paths,
                    reference_compiler=reference_compiler,
                )
                row_references[(index, name)] = annotated

            row_names = set(derivations)
            if grouped:
                diagnostics.extend(
                    _diagnostic(
                        "phase_boundary",
                        filter_path or f"rows[{index}].filter",
                        {
                            "identifier": identifier,
                            "row": row.id,
                            "available_phase": "column_derivation",
                            "required_phase": "grouped_row_filter",
                        },
                    )
                    for identifier in filter_names
                    if "." not in identifier and identifier not in row_names
                )
            else:
                # REQ-0068: an ungrouped filter evaluates before the window
                # pass, so a bare identifier must name a column this row
                # template derives without reading a window result.
                deferred_columns: set[str] = set()
                if filter_names:
                    deferred_columns = {
                        name
                        for name, planned in derivations.items()
                        if planned.declaration.value.operation in WINDOW_OPERATIONS
                    }
                    changed = True
                    while changed:
                        changed = False
                        for name, planned in derivations.items():
                            if (
                                name not in deferred_columns
                                and set(planned.dependencies) & deferred_columns
                            ):
                                deferred_columns.add(name)
                                changed = True
                diagnostics.extend(
                    _diagnostic(
                        "phase_boundary",
                        filter_path or f"rows[{index}].filter",
                        {
                            "identifier": identifier,
                            "row": row.id,
                            "available_phase": (
                                "window_derivation"
                                if identifier in deferred_columns
                                else "column_derivation"
                            ),
                            "required_phase": "row_filter",
                        },
                    )
                    for identifier in filter_names
                    if "." not in identifier
                    and (identifier not in row_names or identifier in deferred_columns)
                )
                # A qualified lookup read also consumes its match values.
                # They must be available before the window pass just like
                # bare identifiers named directly by the filter.
                diagnostics.extend(
                    _diagnostic(
                        "phase_boundary",
                        filter_path or f"rows[{index}].filter",
                        {
                            "identifier": match,
                            "row": row.id,
                            "intermediate": qualifier,
                            "available_phase": (
                                "window_derivation"
                                if match in deferred_columns
                                else "column_derivation"
                            ),
                            "required_phase": "row_filter",
                        },
                    )
                    for identifier in filter_names
                    for qualifier, separator, _ in (identifier.partition("."),)
                    if separator and qualifier != driver and qualifier in intermediates
                    for match in intermediates[qualifier].dependencies
                    if match in deferred_columns
                    or (
                        match not in row_names
                        and not _lookup_match_available_at_row_construction(
                            match, row=row, driver=driver, bindings=bindings
                        )
                    )
                )
            for name, planned in derivations.items():
                for reference in row_references[(index, name)]:
                    if "." in reference.name:
                        lookup = intermediates.get(reference.name.split(".", 1)[0])
                        if lookup is not None and lookup.identifier == driver:
                            lookup = None
                        if (
                            index == 0
                            and lookup is not None
                            and lookup.dataset == "SELF"
                        ):
                            diagnostics.append(
                                _diagnostic(
                                    "phase_boundary",
                                    reference.path,
                                    {
                                        "identifier": reference.name,
                                        "available_phase": "completed_prior_row_template",
                                        "required_phase": "row_construction",
                                    },
                                )
                            )
                        _validate_qualified_reference(
                            reference,
                            {driver} if driver is not None else frozenset(),
                            bindings,
                            column_types,
                            diagnostics,
                            intermediates=intermediates,
                            row=row,
                            reference_compiler=reference_compiler,
                        )
                        diagnostics.extend(
                            _diagnostic(
                                "phase_boundary",
                                reference.path,
                                {
                                    "identifier": match,
                                    "row": row.id,
                                    "intermediate": reference.name.split(".", 1)[0],
                                    "available_phase": "column_derivation",
                                    "required_phase": "row_construction",
                                },
                                requirement="REQ-0126",
                            )
                            # REQ-0126: during row construction, every value
                            # the intermediate matches on must be derived by
                            # this template rather than by a later phase.
                            # Group keys and ungrouped driver fields are known
                            # at construction, so they need no derivation
                            # (issue #711).
                            for match in (
                                _lookup_dependencies(reference, intermediates)
                                if lookup is not None
                                else ()
                            )
                            if match not in row_names
                            and not _lookup_match_available_at_row_construction(
                                match, row=row, driver=driver, bindings=bindings
                            )
                        )
                    elif reference.key_expression is not None:
                        # REQ-1259: an expression match value's synthetic match
                        # name is not a real variable; the inner references
                        # were validated separately, so only the paired-type
                        # check below applies.
                        pass
                    elif reference_compiler is not None:
                        _append_reference_finding(
                            reference_compiler.validate_output(
                                reference.name,
                                reference.expected_type,
                                row_names,
                                (driver,),
                            ),
                            reference,
                            diagnostics,
                            row=row,
                        )
                    elif reference.name not in column_types:
                        # REQ-0106: an unqualified name only ever binds to an
                        # output column; when the bare name is a driver field
                        # the author meant the qualified source read.
                        diagnostics.append(
                            _unresolvable_reference_diagnostic(
                                reference, (driver,), dataset_fields
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
                    _validate_paired_type(
                        reference,
                        bindings,
                        column_types,
                        diagnostics,
                        reference_compiler=reference_compiler,
                    )

            graph = {
                name: tuple(
                    dependency
                    for dependency in planned.dependencies
                    if dependency in derivations
                )
                for name, planned in derivations.items()
            }
            names = [name for name in column_order if name in derivations]
            analysis = (
                dependency_analyzer(names, graph)
                if dependency_analyzer is not None
                else None
            )
            cycle = _find_cycle(names, graph) if analysis is None else analysis.cycle
            if cycle is not None:
                paths = tuple(
                    dict.fromkeys(
                        derivations[name].operation_path for name in cycle[:-1]
                    )
                )
                diagnostics.append(
                    _diagnostic(
                        "dependency_cycle",
                        paths,
                        {"cycle": list(cycle)},
                        requirement="REQ-0072",
                    )
                )
            ordered = (
                _topological_row_order(derivations, column_order)
                if analysis is None
                else tuple(derivations[name] for name in analysis.order)
            )
            row_plans.append(
                PlannedRow(
                    index=index,
                    declaration=row,
                    driver=driver,
                    group_variables=tuple(row.group_by or ()),
                    filter_path=filter_path,
                    filter_predicate=filter_ast,
                    derivations=ordered,
                )
            )

    column_plans: list[PlannedDerivation] = []
    drivers = {plan.driver for plan in row_plans}
    # REQ-0107: the group_by of every grouped template, keyed by driver
    # dataset, so column-level scalar sources can be checked against them.
    grouped_by_driver: dict[str, list[tuple[str, ...]]] = {}
    for plan in row_plans:
        if plan.group_variables:
            grouped_by_driver.setdefault(plan.driver, []).append(plan.group_variables)
    for column in specification.columns:
        if column.derivation is None:
            continue
        if column.name in default_derivations:
            # REQ-1260: a default derivation evaluates in each inheriting
            # template's row phase, never in the column phase.
            continue
        planned, references, inferred_paths = _plan_derivation(
            column.name,
            column.derivation,
            f"columns.{column.name}.derivation",
            supported_operations,
            diagnostics,
            unsupported,
            scope=_Scope(
                intermediates=frozenset(intermediates),
                keep_intermediates=frozenset(
                    identifier
                    for identifier, plan in intermediates.items()
                    if plan.keep is not None
                ),
            ),
            infer_keys=infer_lookup_keys,
            dataset_fields=dataset_fields,
        )
        annotated = _resolve_implicit_joins(
            references,
            specification,
            bindings,
            drivers,
            intermediates,
            column_types,
            diagnostics,
            reference_compiler=reference_compiler,
        )
        column_plans.append(
            _with_relation_dependencies(
                planned,
                annotated,
                specification,
                bindings,
                intermediates,
                drivers,
                diagnostics,
                column_types,
                resolved_joins,
                inferred_paths=inferred_paths,
                reference_compiler=reference_compiler,
            )
        )
        for reference in annotated:
            if "." in reference.name:
                _validate_qualified_reference(
                    reference,
                    drivers,
                    bindings,
                    column_types,
                    diagnostics,
                    intermediates=intermediates,
                    grouped_by_driver=grouped_by_driver,
                    reference_compiler=reference_compiler,
                )
            elif reference.key_expression is not None:
                # REQ-1259: an expression match value's synthetic match name is
                # not a real variable; the inner references were validated
                # separately, so only the paired-type check below applies.
                pass
            elif reference_compiler is not None:
                _append_reference_finding(
                    reference_compiler.validate_output(
                        reference.name, reference.expected_type, None, drivers
                    ),
                    reference,
                    diagnostics,
                )
            elif reference.name not in column_types:
                # REQ-0106: an unqualified name only ever binds to an output
                # column; when the bare name is a field of a row driver the
                # author meant the qualified source read.
                diagnostics.append(
                    _unresolvable_reference_diagnostic(
                        reference, drivers, dataset_fields
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
            _validate_paired_type(
                reference,
                bindings,
                column_types,
                diagnostics,
                reference_compiler=reference_compiler,
            )

    column_graph = {
        planned.column: tuple(
            dependency
            for dependency in planned.dependencies
            if dependency in column_types
        )
        for planned in column_plans
    }
    if column_dependency_analyzer is not None:
        column_analysis = column_dependency_analyzer(
            column_order, column_graph, specification.keys, bool(specification.rows)
        )
        planned_by_column = {planned.column: planned for planned in column_plans}
        for finding in column_analysis.diagnostics:
            if finding.condition == "dependency_cycle":
                paths = tuple(
                    dict.fromkeys(
                        planned_by_column[name].operation_path
                        for name in finding.columns[:-1]
                    )
                )
                context = {"cycle": list(finding.columns)}
            else:
                column = finding.columns[0]
                if finding.location == "declaration":
                    paths = f"columns.{column}.derivation"
                elif finding.location == "operation":
                    paths = planned_by_column[column].operation_path
                elif finding.location == "expression":
                    paths = planned_by_column[column].expression_path
                else:
                    raise ValueError("unknown native column dependency location")
                context = {"column": column}
                if len(finding.columns) == 2:
                    context["dependency"] = finding.columns[1]
            diagnostics.append(
                _diagnostic(
                    finding.condition, paths, context, requirement=finding.requirement
                )
            )
    else:
        names = [name for name in column_order if name in column_graph]
        column_analysis = (
            dependency_analyzer(names, column_graph)
            if dependency_analyzer is not None
            else None
        )
        cycle = (
            _find_cycle(names, column_graph)
            if column_analysis is None
            else column_analysis.cycle
        )
        cycle_members = set(cycle[:-1]) if cycle is not None else set()
        if cycle is not None:
            by_name = {planned.column: planned for planned in column_plans}
            paths = tuple(
                dict.fromkeys(by_name[name].operation_path for name in cycle[:-1])
            )
            diagnostics.append(
                _diagnostic(
                    "dependency_cycle",
                    paths,
                    {"cycle": list(cycle)},
                    requirement="REQ-0072",
                )
            )

        key_set = set(specification.keys)
        for planned in column_plans:
            if planned.column in cycle_members:
                continue
            for dependency in planned.dependencies:
                if dependency in key_set or dependency not in column_positions:
                    continue  # keys predate column derivation (REQ-0074)
                if column_positions[dependency] >= column_positions[planned.column]:
                    diagnostics.append(
                        _diagnostic(
                            "forward_reference",
                            planned.operation_path,
                            {"column": planned.column, "dependency": dependency},
                            requirement="REQ-0071",
                        )
                    )

        planned_by_column = {planned.column: planned for planned in column_plans}
        has_templates = bool(specification.rows)
        for key in specification.keys:
            planned = planned_by_column.get(key)
            if planned is None:
                if has_templates:
                    continue
                diagnostics.append(
                    _diagnostic(
                        "key_dependency",
                        f"columns.{key}.derivation",
                        {"column": key},
                        requirement="REQ-0074",
                    )
                )
                continue
            for dependency in planned.dependencies:
                if dependency in column_types and dependency not in key_set:
                    if has_templates:
                        continue
                    diagnostics.append(
                        _diagnostic(
                            "key_dependency",
                            planned.expression_path,
                            {"column": key, "dependency": dependency},
                            requirement="REQ-0074",
                        )
                    )

    if diagnostics:
        raise ExecutionPlanningError(diagnostics)
    unique_unsupported = _unique_features(unsupported)
    if unique_unsupported:
        raise UnsupportedPlanningError(unique_unsupported)

    row_derived = tuple(
        column.name
        for column in specification.columns
        # REQ-1260: a default derivation is a row-phase value in every
        # template (overridden or inherited), so SELF sees it like any
        # other template-derived column.
        if column.derivation is None or column.name in default_derivations
    )
    # REQ-0050 records an intermediate's match values as dependencies so the
    # keys' inputs come before the read. The forward_reference check above
    # exempts keys (REQ-0074); the execution order honors the exemption by
    # deriving a column after the dependencies the validator allowed through.
    # The sort is stable: declaration order is kept wherever no dependency
    # forces a move.
    by_name = {planned.column: planned for planned in column_plans}
    ordered_columns = (
        _topological_row_order(by_name, column_order)
        if column_analysis is None
        else tuple(by_name[name] for name in column_analysis.order)
    )
    return ExecutionPlan(
        specification=specification,
        bindings=bindings,
        rows=tuple(row_plans),
        columns=tuple(ordered_columns),
        row_derived_columns=row_derived,
        intermediates=tuple(intermediates.values()),
        resolved_joins=tuple(resolved_joins),
    )
