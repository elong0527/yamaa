"""Replay independent scalar conversion truth through the Python reference."""

import csv
import struct
from pathlib import Path

import pytest

from yamaa.models import (
    MISSING,
    ConditionResult,
    DateTimeValue,
    DateValue,
    ValueResult,
    convert_value,
    runtime_type_name,
)

FIXTURE = (
    Path(__file__).parents[3] / "rust/crates/yamaa-core/tests/fixtures/conversion.tsv"
)
with FIXTURE.open(encoding="utf-8", newline="") as stream:
    VECTORS = list(csv.DictReader(stream, delimiter="\t"))


def _input(token):
    """Decode explicit fixture types, including raw bits for shortest-decimal ties."""
    if token == "missing":
        return MISSING
    kind, text = token.split(":", 1)
    if kind == "str":
        return text
    if kind == "int":
        return int(text)
    if kind == "float":
        return float(text)
    if kind == "bits":
        return struct.unpack(">d", bytes.fromhex(text))[0]
    if kind == "bool":
        assert text in {"true", "false"}
        return text == "true"
    if kind == "date":
        return DateValue.parse(text)
    assert kind == "datetime"
    return DateTimeValue.parse(text)


def _encoded(kind, value):
    """Preserve exact float bits and the source type of a diagnostic context value."""
    if kind is None:
        return "missing"
    if kind == "float":
        return f"float:{struct.pack('>d', value).hex()}"
    if kind == "bool":
        return "bool:true" if value else "bool:false"
    if isinstance(value, (DateValue, DateTimeValue)):
        value = value.to_text()
    return f"{kind}:{value}"


@pytest.mark.parametrize("vector", VECTORS, ids=[row["id"] for row in VECTORS])
def test_reference_matches_conversion_vector(vector):
    """Check the complete conversion matrix, canonical text and portable failures."""
    result = convert_value(_input(vector["source"]), vector["target"])
    if isinstance(result, ConditionResult):
        condition = result.condition
        assert condition.phase == "convert"
        assert condition.condition == "conversion_failed"
        assert condition.applicable_handler == "unconvertible"
        assert condition.context["to"] == vector["target"]
        context = _encoded(condition.context["from"], condition.context["value"])
        actual = f"error:{condition.requirement}:{context}"
    else:
        assert isinstance(result, ValueResult)
        actual = _encoded(runtime_type_name(result.value), result.value)
    assert actual == vector["expected"]
