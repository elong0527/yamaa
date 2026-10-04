"""Independent ordered SUM/MEAN truth shared with the Rust primitive."""

from __future__ import annotations

import csv
import struct
from pathlib import Path

import pytest

from yamaa.expressions import evaluate_aggregate, parse_aggregate
from yamaa.models import MISSING, ConditionResult, ValueResult

FIXTURE = (
    Path(__file__).parents[3] / "rust/crates/yamaa-core/tests/fixtures/reduction.tsv"
)
with FIXTURE.open(encoding="ascii", newline="") as stream:
    CASES = list(csv.DictReader(stream, delimiter="\t"))


def _value(token: str) -> object:
    """Decode independent fixture values without deriving any expected result."""
    if token == "missing":
        return MISSING
    kind, value = token.split(":", 1)
    if kind == "int":
        return int(value)
    if kind == "float":
        return float(value)
    if kind == "str":
        return value
    if kind == "bool":
        return value == "true"
    raise AssertionError(token)


@pytest.mark.parametrize("case", CASES, ids=[case["id"] for case in CASES])
def test_reference_matches_independent_reduction_truth(case: dict[str, str]) -> None:
    """Compare reference outcomes and diagnostic facts with committed truth."""
    expression = f"{case['reducer']}(A)"
    records = (
        []
        if case["values"] == "-"
        else [{"A": _value(token)} for token in case["values"].split(",")]
    )
    result = evaluate_aggregate(parse_aggregate(expression), expression, records)
    if isinstance(result, ConditionResult):
        diagnostic = result.condition
        assert diagnostic.context["expr"] == expression
        assert diagnostic.path_suffix == "expr"
        if diagnostic.condition == "integer_overflow":
            assert diagnostic.phase == "derivation"
            detail = diagnostic.context["value"]
        else:
            assert diagnostic.phase == "validation"
            assert diagnostic.context["expected"] == "numeric"
            assert diagnostic.context["source"] == "A"
            assert diagnostic.context["reducer"] == case["reducer"]
            detail = diagnostic.context["actual"]
        actual = f"error:{diagnostic.condition}:{diagnostic.requirement}:{detail}"
    else:
        assert isinstance(result, ValueResult)
        value = result.value
        if value is MISSING:
            actual = "missing"
        elif isinstance(value, float):
            actual = f"float:{struct.pack('>d', value).hex()}"
        else:
            assert type(value) is int
            actual = f"int:{value}"
    assert actual == case["expected"]


@pytest.mark.parametrize("prefix", [[2**63 - 1, 1], ["bad", 1], [MISSING, 1]])
def test_argument_collection_failure_precedes_fold_failure(
    prefix: list[object],
) -> None:
    """Keep eager record access ahead of arithmetic and type checking."""
    expression = "SUM(A)"
    records = [{"A": value} for value in prefix] + [{}]
    result = evaluate_aggregate(parse_aggregate(expression), expression, records)
    assert isinstance(result, ConditionResult)
    assert result.condition.condition == "unknown_field"
    assert result.condition.requirement == "REQ-0506"
    assert result.condition.context == {"expr": expression, "identifier": "A"}
