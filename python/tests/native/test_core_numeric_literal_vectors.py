"""Replay compiler literal expectations through Python's real numeric evaluator."""

import csv
import json
import struct
from pathlib import Path

import pytest

from yamaa.expressions import MappingResolver, evaluate_numeric, parse_numeric
from yamaa.models import INT64_MAX, INT64_MIN, MISSING, ConditionResult, ValueResult

FIXTURE = (
    Path(__file__).parents[3]
    / "rust/crates/yamaa-core/tests/fixtures/numeric_literals.tsv"
)
with FIXTURE.open(encoding="utf-8", newline="") as stream:
    VECTORS = list(csv.DictReader(stream, delimiter="\t"))


@pytest.mark.parametrize("vector", VECTORS, ids=[row["id"] for row in VECTORS])
def test_reference_matches_compiler_literal_vector(vector):
    """Check explicit values, float bits and exact positive-literal overflow text."""
    expression = vector["expression"]
    result = evaluate_numeric(
        parse_numeric(expression), expression, MappingResolver({})
    )
    if isinstance(result, ConditionResult):
        condition = result.condition
        assert condition.phase == "derivation"
        assert condition.condition == "integer_overflow"
        assert condition.requirement == "REQ-0434"
        assert condition.applicable_handler is None
        assert condition.context["expr"] == expression
        assert condition.context["minimum"] == INT64_MIN
        assert condition.context["maximum"] == INT64_MAX
        assert json.loads(condition.model_dump_json())["context"] == condition.context
        actual = f"error:integer_overflow:REQ-0434:{condition.context['value']}"
    else:
        assert isinstance(result, ValueResult)
        value = result.value
        if value is MISSING:
            actual = "missing"
        elif type(value) is float:
            actual = f"float:{struct.pack('>d', value).hex()}"
        else:
            assert type(value) is int
            actual = f"int:{value}"
    assert actual == vector["expected"]
