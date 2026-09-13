"""Run the R005 value lifecycle and account for every R008 handler path."""

from __future__ import annotations

from collections import OrderedDict
from collections.abc import Callable, Mapping, Sequence

from pydantic import BaseModel, ConfigDict, Field

from yamaa.expressions import (
    ExpressionDispatcher,
    PredicateValue,
    Resolver,
    TruthValue,
    evaluate_predicate,
)
from yamaa.models import (
    ConditionResult,
    HandlerName,
    RuntimeCondition,
    RuntimeValue,
    UnsupportedResult,
    ValueResult,
    convert_value,
)
from yamaa.planning import ExecutionDiagnostic, PlannedDerivation, UnsupportedFeature
from yamaa.specification.models import ColumnType, Expression

ResolverFactory = Callable[[Mapping[str, object]], Resolver]

# Which R008 handler fields each registered operation offers. R008-22 makes a
# handler on an operation that does not register it a schema failure, so this
# map is the one place a new operation declares its handler paths.
DECLARED_HANDLERS: dict[str, tuple[HandlerName, ...]] = {
    "source": ("missing", "multiple_matches"),
    "mapping": ("missing", "unmapped"),
    "mapping_from": ("missing", "unmapped"),
    "cut": ("missing",),
    "date_impute": ("missing", "invalid"),
    "date_precision": ("missing", "invalid"),
    "str_extract": ("missing", "no_match"),
    "str_concat": ("missing",),
    "str_template": ("missing",),
    "str_upper": ("missing",),
    "str_lower": ("missing",),
}


class HandlerCount(BaseModel):
    """How often one declared handler path fired during execution."""

    model_config = ConfigDict(strict=True, extra="forbid", frozen=True)

    spec_path: str = Field(min_length=1)
    handler: HandlerName
    count: int = Field(ge=0)


class HandlerCounter:
    """Mutable run-local counts with deterministic declaration ordering."""

    def __init__(self) -> None:
        self._counts: OrderedDict[tuple[str, HandlerName], int] = OrderedDict()

    def register(self, spec_path: str, handler: HandlerName) -> None:
        self._counts.setdefault((spec_path, handler), 0)

    def increment(self, spec_path: str, handler: HandlerName) -> None:
        identity = (spec_path, handler)
        self._counts[identity] = self._counts.get(identity, 0) + 1

    def register_derivation(self, planned: PlannedDerivation) -> None:
        self._register_expression(planned.declaration.value, planned.expression_path)
        declaration = planned.declaration
        if "conversion_failure" in declaration.model_fields_set:
            self.register(f"{planned.path}.conversion_failure", "conversion_failure")
        for index, override in enumerate(declaration.override or ()):
            override_path = f"{planned.path}.override[{index}]"
            self.register(override_path, "override")
            self._register_expression(override.value, f"{override_path}.value")

    def _register_expression(self, expression: Expression, path: str) -> None:
        operation = expression.operation
        payload = expression.root[operation]
        self._register_operation(operation, payload, f"{path}.{operation}")

    def _register_operation(
        self,
        operation: str,
        payload: object,
        operation_path: str,
    ) -> None:
        if not isinstance(payload, Mapping):
            return
        for handler in DECLARED_HANDLERS.get(operation, ()):
            if handler in payload:
                self.register(f"{operation_path}.{handler}", handler)
        # R007-3 lets these fields nest an expression that owns handlers of
        # its own, so their paths are registered too.
        if operation == "str_concat":
            self._register_nested(payload.get("sources"), operation_path, "sources")
        elif operation == "case":
            branches = payload.get("branches")
            if isinstance(branches, Sequence) and not isinstance(branches, str):
                for index, branch in enumerate(branches):
                    if isinstance(branch, Mapping):
                        self._register_one(
                            branch.get("then"),
                            f"{operation_path}.branches[{index}].then",
                        )
            if "otherwise" in payload:
                self._register_one(payload["otherwise"], f"{operation_path}.otherwise")

    def _register_nested(self, values: object, prefix: str, field: str) -> None:
        if not isinstance(values, Sequence) or isinstance(values, str):
            return
        for index, value in enumerate(values):
            self._register_one(value, f"{prefix}.{field}[{index}]")

    def _register_one(self, expression: object, path: str) -> None:
        if not isinstance(expression, Mapping) or len(expression) != 1:
            return
        operation, payload = next(iter(expression.items()))
        self._register_operation(operation, payload, f"{path}.{operation}")

    def record_expression(
        self, planned_path: str, expression: Expression, result: ValueResult
    ) -> None:
        operation_path = f"{planned_path}.{expression.operation}"
        if result.handled_by is not None:
            self.increment(f"{operation_path}.{result.handled_by}", result.handled_by)
        for observation in result.observations:
            self.increment(
                f"{operation_path}.{observation.path}.{observation.handler}",
                observation.handler,
            )

    def snapshot(self) -> tuple[HandlerCount, ...]:
        return tuple(
            HandlerCount(spec_path=path, handler=handler, count=count)
            for (path, handler), count in self._counts.items()
        )


class LifecycleCondition(ValueError):
    """A derivation reached a fatal structured condition."""

    def __init__(self, diagnostic: ExecutionDiagnostic) -> None:
        self.diagnostic = diagnostic
        super().__init__(f"{diagnostic.spec_paths[0]}: {diagnostic.condition}")


class LifecycleUnsupported(ValueError):
    """A dispatcher reported a valid but unimplemented expression."""

    def __init__(self, feature: UnsupportedFeature) -> None:
        self.feature = feature
        super().__init__(f"{feature.spec_path}: unsupported {feature.operation}")


def _condition_diagnostic(
    condition: RuntimeCondition,
    path: str,
) -> ExecutionDiagnostic:
    if condition.path_suffix is not None:
        path = f"{path}.{condition.path_suffix}"
    return ExecutionDiagnostic(
        phase=condition.phase,
        condition=condition.condition,
        spec_paths=(path,),
        requirement=condition.requirement,
        context=condition.context,
    )


def _value_or_raise(
    result: ValueResult | ConditionResult | UnsupportedResult,
    path: str,
    expression: Expression,
    counter: HandlerCounter,
) -> RuntimeValue:
    operation_path = f"{path}.{expression.operation}"
    if isinstance(result, UnsupportedResult):
        raise LifecycleUnsupported(
            UnsupportedFeature(
                operation=result.operation,
                spec_path=operation_path,
            )
        )
    if isinstance(result, ConditionResult):
        raise LifecycleCondition(
            _condition_diagnostic(result.condition, operation_path)
        )
    counter.record_expression(path, expression, result)
    return result.value


def _converted_or_raise(value: object, target: ColumnType, path: str) -> RuntimeValue:
    result = convert_value(value, target)
    if isinstance(result, ConditionResult):
        raise LifecycleCondition(_condition_diagnostic(result.condition, path))
    if isinstance(result, UnsupportedResult):
        raise TypeError("conversion cannot return unsupported status")
    return result.value


def evaluate_derivation(
    planned: PlannedDerivation,
    target: ColumnType,
    output_values: Mapping[str, object],
    resolver_factory: ResolverFactory,
    dispatcher: ExpressionDispatcher,
    counter: HandlerCounter,
) -> RuntimeValue:
    """Evaluate, convert, handle, and override one scalar in R005 order."""
    declaration = planned.declaration
    expression = declaration.value
    evaluated = dispatcher.evaluate(expression, resolver_factory(output_values))
    raw = _value_or_raise(
        evaluated,
        planned.expression_path,
        expression,
        counter,
    )

    converted = convert_value(raw, target)
    if isinstance(converted, ConditionResult):
        if "conversion_failure" not in declaration.model_fields_set:
            raise LifecycleCondition(
                _condition_diagnostic(converted.condition, planned.expression_path)
            )
        handler_path = f"{planned.path}.conversion_failure"
        counter.increment(handler_path, "conversion_failure")
        current = _converted_or_raise(
            declaration.conversion_failure,
            target,
            handler_path,
        )
    elif isinstance(converted, UnsupportedResult):
        raise TypeError("conversion cannot return unsupported status")
    else:
        current = converted.value

    for index, override in enumerate(declaration.override or ()):
        override_path = f"{planned.path}.override[{index}]"
        resolver = resolver_factory({**output_values, planned.column: current})
        predicate = evaluate_predicate(planned.override_predicates[index], resolver)
        if isinstance(predicate, ConditionResult):
            raise LifecycleCondition(
                _condition_diagnostic(predicate.condition, f"{override_path}.when")
            )
        assert isinstance(predicate, PredicateValue)
        if predicate.value is not TruthValue.TRUE:
            continue

        counter.increment(override_path, "override")
        override_result = dispatcher.evaluate(override.value, resolver)
        override_raw = _value_or_raise(
            override_result,
            f"{override_path}.value",
            override.value,
            counter,
        )
        current = _converted_or_raise(
            override_raw,
            target,
            f"{override_path}.value",
        )
        break
    return current
