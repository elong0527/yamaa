"""Independent input specification for numeric-math-v1; never expected math outputs.

The Rust probe and this Python specification implement the documented corpus
separately. CI binds case identities and input bits to this specification before
comparing observed outputs. See MATH_POLICY.md for the versioned algorithm.
"""

from assess_math import decode, normalized_bits

CORPUS = "numeric-math-v1"


def expected_inputs():
    """Construct exact named inputs using fixed boundaries and the specified u64 sequence."""
    boundaries = [
        ("8000000000000000", "3ff0000000000000"),
        ("0000000000000000", "4000000000000000"),
        ("3ff0000000000000", "3fe0000000000000"),
        ("bff0000000000000", "0000000000000001"),
        (normalized_bits(709.0), "0010000000000000"),
        (normalized_bits(710.0), "7fefffffffffffff"),
        (normalized_bits(-744.0), "3fefffffffffffff"),
        (normalized_bits(-746.0), "3ff0000000000001"),
        ("7fefffffffffffff", "3ff0000000000000"),
        ("ffefffffffffffff", "3ff0000000000000"),
        ("c05098b14e6ba5d0", "4047354b5f3e6095"),
    ]
    pairs = [
        (f"boundary-{i}", decode(x), decode(y)) for i, (x, y) in enumerate(boundaries)
    ]
    state = 1585
    mask = (1 << 64) - 1
    for index in range(10000):
        state ^= (state << 13) & mask
        state ^= state >> 7
        state ^= (state << 17) & mask
        unit = float(state >> 11) / float(1 << 53)
        pairs.append((f"sample-{index}", unit * 1400.0 - 700.0, unit * 200.0 + 0.001))
    result = {}
    for case, x, y in pairs:
        for name, left, right in (
            ("EXP", x, 0.0),
            ("LN", y, 0.0),
            ("POWER", y, x / 100.0),
        ):
            result[case, name] = normalized_bits(left), normalized_bits(right)
    return result


def validate_corpus(rows):
    """Reject omitted/replaced cases or common input drift even if all reports agree."""
    expected = expected_inputs()
    if rows.keys() != expected.keys():
        missing = sorted(expected.keys() - rows.keys())[:3]
        extra = sorted(rows.keys() - expected.keys())[:3]
        raise AssertionError(
            f"{CORPUS} case identities differ: missing={missing}, extra={extra}"
        )
    for key, inputs in expected.items():
        if rows[key][0] != inputs:
            raise AssertionError(
                f"{CORPUS} inputs differ at {key}: {rows[key][0]} != {inputs}"
            )
