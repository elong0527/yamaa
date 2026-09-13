from __future__ import annotations

import pytest

from yamaa.expressions import MappingResolver, evaluate_expression
from yamaa.models import (
    MISSING,
    ConditionResult,
    DateValue,
    UnsupportedResult,
    ValueResult,
)

MARCH = DateValue.parse("2025-03-01")
JUNE = DateValue.parse("2025-06-01")


def _evaluate(expression: dict[str, object], values: dict[str, object]) -> object:
    return evaluate_expression(expression, MappingResolver(values))


@pytest.mark.parametrize(
    ("values", "expected"),
    [
        ({"A": "first", "B": "second"}, "first"),
        ({"A": MISSING, "B": "second"}, "second"),
        ({"A": MISSING, "B": MISSING}, "UNKNOWN"),
        ({"A": "", "B": "second"}, ""),
        ({"A": 0, "B": 1}, 0),
    ],
)
def test_coalesce_returns_the_first_non_missing_source(
    values: dict[str, object], expected: object
) -> None:
    # R019 keeps the empty string distinct from missing, and zero is a value.
    result = _evaluate(
        {"coalesce": {"sources": ["A", "B"], "default": "UNKNOWN"}}, values
    )

    assert result == ValueResult(value=expected)


def test_coalesce_without_a_default_returns_missing() -> None:
    result = _evaluate(
        {"coalesce": {"sources": ["A", "B"]}}, {"A": MISSING, "B": MISSING}
    )

    assert result == ValueResult(value=MISSING)


def test_coalesce_retains_the_selected_value_type() -> None:
    # R007-33: a selection expression does not convert what it selects.
    result = _evaluate({"coalesce": {"sources": ["A", "B"]}}, {"A": MISSING, "B": 7})

    assert isinstance(result, ValueResult)
    assert type(result.value) is int


def test_coalesce_default_is_not_a_handler_path() -> None:
    # R008-1 does not list `coalesce.default`, so it fires no handler count.
    result = _evaluate(
        {"coalesce": {"sources": ["A"], "default": "UNKNOWN"}}, {"A": MISSING}
    )

    assert result == ValueResult(value="UNKNOWN", handled_by=None)


@pytest.mark.parametrize(
    ("operation", "values", "expected"),
    [
        ("greatest", {"A": 1, "B": 3, "C": 2}, 3),
        ("least", {"A": 1, "B": 3, "C": 2}, 1),
        ("greatest", {"A": 1, "B": MISSING, "C": 2}, 2),
        ("least", {"A": MISSING, "B": MISSING, "C": 2}, 2),
        ("greatest", {"A": MISSING, "B": MISSING, "C": MISSING}, MISSING),
        ("least", {"A": MISSING, "B": MISSING, "C": MISSING}, MISSING),
        ("greatest", {"A": "apple", "B": "Banana", "C": "cherry"}, "cherry"),
        ("least", {"A": "apple", "B": "Banana", "C": "cherry"}, "Banana"),
        ("greatest", {"A": MARCH, "B": JUNE, "C": MISSING}, JUNE),
        ("least", {"A": MARCH, "B": JUNE, "C": MISSING}, MARCH),
    ],
)
def test_the_row_wise_extremes_use_each_type_own_order(
    operation: str, values: dict[str, object], expected: object
) -> None:
    # R019-8 orders text by scalar value, so upper case sorts before lower.
    result = _evaluate({operation: {"sources": ["A", "B", "C"]}}, values)

    assert result == ValueResult(value=expected)


def test_int_and_float_sources_are_mutually_comparable() -> None:
    # R007-31: R010 promotes them, so the pair is comparable by construction.
    result = _evaluate({"greatest": {"sources": ["A", "B"]}}, {"A": 2, "B": 2.5})

    assert result == ValueResult(value=2.5)


@pytest.mark.parametrize("operation", ["greatest", "least"])
def test_incomparable_sources_fail_rather_than_coerce_an_operand(
    operation: str,
) -> None:
    result = _evaluate(
        {operation: {"sources": ["ADT", "DAY"]}}, {"ADT": JUNE, "DAY": 12}
    )

    assert isinstance(result, ConditionResult)
    assert result.condition.condition == "incomparable_sources"
    assert result.condition.requirement == "R007-39"
    assert result.condition.context == {
        "sources": ["ADT", "DAY"],
        "types": ["date", "int"],
    }


def _case(branches: list[dict[str, object]], **extra: object) -> dict[str, object]:
    return {"case": {"branches": branches, **extra}}


def test_case_returns_the_first_true_branch() -> None:
    expression = _case(
        [
            {"when": "AGE >= 65", "then": {"literal": "elderly"}},
            {"when": "AGE >= 18", "then": {"literal": "adult"}},
        ],
        otherwise={"literal": "child"},
    )

    assert _evaluate(expression, {"AGE": 70}) == ValueResult(value="elderly")
    assert _evaluate(expression, {"AGE": 30}) == ValueResult(value="adult")
    assert _evaluate(expression, {"AGE": 5}) == ValueResult(value="child")


def test_case_without_a_matching_branch_or_otherwise_is_missing() -> None:
    expression = _case([{"when": "AGE >= 65", "then": {"literal": "elderly"}}])

    assert _evaluate(expression, {"AGE": 5}) == ValueResult(value=MISSING)


def test_an_unknown_branch_condition_does_not_select_it() -> None:
    # R004's three-valued logic: only TRUE selects, so a missing input falls
    # through to the next branch rather than taking this one.
    expression = _case(
        [
            {"when": "AGE >= 65", "then": {"literal": "elderly"}},
            {"when": "TRUE", "then": {"literal": "fallback"}},
        ]
    )

    assert _evaluate(expression, {"AGE": MISSING}) == ValueResult(value="fallback")


def test_case_evaluates_a_nested_expression_rather_than_a_literal_only() -> None:
    expression = _case(
        [{"when": "TRUE", "then": {"compute": {"expr": "AGE * 2"}}}],
    )

    assert _evaluate(expression, {"AGE": 21}) == ValueResult(value=42)


def test_a_handler_inside_a_case_branch_is_observed_at_its_own_path() -> None:
    result = _evaluate(
        _case(
            [{"when": "TRUE", "then": {"str_lower": {"source": "A", "missing": "x"}}}]
        ),
        {"A": MISSING},
    )

    assert isinstance(result, ValueResult)
    assert result.value == "x"
    assert [(item.path, item.handler) for item in result.observations] == [
        ("branches[0].then.str_lower", "missing")
    ]


def test_a_branch_predicate_outside_the_grammar_names_its_branch() -> None:
    result = _evaluate(
        _case([{"when": "AE.AESEQ + 1 > 1", "then": {"literal": "y"}}]), {}
    )

    assert isinstance(result, ConditionResult)
    assert result.condition.condition == "invalid_predicate"
    assert result.condition.requirement == "R004-31"
    assert result.condition.path_suffix == "branches[0].when"


def test_an_unsupported_nested_operation_does_not_bypass_validation() -> None:
    result = _evaluate(
        _case([{"when": "TRUE", "then": {"function": {"name": "f", "args": ["A"]}}}]),
        {"A": 1},
    )

    assert result == UnsupportedResult(operation="function")


def _cut(**extra: object) -> dict[str, object]:
    return {
        "cut": {
            "source": "AGE",
            "breaks": [18, 65],
            "labels": ["<18", "18-64", ">=65"],
            **extra,
        }
    }


@pytest.mark.parametrize(
    ("age", "expected"),
    [(0, "<18"), (17, "<18"), (18, "18-64"), (64, "18-64"), (65, ">=65"), (99, ">=65")],
)
def test_cut_is_left_closed_and_right_open_by_default(age: int, expected: str) -> None:
    assert _evaluate(_cut(), {"AGE": age}) == ValueResult(value=expected)


@pytest.mark.parametrize(
    ("age", "expected"),
    [(17, "<18"), (18, "<18"), (19, "18-64"), (65, "18-64"), (66, ">=65")],
)
def test_cut_right_true_closes_the_upper_bound_instead(age: int, expected: str) -> None:
    assert _evaluate(_cut(right=True), {"AGE": age}) == ValueResult(value=expected)


def test_cut_accepts_a_float_source() -> None:
    assert _evaluate(_cut(), {"AGE": 17.9}) == ValueResult(value="<18")


def test_cut_uses_its_missing_handler() -> None:
    result = _evaluate(_cut(missing="UNKNOWN"), {"AGE": MISSING})

    assert result == ValueResult(value="UNKNOWN", handled_by="missing")


def test_an_undeclared_cut_missing_handler_is_fatal() -> None:
    result = _evaluate(_cut(), {"AGE": MISSING})

    assert isinstance(result, ConditionResult)
    assert result.condition.condition == "missing_input"
    assert result.condition.applicable_handler == "missing"


def test_cut_refuses_a_non_numeric_source() -> None:
    result = _evaluate(_cut(), {"AGE": "sixty"})

    assert isinstance(result, ConditionResult)
    assert result.condition.condition == "incompatible_input_type"
    assert result.condition.requirement == "R007-22"
    assert result.condition.path_suffix == "source"
    assert result.condition.context == {
        "source": "AGE",
        "expected": "numeric",
        "actual": "str",
    }


@pytest.mark.parametrize(
    "payload",
    [
        {"source": "AGE", "breaks": [18, 65], "labels": ["a", "b"]},
        {"source": "AGE", "breaks": [65, 18], "labels": ["a", "b", "c"]},
        {"source": "AGE", "breaks": [18, 18], "labels": ["a", "b", "c"]},
        {"source": "AGE", "breaks": ["18"], "labels": ["a", "b"]},
    ],
)
def test_a_cut_declaration_that_labels_nothing_usable_is_refused(
    payload: dict[str, object],
) -> None:
    result = _evaluate({"cut": payload}, {"AGE": 30})

    assert isinstance(result, ConditionResult)
    assert result.condition.condition == "invalid_field_type"


@pytest.mark.parametrize(
    "operation",
    ["coalesce", "greatest", "least", "cut"],
)
def test_an_unresolved_source_is_a_structured_condition(operation: str) -> None:
    payloads: dict[str, object] = {
        "coalesce": {"sources": ["ABSENT"]},
        "greatest": {"sources": ["ABSENT", "ALSO_ABSENT"]},
        "least": {"sources": ["ABSENT", "ALSO_ABSENT"]},
        "cut": {"source": "ABSENT", "breaks": [1], "labels": ["a", "b"]},
    }

    result = _evaluate({operation: payloads[operation]}, {})

    assert isinstance(result, ConditionResult)
    assert result.condition.condition == "unknown_field"
    assert result.condition.context == {"identifier": "ABSENT"}
