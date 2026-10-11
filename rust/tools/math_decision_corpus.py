"""Independent, bounded decision inputs; observed outputs never become truth.

These exact input bits specify math-decision-v1. The corpus deliberately covers
range transitions, zero signs, promotion, missingness and two opposing integer
admission witnesses. It does not reconstruct the unavailable 334-case experiment.
"""

import struct

CORPUS = "math-decision-v1"


def bits(value):
    """Specify ordinary exactly representable constants as binary64 operands."""
    return struct.pack(">d", float(value)).hex()


def cases():
    """Return ordered input declarations, independent of any math implementation."""
    result = []

    def add(identity, function, left, right=None):
        operands = [left] if right is None else [left, right]
        result.append({"id": identity, "function": function, "inputs": operands})

    for identity, value in (
        ("positive-zero", "0000000000000000"),
        ("negative-zero", "8000000000000000"),
        ("one", bits(1)),
        ("negative-one", bits(-1)),
        ("large-finite", bits(709)),
        ("overflow", bits(710)),
        ("subnormal", bits(-744)),
        ("underflow", bits(-746)),
        ("maximum-input", "7fefffffffffffff"),
        ("minimum-input", "ffefffffffffffff"),
        ("missing", "missing"),
    ):
        add(f"exp-{identity}", "EXP", value)
    # Fixed adjacent input triples around finite/overflow, normal/subnormal and
    # half-minimum-subnormal transitions. Their outputs require certificates.
    for identity, values in (
        ("overflow-edge", ("40862e42fefa39ee", "40862e42fefa39ef", "40862e42fefa39f0")),
        ("normal-edge", ("c086232bdd7abcd3", "c086232bdd7abcd2", "c086232bdd7abcd1")),
        ("zero-edge", ("c0874910d52d3052", "c0874910d52d3051", "c0874910d52d3050")),
    ):
        for index, value in enumerate(values):
            add(f"exp-{identity}-{index}", "EXP", value)
    for identity, value in (
        ("one", bits(1)),
        ("below-one", "3fefffffffffffff"),
        ("above-one", "3ff0000000000001"),
        ("minimum-subnormal", "0000000000000001"),
        ("maximum-subnormal", "000fffffffffffff"),
        ("minimum-normal", "0010000000000000"),
        ("maximum", "7fefffffffffffff"),
        ("positive-zero", "0000000000000000"),
        ("negative-zero", "8000000000000000"),
        ("negative", bits(-1)),
        ("missing", "missing"),
    ):
        add(f"ln-{identity}", "LN", value)
    for identity, left, right in (
        ("positive-zero-zero", bits(0), bits(0)),
        ("negative-zero-zero", "8000000000000000", bits(0)),
        ("positive-zero-odd", bits(0), bits(3)),
        ("negative-zero-odd", "8000000000000000", bits(3)),
        ("negative-zero-even", "8000000000000000", bits(2)),
        ("negative-zero-fraction", "8000000000000000", bits(0.5)),
        ("positive-zero-negative", bits(0), bits(-1)),
        ("negative-zero-negative", "8000000000000000", bits(-1)),
        ("negative-odd", bits(-2), bits(3)),
        ("negative-even", bits(-2), bits(2)),
        ("negative-fraction", bits(-2), bits(0.5)),
        ("fraction", bits(4), bits(0.5)),
        ("reciprocal", bits(2), bits(-3)),
        ("overflow", bits(2), bits(1024)),
        ("large-finite", bits(2), bits(1023)),
        ("minimum-subnormal", bits(2), bits(-1074)),
        ("positive-underflow-tie", bits(2), bits(-1075)),
        ("negative-underflow-tie", bits(-2), bits(-1075)),
        ("underflow", bits(2), bits(-1076)),
        ("below-one-large-exponent", "3fefffffffffffff", bits(2**52)),
        ("above-one-large-exponent", "3ff0000000000001", bits(2**52)),
        ("missing-base", "missing", bits(-1)),
        ("missing-exponent", bits(0), "missing"),
        # numeric-math-v1 sample-122: the named historical witness.
        ("integer-admitted", "40689ed31ed9c3bf", "401b2641709adeb1"),
        # Independent reverse witness: reciprocal of the float below one quarter.
        ("integer-refused", "3fcfffffffffffff", bits(-1)),
    ):
        add(f"power-{identity}", "POWER", left, right)
    for identity, written in (
        ("promoted-odd", "9007199254740993"),
        ("promoted-maximum", "9223372036854775807"),
    ):
        result.append(
            {
                "id": f"power-{identity}",
                "function": "POWER",
                "inputs": [bits(-1), bits(int(written))],
                "original_integer_exponent": written,
            }
        )
    return result
