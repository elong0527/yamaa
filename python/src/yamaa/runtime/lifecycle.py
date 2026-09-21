"""Run the R005 value lifecycle and account for every R008 handler path."""

from __future__ import annotations

from collections import OrderedDict
from collections.abc import Callable, Mapping, Sequence

from pydantic import BaseModel, ConfigDict, Field

from yamaa.expressions import (
    ExpressionDispatcher,
    Resolver,
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

# Which handler fields the Local handlers contract gives each registered
# operation. REQ-0362 makes a handler on an operation that does not register
# it a schema failure, so this map is the one place a new operation declares
# its handler paths.
DECLARED_HANDLERS: dict[str, tuple[HandlerName, ...]] = {
    "source": ("missing", "multiple_matches"),
    "intermediate": ("missing", "multiple_matches"),
    "mapping": ("missing",),
    "cut": ("missing",),
    "date_impute": ("missing", "invalid"),
    "date_precision": ("missing", "invalid"),
    "datetime_impute": ("missing", "invalid"),
    "datetime_precision": ("missing", "invalid"),
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
        if "missing" in declaration.model_fields_set:
            self.register(f"{planned.path}.missing", "missing")

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
        # REQ-0290 lets these fields nest an expression that owns handlers of
        # its own, so their paths are registered too. `case` takes a list
        # payload; every other operation takes a mapping.
        if operation == "case":
            if isinstance(payload, Sequence) and not isinstance(payload, str):
                for index, item in enumerate(payload):
                    if isinstance(item, Mapping):
                        if "otherwise" in item:
                            self._register_one(
                                item["otherwise"],
                                f"{operation_path}[{index}].otherwise",
                            )
                        else:
                            self._register_one(
                                item.get("then"),
                                f"{operation_path}[{index}].then",
                            )
            return
        if not isinstance(payload, Mapping):
            return
        for handler in DECLARED_HANDLERS.get(operation, ()):
            # A lookup declares its selection handler with `keep` rather than
            # naming it: the choice exists exactly where keep does.
            if handler in payload or (
                operation == "lookup"
                and handler == "multiple_matches"
                and "keep" in payload
            ):
                self.register(f"{operation_path}.{handler}", handler)
        if operation == "str_concat":
            self._register_nested(payload.get("sources"), operation_path, "sources")

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
            sep = "" if observation.path.startswith("[") else "."
            self.increment(
                f"{operation_path}{sep}{observation.path}.{observation.handler}",
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
        # A suffix starting with "[" continues a list item (case[0].when),
        # so no dot separator is added.
        sep = "" if condition.path_suffix.startswith("[") else "."
        path = f"{path}{sep}{condition.path_suffix}"
    return ExecutionDiagnostic(
        phase=condition.phase,
        condition=condition.condition,
        spec_paths=(path,),
        requirement=condition.requirement,
        context=condition.context,
    )


def _condition_at_operation(
    condition: RuntimeCondition,
    expression: Expression,
) -> RuntimeCondition:
    """Keep normalized scalar shorthand from inventing a child path."""
    payload = expression.root[expression.operation]
    suffix = condition.path_suffix
    if (
        expression.operation == "aggregate"
        and isinstance(payload, Mapping)
        and set(payload) == {"expr"}
        and suffix is not None
        and (suffix == "expr" or suffix.startswith("expr."))
    ):
        remainder = suffix.removeprefix("expr").removeprefix(".")
        return condition.model_copy(update={"path_suffix": remainder or None})
    return condition


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
            _condition_diagnostic(
                _condition_at_operation(result.condition, expression),
                operation_path,
            )
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
    """Evaluate, convert, and handle one scalar in R005 order."""
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
        if declaration.strict or "missing" not in declaration.model_fields_set:
            raise LifecycleCondition(
                _condition_diagnostic(
                    converted.condition,
                    f"columns.{planned.column}",
                )
            )
        handler_path = f"{planned.path}.missing"
        counter.increment(handler_path, "missing")
        current = _converted_or_raise(
            declaration.missing,
            target,
            handler_path,
        )
    elif isinstance(converted, UnsupportedResult):
        raise TypeError("conversion cannot return unsupported status")
    else:
        current = converted.value

    return current
