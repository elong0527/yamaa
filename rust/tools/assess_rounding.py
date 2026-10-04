"""Characterize REQ-0418 rounding before porting the reference implementation.

The oracle uses exact rational arithmetic on the promoted binary64 input and the
specified decimal quantum. It does not call Python round, the reference helper,
or a candidate Rust implementation. Agreement on this sample is not qualification.
"""

import argparse
import json
import math
import platform
import struct
import sys
from fractions import Fraction
from pathlib import Path

TIE_MARGIN = Fraction(1, 1 << 26)


def bits(value):
    """Preserve finite binary64 bits and normalize nonfinite results to missing."""
    return struct.pack(">d", value).hex() if math.isfinite(value) else "missing"


def rational_round(value, digits):
    """Apply the exact REQ-0418 interval, then convert the decimal result to binary64."""
    if not math.isfinite(value) or type(digits) is not int:
        raise ValueError("assessment requires finite floats and integer digits")
    # Every finite binary64 is unchanged beyond 340 decimal places. At -310
    # places even the largest finite value is below the first near-tie interval.
    # These bounds also avoid constructing powers for arbitrary i64 digits.
    if digits > 340:
        return bits(0.0 if value == 0 else value)
    if digits < -309:
        return bits(0.0)
    quantum = Fraction(10) ** -digits
    scaled = Fraction(abs(value)) / quantum
    lower, remainder = divmod(scaled.numerator, scaled.denominator)
    if Fraction(remainder, scaled.denominator) >= Fraction(1, 2) - TIE_MARGIN:
        lower += 1
    rounded = lower * quantum
    if value < 0:
        rounded = -rounded
    try:
        return bits(float(rounded))
    except OverflowError:
        return "missing"


def cases():
    """Specify boundary neighbors and deterministic finite bit patterns independently."""
    result = []
    for digits in (-308, -100, -2, -1, 0, 1, 2, 15, 16, 100, 308, 323):
        quantum = Fraction(10) ** -digits
        for lower in (0, 1, 2, 9, 99, 1000001):
            for label, fraction in (
                ("tie", Fraction(1, 2)),
                ("threshold", Fraction(1, 2) - TIE_MARGIN),
            ):
                try:
                    center = float((lower + fraction) * quantum)
                except OverflowError:
                    continue
                if not math.isfinite(center):
                    continue
                for direction, value in (
                    ("below", math.nextafter(center, -math.inf)),
                    ("at", center),
                    ("above", math.nextafter(center, math.inf)),
                ):
                    if not math.isfinite(value):
                        continue
                    for sign in (1, -1):
                        result.append(
                            (
                                f"boundary/{digits}/{lower}/{label}/{direction}/{sign}",
                                sign * value,
                                digits,
                            )
                        )
    for value in (
        0.0,
        -0.0,
        5e-324,
        -5e-324,
        sys.float_info.max,
        -sys.float_info.max,
        float((1 << 63) - 1),
        float(-(1 << 63)),
    ):
        for digits in (
            -9223372036854775808,
            -400,
            -310,
            -309,
            -308,
            -1,
            0,
            1,
            308,
            323,
            324,
            340,
            341,
            400,
            9223372036854775807,
        ):
            result.append((f"range/{bits(value)}/{digits}", value, digits))
    state = 1585
    mask = (1 << 64) - 1
    digit_choices = (-309, -308, -100, -2, -1, 0, 1, 2, 15, 16, 100, 308, 323)
    for index in range(2000):
        state ^= (state << 13) & mask
        state ^= state >> 7
        state ^= (state << 17) & mask
        value = struct.unpack(">d", state.to_bytes(8, "big"))[0]
        if math.isfinite(value):
            result.append(
                (f"sample/{index}", value, digit_choices[index % len(digit_choices)])
            )
    return result


def assess(samples, reference):
    """Retain exact discrepancies and escaped exceptions without blessing reference truth."""
    observations = []
    mismatches = []
    seen = set()
    for case, value, digits in samples:
        if case in seen:
            raise ValueError(f"duplicate rounding case {case}")
        seen.add(case)
        expected = rational_round(value, digits)
        try:
            actual = bits(reference(value, digits))
        except (OverflowError, ValueError) as error:
            actual = f"exception:{type(error).__name__}"
        row = {
            "case": case,
            "input": bits(value),
            "digits": str(digits),
            "rational": expected,
            "reference": actual,
        }
        observations.append(row)
        if actual != expected:
            mismatches.append(row)
    if not observations:
        raise ValueError("empty rounding assessment")
    return {
        "schema_version": 1,
        "corpus": "rounding-boundaries-v1",
        "oracle": "exact binary64 input, exact decimal quantum and 2^-26 near-tie interval",
        "host": {
            "system": platform.system(),
            "machine": platform.machine(),
            "python": platform.python_version(),
        },
        "qualification": "blocked-by-mismatches" if mismatches else "not-qualified",
        "rust_rounding_remains_unsupported": True,
        "counts": {
            "samples": len(observations),
            "exact": len(observations) - len(mismatches),
            "mismatches": len(mismatches),
        },
        "observations": observations,
        "mismatches": mismatches,
    }


def main():
    """Assess the existing scalar helper and save evidence separately from golden outputs."""
    from yamaa.expressions.numeric import _round_half_away_from_zero_scalar

    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    report = assess(cases(), _round_half_away_from_zero_scalar)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(f"Rounding: {report['counts']}; {report['qualification']}")
    print(
        "Rust rounding remains unsupported; this report is not approved migration truth"
    )


if __name__ == "__main__":
    main()
