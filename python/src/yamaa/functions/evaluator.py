"""Register `function` with the one dispatcher every other operation uses.

R007 keeps expression dispatch a closed map, and R018 does not change that:
an activated project adds exactly one operation to it. Registering rather
than special-casing is what lets a call sit wherever an expression sits --
a column derivation, a row derivation, an override, a branch of a `case` --
without any of those places knowing that a project supplied the code.
"""

from __future__ import annotations

from collections.abc import Mapping

from yamaa.expressions.core import (
    AbsentValue,
    ExpressionHandler,
    FailedResolution,
    ResolvedValue,
    Resolver,
    expression_condition,
)
from yamaa.expressions.dispatch import ExpressionDispatcher
from yamaa.functions.activation import ActivatedEnvironment
from yamaa.functions.invocation import AuthoredValueError, runtime_value
from yamaa.models.values import (
    MISSING,
    ConditionResult,
    EvaluationResult,
    RuntimeValue,
    ValueResult,
    normalize_runtime_value,
)

FUNCTION_OPERATION = "function"


def _invalid(reason: str, **context: object) -> ConditionResult:
    return expression_condition(
        "validation",
        "invalid_field_type",
        {"operation": FUNCTION_OPERATION, "reason": reason, **context},  # type: ignore[arg-type]
    )


def _argument(value: object, resolver: Resolver) -> RuntimeValue | ConditionResult:
    """Resolve one R018-18 argument leaf to the value the binding receives."""
    if isinstance(value, str):
        resolved = resolver.resolve(value)
        if isinstance(resolved, FailedResolution):
            return ConditionResult(condition=resolved.condition)
        if isinstance(resolved, AbsentValue):
            # The planner validated this name against the specification, so
            # a context that does not carry it here reaches no value, which
            # R018-20 answers the same way it answers any missing argument.
            return MISSING
        assert isinstance(resolved, ResolvedValue)
        normalized = normalize_runtime_value(resolved.value)
        if isinstance(normalized, ValueResult):
            return normalized.value
        assert isinstance(normalized, ConditionResult)
        return normalized
    if isinstance(value, Mapping) and set(value) == {"literal"}:
        value = value["literal"]
    try:
        return runtime_value(value)  # type: ignore[arg-type]
    except AuthoredValueError as error:
        return _invalid("an argument carries no exact scalar type", detail=str(error))


def function_handlers(
    activated: ActivatedEnvironment,
) -> dict[str, ExpressionHandler]:
    """Return the one operation an activated project environment registers."""

    def handle(payload: object, resolver: Resolver) -> EvaluationResult:
        if not isinstance(payload, Mapping):
            return _invalid("a call must be a mapping")
        name = payload.get("name")
        requested = payload.get("contract_version")
        arguments = payload.get("args") or {}
        if (
            not isinstance(name, str)
            or not isinstance(requested, str)
            or not isinstance(arguments, Mapping)
        ):
            return _invalid("a call names one function, one version, and its arguments")

        bound = activated.bound(name)
        if bound is None:
            return expression_condition(
                "validation",
                "unknown_project_function",
                {"function": name},
                requirement="R018-37",
                field="name",
            )
        if bound.contract.contract_version != requested:
            return expression_condition(
                "validation",
                "function_contract_mismatch",
                {
                    "function": name,
                    "requested": requested,
                    "available": bound.contract.contract_version,
                },
                requirement="R018-38",
                field="contract_version",
            )

        supplied: dict[str, RuntimeValue] = {}
        for argument, value in arguments.items():
            resolved = _argument(value, resolver)
            if isinstance(resolved, ConditionResult):
                return resolved
            supplied[str(argument)] = resolved
        return bound.invoke(supplied)

    return {FUNCTION_OPERATION: handle}


def function_dispatcher(activated: ActivatedEnvironment) -> ExpressionDispatcher:
    """Return the dispatcher a run with an activated project executes on."""
    return ExpressionDispatcher(extensions=function_handlers(activated))


__all__ = ["FUNCTION_OPERATION", "function_dispatcher", "function_handlers"]
