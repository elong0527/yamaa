"""Conditional and scalar selection expressions registered under R007.

`first_available`, `greatest`, `least`, and `case` select one already-computed value
rather than compute a new one, so each retains the selected value's type
(R007-33). `cut` is the one operation here that produces a new string.
"""

from __future__ import annotations

from bisect import bisect_left, bisect_right
from collections.abc import Mapping, Sequence
from itertools import pairwise
from typing import TypeAlias

from yamaa.expressions.core import (
    AbsentValue,
    ExpressionHandler,
    FailedResolution,
    NestedDispatcher,
    ResolvedValue,
    Resolver,
    evaluate_nested,
    expression_condition,
    handler_value,
    resolve_operand,
    source_operand,
)
from yamaa.expressions.predicates import (
    PredicateError,
    PredicateValue,
    TruthValue,
    evaluate_predicate,
    parse_predicate_cached,
)
from yamaa.models import (
    MISSING,
    ConditionResult,
    DateTimeValue,
    DateValue,
    EvaluationResult,
    HandlerObservation,
    RuntimeValue,
    UnsupportedResult,
    ValueResult,
    normalize_runtime_value,
    runtime_type_name,
    values_comparable,
)

OrderKey: TypeAlias = object


def _invalid_payload(operation: str, expected: str) -> ConditionResult:
    return expression_condition(
        "validation",
        "invalid_field_type",
        {"operation": operation, "expected": expected},
        requirement="R007-36",
    )


def _resolve(
    variable: object,
    resolver: Resolver,
    operation: str,
    field: str = "source",
    *,
    filtered: bool = False,
) -> ValueResult | ConditionResult:
    """Read one operand, which R003-21b lets `first_available` narrow to records."""
    operand = source_operand(variable) if filtered else None
    if operand is None:
        if not isinstance(variable, str):
            return _invalid_payload(operation, "a variable name")
        operand = (variable, None)
    variable, selector = operand
    resolved = resolve_operand(resolver, variable, selector)
    if isinstance(resolved, ConditionResult):
        return resolved
    if isinstance(resolved, FailedResolution):
        return ConditionResult(condition=resolved.condition)
    if isinstance(resolved, AbsentValue):
        return expression_condition(
            "validation",
            "unknown_field",
            {"identifier": variable},
            requirement="R002-27",
            field=field,
        )
    assert isinstance(resolved, ResolvedValue)
    normalized = normalize_runtime_value(resolved.value)
    if isinstance(normalized, ValueResult):
        return normalized
    assert isinstance(normalized, ConditionResult)
    return normalized


def _variables(payload: object, operation: str) -> Sequence[object] | ConditionResult:
    if not isinstance(payload, Mapping):
        return _invalid_payload(operation, "a mapping")
    sources = payload.get("sources")
    if not isinstance(sources, Sequence) or isinstance(sources, (str, bytes)):
        return _invalid_payload(operation, "a list of variables")
    return sources


def _first_available(payload: object, resolver: Resolver) -> EvaluationResult:
    sources = _variables(payload, "first_available")
    if isinstance(sources, ConditionResult):
        return sources
    assert isinstance(payload, Mapping)
    for variable in sources:
        resolved = _resolve(
            variable, resolver, "first_available", "sources", filtered=True
        )
        if not isinstance(resolved, ValueResult):
            return resolved
        if resolved.value is not MISSING:
            return ValueResult(value=resolved.value)
    if "default" in payload:
        # `default` is not an R008 handler, so it is counted by no path.
        return normalize_runtime_value(payload["default"])
    return ValueResult(value=MISSING)


def _order_key(value: RuntimeValue) -> OrderKey:
    if isinstance(value, (DateValue, DateTimeValue)):
        return value.ordering_key
    return value


def _extreme(operation: str, *, largest: bool) -> ExpressionHandler:
    def handler(payload: object, resolver: Resolver) -> EvaluationResult:
        sources = _variables(payload, operation)
        if isinstance(sources, ConditionResult):
            return sources
        values: list[RuntimeValue] = []
        for variable in sources:
            resolved = _resolve(variable, resolver, operation, "sources")
            if not isinstance(resolved, ValueResult):
                return resolved
            values.append(resolved.value)

        present = [value for value in values if value is not MISSING]
        # R007-26 and R007-39: a mixed `sources` list fails rather than
        # coercing one operand into the other's type.
        for other in present[1:]:
            if not values_comparable(present[0], other):
                return expression_condition(
                    "validation",
                    "incomparable_sources",
                    {
                        "sources": [str(name) for name in sources],
                        "types": [runtime_type_name(value) for value in values],
                    },
                    requirement="R007-39",
                )
        if not present:
            return ValueResult(value=MISSING)
        chooser = max if largest else min
        return ValueResult(value=chooser(present, key=_order_key))

    return handler


def _case(dispatcher: NestedDispatcher) -> ExpressionHandler:
    def handler(payload: object, resolver: Resolver) -> EvaluationResult:
        # R007-54: a non-empty list of when/then items with an optional
        # single trailing otherwise item.
        if not isinstance(payload, Sequence) or isinstance(payload, (str, bytes)):
            return _invalid_payload("case", "a list of when/then items")
        if not payload:
            return _invalid_payload("case", "at least one when/then item")
        otherwise = None
        has_otherwise = False
        for index, item in enumerate(payload):
            if not isinstance(item, Mapping):
                return _invalid_payload("case", "a when/then or otherwise item")
            if "otherwise" in item:
                if has_otherwise or index != len(payload) - 1:
                    return _invalid_payload("case", "a single trailing otherwise item")
                has_otherwise = True
                otherwise = item["otherwise"]
                continue
            when = item.get("when")
            if not isinstance(when, str):
                return _invalid_payload("case", "a branch predicate")
            try:
                ast = parse_predicate_cached(when)
            except PredicateError as error:
                return expression_condition(
                    "validation",
                    "invalid_predicate",
                    {"predicate": when, "position": error.position},
                    requirement="R004-31",
                    field=f"[{index}].when",
                )
            decided = evaluate_predicate(ast, resolver)
            if isinstance(decided, ConditionResult):
                path_suffix = f"[{index}].when"
                if decided.condition.path_suffix is not None:
                    path_suffix = f"{path_suffix}.{decided.condition.path_suffix}"
                return ConditionResult(
                    condition=decided.condition.model_copy(
                        update={"path_suffix": path_suffix}
                    )
                )
            assert isinstance(decided, PredicateValue)
            # R004 three-valued logic: only TRUE selects the branch.
            if decided.value is not TruthValue.TRUE:
                continue
            result, observations = evaluate_nested(
                dispatcher,
                item.get("then"),
                resolver,
                f"[{index}].then",
            )
            return _selected(result, observations)

        if has_otherwise:
            result, observations = evaluate_nested(
                dispatcher, otherwise, resolver, "otherwise"
            )
            return _selected(result, observations)
        return ValueResult(value=MISSING)

    return handler


def _selected(
    result: EvaluationResult,
    observations: tuple[HandlerObservation, ...],
) -> EvaluationResult:
    """Return the selected value, carrying what its branch handled."""
    if isinstance(result, (ConditionResult, UnsupportedResult)):
        return result
    return ValueResult(value=result.value, observations=observations)


def _cut(payload: object, resolver: Resolver) -> EvaluationResult:
    if not isinstance(payload, Mapping):
        return _invalid_payload("cut", "a mapping")
    variable = payload.get("source")
    breaks = payload.get("breaks")
    labels = payload.get("labels")
    right = payload.get("right", False)
    if (
        not isinstance(breaks, Sequence)
        or isinstance(breaks, (str, bytes))
        or not isinstance(labels, Sequence)
        or isinstance(labels, (str, bytes))
        or type(right) is not bool
    ):
        return _invalid_payload("cut", "breaks, labels, and right")
    if len(labels) != len(breaks) + 1 or not all(
        isinstance(label, str) for label in labels
    ):
        return _invalid_payload("cut", "one label more than breaks")
    if any(
        type(value) is bool or not isinstance(value, (int, float)) for value in breaks
    ):
        return _invalid_payload("cut", "numeric breaks")
    thresholds = [float(value) for value in breaks]
    if any(later <= earlier for earlier, later in pairwise(thresholds)):
        return _invalid_payload("cut", "ascending breaks")

    resolved = _resolve(variable, resolver, "cut")
    if not isinstance(resolved, ValueResult):
        return resolved
    value = resolved.value
    if value is MISSING:
        if "missing" in payload:
            return handler_value(payload, "missing")
        return expression_condition(
            "mapping",
            "missing_input",
            {"variable": str(variable)},
            applicable_handler="missing",
            requirement="R007-49",
        )
    actual = runtime_type_name(value)
    if actual not in {"int", "float"}:
        return expression_condition(
            "validation",
            "incompatible_input_type",
            {"source": str(variable), "expected": "numeric", "actual": actual},
            requirement="R007-22",
            field="source",
        )

    number = float(value)  # type: ignore[arg-type]
    # `right` false is left-closed and right-open; true is its mirror.
    index = (
        bisect_left(thresholds, number) if right else bisect_right(thresholds, number)
    )
    return ValueResult(value=labels[index])


def scalar_handlers(dispatcher: NestedDispatcher) -> dict[str, ExpressionHandler]:
    """Return the R007 selection operations this component registers."""
    return {
        "first_available": _first_available,
        "greatest": _extreme("greatest", largest=True),
        "least": _extreme("least", largest=False),
        "case": _case(dispatcher),
        "cut": _cut,
    }
