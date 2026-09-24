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


def test_mapping_handles_missing_unlisted_and_ascii_case() -> None:
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

    unlisted = evaluate_expression(
        {
            "mapping": {
                "source": "CODE",
                "dict": {"N": "No"},
                "missing": None,
            }
        },
        resolver,
    )
    assert isinstance(unlisted, ValueResult)
    assert unlisted.value is MISSING
    assert unlisted.handled_by == "missing"


def test_mapping_distinguishes_absent_source_from_unmapped_value() -> None:
    resolver = MappingResolver({"ABSENT": MISSING, "CODE": "ZZ"})

    split = evaluate_expression(
        {
            "mapping": {
                "source": "CODE",
                "dict": {"L": "Left"},
                "missing": "Not collected",
                "unmapped": "Outside codelist",
            }
        },
        resolver,
    )
    assert isinstance(split, ValueResult)
    assert split.value == "Outside codelist"
    assert split.handled_by == "unmapped"

    absent = evaluate_expression(
        {
            "mapping": {
                "source": "ABSENT",
                "dict": {"L": "Left"},
                "missing": "Not collected",
                "unmapped": "Outside codelist",
            }
        },
        resolver,
    )
    assert isinstance(absent, ValueResult)
    assert absent.value == "Not collected"
    assert absent.handled_by == "missing"

    fallback = evaluate_expression(
        {
            "mapping": {
                "source": "CODE",
                "dict": {"L": "Left"},
                "missing": "Unknown",
            }
        },
        resolver,
    )
    assert isinstance(fallback, ValueResult)
    assert fallback.value == "Unknown"
    assert fallback.handled_by == "missing"

    strict_absent = evaluate_expression(
        {
            "mapping": {
                "source": "ABSENT",
                "dict": {"L": "Left"},
                "strict": True,
                "unmapped": "Outside codelist",
            }
        },
        resolver,
    )
    assert isinstance(strict_absent, ConditionResult)
    assert strict_absent.condition.condition == "missing_input"

    strict_absent_handled = evaluate_expression(
        {
            "mapping": {
                "source": "ABSENT",
                "dict": {"L": "Left"},
                "strict": True,
                "missing": "Not collected",
            }
        },
        resolver,
    )
    assert isinstance(strict_absent_handled, ValueResult)
    assert strict_absent_handled.value == "Not collected"
    assert strict_absent_handled.handled_by == "missing"

    strict_unmapped_handled = evaluate_expression(
        {
            "mapping": {
                "source": "CODE",
                "dict": {"L": "Left"},
                "strict": True,
                "unmapped": "Outside codelist",
            }
        },
        resolver,
    )
    assert isinstance(strict_unmapped_handled, ValueResult)
    assert strict_unmapped_handled.value == "Outside codelist"

    strict_missing_only = evaluate_expression(
        {
            "mapping": {
                "source": "CODE",
                "dict": {"L": "Left"},
                "strict": True,
                "missing": "Not collected",
            }
        },
        resolver,
    )
    # Under strict, `missing` answers only the absent source: an unmapped
    # value still needs its own `unmapped` handler.
    assert isinstance(strict_missing_only, ConditionResult)
    assert strict_missing_only.condition.condition == "unmapped_value"
    assert strict_missing_only.condition.applicable_handler == "unmapped"

    strict_unmapped_bare = evaluate_expression(
        {
            "mapping": {
                "source": "CODE",
                "dict": {"L": "Left"},
                "strict": True,
            }
        },
        resolver,
    )
    assert isinstance(strict_unmapped_bare, ConditionResult)
    assert strict_unmapped_bare.condition.condition == "unmapped_value"
    assert strict_unmapped_bare.condition.applicable_handler == "unmapped"


def test_mapping_resolver_failures_do_not_fire_data_handlers() -> None:
    result = evaluate_expression(
        {
            "mapping": {
                "source": "UNKNOWN",
                "dict": {"Y": "Yes"},
                "missing": "Unknown",
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
    assert result.condition.path_suffix == "dict"


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


def test_mapping_rejects_unresolved_dict_yaml() -> None:
    result = evaluate_expression(
        {"mapping": {"source": "CODE", "dict_yaml": "sevord.yaml"}},
        MappingResolver({"CODE": "MILD"}),
    )

    assert isinstance(result, ConditionResult)
    assert result.condition.condition == "invalid_field_type"


def test_unsupported_operation_is_not_a_failure_condition() -> None:
    # `function` is a registered R018 project extension this component does
    # not implement, so it reports its status rather than a failure.
    result = evaluate_expression(
        {"function": {"name": "bmi", "args": ["VALUE"]}},
        MappingResolver({"VALUE": 1}),
    )

    assert result == UnsupportedResult(operation="function")


def test_dispatch_protocol_accepts_a_bounded_custom_handler() -> None:
    def copy(payload: object, resolver: Resolver) -> ValueResult:
        del resolver
        return ValueResult(value=str(payload))

    dispatcher = ExpressionDispatcher({"copy": copy})

    assert dispatcher.supported_operations == ("copy",)
    assert dispatcher.evaluate({"copy": "kept"}, MappingResolver({})) == ValueResult(
        value="kept"
    )
