from __future__ import annotations

from yamaa.expressions import ExpressionDispatcher, MappingResolver, parse_predicate
from yamaa.planning import PlannedDerivation
from yamaa.runtime.lifecycle import HandlerCounter, evaluate_derivation
from yamaa.specification.models import Expression, HandledExpression, OverrideRule


def test_conversion_failure_is_replaced_and_counted() -> None:
    declaration = HandledExpression(
        value=Expression(root={"source": "RAW.X"}),
        conversion_failure=7,
    )
    planned = PlannedDerivation(
        column="A",
        path="columns.A.derivation",
        expression_path="columns.A.derivation.value",
        declaration=declaration,
        dependencies=(),
        override_predicates=(),
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
            "spec_path": "columns.A.derivation.conversion_failure",
            "handler": "conversion_failure",
            "count": 1,
        }
    ]


def test_only_the_first_matching_override_runs_and_all_paths_are_reported() -> None:
    overrides = [
        OverrideRule(when="A = 1", value=Expression(root={"literal": 9})),
        OverrideRule(when="A = 1", value=Expression(root={"literal": 10})),
    ]
    declaration = HandledExpression(
        value=Expression(root={"source": "RAW.X"}),
        override=overrides,
    )
    planned = PlannedDerivation(
        column="A",
        path="columns.A.derivation",
        expression_path="columns.A.derivation.value",
        declaration=declaration,
        dependencies=("A",),
        override_predicates=tuple(parse_predicate(item.when) for item in overrides),
    )
    counter = HandlerCounter()
    counter.register_derivation(planned)

    value = evaluate_derivation(
        planned,
        "int",
        {},
        lambda output: MappingResolver({"RAW.X": "1", **output}),
        ExpressionDispatcher(),
        counter,
    )

    assert value == 9
    assert [(item.spec_path, item.count) for item in counter.snapshot()] == [
        ("columns.A.derivation.override[0]", 1),
        ("columns.A.derivation.override[1]", 0),
    ]
