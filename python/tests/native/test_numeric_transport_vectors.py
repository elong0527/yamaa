"""Verify independent numeric boundary truth through Python's real lifecycle."""

import csv
import json
import struct
from pathlib import Path

import pytest

from yamaa.expressions import ExpressionDispatcher, MappingResolver
from yamaa.models import MISSING, DateTimeValue, DateValue
from yamaa.planning import PlannedDerivation
from yamaa.runtime.lifecycle import (
    HandlerCounter,
    LifecycleCondition,
    evaluate_derivation,
)
from yamaa.specification.models import Expression, HandledExpression

FIXTURE = (
    Path(__file__).parents[3]
    / "rust/crates/yamaa-adapters/tests/fixtures/numeric_transport.tsv"
)
with FIXTURE.open(encoding="utf-8", newline="") as stream:
    VECTORS = [
        (row["id"], json.loads(row["request"]), json.loads(row["expected"]))
        for row in csv.DictReader(stream, delimiter="\t")
        if not row["expected"].startswith("error:")
        and json.loads(row["expected"])["outcome"]["status"] != "unsupported"
    ]


def decode(value):
    """Decode explicitly typed test bindings without asking the native implementation."""
    kind, data = next(iter(value.items()))
    if kind == "missing":
        return MISSING
    if kind == "int":
        return int(data)
    if kind == "float":
        return struct.unpack(">d", bytes.fromhex(data))[0]
    if kind in {"date", "datetime"}:
        cls = DateValue if kind == "date" else DateTimeValue
        return cls.parse(data["text"]).model_copy(
            update={"collected_precision": data["precision"]}
        )
    return data


def encode(value):
    """Preserve exact values/types and precision in the independently defined wire form."""
    if value is MISSING or value is None:
        return {"missing": None}
    if isinstance(value, (DateValue, DateTimeValue)):
        kind = "date" if isinstance(value, DateValue) else "datetime"
        return {kind: {"text": value.to_text(), "precision": value.collected_precision}}
    if type(value) is float:
        return {"float": struct.pack(">d", value).hex()}
    if type(value) is int:
        return {"int": str(value)}
    if type(value) is bool:
        return {"bool": value}
    return {"str": value}


class RecordingResolver(MappingResolver):
    """Observe each occurrence without memoization or special missing-value handling."""

    def __init__(self, values):
        """Bind normalized values and an initially empty per-call trace."""
        super().__init__(values)
        self.trace = []

    def resolve(self, name):
        """Append the reached name before invoking normal reference resolution."""
        self.trace.append(name)
        return super().resolve(name)


@pytest.mark.parametrize(
    "name,invocation,expected", VECTORS, ids=[v[0] for v in VECTORS]
)
def test_numeric_transport_truth_matches_reference(name, invocation, expected):
    """Compare normative values and complete primary diagnostic context with independent truth."""
    declaration = {"value": Expression(root={"compute": invocation["expression"]})}
    if "unconvertible" in invocation:
        handler = decode(invocation["unconvertible"]["value"])
        declaration["unconvertible"] = None if handler is MISSING else handler
    plan = PlannedDerivation(
        column="A",
        path="columns.A.derivation",
        expression_path="columns.A.derivation.value",
        declaration=HandledExpression(**declaration),
        dependencies=(),
    )
    resolver = RecordingResolver(
        {b["name"]: decode(b["value"]) for b in invocation["bindings"]}
    )
    counter = HandlerCounter()
    counter.register_derivation(plan)
    try:
        result = evaluate_derivation(
            plan,
            invocation["target"],
            {},
            lambda outputs: resolver,
            ExpressionDispatcher(),
            counter,
        )
        assert expected["outcome"] == {"status": "value", "value": encode(result)}
    except LifecycleCondition as failure:
        actual = failure.diagnostic
        wanted = expected["outcome"]["diagnostic"]
        assert expected["outcome"]["status"] == "failure"
        assert (
            actual.phase,
            actual.condition,
            actual.requirement,
            list(actual.spec_paths),
        ) == (
            wanted["phase"],
            wanted["condition"],
            wanted["requirement"],
            wanted["spec_paths"],
        )
        context = {key: encode(value) for key, value in actual.context.items()}
        if (
            actual.condition == "conversion_failed"
            and "integer" in wanted["context"]["value"]
        ):
            context["value"] = {"integer": str(actual.context["value"])}
        assert context == wanted["context"]
    assert resolver.trace == expected["resolutions"]
    assert [
        {"spec_path": c.spec_path, "handler": c.handler, "count": str(c.count)}
        for c in counter.snapshot()
    ] == expected["handler_counts"]
