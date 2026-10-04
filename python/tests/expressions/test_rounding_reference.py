"""Pin the intentional REQ-0418 correction independently of implementation output."""

import csv
import importlib.util
import struct
from pathlib import Path

import pytest

from yamaa.expressions import MappingResolver, evaluate_expression
from yamaa.expressions.numeric import _round_half_away_from_zero_scalar
from yamaa.models import MISSING, ValueResult

ROOT = Path(__file__).parents[3]
with (ROOT / "rust/crates/yamaa-core/tests/fixtures/numeric_rounding.tsv").open(
    encoding="utf-8", newline=""
) as stream:
    STANDALONE = [
        row
        for row in csv.DictReader(stream, delimiter="\t")
        if row["expression"] == "ROUND_HALF_AWAY_FROM_ZERO(A, D)"
        and ";D=int:" in row["bindings"]
        and not row["expected"].startswith("error:")
    ]


@pytest.mark.parametrize("vector", STANDALONE, ids=lambda row: row["id"])
def test_standalone_rounding_matches_independent_shared_values(vector):
    """Both expression forms must preserve the same numeric result and zero sign."""
    value, digits = vector["bindings"].split(";")
    token = value.removeprefix("A=")
    if token == "missing":
        value = MISSING
    else:
        kind, number = token.split(":", 1)
        value = int(number) if kind == "int" else float(number)
    result = evaluate_expression(
        {
            "round_half_away_from_zero": {
                "source": "A",
                "digits": int(digits.split(":")[1]),
            }
        },
        MappingResolver({"A": value}),
    )
    assert isinstance(result, ValueResult)
    if result.value is MISSING:
        actual = "missing"
    else:
        assert type(result.value) is float
        actual = "float:" + struct.pack(">d", result.value).hex()
    assert actual == vector["expected"]


def test_reference_matches_independent_rational_boundary_assessment():
    """Every specified boundary/range sample must satisfy the rational rule exactly."""
    spec = importlib.util.spec_from_file_location(
        "rounding_oracle", ROOT / "rust/tools/assess_rounding.py"
    )
    oracle = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(oracle)
    report = oracle.assess(oracle.cases(), _round_half_away_from_zero_scalar)
    assert report["counts"] == {"samples": 2936, "exact": 2936, "mismatches": 0}
    assert report["mismatches"] == []
    # Matching this sample alone must not claim engine-wide qualification.
    assert report["qualification"] == "not-qualified"


@pytest.mark.parametrize("digits", [341, 400, 10**6])
def test_large_digits_return_positive_zero_for_negative_zero(digits: int) -> None:
    # REQ-0418: a value that rounds to zero returns positive zero, never
    # negative zero - including past the 340-digit guard, which must not
    # hand the untouched -0.0 back.
    result = evaluate_expression(
        {"round_half_away_from_zero": {"source": "A", "digits": digits}},
        MappingResolver({"A": -0.0}),
    )

    assert isinstance(result, ValueResult)
    assert type(result.value) is float
    assert struct.pack(">d", result.value).hex() == "0000000000000000"
