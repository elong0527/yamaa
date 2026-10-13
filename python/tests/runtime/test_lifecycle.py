from __future__ import annotations

import pytest
from pydantic import ValidationError

from yamaa.expressions import (
    ExpressionDispatcher,
    MappingResolver,
    expression_condition,
)
from yamaa.models import HandlerObservation, ValueResult
from yamaa.planning import ExecutionDiagnostic, PlannedDerivation
from yamaa.runtime.lifecycle import (
    HandlerCount,
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


def _planned(raw: dict) -> PlannedDerivation:
    return PlannedDerivation(
        column="A",
        path="columns.A.derivation",
        expression_path="columns.A.derivation.value",
        declaration=HandledExpression(value=Expression(root=raw)),
        dependencies=(),
    )


def _snapshot(counter: HandlerCounter) -> list[dict]:
    return [count.model_dump() for count in counter.snapshot()]


def test_increment_counts_every_repeated_fire() -> None:
    """A handler that fires twice counts 2, not a saturated or reset 1."""
    counter = HandlerCounter()
    counter.register("columns.A.derivation.value.mapping.missing", "missing")
    counter.increment("columns.A.derivation.value.mapping.missing", "missing")
    counter.increment("columns.A.derivation.value.mapping.missing", "missing")
    counter.increment("columns.A.derivation.value.mapping.unmapped", "unmapped")

    assert _snapshot(counter) == [
        {
            "spec_path": "columns.A.derivation.value.mapping.missing",
            "handler": "missing",
            "count": 2,
        },
        {
            "spec_path": "columns.A.derivation.value.mapping.unmapped",
            "handler": "unmapped",
            "count": 1,
        },
    ]


def test_register_derivation_registers_only_declared_handlers() -> None:
    """Declared handler paths register at zero; undeclared ones stay absent."""
    counter = HandlerCounter()
    counter.register_derivation(
        _planned(
            {
                "mapping": {
                    "source": "X",
                    "dict": {"Y": "Yes"},
                    "missing": "Unknown",
                    "unmapped": None,
                }
            }
        )
    )

    assert _snapshot(counter) == [
        {
            "spec_path": "columns.A.derivation.value.mapping.missing",
            "handler": "missing",
            "count": 0,
        },
        {
            "spec_path": "columns.A.derivation.value.mapping.unmapped",
            "handler": "unmapped",
            "count": 0,
        },
    ]

    plain = HandlerCounter()
    plain.register_derivation(_planned({"source": "RAW.X"}))
    assert plain.snapshot() == ()


def test_register_derivation_registers_case_branch_handlers() -> None:
    """REQ-0290: handlers nested in `then`/`otherwise` branches register too."""
    counter = HandlerCounter()
    counter.register_derivation(
        _planned(
            {
                "case": [
                    {
                        "when": "A > 1",
                        "then": {
                            "mapping": {
                                "source": "X",
                                "dict": {},
                                "missing": "m",
                            }
                        },
                    },
                    {"otherwise": {"source": {"variable": "Z", "absent": "NA"}}},
                ]
            }
        )
    )

    assert _snapshot(counter) == [
        {
            "spec_path": "columns.A.derivation.value.case[0].then.mapping.missing",
            "handler": "missing",
            "count": 0,
        },
        {
            "spec_path": "columns.A.derivation.value.case[1].otherwise.source.absent",
            "handler": "absent",
            "count": 0,
        },
    ]


def test_register_derivation_registers_str_concat_nested_handlers() -> None:
    """REQ-0290: handlers nested in `str_concat` sources register too."""
    counter = HandlerCounter()
    counter.register_derivation(
        _planned(
            {
                "str_concat": {
                    "sources": [{"source": {"variable": "A", "absent": "NA"}}],
                    "missing": "U",
                }
            }
        )
    )

    assert _snapshot(counter) == [
        {
            "spec_path": "columns.A.derivation.value.str_concat.missing",
            "handler": "missing",
            "count": 0,
        },
        {
            "spec_path": "columns.A.derivation.value.str_concat.sources[0].source.absent",
            "handler": "absent",
            "count": 0,
        },
    ]


def test_register_derivation_ignores_malformed_nested_expressions() -> None:
    """A nested item that is not a one-key expression map registers nothing."""
    counter = HandlerCounter()
    counter.register_derivation(
        _planned(
            {
                "str_concat": {
                    "sources": [
                        {"source": {"variable": "A", "absent": "NA"}, "literal": "x"}
                    ],
                    "missing": "U",
                }
            }
        )
    )

    assert _snapshot(counter) == [
        {
            "spec_path": "columns.A.derivation.value.str_concat.missing",
            "handler": "missing",
            "count": 0,
        }
    ]


def test_register_derivation_ignores_empty_and_non_sequence_nesting() -> None:
    """An empty nested map and a non-sequence `sources` value register nothing."""
    for sources in ([{}], 5):
        counter = HandlerCounter()
        counter.register_derivation(
            _planned({"str_concat": {"sources": sources, "missing": "U"}})
        )

        assert _snapshot(counter) == [
            {
                "spec_path": "columns.A.derivation.value.str_concat.missing",
                "handler": "missing",
                "count": 0,
            }
        ]


def test_record_expression_counts_observations_at_completed_paths() -> None:
    """REQ-0361: nested observations count at their completed spec paths."""
    counter = HandlerCounter()
    expression = Expression(root={"str_concat": {"sources": [{"literal": "x"}]}})
    result = ValueResult(
        value="x",
        observations=(
            HandlerObservation(path="sources[0].source", handler="absent"),
            HandlerObservation(path="[0].source", handler="missing"),
        ),
    )

    counter.record_expression("columns.A.derivation.value", expression, result)

    assert _snapshot(counter) == [
        {
            "spec_path": "columns.A.derivation.value.str_concat.sources[0].source.absent",
            "handler": "absent",
            "count": 1,
        },
        {
            "spec_path": "columns.A.derivation.value.str_concat[0].source.missing",
            "handler": "missing",
            "count": 1,
        },
    ]


def _condition_failure(field: str | None) -> LifecycleCondition:
    declaration = HandledExpression(
        value=Expression(root={"mapping": {"source": "X", "dict": {}}})
    )
    planned = PlannedDerivation(
        column="A",
        path="columns.A.derivation",
        expression_path="columns.A.derivation.value",
        declaration=declaration,
        dependencies=(),
    )
    dispatcher = ExpressionDispatcher(
        handlers={
            "mapping": lambda payload, resolver: expression_condition(
                "validation",
                "incompatible_input_type",
                {"expected": "numeric", "actual": "str"},
                field=field,
            )
        }
    )
    with pytest.raises(LifecycleCondition) as raised:
        evaluate_derivation(
            planned,
            "str",
            {},
            lambda output: MappingResolver(output),
            dispatcher,
            HandlerCounter(),
        )
    return raised.value


def test_condition_suffix_joins_with_a_dot_unless_it_opens_a_list_item() -> None:
    """A plain suffix takes a dot separator; a `[` suffix continues the path."""
    assert _condition_failure("dict").diagnostic.spec_paths == (
        "columns.A.derivation.value.mapping.dict",
    )
    assert _condition_failure("[0]").diagnostic.spec_paths == (
        "columns.A.derivation.value.mapping[0]",
    )


def test_aggregate_shorthand_condition_without_suffix_stays_on_operation() -> None:
    """A suffix-free condition on the `{expr}` shorthand invents no child path."""
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


def test_handler_count_rejects_empty_path_and_negative_count() -> None:
    with pytest.raises(ValidationError):
        HandlerCount(spec_path="", handler="missing", count=0)
    with pytest.raises(ValidationError):
        HandlerCount(spec_path="columns.A", handler="missing", count=-1)


def test_handler_count_is_strict_frozen_and_allows_a_one_character_path() -> None:
    count = HandlerCount(spec_path="p", handler="missing", count=0)

    assert count.spec_path == "p"
    with pytest.raises(ValidationError):
        HandlerCount(spec_path="p", handler="missing", count="0")
    with pytest.raises(ValidationError):
        count.count = 5


def test_lifecycle_condition_message_names_the_first_spec_path() -> None:
    diagnostic = ExecutionDiagnostic(
        phase="validation",
        condition="some_condition",
        spec_paths=("first.path", "second.path"),
        context={},
    )

    assert str(LifecycleCondition(diagnostic)) == "first.path: some_condition"
