"""Initial expression dispatch with injected source resolution."""

from __future__ import annotations

from collections.abc import Callable, Mapping
from typing import Literal, Protocol, TypeAlias

from pydantic import BaseModel, ConfigDict, Field, JsonValue

from yamaa.models.values import (
    MISSING,
    ConditionResult,
    EvaluationResult,
    HandlerName,
    RuntimeCondition,
    UnsupportedResult,
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


class MappingResolver:
    """Small in-memory resolver useful for scalar execution and tests."""

    def __init__(self, values: Mapping[str, object]) -> None:
        self._values = dict(values)

    def resolve(self, variable: str) -> Resolution:
        if variable not in self._values:
            return AbsentValue(variable=variable)
        return ResolvedValue(value=self._values[variable])


ExpressionHandler: TypeAlias = Callable[[object, Resolver], EvaluationResult]
ExpressionInput: TypeAlias = Expression | Mapping[str, object]


def _condition(
    phase: Literal["validation", "mapping"],
    condition: str,
    context: dict[str, JsonValue],
    applicable_handler: Literal["missing", "unmapped"] | None = None,
) -> ConditionResult:
    return ConditionResult(
        condition=RuntimeCondition(
            phase=phase,
            condition=condition,
            context=context,
            applicable_handler=applicable_handler,
        )
    )


def _handler_value(
    payload: Mapping[object, object],
    name: HandlerName,
) -> EvaluationResult:
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
            return _condition(
                "validation",
                "invalid_field_type",
                {"field": "variable", "expected": "str"},
            )
    else:
        return _condition(
            "validation",
            "invalid_field_type",
            {"operation": "source", "expected": "str or mapping"},
        )

    resolved = resolver.resolve(variable)
    if isinstance(resolved, ResolvedValue):
        return normalize_runtime_value(resolved.value)
    if isinstance(resolved, FailedResolution):
        return ConditionResult(condition=resolved.condition)
    if "missing" in options:
        return _handler_value(options, "missing")
    return _condition(
        "mapping",
        "missing_input",
        {"variable": variable},
        "missing",
    )


def _literal(payload: object, resolver: Resolver) -> EvaluationResult:
    del resolver
    return normalize_runtime_value(payload)


def _ascii_fold(value: str) -> str:
    return "".join(
        chr(ord(character) - 32) if "a" <= character <= "z" else character
        for character in value
    )


def _mapping(payload: object, resolver: Resolver) -> EvaluationResult:
    if not isinstance(payload, Mapping):
        return _condition(
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
        return _condition(
            "validation",
            "invalid_field_type",
            {"operation": "mapping", "expected": "source and dict"},
        )

    keys = list(dictionary)
    if not all(isinstance(key, str) for key in keys):
        invalid_key = next(key for key in keys if not isinstance(key, str))
        return _condition(
            "validation",
            "incompatible_input_type",
            {"expected": "str", "actual": type(invalid_key).__name__},
        )

    folded: dict[str, str] = {}
    if not case_sensitive:
        originals: dict[str, list[str]] = {}
        for key in keys:
            folded_key = _ascii_fold(key)
            originals.setdefault(folded_key, []).append(key)
            folded[folded_key] = key
        collisions = {
            key: entries for key, entries in originals.items() if len(entries) > 1
        }
        if collisions:
            folded_key = min(collisions)
            return _condition(
                "validation",
                "ambiguous_dictionary",
                {"folded_key": folded_key, "entries": collisions[folded_key]},
            )

    resolved = resolver.resolve(variable)
    if isinstance(resolved, FailedResolution):
        return ConditionResult(condition=resolved.condition)
    if isinstance(resolved, AbsentValue):
        return _condition(
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
            return _handler_value(payload, "missing")
        return _condition(
            "mapping",
            "missing_input",
            {"variable": variable},
            "missing",
        )
    if not isinstance(value, str):
        return _condition(
            "validation",
            "incompatible_input_type",
            {"expected": "str", "actual": runtime_type_name(value)},
        )

    if case_sensitive:
        matched = value if value in dictionary else None
    else:
        matched = folded.get(_ascii_fold(value))

    if matched is not None:
        return normalize_runtime_value(dictionary[matched])
    if "unmapped" in payload:
        return _handler_value(payload, "unmapped")
    return _condition(
        "mapping",
        "unmapped_value",
        {"value": value},
        "unmapped",
    )


DEFAULT_EXPRESSION_HANDLERS: dict[str, ExpressionHandler] = {
    "source": _source,
    "literal": _literal,
    "mapping": _mapping,
}


class ExpressionDispatcher:
    """Dispatch normalized one-operation expressions through a closed handler map."""

    def __init__(
        self,
        handlers: Mapping[str, ExpressionHandler] | None = None,
    ) -> None:
        self._handlers = dict(
            DEFAULT_EXPRESSION_HANDLERS if handlers is None else handlers
        )

    @property
    def supported_operations(self) -> tuple[str, ...]:
        return tuple(self._handlers)

    def evaluate(
        self,
        expression: ExpressionInput,
        resolver: Resolver,
    ) -> EvaluationResult:
        operations = (
            expression.root if isinstance(expression, Expression) else expression
        )
        if len(operations) != 1:
            return _condition(
                "validation",
                "invalid_field_type",
                {"expected": "one expression operation", "count": len(operations)},
            )
        operation, payload = next(iter(operations.items()))
        handler = self._handlers.get(operation)
        if handler is None:
            return UnsupportedResult(operation=operation)
        return handler(payload, resolver)


def evaluate_expression(
    expression: ExpressionInput,
    resolver: Resolver,
) -> EvaluationResult:
    """Evaluate one expression in the initial supported scalar subset."""
    return ExpressionDispatcher().evaluate(expression, resolver)
