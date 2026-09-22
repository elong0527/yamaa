from __future__ import annotations

from pathlib import Path

import pytest
import yaml

from yamaa.expressions import (
    MappingResolver,
    PredicateAst,
    PredicateError,
    PredicateValue,
    TruthValue,
    evaluate_predicate,
    parse_predicate,
)
from yamaa.models import ConditionResult, DateValue

REPOSITORY_ROOT = Path(__file__).parents[3]
GRAMMAR = yaml.safe_load(
    (REPOSITORY_ROOT / "yaml/grammar/predicate.yaml").read_text(encoding="ascii")
)


def _quote_shape(value: str) -> str:
    return f"'{value.replace(chr(39), chr(39) * 2)}'"


def _operand_shape(node: PredicateAst) -> str:
    if node["kind"] == "identifier":
        return f"(id {node['name']})"
    value_type = node["type"]
    if value_type is None:
        return "null"
    if value_type in {"str", "date", "datetime"}:
        return f"({value_type} {_quote_shape(node['value'])})"
    return f"({value_type} {node['value']})"


def _ast_shape(node: PredicateAst) -> str:
    kind = node["kind"]
    if kind in {"and", "or"}:
        return f"({kind} {_ast_shape(node['left'])} {_ast_shape(node['right'])})"
    if kind == "not":
        return f"(not {_ast_shape(node['value'])})"
    if kind == "boolean":
        return "true" if node["value"] else "false"
    if kind == "comparison":
        return (
            f"({node['operator']} {_operand_shape(node['left'])} "
            f"{_operand_shape(node['right'])})"
        )
    if kind == "null_test":
        name = "is-not-null" if node["negated"] else "is-null"
        return f"({name} {_operand_shape(node['value'])})"
    if kind == "in":
        name = "not-in" if node["negated"] else "in"
        operands = " ".join(_operand_shape(value) for value in node["values"])
        return f"({name} {_operand_shape(node['value'])} {operands})"
    if kind == "between":
        name = "not-between" if node["negated"] else "between"
        return (
            f"({name} {_operand_shape(node['value'])} "
            f"{_operand_shape(node['lower'])} {_operand_shape(node['upper'])})"
        )
    if kind == "like":
        name = "not-like" if node["negated"] else "like"
        rendered = (
            f"({name} {_operand_shape(node['value'])} {_operand_shape(node['pattern'])}"
        )
        if node["escape"] is not None:
            rendered += f" (escape {_quote_shape(node['escape'])})"
        return rendered + ")"
    if kind == "call":
        return (
            f"(str-contains {_operand_shape(node['source'])} "
            f"(str {_quote_shape(node['pattern'])})"
            f")"
        )
    raise AssertionError(f"unknown AST node {kind!r}")


@pytest.mark.parametrize(
    "case",
    GRAMMAR["cases"],
    ids=[case["id"] for case in GRAMMAR["cases"]],
)
def test_parser_matches_the_shared_r004_contract(case: dict[str, object]) -> None:
    text = case["text"]
    assert isinstance(text, str)
    accepted = case["parse"] == "accept"
    if accepted:
        expected_shape = case["shape"]
        assert isinstance(expected_shape, str)
        assert _ast_shape(parse_predicate(text)) == expected_shape
    else:
        with pytest.raises(PredicateError):
            parse_predicate(text)


# Python-only `call` acceptance vectors (issue #778). The shared
# yaml/grammar/predicate.yaml keeps the `call` production but not its
# acceptance vectors, because the R predicate parser has no function-call
# support and the R engine is divergent here by design. These assert the same
# parse shapes and identifier collections the shared vectors recorded before
# they moved here, so Python coverage of the production is not weakened.
@pytest.mark.parametrize(
    ("text", "identifiers", "shape"),
    [
        (
            "str_contains(AEDECOD, 'DERM|ERYTH')",
            ["AEDECOD"],
            "(str-contains (id AEDECOD) (str 'DERM|ERYTH'))",
        ),
        (
            "str_contains(AE.AEDECOD, 'APPLICATION|DERMATITIS|ERYTHEMA|BLISTER') OR AE.AESER = 'Y'",
            ["AE.AEDECOD", "AE.AESER"],
            "(or (str-contains (id AE.AEDECOD) (str 'APPLICATION|DERMATITIS|ERYTHEMA|BLISTER')) (= (id AE.AESER) (str 'Y')))",
        ),
        (
            "NOT str_contains(AEDECOD, 'X')",
            ["AEDECOD"],
            "(not (str-contains (id AEDECOD) (str 'X')))",
        ),
        (
            "str_contains('hello', 'ELL')",
            [],
            "(str-contains (str 'hello') (str 'ELL'))",
        ),
        (
            "STR_CONTAINS(AEDECOD, 'X')",
            ["AEDECOD"],
            "(str-contains (id AEDECOD) (str 'X'))",
        ),
    ],
)
def test_str_contains_call_vectors_match_the_call_production(
    text: str, identifiers: list[str], shape: str
) -> None:
    from yamaa.expressions import predicate_identifiers

    ast = parse_predicate(text)
    assert _ast_shape(ast) == shape
    assert list(predicate_identifiers(ast)) == identifiers


def _evaluate(
    text: str, values: dict[str, object] | None = None
) -> PredicateValue | ConditionResult:
    return evaluate_predicate(parse_predicate(text), MappingResolver(values or {}))


@pytest.mark.parametrize(
    ("text", "expected"),
    [
        ("TRUE AND TRUE", TruthValue.TRUE),
        ("TRUE AND FALSE", TruthValue.FALSE),
        ("FALSE OR TRUE", TruthValue.TRUE),
        ("FALSE OR FALSE", TruthValue.FALSE),
        ("NOT TRUE", TruthValue.FALSE),
        ("NOT NOT TRUE", TruthValue.TRUE),
        ("NOT MISSING = 1", TruthValue.UNKNOWN),
        ("TRUE AND MISSING = 1", TruthValue.UNKNOWN),
        ("FALSE AND MISSING = 1", TruthValue.FALSE),
        ("MISSING = 1 AND TRUE", TruthValue.UNKNOWN),
        ("MISSING = 1 AND FALSE", TruthValue.FALSE),
        ("MISSING = 1 AND MISSING = 2", TruthValue.UNKNOWN),
        ("TRUE OR MISSING = 1", TruthValue.TRUE),
        ("FALSE OR MISSING = 1", TruthValue.UNKNOWN),
        ("MISSING = 1 OR TRUE", TruthValue.TRUE),
        ("MISSING = 1 OR FALSE", TruthValue.UNKNOWN),
        ("MISSING = 1 OR MISSING = 2", TruthValue.UNKNOWN),
    ],
)
def test_boolean_logic(text: str, expected: TruthValue) -> None:
    result = _evaluate(text, {"MISSING": None})
    assert isinstance(result, PredicateValue)
    assert result.value is expected


@pytest.mark.parametrize(
    ("text", "expected"),
    [
        ("MISSING = 1", TruthValue.UNKNOWN),
        ("MISSING <> 1", TruthValue.UNKNOWN),
        ("MISSING IS NULL", TruthValue.TRUE),
        ("MISSING IS NOT NULL", TruthValue.FALSE),
        ("MISSING IN (1, 2)", TruthValue.UNKNOWN),
        ("MISSING NOT IN (1, 2)", TruthValue.UNKNOWN),
        ("MISSING BETWEEN 1 AND 2", TruthValue.UNKNOWN),
        ("MISSING LIKE '%'", TruthValue.UNKNOWN),
        ("FALSE AND MISSING = 1", TruthValue.FALSE),
        ("TRUE OR MISSING = 1", TruthValue.TRUE),
    ],
)
def test_three_valued_missing_logic(text: str, expected: TruthValue) -> None:
    result = _evaluate(text, {"MISSING": None})
    assert isinstance(result, PredicateValue)
    assert result.value is expected


def test_comparisons_preserve_types_and_temporal_order() -> None:
    values = {
        "INTEGER": 9_007_199_254_740_993,
        "FLOAT": 9_007_199_254_740_992.0,
        "ADT": DateValue.parse("2025-06-01"),
    }

    numeric = _evaluate("INTEGER = FLOAT", values)
    temporal = _evaluate("ADT >= DATE '2025-06-01'", values)
    assert isinstance(numeric, PredicateValue)
    assert isinstance(temporal, PredicateValue)
    assert numeric.value is TruthValue.TRUE
    assert temporal.value is TruthValue.TRUE

    incompatible = _evaluate("ADT = '2025-06-01'", values)
    assert isinstance(incompatible, ConditionResult)
    assert incompatible.condition.condition == "incompatible_input_type"


@pytest.mark.parametrize(
    ("text", "expected"),
    [
        ("CODE IN ('A', 'B')", TruthValue.TRUE),
        ("VALUE BETWEEN 1 AND 3", TruthValue.TRUE),
        ("TEXT LIKE 'A_%'", TruthValue.TRUE),
        ("PERCENT LIKE '100!%' ESCAPE '!'", TruthValue.TRUE),
        ("TEXT NOT LIKE 'B%'", TruthValue.TRUE),
    ],
)
def test_compound_predicates(text: str, expected: TruthValue) -> None:
    result = _evaluate(
        text,
        {"CODE": "B", "VALUE": 2, "TEXT": "A\nvalue", "PERCENT": "100%"},
    )

    assert isinstance(result, PredicateValue)
    assert result.value is expected


def test_unknown_identifier_is_a_structured_condition() -> None:
    result = _evaluate("UNKNOWN = 1")

    assert isinstance(result, ConditionResult)
    assert result.condition.phase == "validation"
    assert result.condition.condition == "unknown_field"
    assert result.condition.context == {"identifier": "UNKNOWN"}


@pytest.mark.parametrize(
    ("text", "values", "expected"),
    [
        (
            "str_contains(AEDECOD, 'APPLICATION|DERMATITIS|ERYTHEMA|BLISTER')",
            {"AEDECOD": "APPLICATION SITE DERMATITIS"},
            TruthValue.TRUE,
        ),
        (
            "str_contains(AEDECOD, 'APPLICATION|DERMATITIS|ERYTHEMA|BLISTER')",
            {"AEDECOD": "HEADACHE"},
            TruthValue.FALSE,
        ),
        (
            "str_contains(AE.AEDECOD, 'APPLICATION|DERMATITIS|ERYTHEMA|BLISTER') OR AE.AESER = 'Y'",
            {"AE.AEDECOD": "HEADACHE", "AE.AESER": "Y"},
            TruthValue.TRUE,
        ),
        (
            "NOT str_contains(AEDECOD, 'DERM')",
            {"AEDECOD": "HEADACHE"},
            TruthValue.TRUE,
        ),
        (
            "str_contains(AEDECOD, 'DERM')",
            {"AEDECOD": None},
            TruthValue.UNKNOWN,
        ),
        (
            "NOT str_contains(AEDECOD, 'DERM')",
            {"AEDECOD": None},
            TruthValue.UNKNOWN,
        ),
        (
            "str_contains('APPLICATION SITE DERMATITIS', 'DERM')",
            {},
            TruthValue.TRUE,
        ),
        (
            "STR_CONTAINS(AEDECOD, 'DERM')",
            {"AEDECOD": "DERMATITIS"},
            TruthValue.TRUE,
        ),
    ],
)
def test_str_contains_call(
    text: str, values: dict[str, object], expected: TruthValue
) -> None:
    result = _evaluate(text, values)
    assert isinstance(result, PredicateValue)
    assert result.value is expected


def test_str_contains_call_reports_its_source_identifier() -> None:
    from yamaa.expressions import predicate_identifiers

    ast = parse_predicate("str_contains(AE.AEDECOD, 'DERM') OR AE.AESER = 'Y'")
    assert predicate_identifiers(ast) == ("AE.AEDECOD", "AE.AESER")


def test_str_contains_call_rejects_a_non_string_source() -> None:
    result = _evaluate("str_contains(AVAL, 'DERM')", {"AVAL": 3})
    assert isinstance(result, ConditionResult)
    assert result.condition.condition == "incompatible_input_type"


def test_str_contains_call_rejects_an_invalid_pattern() -> None:
    with pytest.raises(PredicateError):
        parse_predicate("str_contains(AEDECOD, '(')")


def test_str_detect_is_not_a_predicate_function() -> None:
    with pytest.raises(PredicateError):
        parse_predicate("str_detect(AEDECOD, 'DERM')")


def test_bare_str_contains_without_parens_stays_an_identifier() -> None:
    with pytest.raises(PredicateError):
        parse_predicate("str_contains")
