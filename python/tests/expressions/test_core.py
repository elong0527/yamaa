from __future__ import annotations

import math

from yamaa.expressions import (
    ExpressionDispatcher,
    FailedResolution,
    MappingResolver,
    Resolver,
    evaluate_expression,
)
from yamaa.models import (
    MISSING,
    ConditionResult,
    RuntimeCondition,
    UnsupportedResult,
    ValueResult,
)
from yamaa.specification.models import Expression


def test_source_distinguishes_absence_present_missing_and_resolution_failure() -> None:
    resolver = MappingResolver({"PRESENT": MISSING})

    present = evaluate_expression({"source": "PRESENT"}, resolver)
    assert isinstance(present, ValueResult)
    assert present.value is MISSING

    absent = evaluate_expression({"source": "ABSENT"}, resolver)
    assert isinstance(absent, ConditionResult)
    assert absent.condition.condition == "missing_input"
    assert absent.condition.applicable_handler == "missing"

    handled = evaluate_expression(
        {"source": {"variable": "ABSENT", "missing": None}},
        resolver,
    )
    assert isinstance(handled, ValueResult)
    assert handled.value is MISSING
    assert handled.handled_by == "missing"

    failure = RuntimeCondition(
        phase="join",
        condition="multiple_matches",
        context={"variable": "BROKEN"},
    )

    class BrokenResolver:
        def resolve(self, variable: str) -> FailedResolution:
            del variable
            return FailedResolution(condition=failure)

    failed = evaluate_expression({"source": "BROKEN"}, BrokenResolver())
    assert isinstance(failed, ConditionResult)
    assert failed.condition == failure


def test_literal_normalizes_non_finite_values() -> None:
    result = evaluate_expression({"literal": math.inf}, MappingResolver({}))

    assert isinstance(result, ValueResult)
    assert result.value is MISSING


def test_dispatch_accepts_the_normalized_expression_model() -> None:
    expression = Expression(root={"source": {"variable": "VALUE"}})

    result = evaluate_expression(expression, MappingResolver({"VALUE": "kept"}))

    assert result == ValueResult(value="kept")


def test_mapping_handles_missing_unmapped_and_ascii_case() -> None:
    resolver = MappingResolver({"MISSING": MISSING, "CODE": "yes"})

    missing = evaluate_expression(
        {
            "mapping": {
                "source": "MISSING",
                "dict": {"Y": "Yes"},
                "missing": "Unknown",
            }
        },
        resolver,
    )
    assert isinstance(missing, ValueResult)
    assert missing.value == "Unknown"
    assert missing.handled_by == "missing"

    folded = evaluate_expression(
        {
            "mapping": {
                "source": "CODE",
                "dict": {"YES": "Y"},
                "case_sensitive": False,
            }
        },
        resolver,
    )
    assert isinstance(folded, ValueResult)
    assert folded.value == "Y"

    unmapped = evaluate_expression(
        {
            "mapping": {
                "source": "CODE",
                "dict": {"N": "No"},
                "unmapped": None,
            }
        },
        resolver,
    )
    assert isinstance(unmapped, ValueResult)
    assert unmapped.value is MISSING
    assert unmapped.handled_by == "unmapped"


def test_mapping_resolver_failures_do_not_fire_data_handlers() -> None:
    result = evaluate_expression(
        {
            "mapping": {
                "source": "UNKNOWN",
                "dict": {"Y": "Yes"},
                "missing": "Unknown",
                "unmapped": "Other",
            }
        },
        MappingResolver({}),
    )

    assert isinstance(result, ConditionResult)
    assert result.condition.condition == "unknown_field"
    assert result.condition.applicable_handler is None


def test_mapping_detects_ascii_fold_collisions() -> None:
    result = evaluate_expression(
        {
            "mapping": {
                "source": "CODE",
                "dict": {"a": 1, "A": 2},
                "case_sensitive": False,
            }
        },
        MappingResolver({"CODE": "A"}),
    )

    assert isinstance(result, ConditionResult)
    assert result.condition.condition == "ambiguous_dictionary"


def test_mapping_validates_dictionary_before_resolving_its_source() -> None:
    result = evaluate_expression(
        {
            "mapping": {
                "source": "UNKNOWN",
                "dict": {"a": 1, "A": 2},
                "case_sensitive": False,
            }
        },
        MappingResolver({}),
    )

    assert isinstance(result, ConditionResult)
    assert result.condition.condition == "ambiguous_dictionary"


def test_unsupported_operation_is_not_a_failure_condition() -> None:
    result = evaluate_expression(
        {"compute": "VALUE + 1"},
        MappingResolver({"VALUE": 1}),
    )

    assert result == UnsupportedResult(operation="compute")


def test_dispatch_protocol_accepts_a_bounded_custom_handler() -> None:
    def copy(payload: object, resolver: Resolver) -> ValueResult:
        del resolver
        return ValueResult(value=str(payload))

    dispatcher = ExpressionDispatcher({"copy": copy})

    assert dispatcher.supported_operations == ("copy",)
    assert dispatcher.evaluate({"copy": "kept"}, MappingResolver({})) == ValueResult(
        value="kept"
    )
