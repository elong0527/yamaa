from __future__ import annotations

import math
from pathlib import Path

import pytest
import yaml

from yamaa.expressions import (
    MappingResolver,
    NumericAst,
    NumericError,
    evaluate_expression,
    evaluate_numeric,
    numeric_identifiers,
    parse_numeric,
)
from yamaa.models import (
    INT64_MAX,
    INT64_MIN,
    MISSING,
    ConditionResult,
    ValueResult,
)

REPOSITORY_ROOT = Path(__file__).parents[3]
GRAMMAR = yaml.safe_load(
    (REPOSITORY_ROOT / "yaml/grammar/numeric.yaml").read_text(encoding="ascii")
)


def _shape(node: NumericAst) -> str:
    kind = node["kind"]
    if kind == "number":
        return f"({node['type']} {node['value']})"
    if kind == "null":
        return "null"
    if kind == "identifier":
        return f"(id {node['name']})"
    if kind == "unary":
        name = "neg" if node["operator"] == "-" else "pos"
        return f"({name} {_shape(node['value'])})"
    if kind == "binary":
        return f"({node['operator']} {_shape(node['left'])} {_shape(node['right'])})"
    if kind == "call":
        parts = ["call", node["name"], *(_shape(a) for a in node["arguments"])]
        return f"({' '.join(parts)})"
    raise AssertionError(f"unknown AST node {kind!r}")


@pytest.mark.parametrize(
    "case",
    GRAMMAR["cases"],
    ids=[case["id"] for case in GRAMMAR["cases"]],
)
def test_parser_matches_the_shared_r010_contract(case: dict[str, object]) -> None:
    text = case["text"]
    assert isinstance(text, str)
    if case["parse"] == "accept":
        ast = parse_numeric(text)
        expected_shape = case["shape"]
        assert isinstance(expected_shape, str)
        assert _shape(ast) == " ".join(expected_shape.split())
        assert sorted(numeric_identifiers(ast)) == case["identifiers"]
        return
    with pytest.raises(NumericError) as caught:
        parse_numeric(text)
    assert caught.value.condition == case["condition"]


def _evaluate(text: str, values: dict[str, object] | None = None) -> object:
    return evaluate_numeric(parse_numeric(text), text, MappingResolver(values or {}))


def _value(text: str, values: dict[str, object] | None = None) -> object:
    result = _evaluate(text, values)
    assert isinstance(result, ValueResult), result
    return result.value


def _condition(text: str, values: dict[str, object] | None = None) -> ConditionResult:
    result = _evaluate(text, values)
    assert isinstance(result, ConditionResult), result
    return result


@pytest.mark.parametrize(
    ("text", "expected"),
    [
        ("1 + 2 * 3", 7),
        ("(1 + 2) * 3", 9),
        ("10 - 3 - 2", 5),
        ("-2 * 3", -6),
        ("+2 + 3", 5),
        ("-(2 + 3)", -5),
        ("7 / 2", 3.5),
        ("2 + 2.5", 4.5),
        ("1E3", 1000.0),
    ],
)
def test_precedence_association_and_promotion(text: str, expected: object) -> None:
    value = _value(text)

    assert value == expected
    assert type(value) is type(expected)


def test_division_always_returns_float_and_never_truncates() -> None:
    # R010-16: there is no integer division; FLOOR(a / b) is how it is written.
    assert _value("7 / 2") == 3.5
    assert _value("FLOOR(7 / 2)") == 3.0


def test_the_written_association_is_the_one_evaluated() -> None:
    # R010-34: `a / (b * b)` and `a / b / b` may differ in the last place and
    # each must return the double the formula as written produces.
    values = {"A": 1.0, "B": 49.0}

    grouped = _value("A / (B * B)", values)
    chained = _value("A / B / B", values)

    assert grouped == 1.0 / (49.0 * 49.0)
    assert chained == 1.0 / 49.0 / 49.0


def test_the_committed_bmi_formula_reproduces_its_committed_doubles() -> None:
    formula = "WEIGHTKG / POWER(NULLIF(HEIGHTCM, 0) / 100, 2)"

    assert _value(formula, {"WEIGHTKG": 81.0, "HEIGHTCM": 180.0}) == 25.0
    assert _value(formula, {"WEIGHTKG": 64.0, "HEIGHTCM": 160.0}) == 24.999999999999996
    # NULLIF turns the unusable height into missing rather than a failure.
    assert _value(formula, {"WEIGHTKG": 70.0, "HEIGHTCM": 0.0}) is MISSING


@pytest.mark.parametrize(
    ("text", "expected"),
    [
        ("ABS(-3)", 3),
        ("ABS(-2.5)", 2.5),
        ("CEIL(2.1)", 3.0),
        ("FLOOR(2.9)", 2.0),
        ("TRUNC(-2.9)", -2.0),
        ("SQRT(9)", 3.0),
        ("POWER(2, 10)", 1024.0),
        ("EXP(0)", 1.0),
        ("LN(1)", 0.0),
        ("MOD(7, 3)", 1),
        ("MOD(-7, 3)", -1),
        ("MOD(7.5, 2)", 1.5),
        ("GREATEST(1, 2, 3)", 3),
        ("LEAST(1, 2, 3)", 1),
        ("GREATEST(1, 2.5)", 2.5),
        ("NULLIF(1, 2)", 1),
        ("COALESCE(3, 4)", 3),
    ],
)
def test_every_permitted_function_returns_its_declared_type(
    text: str, expected: object
) -> None:
    value = _value(text)

    assert value == expected
    # R010-17 and R010-18 fix which functions return float whatever they are
    # given, so the type is asserted rather than only the number.
    assert type(value) is type(expected)


@pytest.mark.parametrize(
    "text",
    [
        "A + 1",
        "A * 2",
        "A / 2",
        "2 / A",
        "-A",
        "ABS(A)",
        "SQRT(A)",
        "POWER(A, 2)",
        "MOD(A, 2)",
        "LN(A)",
    ],
)
def test_missing_propagates_through_every_operator(text: str) -> None:
    assert _value(text, {"A": MISSING}) is MISSING


@pytest.mark.parametrize(
    ("text", "expected"),
    [
        ("COALESCE(A, 2)", 2),
        ("COALESCE(A, B)", MISSING),
        ("GREATEST(A, 2)", 2),
        ("GREATEST(A, B)", MISSING),
        ("LEAST(A, 2)", 2),
        ("NULLIF(A, 1)", MISSING),
        ("NULLIF(2, A)", 2),
        ("NULLIF(2, 2)", MISSING),
    ],
)
def test_the_four_argument_level_functions_answer_missing_themselves(
    text: str, expected: object
) -> None:
    # R010-22 excepts exactly COALESCE, NULLIF, GREATEST, and LEAST.
    assert _value(text, {"A": MISSING, "B": MISSING}) == expected


@pytest.mark.parametrize(
    ("text", "values", "condition", "requirement"),
    [
        ("1 / 0", {}, "division_by_zero", "R010-26"),
        ("1 / A", {"A": 0.0}, "division_by_zero", "R010-26"),
        ("MOD(1, 0)", {}, "division_by_zero", "R010-26"),
        ("SQRT(A)", {"A": -1.0}, "sqrt_of_negative", "R010-27"),
        ("LN(0)", {}, "ln_of_nonpositive", "R010-28"),
        ("LN(A)", {"A": -1.0}, "ln_of_nonpositive", "R010-28"),
        ("POWER(0, -1)", {}, "invalid_power", "R010-29"),
        ("POWER(A, 0.5)", {"A": -4.0}, "invalid_power", "R010-29"),
    ],
)
def test_domain_errors_fail_rather_than_become_missing(
    text: str,
    values: dict[str, object],
    condition: str,
    requirement: str,
) -> None:
    # R010-25: an implementation must not replace an error with missing.
    failure = _condition(text, values)

    assert failure.condition.condition == condition
    assert failure.condition.requirement == requirement
    assert failure.condition.phase == "derivation"
    assert failure.condition.context["expr"] == text


@pytest.mark.parametrize(
    ("text", "values"),
    [
        ("A + 1", {"A": INT64_MAX}),
        ("A - 1", {"A": INT64_MIN}),
        ("A * 2", {"A": INT64_MAX}),
        ("-A", {"A": INT64_MIN}),
        ("ABS(A)", {"A": INT64_MIN}),
    ],
)
def test_integer_overflow_fails_at_the_64_bit_boundary(
    text: str, values: dict[str, object]
) -> None:
    failure = _condition(text, values)

    assert failure.condition.condition == "integer_overflow"
    assert failure.condition.requirement == "R010-30"


@pytest.mark.parametrize(
    ("text", "values", "expected"),
    [
        ("A + 1", {"A": INT64_MAX - 1}, INT64_MAX),
        ("A - 1", {"A": INT64_MIN + 1}, INT64_MIN),
    ],
)
def test_the_representable_boundary_itself_is_not_an_overflow(
    text: str, values: dict[str, object], expected: int
) -> None:
    assert _value(text, values) == expected


@pytest.mark.parametrize(
    ("text", "values"),
    [
        ("EXP(A)", {"A": 1.0e6}),
        ("A * A", {"A": 1.0e308}),
        ("POWER(A, 2)", {"A": 1.0e308}),
    ],
)
def test_a_non_finite_float_result_normalizes_to_missing(
    text: str, values: dict[str, object]
) -> None:
    # R010-24 applies R011's normalization after every operator, so an
    # overflowing double is missing rather than an infinity.
    assert _value(text, values) is MISSING


def test_an_unresolved_identifier_is_a_structured_condition() -> None:
    failure = _condition("UNKNOWN + 1")

    assert failure.condition.condition == "unknown_field"
    assert failure.condition.phase == "validation"
    assert failure.condition.context["identifier"] == "UNKNOWN"


@pytest.mark.parametrize("value", ["text", True])
def test_a_non_numeric_identifier_is_refused_rather_than_converted(
    value: object,
) -> None:
    # R010-21 and R007-19: bind a collected string to a numeric column first.
    failure = _condition("A + 1", {"A": value})

    assert failure.condition.condition == "incompatible_input_type"
    assert failure.condition.context["expected"] == "numeric"


def test_a_compute_expression_reaches_the_registry() -> None:
    result = evaluate_expression(
        {"compute": {"expr": "A * 2"}},
        MappingResolver({"A": 21}),
    )

    assert result == ValueResult(value=42)


def test_a_compute_expression_outside_the_grammar_names_its_field() -> None:
    result = evaluate_expression(
        {"compute": {"expr": "ROUND(A, 2)"}},
        MappingResolver({"A": 1.0}),
    )

    assert isinstance(result, ConditionResult)
    assert result.condition.condition == "prohibited_function"
    assert result.condition.requirement == "R010-36"
    assert result.condition.path_suffix == "expr"
    assert result.condition.context == {"expr": "ROUND(A, 2)", "function": "ROUND"}


def test_parsing_is_shared_without_sharing_a_mutable_parse() -> None:
    first = parse_numeric("A + 1")
    second = parse_numeric("A + 1")

    assert first == second
    assert first is not second


def test_floating_point_results_are_not_rounded_away() -> None:
    # R010-12: no derivation may round, and R010-31 keeps the last place.
    assert _value("0.1 + 0.2") == 0.1 + 0.2
    assert not math.isclose(_value("0.1 + 0.2"), 0.3, rel_tol=0.0, abs_tol=0.0)
