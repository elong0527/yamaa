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


def test_failed_conversion_is_replaced_and_counted() -> None:
    declaration = HandledExpression(
        value=Expression(root={"source": "RAW.X"}),
        missing=7,
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
        lambda output: MappingResolver({"RAW.X": "not-an-int", **output}),
        ExpressionDispatcher(),
        counter,
    )

    assert value == 7
    assert [count.model_dump() for count in counter.snapshot()] == [
        {
            "spec_path": "columns.A.derivation.missing",
            "handler": "missing",
            "count": 1,
        }
    ]


def test_normalized_scalar_aggregate_does_not_invent_an_expr_path() -> None:
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
