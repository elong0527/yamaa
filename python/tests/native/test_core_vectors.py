"""Independently specified arithmetic vectors shared with Rust core tests.

Neither implementation generates expected results. This checks the Python
reference against the same types, binary64 bits, and portable conditions.
"""

import csv
import struct
from pathlib import Path

import pytest

from yamaa.expressions import MappingResolver, evaluate_numeric, parse_numeric
from yamaa.models import MISSING, ConditionResult, ValueResult

FIXTURE = (
    Path(__file__).parents[3] / "rust/crates/yamaa-core/tests/fixtures/arithmetic.tsv"
)
with FIXTURE.open(encoding="utf-8", newline="") as stream:
    VECTORS = list(csv.DictReader(stream, delimiter="\t"))


def _operand(token: str):
    """Decode one typed fixture operand without conflating missing with text."""
    if token == "missing":
        return MISSING
    kind, value = token.split(":", 1)
    if kind == "int":
        return int(value)
    assert kind == "float"
    return float(value)


@pytest.mark.parametrize("vector", VECTORS, ids=[row["id"] for row in VECTORS])
def test_reference_matches_independent_core_vector(vector):
    """Check the Python reference against the same independent Rust test vector."""
    expression = vector["expression"]
    result = evaluate_numeric(
        parse_numeric(expression),
        expression,
        MappingResolver(
            {"L": _operand(vector["left"]), "R": _operand(vector["right"])}
        ),
    )
    if isinstance(result, ConditionResult):
        condition = result.condition
        assert condition.phase == "derivation"
        assert condition.applicable_handler is None
        assert condition.context["expr"] == expression
        actual = f"error:{condition.condition}:{condition.requirement}"
        if condition.condition == "integer_overflow":
            assert condition.context["minimum"] == -(2**63)
            assert condition.context["maximum"] == 2**63 - 1
            actual += f":{condition.context['value']}"
    else:
        assert isinstance(result, ValueResult)
        value = result.value
        if value is MISSING:
            actual = "missing"
        elif type(value) is int:
            actual = f"int:{value}"
        else:
            assert type(value) is float
            actual = f"float:{struct.pack('>d', value).hex()}"
    assert actual == vector["expected"]
