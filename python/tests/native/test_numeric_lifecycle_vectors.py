"""Replay independent numeric application cases through the real Python lifecycle."""

import csv
import struct
from pathlib import Path

import pytest

from yamaa.expressions import ExpressionDispatcher, MappingResolver
from yamaa.models import INT64_MAX, INT64_MIN, MISSING, DateTimeValue, DateValue
from yamaa.planning import PlannedDerivation
from yamaa.runtime.lifecycle import (
    HandlerCounter,
    LifecycleCondition,
    evaluate_derivation,
)
from yamaa.specification.models import Expression, HandledExpression

FIXTURE = (
    Path(__file__).parents[3]
    / "rust/crates/yamaa-engine/tests/fixtures/numeric_lifecycle.tsv"
)
with FIXTURE.open(encoding="utf-8", newline="") as stream:
    VECTORS = list(csv.DictReader(stream, delimiter="\t"))


def decode(token):
    """Read explicit fixture types, never infer them from the destination column."""
    if token == "missing":
        return MISSING
    kind, value = token.split(":", 1)
    return {
        "int": int,
        "float": float,
        "str": str,
        "bool": lambda text: text == "true",
    }[kind](value)


def encode(value):
    """Compare exact numeric types and bits, including negative zero and missing."""
    if value is MISSING or value is None:
        return "missing"
    if isinstance(value, (DateValue, DateTimeValue)):
        kind = "date" if isinstance(value, DateValue) else "datetime"
        return f"{kind}:{value.to_text()}:{value.collected_precision}"
    if type(value) is float:
        return "float:" + struct.pack(">d", value).hex()
    if type(value) is bool:
        return "bool:" + str(value).lower()
    return ("int:" if type(value) is int else "str:") + str(value)


class RecordingResolver(MappingResolver):
    """Retain every occurrence even when a preceding operand is missing."""

    def __init__(self, values):
        """Bind one case's inputs and begin a fresh resolver trace."""
        super().__init__(values)
        self.trace = []

    def resolve(self, identifier):
        """Record the call before normal Python source resolution."""
        self.trace.append(identifier)
        return super().resolve(identifier)


@pytest.mark.parametrize("vector", VECTORS, ids=lambda row: row["id"])
def test_numeric_lifecycle_matches_independent_vectors(vector):
    """Compare completed values, fatal diagnostics, handler paths/counts and traces."""
    assert vector["target"] in {"str", "int", "float", "date", "datetime"}
    kwargs = {"value": Expression(root={"compute": vector["expression"]})}
    if vector["handler"] != "-":
        handler = decode(vector["handler"])
        kwargs["unconvertible"] = None if handler is MISSING else handler
    declaration = HandledExpression(**kwargs)
    planned = PlannedDerivation(
        column="A",
        path="columns.A.derivation",
        expression_path="columns.A.derivation.value",
        declaration=declaration,
        dependencies=(),
    )
    values = (
        {}
        if vector["bindings"] == "-"
        else {
            name: decode(token)
            for name, token in (b.split("=", 1) for b in vector["bindings"].split(";"))
        }
    )
    resolver = RecordingResolver(values)
    counter = HandlerCounter()
    counter.register_derivation(planned)
    context = path = "-"
    try:
        result = evaluate_derivation(
            planned,
            vector["target"],
            {},
            lambda outputs: resolver,
            ExpressionDispatcher(),
            counter,
        )
        actual = encode(result)
    except LifecycleCondition as failure:
        diagnostic = failure.diagnostic
        actual = (
            f"error:{diagnostic.phase}:{diagnostic.condition}:{diagnostic.requirement}"
        )
        assert len(diagnostic.spec_paths) == 1
        path = diagnostic.spec_paths[0]
        detail = diagnostic.context
        if diagnostic.condition == "conversion_failed":
            assert detail["to"] == vector["target"]
            value = detail["value"]
            if detail["from"] == "int" and (
                isinstance(value, str) or not INT64_MIN <= value <= INT64_MAX
            ):
                context = "integer:" + str(value)
            else:
                context = encode(value)
            assert context.split(":", 1)[0].replace("integer", "int") == detail["from"]
        else:
            assert detail["expr"] == vector["expression"]
            if diagnostic.condition == "integer_overflow":
                context = "integer:" + str(detail["value"])
            elif diagnostic.condition == "unknown_field":
                context = "identifier:" + detail["identifier"]
            elif diagnostic.condition == "incompatible_input_type":
                context = (
                    "digits:" + detail["actual"]
                    if detail["expected"] == "int"
                    else f"source:{detail['source']}:{detail['actual']}"
                )
    assert (actual, path, context) == (
        vector["expected"],
        vector["path"],
        vector["context"],
    )
    assert ",".join(resolver.trace) == vector["trace"].strip("-")
    expected = (
        []
        if vector["count"] == "-"
        else [
            {
                "spec_path": "columns.A.derivation.unconvertible",
                "handler": "unconvertible",
                "count": int(vector["count"]),
            }
        ]
    )
    assert [c.model_dump() for c in counter.snapshot()] == expected
