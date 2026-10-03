"""Numeric literals must obey i64 semantics independently of Python digit policy."""

import json
import subprocess
import sys
from pathlib import Path

import pytest

from yamaa.expressions import (
    MappingResolver,
    evaluate_expression,
    evaluate_numeric,
    parse_numeric,
)
from yamaa.models import INT64_MAX, INT64_MIN, MISSING, ConditionResult, ValueResult
from yamaa.verification.log import _canonical_json

# Explicit expectations; neither the parser nor evaluator generates this truth.
SUCCESS_CASES = [
    ("0" * 5000, 0),
    ("-" + "0" * 5000, 0),
    ("+" + "0" * 5000 + "42", 42),
    ("0" * 5000 + "9223372036854775807", INT64_MAX),
    ("-" + "0" * 5000 + "9223372036854775807", -INT64_MAX),
    ("-9223372036854775807 - 1", INT64_MIN),
    ("0" * 5000 + "1.0", 1.0),
    ("0" * 5000 + "1e2", 100.0),
]
OVERFLOW_CASES = [
    ("9223372036854775808", "9223372036854775808"),
    ("9999999999999999999", "9999999999999999999"),
    ("1" + "0" * 19, "1" + "0" * 19),
    ("9" * 5000, "9" * 5000),
    ("0" * 5000 + "9" * 5000, "9" * 5000),
    ("-9223372036854775808", "9223372036854775808"),
    ("-" + "0" * 5000 + "9" * 5000, "9" * 5000),
    ("+" + "0" * 5000 + "9223372036854775808", "9223372036854775808"),
]


@pytest.mark.parametrize("text, expected", SUCCESS_CASES, ids=range(len(SUCCESS_CASES)))
def test_long_literals_retain_type_and_exact_value(text, expected):
    """Strip zeros only for integer construction; decimal/exponent literals stay float."""
    result = evaluate_numeric(parse_numeric(text), text, MappingResolver({}))
    assert isinstance(result, ValueResult)
    assert type(result.value) is type(expected)
    assert result.value == expected


@pytest.mark.parametrize(
    "text, expected", OVERFLOW_CASES, ids=range(len(OVERFLOW_CASES))
)
def test_literal_overflow_retains_exact_serializable_condition(text, expected):
    """Return the existing REQ-0434 context through compute dispatch without handlers."""
    result = evaluate_expression({"compute": {"expr": text}}, MappingResolver({}))
    assert isinstance(result, ConditionResult)
    condition = result.condition
    assert condition.phase == "derivation"
    assert condition.condition == "integer_overflow"
    assert condition.requirement == "REQ-0434"
    assert condition.applicable_handler is None
    assert condition.context == {
        "expr": text,
        "value": expected,
        "minimum": INT64_MIN,
        "maximum": INT64_MAX,
    }
    assert (
        json.loads(result.model_dump_json())["condition"]["context"]["value"]
        == expected
    )
    assert json.loads(_canonical_json(condition.context))["value"] == expected


class RecordingResolver:
    """Observe resolution order while keeping unknown and missing distinct."""

    def __init__(self):
        """Use the reference resolver for all selection outcomes."""
        self.trace = []
        self.delegate = MappingResolver({"A": 1, "M": MISSING, "B": 2})

    def resolve(self, name):
        """Record exactly the occurrences evaluated before the first failure."""
        self.trace.append(name)
        return self.delegate.resolve(name)


@pytest.mark.parametrize(
    "pattern, condition, trace",
    [
        ("A + {literal} + B", "integer_overflow", ["A"]),
        ("M + {literal} + B", "integer_overflow", ["M"]),
        ("{literal} + A", "integer_overflow", []),
        ("UNKNOWN + {literal}", "unknown_field", ["UNKNOWN"]),
        ("1 / 0 + {literal}", "division_by_zero", []),
        ("COALESCE(A, {literal}, B)", "integer_overflow", ["A"]),
    ],
)
def test_literal_overflow_obeys_written_failure_and_resolution_order(
    pattern, condition, trace
):
    """Large literals fail only when visited, with no earlier or later resolver effects."""
    text = pattern.format(literal="9" * 5000)
    resolver = RecordingResolver()
    result = evaluate_numeric(parse_numeric(text), text, resolver)
    assert isinstance(result, ConditionResult)
    assert result.condition.condition == condition
    assert resolver.trace == trace


@pytest.mark.parametrize("limit", [640, 4300, 0])
def test_literal_results_do_not_depend_on_host_digit_policy(limit):
    """Run exact success/overflow expectations without mutating the parent interpreter."""
    script = """
import runpy
import sys
module = runpy.run_path(sys.argv[1])
before = sys.get_int_max_str_digits()
for text, expected in module['SUCCESS_CASES']:
    module['test_long_literals_retain_type_and_exact_value'](text, expected)
for text, expected in module['OVERFLOW_CASES']:
    module['test_literal_overflow_retains_exact_serializable_condition'](text, expected)
assert sys.get_int_max_str_digits() == before == int(sys.argv[2])
"""
    subprocess.run(
        [
            sys.executable,
            "-X",
            f"int_max_str_digits={limit}",
            "-c",
            script,
            str(Path(__file__).resolve()),
            str(limit),
        ],
        check=True,
        capture_output=True,
        text=True,
        timeout=60,
    )
