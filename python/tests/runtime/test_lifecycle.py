from __future__ import annotations

import pytest

from yamaa.expressions import (
    ExpressionDispatcher,
    MappingResolver,
    expression_condition,
)
from yamaa.planning import PlannedDerivation
from yamaa.runtime.lifecycle import (
    HandlerCounter,
    LifecycleCondition,
    evaluate_derivation,
)
from yamaa.specification.models import Expression, HandledExpression


@pytest.mark.parametrize("text", ["not-an-int", "9" * 5000])
def test_failed_conversion_is_replaced_and_counted(text) -> None:
    """Malformed and arbitrarily long out-of-range text each invoke one handler."""
    declaration = HandledExpression(
        value=Expression(root={"source": "RAW.X"}),
        unconvertible=7,
    )
    planned = PlannedDerivation(
        column="A",
        path="columns.A.derivation",
        expression_path="columns.A.derivation.value",
        declaration=declaration,
        dependencies=(),
    )
    counter = HandlerCounter()
    counter.register_derivation(planned)

    value = evaluate_derivation(
        planned,
        "int",
        {},
        lambda output: MappingResolver({"RAW.X": text, **output}),
        ExpressionDispatcher(),
        counter,
    )

    assert value == 7
    assert [count.model_dump() for count in counter.snapshot()] == [
        {
            "spec_path": "columns.A.derivation.unconvertible",
            "handler": "unconvertible",
            "count": 1,
        }
    ]


def test_normalized_scalar_aggregate_does_not_invent_an_expr_path() -> None:
    """Keep aggregate failure provenance on its normalized declaration path."""
    declaration = HandledExpression(
        value=Expression(root={"aggregate": {"expr": "SUM(A)"}})
    )
    planned = PlannedDerivation(
        column="A",
        path="columns.A.derivation",
        expression_path="columns.A.derivation",
        declaration=declaration,
        dependencies=(),
    )
    dispatcher = ExpressionDispatcher(
        handlers={
            "aggregate": lambda payload, resolver: expression_condition(
                "validation",
                "incompatible_input_type",
                {"expected": "numeric", "actual": "str"},
                field="expr",
            )
        }
    )

    with pytest.raises(LifecycleCondition) as raised:
        evaluate_derivation(
            planned,
            "float",
            {},
            lambda output: MappingResolver(output),
            dispatcher,
            HandlerCounter(),
        )

    assert raised.value.diagnostic.spec_paths == ("columns.A.derivation.aggregate",)
