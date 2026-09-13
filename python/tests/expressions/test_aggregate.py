from __future__ import annotations

from pathlib import Path

import pytest
import yaml

from yamaa.expressions import (
    AggregateAst,
    AggregateError,
    aggregate_identifiers,
    aggregate_star_datasets,
    evaluate_aggregate,
    is_single_reduction,
    parse_aggregate,
    parse_aggregate_cached,
    ungrouped_identifiers,
)
from yamaa.models import (
    MISSING,
    ConditionResult,
    DateValue,
    ValueResult,
)

REPOSITORY_ROOT = Path(__file__).parents[3]
GRAMMAR = yaml.safe_load(
    (REPOSITORY_ROOT / "yaml/grammar/aggregate.yaml").read_text(encoding="ascii")
)


def _shape(node: AggregateAst) -> str:
    kind = node["kind"]
    if kind == "number":
        return f"({node['type']} {node['value']})"
    if kind == "null":
        return "null"
    if kind == "identifier":
        return f"(id {node['name']})"
    if kind == "star":
        return f"(star {node['dataset']})"
    if kind == "reduction":
        return f"(reduce {node['name']} {_shape(node['argument'])})"
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
def test_parser_matches_the_shared_r013_contract(case: dict[str, object]) -> None:
    text = case["text"]
    assert isinstance(text, str)
    if case["parse"] == "accept":
        ast = parse_aggregate(text)
        expected_shape = case["shape"]
        assert isinstance(expected_shape, str)
        assert _shape(ast) == " ".join(expected_shape.split())
        assert sorted(aggregate_identifiers(ast)) == case["identifiers"]
        return
    with pytest.raises(AggregateError) as caught:
        parse_aggregate(text)
    assert caught.value.condition == case["condition"]


def test_the_reducer_vocabulary_is_exactly_the_contract() -> None:
    from yamaa.expressions.aggregate import REDUCERS

    declared = GRAMMAR["vocabulary"]["reducer"]

    assert set(REDUCERS) == set(declared)
    assert {name: REDUCERS[name] for name in declared} == {
        name: "star" in entry["argument"] for name, entry in declared.items()
    }


def test_a_repeated_expression_is_parsed_once() -> None:
    first = parse_aggregate_cached("SUM(EX.EXDOSE)")

    assert parse_aggregate_cached("SUM(EX.EXDOSE)") is first


def _records(name: str, values: list[object]) -> list[dict[str, object]]:
    return [{name: value} for value in values]


def _value(
    text: str,
    records: list[dict[str, object]],
    grouped: dict[str, object] | None = None,
) -> object:
    result = evaluate_aggregate(parse_aggregate(text), text, records, grouped)
    assert isinstance(result, ValueResult), result
    return result.value


def _condition(
    text: str,
    records: list[dict[str, object]],
    grouped: dict[str, object] | None = None,
) -> ConditionResult:
    result = evaluate_aggregate(parse_aggregate(text), text, records, grouped)
    assert isinstance(result, ConditionResult), result
    return result


@pytest.mark.parametrize(
    ("text", "values", "expected"),
    [
        ("SUM(A)", [1, 2, 3], 6),
        ("SUM(A)", [1.5, 2.5], 4.0),
        ("COUNT(A)", [1, MISSING, 3], 2),
        ("MIN(A)", [3, 1, 2], 1),
        ("MAX(A)", [3, 1, 2], 3),
        ("MEAN(A)", [3.1, 2.8, 3.4], 3.1),
        ("ONLY(A)", [7], 7),
        ("MIN(A)", ["b", "a", "c"], "a"),
        ("MAX(A)", ["b", "a", "c"], "c"),
    ],
)
def test_every_reducer_returns_the_value_r013_pins(
    text: str, values: list[object], expected: object
) -> None:
    assert _value(text, _records("A", values)) == expected


def test_an_all_missing_group_distinguishes_count_from_every_other_reducer() -> None:
    # R013-27: an uncollected quantity is never reported as a measured zero,
    # but the records themselves still exist for `COUNT`.
    records = _records("A", [MISSING, MISSING])

    assert _value("SUM(A)", records) is MISSING
    assert _value("MIN(A)", records) is MISSING
    assert _value("MAX(A)", records) is MISSING
    assert _value("MEAN(A)", records) is MISSING
    assert _value("COUNT(A)", records) == 0


def test_an_empty_group_leaves_every_reducer_missing() -> None:
    # R013-27: an absent record stays distinguishable from a collected zero.
    for text in ("SUM(A)", "MIN(A)", "MAX(A)", "MEAN(A)", "COUNT(A)", "ONLY(A)"):
        assert _value(text, []) is MISSING
    assert _value("COUNT(EX.*)", []) is MISSING


def test_count_star_counts_records_where_count_of_a_field_counts_values() -> None:
    records = _records("EX.EXDOSE", [1, MISSING, 3])

    assert _value("COUNT(EX.*)", records) == 3
    assert _value("COUNT(EX.EXDOSE)", records) == 2


def test_only_returns_a_single_record_even_when_its_value_is_missing() -> None:
    assert _value("ONLY(A)", _records("A", [MISSING])) is MISSING


def test_only_rejects_a_group_of_several_records_rather_than_choosing() -> None:
    condition = _condition("ONLY(A)", _records("A", [1, 2]))

    assert condition.condition.condition == "aggregate_multiple_records"
    assert condition.condition.requirement == "R013-36"
    assert condition.condition.context["record_count"] == 2
    assert condition.condition.context["reducer"] == "ONLY"


def test_a_reported_group_is_carried_into_the_only_failure() -> None:
    result = evaluate_aggregate(
        parse_aggregate("ONLY(A)"),
        "ONLY(A)",
        _records("A", [1, 2]),
        phase="row_construction",
        context={"row": "derived", "group": {"USUBJID": "P01"}},
    )

    assert isinstance(result, ConditionResult)
    assert result.condition.phase == "row_construction"
    assert result.condition.context["group"] == {"USUBJID": "P01"}


def test_sum_is_a_left_fold_in_relation_order() -> None:
    # R013-15 forbids reordering, so the two orders are allowed to differ and
    # each must match the fold written out by hand.
    forward = _value("SUM(A)", _records("A", [0.1, 0.2, 0.3]))
    reverse = _value("SUM(A)", _records("A", [0.3, 0.2, 0.1]))

    assert forward == (0.1 + 0.2) + 0.3
    assert reverse == (0.3 + 0.2) + 0.1
    assert forward != reverse


def test_mean_inherits_the_ordered_fold_and_its_division() -> None:
    values = [0.1, 0.2, 0.3]

    assert _value("MEAN(A)", _records("A", values)) == ((0.1 + 0.2) + 0.3) / 3


def test_a_reduction_argument_may_compute_before_it_reduces() -> None:
    records = [
        {"EX.EXDOSE": 2.0, "EX.EXDUR": 3.0},
        {"EX.EXDOSE": 4.0, "EX.EXDUR": MISSING},
        {"EX.EXDOSE": 1.0, "EX.EXDUR": 5.0},
    ]

    # R013-26: a record missing either factor contributes missing, not zero.
    assert _value("SUM(EX.EXDOSE * EX.EXDUR)", records) == 11.0


def test_a_grouped_identifier_is_read_directly_beside_a_reduction() -> None:
    records = _records("EX.EXDOSE", [10, 20])

    value = _value(
        "SUM(EX.EXDOSE) / EX.EXPLDOS",
        records,
        {"EX.EXPLDOS": 60},
    )

    assert value == 0.5


def test_arithmetic_over_reductions_uses_r010_promotion_and_division() -> None:
    records = _records("A", [1, 2, 3, MISSING])

    assert _value("SUM(A) / COUNT(A)", records) == 2.0
    assert _value("SUM(A) + COUNT(A)", records) == 9
    assert _value("-SUM(A)", records) == -6


def test_a_missing_reduction_propagates_through_an_operator() -> None:
    assert _value("SUM(A) + COUNT(A)", []) is MISSING
    assert _value("COALESCE(SUM(A), 0)", []) == 0


def test_min_and_max_order_dates_chronologically() -> None:
    records = _records(
        "EX.EXSTDTC",
        [DateValue.parse("2025-03-01"), DateValue.parse("2025-01-08")],
    )

    assert _value("MIN(EX.EXSTDTC)", records) == DateValue.parse("2025-01-08")
    assert _value("MAX(EX.EXSTDTC)", records) == DateValue.parse("2025-03-01")


def test_min_over_incomparable_values_fails_rather_than_inventing_an_order() -> None:
    condition = _condition("MIN(A)", _records("A", [1, "text"]))

    assert condition.condition.condition == "incomparable_sources"
    assert condition.condition.requirement == "R013-46"
    assert condition.condition.context["types"] == ["int", "str"]


def test_sum_over_a_non_numeric_argument_fails() -> None:
    condition = _condition("SUM(A)", _records("A", ["10"]))

    assert condition.condition.condition == "incompatible_input_type"
    assert condition.condition.requirement == "R013-45"
    assert condition.condition.context["actual"] == "str"


def test_sum_fails_on_integer_overflow_through_r010() -> None:
    condition = _condition("SUM(A)", _records("A", [2**62, 2**62, 2**62]))

    assert condition.condition.condition == "integer_overflow"


def test_identifier_collection_separates_the_grain_rule_from_the_reductions() -> None:
    ast = parse_aggregate("SUM(EX.EXDOSE) / EX.EXPLDOS")

    assert aggregate_identifiers(ast) == ("EX.EXDOSE", "EX.EXPLDOS")
    assert ungrouped_identifiers(ast) == ("EX.EXPLDOS",)
    assert not is_single_reduction(ast)
    assert is_single_reduction(parse_aggregate("MIN(EX.EXSTDTC)"))
    assert aggregate_star_datasets(parse_aggregate("COUNT(EX.*)")) == ("EX",)
