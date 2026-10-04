"""Independent predicate truth and resolution traces shared with the typed Rust core."""

from pathlib import Path

import pytest

from yamaa.expressions.core import MappingResolver
from yamaa.expressions.predicates import evaluate_predicate, parse_predicate

FIXTURE = (
    Path(__file__).resolve().parents[3]
    / "rust/crates/yamaa-core/tests/fixtures/predicate_evaluation.tsv"
)
CASES = [
    line.split("\t")
    for line in FIXTURE.read_text(encoding="ascii").splitlines()
    if line and not line.startswith("#")
]


def scalar(token):
    """Decode authored exact fixture types; text uses UTF-8 hex, never host escapes."""
    if token == "missing":
        return None
    kind, value = token.split(":", 1)
    if kind == "i":
        return int(value)
    if kind == "f":
        return float(value)
    if kind == "b":
        return value == "true"
    assert kind == "s"
    return bytes.fromhex(value).decode("utf-8")


class RecordingResolver(MappingResolver):
    """Record every occurrence, including a failed absent binding."""

    def __init__(self, values):
        """Retain the normal reference binding semantics and a separate trace."""
        super().__init__(values)
        self.trace = []

    def resolve(self, variable):
        """Record before resolution so failures cannot disappear from the trace."""
        self.trace.append(variable)
        return super().resolve(variable)


@pytest.mark.parametrize("case", CASES, ids=[case[0] for case in CASES])
def test_shared_predicate_truth_and_trace(case):
    """Replay authored results against the reference, without regenerating truth."""
    _, expression, _, bindings, expected, trace = case
    values = (
        {}
        if bindings == "-"
        else {
            name: scalar(token)
            for name, token in (entry.split("=", 1) for entry in bindings.split(";"))
        }
    )
    resolver = RecordingResolver(values)
    result = evaluate_predicate(parse_predicate(expression), resolver)
    actual = (
        result.value.name if result.status == "value" else result.condition.condition
    )
    assert actual == expected
    assert resolver.trace == trace.split(",")
