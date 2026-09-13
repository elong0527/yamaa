"""The closed R007 handler map and the dispatcher that reads it.

Registration is one map: an operation the map does not name is reported as
unsupported rather than guessed at, and the planner reads the same map so a
declaration outside the implemented subset fails before any source is read.
"""

from __future__ import annotations

from collections.abc import Mapping
from typing import Final

from yamaa.expressions.aggregate import aggregate_handlers
from yamaa.expressions.core import (
    CORE_EXPRESSION_HANDLERS,
    ExpressionHandler,
    ExpressionInput,
    NestedDispatcher,
    Resolver,
    expression_condition,
)
from yamaa.expressions.dates import date_handlers
from yamaa.expressions.numeric import numeric_handlers
from yamaa.expressions.scalar import scalar_handlers
from yamaa.expressions.strings import string_handlers
from yamaa.expressions.windows import window_handlers
from yamaa.models import EvaluationResult, UnsupportedResult
from yamaa.specification.models import Expression


def build_expression_handlers(
    dispatcher: NestedDispatcher,
) -> dict[str, ExpressionHandler]:
    """Return every operation this component registers.

    The operations R007-3 lets nest an expression evaluate it through the
    dispatcher that owns them, so a nested expression reaches exactly the
    registry its parent was dispatched from.
    """
    return {
        **CORE_EXPRESSION_HANDLERS,
        **numeric_handlers(),
        **aggregate_handlers(),
        **date_handlers(),
        **scalar_handlers(dispatcher),
        **string_handlers(dispatcher),
        **window_handlers(),
    }


class ExpressionDispatcher:
    """Dispatch normalized one-operation expressions through a closed map."""

    def __init__(
        self,
        handlers: Mapping[str, ExpressionHandler] | None = None,
    ) -> None:
        self._handlers = dict(
            build_expression_handlers(self) if handlers is None else handlers
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
            return expression_condition(
                "validation",
                "invalid_field_type",
                {"expected": "one expression operation", "count": len(operations)},
                requirement="R007-36",
            )
        operation, payload = next(iter(operations.items()))
        handler = self._handlers.get(operation)
        if handler is None:
            return UnsupportedResult(operation=operation)
        return handler(payload, resolver)


DEFAULT_EXPRESSION_OPERATIONS: Final[tuple[str, ...]] = tuple(
    build_expression_handlers(ExpressionDispatcher())
)


def evaluate_expression(
    expression: ExpressionInput,
    resolver: Resolver,
) -> EvaluationResult:
    """Evaluate one expression through the registered scalar subset."""
    return ExpressionDispatcher().evaluate(expression, resolver)
