"""Source resolution and the leaf expressions every dispatch starts from."""

from __future__ import annotations

from collections.abc import Callable, Mapping
from typing import Literal, Protocol, TypeAlias

from pydantic import BaseModel, ConfigDict, Field, JsonValue

from yamaa.expressions.text import ascii_upper
from yamaa.models.values import (
    MISSING,
    ConditionPhase,
    ConditionResult,
    EvaluationResult,
    HandlerName,
    HandlerObservation,
    RuntimeCondition,
    ValueResult,
    normalize_runtime_value,
    runtime_type_name,
)
from yamaa.specification.models import Expression


class _FrozenModel(BaseModel):
    model_config = ConfigDict(
        strict=True,
        extra="forbid",
        frozen=True,
        arbitrary_types_allowed=True,
    )


class ResolvedValue(_FrozenModel):
    """A variable exists in the current context and carries this raw value."""

    status: Literal["value"] = "value"
    value: object
    handled_by: Literal["multiple_matches"] | None = None


class AbsentValue(_FrozenModel):
    """A variable or ODM item is absent from the current context."""

    status: Literal["absent"] = "absent"
    variable: str = Field(min_length=1)


class FailedResolution(_FrozenModel):
    """Source resolution failed for a reason other than contextual absence."""

    status: Literal["condition"] = "condition"
    condition: RuntimeCondition


Resolution: TypeAlias = ResolvedValue | AbsentValue | FailedResolution


class Resolver(Protocol):
    """Resolve one name without coupling evaluation to a table or join engine."""

    def resolve(self, variable: str) -> Resolution: ...


class MultipleMatchResolver(Protocol):
    """Optional resolver extension for structured R008 source selection."""

    def resolve_with_multiple_matches(
        self,
        variable: str,
        multiple_matches: Mapping[str, object],
    ) -> Resolution: ...


class MappingResolver:
    """Small in-memory resolver useful for scalar execution and tests."""

    def __init__(self, values: Mapping[str, object]) -> None:
        self._values = dict(values)

    def resolve(self, variable: str) -> Resolution:
        if variable not in self._values:
            return AbsentValue(variable=variable)
        return ResolvedValue(value=self._values[variable])

    def resolve_with_multiple_matches(
        self,
        variable: str,
        multiple_matches: Mapping[str, object],
    ) -> Resolution:
        del multiple_matches
        return self.resolve(variable)


ExpressionHandler: TypeAlias = Callable[[object, Resolver], EvaluationResult]
ExpressionInput: TypeAlias = Expression | Mapping[str, object]


def expression_condition(
    phase: ConditionPhase,
    condition: str,
    context: dict[str, JsonValue],
    applicable_handler: HandlerName | None = None,
    requirement: str | None = None,
    field: str | None = None,
) -> ConditionResult:
    """Build one structured condition an expression returns rather than raises."""
    return ConditionResult(
        condition=RuntimeCondition(
            phase=phase,
            condition=condition,
            context=context,
            applicable_handler=applicable_handler,
            requirement=requirement,
            path_suffix=field,
        )
    )


def handler_value(
    payload: Mapping[object, object],
    name: HandlerName,
) -> EvaluationResult:
    """Substitute one declared R008 literal and record which handler fired."""
    normalized = normalize_runtime_value(payload[name])
    if isinstance(normalized, ValueResult):
        return ValueResult(value=normalized.value, handled_by=name)
    return normalized


def _source(payload: object, resolver: Resolver) -> EvaluationResult:
    if isinstance(payload, str):
        variable = payload
        options: Mapping[object, object] = {}
    elif isinstance(payload, Mapping):
        variable = payload.get("variable")
        options = payload
        if not isinstance(variable, str):
            return expression_condition(
                "validation",
                "invalid_field_type",
                {"field": "variable", "expected": "str"},
            )
    else:
        return expression_condition(
            "validation",
            "invalid_field_type",
            {"operation": "source", "expected": "str or mapping"},
        )

    multiple = options.get("multiple_matches")
    if multiple is not None and not isinstance(multiple, Mapping):
        return expression_condition(
            "validation",
            "invalid_field_type",
            {"field": "multiple_matches", "expected": "mapping"},
        )
    if multiple is None:
        resolved = resolver.resolve(variable)
    else:
        resolve_multiple = getattr(resolver, "resolve_with_multiple_matches", None)
        if not callable(resolve_multiple):
            return expression_condition(
                "validation",
                "invalid_field_type",
                {"field": "multiple_matches", "reason": "resolver unsupported"},
            )
        resolved = resolve_multiple(variable, multiple)
    if isinstance(resolved, ResolvedValue):
        normalized = normalize_runtime_value(resolved.value)
        if isinstance(normalized, ValueResult) and resolved.handled_by is not None:
            return ValueResult(
                value=normalized.value,
                handled_by=resolved.handled_by,
            )
        return normalized
    if isinstance(resolved, FailedResolution):
        return ConditionResult(condition=resolved.condition)
    if "missing" in options:
        return handler_value(options, "missing")
    return expression_condition(
        "mapping",
        "missing_input",
        {"variable": variable},
        "missing",
        requirement="R007-49",
    )


def _literal(payload: object, resolver: Resolver) -> EvaluationResult:
    del resolver
    return normalize_runtime_value(payload)


def _mapping(payload: object, resolver: Resolver) -> EvaluationResult:
    if not isinstance(payload, Mapping):
        return expression_condition(
            "validation",
            "invalid_field_type",
            {"operation": "mapping", "expected": "mapping"},
        )
    variable = payload.get("source")
    dictionary = payload.get("dict")
    case_sensitive = payload.get("case_sensitive", True)
    if (
        not isinstance(variable, str)
        or not isinstance(dictionary, Mapping)
        or type(case_sensitive) is not bool
    ):
        return expression_condition(
            "validation",
            "invalid_field_type",
            {"operation": "mapping", "expected": "source and dict"},
        )

    keys = list(dictionary)
    if not all(isinstance(key, str) for key in keys):
        invalid_key = next(key for key in keys if not isinstance(key, str))
        return expression_condition(
            "validation",
            "incompatible_input_type",
            {"expected": "str", "actual": type(invalid_key).__name__},
        )

    folded: dict[str, str] = {}
    if not case_sensitive:
        originals: dict[str, list[str]] = {}
        for key in keys:
            folded_key = ascii_upper(key)
            originals.setdefault(folded_key, []).append(key)
            folded[folded_key] = key
        collisions = {
            key: entries for key, entries in originals.items() if len(entries) > 1
        }
        if collisions:
            folded_key = min(collisions)
            return expression_condition(
                "validation",
                "ambiguous_dictionary",
                {"folded_key": folded_key, "entries": collisions[folded_key]},
            )

    resolved = resolver.resolve(variable)
    if isinstance(resolved, FailedResolution):
        return ConditionResult(condition=resolved.condition)
    if isinstance(resolved, AbsentValue):
        return expression_condition(
            "validation",
            "unknown_field",
            {"identifier": variable},
        )
    normalized = normalize_runtime_value(resolved.value)
    if not isinstance(normalized, ValueResult):
        return normalized
    value = normalized.value
    if value is MISSING:
        if "missing" in payload:
            return handler_value(payload, "missing")
        return expression_condition(
            "mapping",
            "missing_input",
            {"variable": variable},
            "missing",
            requirement="R007-49",
        )
    if not isinstance(value, str):
        return expression_condition(
            "validation",
            "incompatible_input_type",
            {"expected": "str", "actual": runtime_type_name(value)},
        )

    if case_sensitive:
        matched = value if value in dictionary else None
    else:
        matched = folded.get(ascii_upper(value))

    if matched is not None:
        return normalize_runtime_value(dictionary[matched])
    if "unmapped" in payload:
        return handler_value(payload, "unmapped")
    return expression_condition(
        "mapping",
        "unmapped_value",
        {"source": variable, "value": value},
        "unmapped",
        requirement="R007-49",
    )


CORE_EXPRESSION_HANDLERS: dict[str, ExpressionHandler] = {
    "source": _source,
    "literal": _literal,
    "mapping": _mapping,
}


class NestedDispatcher(Protocol):
    """The dispatch an operation needs to evaluate an expression it nests."""

    def evaluate(
        self,
        expression: ExpressionInput,
        resolver: Resolver,
    ) -> EvaluationResult: ...


def evaluate_nested(
    dispatcher: NestedDispatcher,
    expression: object,
    resolver: Resolver,
    prefix: str,
) -> tuple[EvaluationResult, tuple[HandlerObservation, ...]]:
    """Evaluate one expression R007-3 permits an operation to nest.

    The handler paths a nested expression fires are rebased under `prefix`,
    so the caller that knows the specification path can report every R008-21
    count without the nested operation knowing where it sits.
    """
    if not isinstance(expression, Mapping) or len(expression) != 1:
        return (
            expression_condition(
                "validation",
                "invalid_field_type",
                {"field": prefix, "expected": "one expression operation"},
                requirement="R007-36",
            ),
            (),
        )
    operation = next(iter(expression))
    result = dispatcher.evaluate(expression, resolver)
    if not isinstance(result, ValueResult):
        return result, ()
    observations: list[HandlerObservation] = []
    if result.handled_by is not None:
        observations.append(
            HandlerObservation(
                path=f"{prefix}.{operation}",
                handler=result.handled_by,
            )
        )
    observations.extend(
        HandlerObservation(
            path=f"{prefix}.{operation}.{observation.path}",
            handler=observation.handler,
        )
        for observation in result.observations
    )
    return result, tuple(observations)


class RelationalResolver(Protocol):
    """Resolver extension for the operations that read a whole relation.

    R003's join and R013's reduction reach records scalar resolution cannot
    see, so the runtime that owns the relation answers for them here rather
    than every expression widening its contract to carry a join engine.
    """

    def resolve_relation(
        self,
        operation: str,
        payload: Mapping[str, object],
    ) -> EvaluationResult: ...


def relational_handler(operation: str) -> ExpressionHandler:
    def handler(payload: object, resolver: Resolver) -> EvaluationResult:
        if isinstance(payload, str):
            payload = {"expr": payload}
        if not isinstance(payload, Mapping):
            return expression_condition(
                "validation",
                "invalid_field_type",
                {"operation": operation, "expected": "a mapping"},
                requirement="R007-36",
            )
        resolve_relation = getattr(resolver, "resolve_relation", None)
        if not callable(resolve_relation):
            return expression_condition(
                "validation",
                "invalid_field_type",
                {"operation": operation, "reason": "resolver unsupported"},
            )
        return resolve_relation(operation, dict(payload))

    return handler
