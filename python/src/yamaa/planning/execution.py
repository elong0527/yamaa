"""Build a deterministic plan for the initial R001 execution subset."""

from __future__ import annotations

from collections.abc import Collection, Mapping, Sequence
from dataclasses import dataclass
from typing import Any, Literal

from pydantic import BaseModel, ConfigDict, Field, JsonValue

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
    template_identifiers,
    ungrouped_identifiers,
)
from yamaa.io.source import LoadedDataset
from yamaa.models import ColumnType, ConditionPhase, TypedTable
from yamaa.odm import BindingFailure, BindingPlan, BoundReference, build_binding_plan
from yamaa.specification.models import (
    Expression,
    HandledExpression,
    OrderTerm,
    RecordLookupBetween,
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


class PlannedRecordLookup(_FrozenModel):
    """One validated `record_lookups` entry, ready to select a record.

    `match_variables` and `match_fields` pair by position: the first names
    what the current row reads, the second the right-side column it must
    equal. When the entry declares neither `source` nor `key`, both hold the
    applicable output keys R003 infers, which carry that one name on both
    sides.
    """

    identifier: str = Field(min_length=1)
    dataset: str = Field(min_length=1)
    path: str = Field(min_length=1)
    match_variables: tuple[str, ...]
    match_fields: tuple[str, ...]
    on_output_keys: bool
    filter_predicate: dict[str, Any] | None = None
    order_terms: tuple[tuple[OrderTerm, str], ...] = ()
    keep: Literal["first", "last"] | None = None
    between_value: str | None = None
    between_lower: str | None = None
    between_upper: str | None = None
    unmatched: Literal["missing", "fail"] = "missing"
    incomplete: Literal["missing", "fail"] = "fail"

    @property
    def dependencies(self) -> tuple[str, ...]:
        """Return the current-row variables R001-18 makes this lookup need.

        `filter` and `order_by` name records of the lookup's own dataset and
        contribute no output-column dependency; `source` and `between.value`
        do, exactly as a column using `mapping_from` depends on its sources.
        """
        names = list(self.match_variables)
        if self.between_value is not None:
            names.append(self.between_value)
        return tuple(dict.fromkeys(names))


class ResolvedJoin(_FrozenModel):
    """The columns one qualified reference resolved to matching on.

    R003-38 makes validation report the inferred applicable keys for every
    qualified source, so a reviewer sees which same-named columns the join
    matches on rather than having to infer them from two schemas. A record
    lookup reports the same thing through `PlannedRecordLookup.match_fields`.
    """

    spec_path: str = Field(min_length=1)
    dataset: str = Field(min_length=1)
    keys: tuple[str, ...]
    # R003-20 lets a reduction declare a grain coarser than the applicable
    # keys, and the join then matches on that instead.
    declared_grain: bool = False


class PlannedRow(_FrozenModel):
    """One record-driven or group-driven row template.

    R001-5 gives a template one of two modes, and `group_variables` is what
    tells them apart: empty for a record-driven template, and the driver
    variables the grain partitions on for a group-driven one. A grouped
    template's filter selects completed candidates rather than driver
    records, which R001-8 evaluates after the whole derivation graph.
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
        """Return the driver columns the grain partitions on."""
        return tuple(name.split(".", 1)[1] for name in self.group_variables)


class ExecutionPlan(_FrozenModel):
    """Validated work in the exact order the initial executor will perform it."""

    specification: Specification
    bindings: BindingPlan
    rows: tuple[PlannedRow, ...]
    columns: tuple[PlannedDerivation, ...]
    row_derived_columns: tuple[str, ...]
    record_lookups: tuple[PlannedRecordLookup, ...] = ()
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
    # The R007 requirement the owning operation's input type is held to.
    requirement: str | None = None
    # The relation this reference reaches through an R003 join, whose
    # applicable keys the reading derivation therefore depends on.
    join_relation: str | None = None
    # The columns that join replaces the applicable keys with, when R003-20
    # lets a reduction declare a grain coarser than they are.
    join_group_by: tuple[str, ...] | None = None
    # How the reference reaches its relation: as one scalar of the current
    # row driver or of an R003 join, as the records R013 reduces, or as a
    # right-side column R007 pairs with a declared key rather than a key of
    # its own. R001-15 and R003-15 each turn on the difference.
    reach: Literal["scalar", "relation", "declared"] = "scalar"
    # The other name this reference's runtime type must be comparable with,
    # which is how R007-21 pairs a `mapping_from` source with its key column.
    same_type_as: str | None = None


@dataclass(frozen=True, slots=True)
class _Scope:
    """Where an expression sits, which is what fixes the contexts it may use.

    R007-8 through R007-11 make an aggregate valid in exactly three places,
    and each is told apart by the phase it evaluates in and by whether the
    enclosing row template groups its driver.
    """

    column_phase: bool = True
    grouped_driver: str | None = None
    group_variables: tuple[str, ...] = ()
    record_lookups: frozenset[str] = frozenset()


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


def _expression_path(path: str, derivation: HandledExpression) -> str:
    """Recover the authored bare-expression path where normalization permits it."""
    handled = {"conversion_failure", "override"} & derivation.model_fields_set
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
            reference.join_group_by,
            reference.reach,
            reference.same_type_as,
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

# R016-54 types every temporal operand. `date_precision` reads either kind of
# source, so it states no expected type and answers at evaluation.
_TEMPORAL_VARIABLES: dict[str, tuple[tuple[str, ColumnType | None, str], ...]] = {
    "date_diff": (("start", "date", "R016-65"), ("end", "date", "R016-65")),
    "study_day": (("date", "date", "R016-65"), ("reference", "date", "R016-65")),
    "date_impute": (
        ("source", "str", "R016-54"),
        ("not_before", "date", "R016-49"),
    ),
    "date_precision": (("source", None, "R016-45"),),
    "to_date": (("source", "datetime", "R016-66"),),
}


def _expression_info(
    expression: Expression,
    path: str,
    supported_operations: Collection[str],
    *,
    scope: _Scope = _COLUMN_SCOPE,
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
            scope=scope,
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
            _compute_references(payload, operation_path, references, scope)
        )
    elif operation == "aggregate":
        diagnostics.extend(
            _aggregate_references(payload, operation_path, references, scope)
        )
    elif operation == "mapping_from" and isinstance(payload, Mapping):
        diagnostics.extend(
            _mapping_from_references(payload, operation_path, references)
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
                            _Reference(
                                name,
                                f"{branch_path}.when",
                                requirement="R004-32",
                            )
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
            and qualifier not in scope.record_lookups
        ):
            # R010-3 and R015-13: a qualified identifier is admitted only for
            # a record already selected by a declared record lookup, so every
            # other join stays under R003 rather than inside the formula.
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


def _as_names(value: object) -> tuple[str, ...] | None:
    if isinstance(value, str):
        return (value,)
    if isinstance(value, Sequence) and all(isinstance(item, str) for item in value):
        return tuple(value)  # type: ignore[arg-type]
    return None


def _mapping_from_references(
    payload: Mapping[str, object],
    operation_path: str,
    references: list[_Reference],
) -> list[ExecutionDiagnostic]:
    """Collect the declared key pairs R007 makes this lookup match on."""
    sources = _as_names(payload.get("source"))
    keys = _as_names(payload.get("key"))
    dataset = payload.get("dataset")
    value = payload.get("value")
    if sources is None or keys is None or not isinstance(dataset, str):
        return [
            _diagnostic(
                "invalid_field_type",
                operation_path,
                {"operation": "mapping_from", "expected": "source, dataset, and key"},
                requirement="R007-36",
            )
        ]
    if len(sources) != len(keys):
        # R007-48: the lists pair by position, so unequal lengths name no key.
        return [
            _diagnostic(
                "source_key_length_mismatch",
                operation_path,
                {
                    "source": list(sources),
                    "key": list(keys),
                    "source_count": len(sources),
                    "key_count": len(keys),
                },
                requirement="R007-48",
            )
        ]
    for index, (name, key) in enumerate(zip(sources, keys, strict=True)):
        references.append(
            _Reference(
                name,
                f"{operation_path}.source[{index}]",
                # R007-21: each source and its key column must have the same
                # comparable type, so the pair is checked rather than coerced.
                same_type_as=f"{dataset}.{key}",
                requirement="R007-21",
            )
        )
        references.append(
            _Reference(
                f"{dataset}.{key}",
                f"{operation_path}.key[{index}]",
                reach="declared",
            )
        )
    if isinstance(value, str):
        references.append(
            _Reference(
                f"{dataset}.{value}",
                f"{operation_path}.value",
                reach="declared",
            )
        )
    return []


# R007-12 types each window field as a variable, so each may name a current
# output column or a qualified source variable of the row's driver.
_WINDOW_VARIABLES: dict[str, tuple[str, ...]] = {
    "row_number": (),
    "rank": (),
    "row_value": ("source",),
    "previous_non_missing": ("source",),
    "baseline_flag": ("date", "reference_date"),
    "baseline_value": ("value", "flag"),
}


def _window_references(
    operation: str,
    payload: Mapping[str, object],
    operation_path: str,
    references: list[_Reference],
    scope: _Scope,
) -> list[ExecutionDiagnostic]:
    """Collect what one window reads, and reject the contexts R007 refuses."""
    if not scope.column_phase:
        # R007-41: a window partitions constructed output rows, which do not
        # exist until row construction has finished.
        return [
            _diagnostic(
                "phase_boundary",
                operation_path,
                {
                    "operation": operation,
                    "available_phase": "column_derivation",
                    "required_phase": "row_construction",
                },
                requirement="R007-41",
            )
        ]
    diagnostics: list[ExecutionDiagnostic] = []
    if operation == "row_value" and payload.get("offset") == 0:
        # R007-43: the current row's own value is `source`, and a window must
        # not be a second spelling of it.
        diagnostics.append(
            _diagnostic(
                "zero_offset",
                f"{operation_path}.offset",
                {"offset": 0},
                requirement="R007-43",
            )
        )
    for field in _WINDOW_VARIABLES[operation]:
        name = payload.get(field)
        if isinstance(name, str):
            references.append(_Reference(name, f"{operation_path}.{field}"))
    for index, name in enumerate(_as_names(payload.get("group_by")) or ()):
        references.append(_Reference(name, f"{operation_path}.group_by[{index}]"))
    for index, term in enumerate(payload.get("order_by") or ()):
        variable = term if isinstance(term, str) else None
        if isinstance(term, Mapping) and isinstance(term.get("variable"), str):
            variable = str(term["variable"])
        if isinstance(variable, str):
            references.append(
                _Reference(variable, f"{operation_path}.order_by[{index}]")
            )
    predicate = payload.get("filter")
    if isinstance(predicate, str):
        filter_path = f"{operation_path}.filter"
        ast = _parse_predicate_at(predicate, filter_path, diagnostics)
        if ast is not None:
            references.extend(
                _Reference(name, filter_path) for name in _predicate_identifiers(ast)
            )
    return diagnostics


def _aggregate_references(
    payload: object,
    operation_path: str,
    references: list[_Reference],
    scope: _Scope,
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
                requirement="R007-36",
            )
        ]
    expr = payload.get("expr")
    if not isinstance(expr, str):
        return [
            _diagnostic(
                "invalid_field_type",
                operation_path,
                {"operation": "aggregate", "expected": "a reducer expression"},
                requirement="R007-36",
            )
        ]
    expr_path = f"{operation_path}.expr"
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
        # R013-4: a reduction is not a join, so one expression names one
        # relation and two datasets are composed through two columns.
        named = sorted(relations) + (["<output>"] if unqualified else [])
        return [
            _diagnostic(
                "mixed_relations",
                expr_path,
                {"expr": expr, "relations": named},
                requirement="R013-39",
            )
        ]

    relation = next(iter(sorted(relations)), None)
    group_by = _as_names(payload.get("group_by")) or ()
    between = payload.get("between")
    diagnostics = _aggregate_context(
        relation, group_by, between, expr, operation_path, scope
    )
    if diagnostics:
        return diagnostics

    grain = scope.group_variables if scope.grouped_driver is not None else group_by
    diagnostics.extend(
        _diagnostic(
            "aggregate_identifier_not_grouped",
            expr_path,
            {"expr": expr, "dataset": relation, "identifier": name},
            requirement="R013-38",
        )
        for name in ungrouped_identifiers(ast)
        if name not in grain
    )

    # R003-17 joins a right-side reduction back on the applicable keys, or on
    # the coarser grain R003-20 lets it declare. A grouped-row reduction
    # reads its own driver group and joins nothing.
    joined = relation if scope.grouped_driver is None and relation else None
    grain = (
        tuple(name.split(".", 1)[-1] for name in group_by)
        if group_by and joined
        else None
    )

    def relational(name: str, path: str) -> _Reference:
        qualified = "." in name
        return _Reference(
            name,
            path,
            join_relation=joined if qualified else None,
            join_group_by=grain if qualified and joined else None,
            reach="relation" if qualified else "scalar",
        )

    references.extend(relational(name, expr_path) for name in identifiers)
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
                for name in _predicate_identifiers(ast_filter)
            )
    if isinstance(between, Mapping):
        value = between.get("value")
        if isinstance(value, str):
            references.append(_Reference(value, f"{operation_path}.between.value"))
        for bound in ("lower", "upper"):
            name = between.get(bound)
            if isinstance(name, str):
                references.append(relational(name, f"{operation_path}.between.{bound}"))
    return diagnostics


def _aggregate_context(
    relation: str | None,
    group_by: Sequence[str],
    between: object,
    expr: str,
    operation_path: str,
    scope: _Scope,
) -> list[ExecutionDiagnostic]:
    """Reject every aggregate context outside the three R007-8 permits."""

    def reject(reason: str, path: str = operation_path) -> list[ExecutionDiagnostic]:
        return [
            _diagnostic(
                "invalid_aggregate_context",
                path,
                {"expr": expr, "reason": reason},
                requirement="R007-44",
            )
        ]

    if scope.grouped_driver is not None:
        # Context 3: the enclosing `row.group_by` owns the grain, so the
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
                "the enclosing row.group_by owns the grain",
                f"{operation_path}.group_by",
            )
        if between is not None:
            return reject(
                "a grouped row aggregate has no separate right side",
                f"{operation_path}.between",
            )
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
                    requirement="R013-42",
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
            # R013-7 and R003-25: at least one bound, or the narrowing states
            # nothing and silently reduces the unrestricted right side.
            return reject(
                "between declares at least one bound", f"{operation_path}.between"
            )
        if any(not str(bound).startswith(f"{relation}.") for bound in bounds):
            # R013-7: both bounds are columns of the expression's one relation.
            return reject(
                f"a between bound names a column of {relation!r}",
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
    scope: _Scope = _COLUMN_SCOPE,
) -> tuple[PlannedDerivation, tuple[_Reference, ...]]:
    value_path = _expression_path(path, declaration)
    info = _expression_info(
        declaration.value,
        value_path,
        supported_operations,
        scope=scope,
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
            scope=scope,
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
    driver: str | None,
    bindings: BindingPlan,
    column_types: Mapping[str, ColumnType],
    diagnostics: list[ExecutionDiagnostic],
    *,
    lookups: Mapping[str, PlannedRecordLookup] = {},
    row: Row | None = None,
) -> None:
    """Check one qualified name against the relation it reaches.

    During column derivation a name qualified to another dataset is the R003
    join; during row construction it is not, because R001-15 lets a row
    derivation read its driver, its group keys, an earlier row-derived
    column, or a record lookup, and nothing else.
    """
    qualifier = reference.name.split(".", 1)[0]
    if qualifier in lookups:
        _validate_lookup_reference(reference, lookups[qualifier], bindings, diagnostics)
        return
    bound = bindings.bind(reference.name)
    if isinstance(bound, BindingFailure):
        diagnostics.append(
            _diagnostic(
                "unknown_field",
                reference.path,
                {"identifier": reference.name},
                requirement="R002-27",
            )
        )
        return
    if bound.kind == "output":
        raise AssertionError("a qualified name cannot bind as an output column")
    assert bound.dataset is not None
    if row is not None:
        _validate_row_phase_reference(
            reference, bound.dataset, driver, row, diagnostics
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
        # R001-36: a driver field that varies within the group has no single
        # value for the candidate, so it is read through an aggregate or not
        # at all.
        diagnostics.append(
            _diagnostic(
                "ungrouped_driver_field",
                reference.path,
                {"identifier": reference.name, "row": row.id, "dataset": dataset},
                requirement="R001-36",
            )
        )


def _validate_lookup_reference(
    reference: _Reference,
    lookup: PlannedRecordLookup,
    bindings: BindingPlan,
    diagnostics: list[ExecutionDiagnostic],
) -> None:
    """Check that a record lookup id qualifies a column its dataset has."""
    field = reference.name.split(".", 1)[1]
    dataset = bindings.datasets.get(lookup.dataset)
    if dataset is not None and field not in dataset.field_names:
        # R015-31: the named column must exist in the lookup's dataset.
        diagnostics.append(
            _diagnostic("unknown_field", reference.path, {"identifier": reference.name})
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
) -> None:
    """Check the two halves of a declared key pair against R007-21."""
    if reference.same_type_as is None:
        return
    actual = _reference_type(reference.name, bindings, column_types)
    expected = _reference_type(reference.same_type_as, bindings, column_types)
    if actual is None or expected is None or _comparable_types(actual, expected):
        return
    diagnostics.append(
        _diagnostic(
            "incompatible_input_type",
            reference.path,
            {"source": reference.name, "expected": expected, "actual": actual},
            requirement=reference.requirement or "R007-21",
        )
    )


def _comparable_types(left: ColumnType, right: ColumnType) -> bool:
    """Return whether R007-31 makes two declared types mutually comparable."""
    return left == right or {left, right} <= {"int", "float"}


def _applicable_keys(
    specification: Specification,
    bindings: BindingPlan,
    dataset: str,
) -> tuple[str, ...]:
    """Return the output keys the right side also carries, in `keys` order."""
    fields = _dataset_types(bindings, dataset)
    return tuple(key for key in specification.keys if key in fields)


def _with_relation_dependencies(
    planned: PlannedDerivation,
    references: Sequence[_Reference],
    specification: Specification,
    bindings: BindingPlan,
    lookups: Mapping[str, PlannedRecordLookup],
    drivers: Collection[str],
    diagnostics: list[ExecutionDiagnostic],
    column_types: Mapping[str, ColumnType],
    resolved: list[ResolvedJoin],
) -> PlannedDerivation:
    """Add the current-row values a derivation needs to reach another relation.

    R003-34 makes an unavailable applicable left key a failure, and R001-18
    makes a record lookup's match values dependencies of every column that
    reads it. Recording them as ordinary dependencies is what puts the join's
    inputs before the join in R001's declaration order.
    """
    extra: list[str] = []
    # One reading per relation this derivation reaches: an aggregate names
    # the same right side from its `expr` and its `filter`, and R003-38 asks
    # for the keys the join matches on, not for one line per mention.
    checked: set[str] = set()
    for reference in references:
        if "." not in reference.name:
            continue
        qualifier = reference.name.split(".", 1)[0]
        if qualifier in lookups:
            extra.extend(lookups[qualifier].dependencies)
            continue
        if reference.reach == "declared":
            # R003-15: `mapping_from` declares its own pairs and never
            # consults output keys, so it depends on no applicable key.
            continue
        if reference.join_relation is not None:
            keys = (
                reference.join_group_by
                if reference.join_group_by is not None
                else _applicable_keys(specification, bindings, reference.join_relation)
            )
        elif qualifier in bindings.datasets and qualifier not in drivers:
            # R003-18: a scalar source qualified to the current row driver
            # reads the driver record, so it joins nothing and needs no key.
            keys = _applicable_keys(specification, bindings, qualifier)
        else:
            continue
        extra.extend(keys)
        if qualifier in checked:
            continue
        checked.add(qualifier)
        resolved.append(
            ResolvedJoin(
                spec_path=reference.path,
                dataset=qualifier,
                keys=tuple(keys),
                declared_grain=reference.join_group_by is not None,
            )
        )
        _validate_join_key_types(
            reference.path, qualifier, keys, bindings, column_types, diagnostics
        )
    if not extra:
        return planned
    dependencies = tuple(
        dict.fromkeys(
            (*planned.dependencies, *(name for name in extra if name != planned.column))
        )
    )
    return planned.model_copy(update={"dependencies": dependencies})


def _lookup_dependencies(
    reference: _Reference,
    lookups: Mapping[str, PlannedRecordLookup],
) -> tuple[str, ...]:
    """Return the current-row values a lookup-qualified reference needs."""
    lookup = lookups.get(reference.name.split(".", 1)[0])
    return lookup.dependencies if lookup is not None else ()


def _validate_join_key_types(
    path: str,
    dataset: str,
    keys: Sequence[str],
    bindings: BindingPlan,
    column_types: Mapping[str, ColumnType],
    diagnostics: list[ExecutionDiagnostic],
) -> None:
    """Check that each side of the match carries one comparable type.

    R007-19 performs no implicit conversion between operation inputs, so an
    output key and the same-named right-side column must already agree.
    Reporting the disagreement is what keeps a join from quietly matching
    nothing and presenting an unasked question as an absent record.
    """
    fields = _dataset_types(bindings, dataset)
    for key in keys:
        left = column_types.get(key)
        right = fields.get(key)
        if left is None or right is None or _comparable_types(left, right):
            continue
        diagnostics.append(
            _diagnostic(
                "incompatible_input_type",
                path,
                {"source": f"{dataset}.{key}", "expected": left, "actual": right},
                requirement="R007-38",
            )
        )


def _plan_record_lookups(
    specification: Specification,
    bindings: BindingPlan,
    column_types: Mapping[str, ColumnType],
    diagnostics: list[ExecutionDiagnostic],
) -> dict[str, PlannedRecordLookup]:
    """Validate each declared record lookup against its loaded dataset."""
    planned: dict[str, PlannedRecordLookup] = {}
    for index, lookup in enumerate(specification.record_lookups or ()):
        path = f"record_lookups[{index}]"
        if lookup.dataset not in bindings.datasets:
            continue
        fields = _dataset_types(bindings, lookup.dataset)
        declared = lookup.source is not None and lookup.key is not None
        if declared:
            assert lookup.source is not None and lookup.key is not None
            if len(lookup.source) != len(lookup.key):
                continue
            variables = tuple(lookup.source)
            match_fields = tuple(lookup.key)
        else:
            # R015-5: with neither declared, the applicable output keys match
            # exactly as R003 defines them, and R015-27 requires at least one.
            match_fields = _applicable_keys(specification, bindings, lookup.dataset)
            variables = match_fields
            if not match_fields:
                diagnostics.append(
                    _diagnostic(
                        "no_applicable_keys",
                        path,
                        {
                            "record_lookup": lookup.id,
                            "dataset": lookup.dataset,
                            "keys": list(specification.keys),
                        },
                        requirement="R003-33",
                    )
                )
                continue

        failed = False
        for variable, field in zip(variables, match_fields, strict=True):
            left = _reference_type(variable, bindings, column_types)
            right = fields.get(field)
            if left is None or right is None or _comparable_types(left, right):
                continue
            diagnostics.append(
                _diagnostic(
                    "incompatible_input_type",
                    f"{path}.key",
                    {
                        "record_lookup": lookup.id,
                        "source": variable,
                        "expected": left,
                        "actual": right,
                    },
                    requirement="R007-38",
                )
            )
            failed = True
        for field in match_fields:
            if field not in fields:
                diagnostics.append(
                    _diagnostic(
                        "unknown_field",
                        f"{path}.key",
                        {
                            "record_lookup": lookup.id,
                            "identifier": f"{lookup.dataset}.{field}",
                        },
                    )
                )
                failed = True
        for variable in variables:
            if _reference_type(variable, bindings, column_types) is None:
                diagnostics.append(
                    _diagnostic(
                        "unknown_field",
                        f"{path}.source",
                        {"record_lookup": lookup.id, "identifier": variable},
                    )
                )
                failed = True

        predicate = None
        if lookup.filter is not None:
            predicate = _parse_predicate_at(
                lookup.filter, f"{path}.filter", diagnostics
            )
            if predicate is not None:
                for identifier in _predicate_identifiers(predicate):
                    if identifier.split(".", 1)[0] != lookup.dataset or (
                        identifier.split(".", 1)[-1] not in fields
                    ):
                        diagnostics.append(
                            _diagnostic(
                                "unknown_field",
                                f"{path}.filter",
                                {
                                    "record_lookup": lookup.id,
                                    "identifier": identifier,
                                },
                            )
                        )
                        failed = True

        terms: list[tuple[OrderTerm, str]] = []
        for term_index, term in enumerate(lookup.order_by or ()):
            qualifier, _, field = term.variable.partition(".")
            if qualifier != lookup.dataset or field not in fields:
                diagnostics.append(
                    _diagnostic(
                        "unknown_field",
                        f"{path}.order_by[{term_index}]",
                        {"record_lookup": lookup.id, "identifier": term.variable},
                    )
                )
                failed = True
                continue
            terms.append((term, field))

        between = lookup.between
        if between is not None:
            failed = (
                _validate_lookup_between(
                    lookup.id,
                    between,
                    path,
                    fields,
                    bindings,
                    column_types,
                    diagnostics,
                )
                or failed
            )
        if failed:
            continue

        planned[lookup.id] = PlannedRecordLookup(
            identifier=lookup.id,
            dataset=lookup.dataset,
            path=path,
            match_variables=tuple(variables),
            match_fields=tuple(match_fields),
            on_output_keys=not declared,
            filter_predicate=predicate,
            order_terms=tuple(terms),
            keep=lookup.keep if lookup.order_by is not None else None,
            between_value=between.value if between is not None else None,
            between_lower=between.lower if between is not None else None,
            between_upper=between.upper if between is not None else None,
            # R015-19 keeps the behavior of the match the lookup performs, so
            # replacing a derivation with a lookup never changes what an
            # absent record does.
            unmatched=lookup.unmatched or ("fail" if declared else "missing"),
            incomplete=lookup.incomplete or "fail",
        )
    return planned


def _validate_lookup_between(
    identifier: str,
    between: RecordLookupBetween,
    path: str,
    fields: Mapping[str, ColumnType],
    bindings: BindingPlan,
    column_types: Mapping[str, ColumnType],
    diagnostics: list[ExecutionDiagnostic],
) -> bool:
    """Check the closed range a lookup matches by, before any data is read."""
    missing = [name for name in (between.lower, between.upper) if name not in fields]
    if missing:
        # R015-29: a bound naming a column the dataset does not have.
        diagnostics.extend(
            _diagnostic(
                "unknown_field",
                f"{path}.between",
                {"record_lookup": identifier, "identifier": name},
                requirement="R015-29",
            )
            for name in missing
        )
        return True
    value_type = _reference_type(between.value, bindings, column_types)
    if value_type is None:
        diagnostics.append(
            _diagnostic(
                "unknown_field",
                f"{path}.between.value",
                {"record_lookup": identifier, "identifier": between.value},
            )
        )
        return True
    lower_type = fields[between.lower]
    upper_type = fields[between.upper]
    if _comparable_types(value_type, lower_type) and _comparable_types(
        value_type, upper_type
    ):
        return False
    # R015-30: report the runtime types before any record is compared.
    diagnostics.append(
        _diagnostic(
            "incomparable_range_types",
            f"{path}.between",
            {
                "record_lookup": identifier,
                "value_type": value_type,
                "lower_type": lower_type,
                "upper_type": upper_type,
            },
            requirement="R015-11",
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


def _lookup_ids(specification: Specification) -> frozenset[str]:
    return frozenset(lookup.id for lookup in specification.record_lookups or ())


def _row_scope(
    specification: Specification,
    row: Row,
    driver: str | None,
) -> _Scope:
    """Return where this template's row derivations sit."""
    lookups = _lookup_ids(specification)
    if row.group_by is None or driver is None:
        return _Scope(column_phase=False, record_lookups=lookups)
    return _Scope(
        column_phase=False,
        grouped_driver=driver,
        group_variables=tuple(row.group_by),
        record_lookups=lookups,
    )


def _group_by_declaration(
    row: Row,
    index: int,
    driver: str | None,
) -> list[ExecutionDiagnostic]:
    """Check the grain a grouped template declares, before any record is read."""
    if row.group_by is None:
        return []
    path = f"rows[{index}].group_by"
    names = tuple(row.group_by)
    if not names or len(set(names)) != len(names):
        # R001-34: an empty grain names no partition and a repeated column
        # states the same one twice.
        return [
            _diagnostic(
                "invalid_field_type",
                path,
                {"row": row.id, "group_by": list(names)},
                requirement="R001-34",
            )
        ]
    if driver is None:
        return []
    return [
        _diagnostic(
            "unknown_field",
            path,
            {"row": row.id, "identifier": name, "dataset": driver},
            requirement="R001-35",
        )
        for name in names
        if not name.startswith(f"{driver}.") or name.count(".") != 1
    ]


def _record_lookup_declarations(
    specification: Specification,
) -> list[ExecutionDiagnostic]:
    """Check every `record_lookups` entry that needs no source data.

    R015-23 through R015-25 are decided by the declaration alone, so they are
    answered before a dataset is read rather than on the first row that
    reaches the lookup.
    """
    diagnostics: list[ExecutionDiagnostic] = []
    seen: dict[str, str] = {}
    for index, lookup in enumerate(specification.record_lookups or ()):
        path = f"record_lookups[{index}]"
        collision = None
        if lookup.id in specification.datasets:
            collision = f"datasets.{lookup.id}"
        elif lookup.id == specification.domain:
            collision = "domain"
        elif lookup.id in seen:
            collision = seen[lookup.id]
        if collision is not None:
            # R015-23: the id shares one namespace with dataset identifiers,
            # so a qualified name would otherwise reach two relations.
            diagnostics.append(
                _diagnostic(
                    "duplicate_identifier",
                    (f"{path}.id", collision),
                    {"identifier": lookup.id},
                    requirement="R015-23",
                )
            )
        seen.setdefault(lookup.id, f"{path}.id")
        if lookup.dataset not in specification.datasets:
            diagnostics.append(
                _diagnostic(
                    "unknown_field",
                    f"{path}.dataset",
                    {"record_lookup": lookup.id, "identifier": lookup.dataset},
                )
            )
        for declared, missing, requirement in (
            ("source", "key", "R015-26"),
            ("key", "source", "R015-26"),
            ("order_by", "keep", "R015-25"),
            ("keep", "order_by", "R015-25"),
        ):
            if (
                getattr(lookup, declared) is not None
                and getattr(lookup, missing) is None
            ):
                diagnostics.append(
                    _diagnostic(
                        "unpaired_fields",
                        path,
                        {
                            "record_lookup": lookup.id,
                            "declared": [declared],
                            "missing": [missing],
                        },
                        requirement=requirement,
                    )
                )
        if (
            lookup.source is not None
            and lookup.key is not None
            and len(lookup.source) != len(lookup.key)
        ):
            diagnostics.append(
                _diagnostic(
                    "source_key_length_mismatch",
                    path,
                    {
                        "record_lookup": lookup.id,
                        "source": list(lookup.source),
                        "key": list(lookup.key),
                        "source_count": len(lookup.source),
                        "key_count": len(lookup.key),
                    },
                    requirement="R015-26",
                )
            )
    return diagnostics


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
                requirement="R002-28",
            )
        )
    if specification.parents:
        unsupported.append(
            UnsupportedFeature(operation="inheritance", spec_path="parents")
        )
    diagnostics.extend(_record_lookup_declarations(specification))

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
        driver = row.dataset or specification.default_driver
        diagnostics.extend(_group_by_declaration(row, index, driver))
        scope = _row_scope(specification, row, driver)
        for name, declaration in row.derivations.items():
            path = f"rows[{index}].derivations.{name}"
            unsupported.extend(
                _expression_info(
                    declaration.value,
                    _expression_path(path, declaration),
                    supported_operations,
                    scope=scope,
                ).unsupported
            )
            for override_index, override in enumerate(declaration.override or ()):
                unsupported.extend(
                    _expression_info(
                        override.value,
                        f"{path}.override[{override_index}].value",
                        supported_operations,
                        scope=scope,
                    ).unsupported
                )

    column_scope = _Scope(record_lookups=_lookup_ids(specification))
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
                scope=column_scope,
            ).unsupported
        )
        for override_index, override in enumerate(declaration.override or ()):
            unsupported.extend(
                _expression_info(
                    override.value,
                    f"{path}.override[{override_index}].value",
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
    lookups = _plan_record_lookups(specification, bindings, column_types, diagnostics)
    resolved_joins: list[ResolvedJoin] = []
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
            driver = row.dataset or specification.default_driver
            if driver is None or driver not in specification.datasets:
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
                filter_names = _predicate_identifiers(filter_ast)
                for identifier in filter_names:
                    if "." in identifier:
                        if grouped:
                            # R001-37: a grouped filter reads the candidate's
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
                                driver,
                                bindings,
                                column_types,
                                diagnostics,
                                lookups=lookups,
                            )
                    elif not grouped:
                        diagnostics.append(
                            _diagnostic(
                                "phase_boundary",
                                path,
                                {
                                    "identifier": identifier,
                                    "row": row.id,
                                    "available_phase": "column_derivation",
                                    "required_phase": "row_filter",
                                },
                            )
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
                    scope=row_scope,
                )
                derivations[name] = _with_relation_dependencies(
                    planned,
                    references,
                    specification,
                    bindings,
                    lookups,
                    {driver},
                    diagnostics,
                    column_types,
                    resolved_joins,
                )
                row_references[(index, name)] = references

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
            for name, planned in derivations.items():
                for reference in row_references[(index, name)]:
                    if "." in reference.name:
                        _validate_qualified_reference(
                            reference,
                            driver,
                            bindings,
                            column_types,
                            diagnostics,
                            lookups=lookups,
                            row=row,
                        )
                        diagnostics.extend(
                            _diagnostic(
                                "phase_boundary",
                                reference.path,
                                {
                                    "identifier": match,
                                    "row": row.id,
                                    "record_lookup": reference.name.split(".", 1)[0],
                                    "available_phase": "column_derivation",
                                    "required_phase": "row_construction",
                                },
                                requirement="R015-9",
                            )
                            # R015-9: during grouped row construction, every
                            # value the lookup matches on must be derived by
                            # this template rather than by a later phase.
                            for match in _lookup_dependencies(reference, lookups)
                            if match not in row_names
                        )
                    elif reference.name not in column_types:
                        diagnostics.append(
                            _diagnostic(
                                "unknown_field",
                                reference.path,
                                {"identifier": reference.name},
                                requirement=reference.requirement,
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
                        reference, bindings, column_types, diagnostics
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
                    group_variables=tuple(row.group_by or ()),
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
            scope=_Scope(record_lookups=frozenset(lookups)),
        )
        column_plans.append(
            _with_relation_dependencies(
                planned,
                references,
                specification,
                bindings,
                lookups,
                drivers,
                diagnostics,
                column_types,
                resolved_joins,
            )
        )
        for reference in references:
            if "." in reference.name:
                _validate_qualified_reference(
                    reference,
                    None,
                    bindings,
                    column_types,
                    diagnostics,
                    lookups=lookups,
                )
            elif reference.name not in column_types:
                diagnostics.append(
                    _diagnostic(
                        "unknown_field",
                        reference.path,
                        {"identifier": reference.name},
                        requirement=reference.requirement,
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
            _validate_paired_type(reference, bindings, column_types, diagnostics)

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
                        requirement="R001-40",
                    )
                )

    key_set = set(specification.keys)
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
                    requirement="R001-43",
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
                        requirement="R001-43",
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
        record_lookups=tuple(lookups.values()),
        resolved_joins=tuple(resolved_joins),
    )
