"""Replay typed-evaluation expectations through Python's real numeric parser."""

import csv
import struct
from pathlib import Path

import pytest

from yamaa.expressions import (
    AbsentValue,
    ResolvedValue,
    evaluate_numeric,
    parse_numeric,
)
from yamaa.models import MISSING, ConditionResult, ValueResult

FIXTURE = (
    Path(__file__).parents[3] / "rust/crates/yamaa-core/tests/fixtures/evaluation.tsv"
)
with FIXTURE.open(encoding="utf-8", newline="") as stream:
    VECTORS = list(csv.DictReader(stream, delimiter="\t"))


def _value(token):
    """Decode only the explicit scalar types used by the independent fixture."""
    if token == "missing":
        return MISSING
    kind, text = token.split(":", 1)
    if kind == "str":
        return text
    if kind == "int":
        return int(text)
    if kind == "float":
        return float(text)
    assert kind == "bool" and text in {"true", "false"}
    return text == "true"


class RecordingResolver:
    """Record every identifier occurrence, retaining absent versus missing values."""

    def __init__(self, bindings):
        """Decode declared values independently of the expression's resolution order."""
        self.values = (
            {}
            if bindings == "-"
            else {
                name: _value(value)
                for name, value in (s.split("=", 1) for s in bindings.split(";"))
            }
        )
        self.trace = []

    def resolve(self, name):
        """Observe resolution even when a prior operand is missing."""
        self.trace.append(name)
        if name not in self.values:
            return AbsentValue(variable=name)
        return ResolvedValue(value=self.values[name])


@pytest.mark.parametrize("vector", VECTORS, ids=[row["id"] for row in VECTORS])
def test_reference_matches_evaluation_and_resolution_trace(vector):
    """Compare values, failures and all resolver calls to independently written truth."""
    resolver = RecordingResolver(vector["bindings"])
    expression = vector["expression"]
    result = evaluate_numeric(parse_numeric(expression), expression, resolver)
    if isinstance(result, ConditionResult):
        condition = result.condition
        assert condition.context["expr"] == expression
        assert condition.applicable_handler is None
        actual = f"error:{condition.condition}:{condition.requirement}"
        if condition.condition == "unknown_field":
            assert condition.phase == "validation"
            actual += f":{condition.context['identifier']}"
        elif condition.condition == "incompatible_input_type":
            assert condition.phase == "validation"
            assert condition.context["expected"] == "numeric"
            actual += f":{condition.context['source']}:{condition.context['actual']}"
        else:
            assert condition.phase == "derivation"
            if condition.condition == "integer_overflow":
                actual += f":{condition.context['value']}"
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
    assert ",".join(resolver.trace) == (
        "" if vector["trace"] == "-" else vector["trace"]
    )
