"""Characterize candidate libm functions; a successful run never means parity.

Exact mismatches are reported, not tolerated or promoted into expected fixtures.
The default compiler keeps EXP/LN/POWER unsupported; opt-in uses PortableLibmV1.
Only finite EXP inputs, positive LN inputs and positive POWER bases are sampled;
domain diagnostics, decimal rounding and completed-result formatting need separate
qualification. Python math calls mirror the numeric reference's promoted inputs.
"""

import argparse
import json
import math
import platform
import struct
import subprocess
from pathlib import Path

WORKSPACE = Path(__file__).resolve().parents[1]
FUNCTIONS = ("EXP", "LN", "POWER")


def decode(bits):
    """Decode an exact binary64 spelling without decimal text conversion."""
    return struct.unpack(">d", bytes.fromhex(bits))[0]


def normalized_bits(value):
    """Normalize nonfinite operation results to missing, preserving finite zero signs."""
    return struct.pack(">d", value).hex() if math.isfinite(value) else "missing"


def reference(name, left, right):
    """Apply Python's math primitive on the probe's restricted valid input domain."""
    if not math.isfinite(left) or not math.isfinite(right):
        raise ValueError("probe inputs must be finite")
    if name in ("LN", "POWER") and left <= 0:
        raise ValueError("LN and POWER assessment inputs must be positive")
    try:
        if name == "EXP":
            return normalized_bits(math.exp(left))
        if name == "LN":
            return normalized_bits(math.log(left))
        if name == "POWER":
            return normalized_bits(math.pow(left, right))
    except OverflowError:
        return "missing"
    raise ValueError(f"unknown operation {name!r}")


def difference(actual, expected):
    """Classify mismatches without treating ULP distance as an acceptance tolerance."""
    if actual == expected:
        return None, None
    if "missing" in (actual, expected):
        return "missingness", None
    left, right = decode(actual), decode(expected)
    if left == right == 0.0:
        return "zero_sign", None
    if math.copysign(1, left) != math.copysign(1, right):
        return "sign", None
    return "finite_bits", abs(int(actual, 16) - int(expected, 16))


def assess(lines):
    """Retain every mismatch and explicit counts so CI completion cannot imply parity."""
    counts = {
        name: {"samples": 0, "exact": 0, "mismatches": 0, "max_ulp_distance": 0}
        for name in FUNCTIONS
    }
    mismatches = []
    candidate_results = []
    seen = set()
    for line in lines:
        case, name, left_bits, right_bits, result_bits = line.strip().split("\t")
        if name not in counts or (case, name) in seen:
            raise ValueError(f"unknown or duplicate case: {case} {name}")
        seen.add((case, name))
        expected = reference(name, decode(left_bits), decode(right_bits))
        actual = normalized_bits(decode(result_bits))
        candidate_results.append(
            {
                "case": case,
                "function": name,
                "inputs": [left_bits, right_bits],
                "result": actual,
            }
        )
        category, ulps = difference(actual, expected)
        counts[name]["samples"] += 1
        if category is None:
            counts[name]["exact"] += 1
            continue
        counts[name]["mismatches"] += 1
        if ulps is not None:
            counts[name]["max_ulp_distance"] = max(
                counts[name]["max_ulp_distance"], ulps
            )
        mismatches.append(
            {
                "case": case,
                "function": name,
                "inputs": [left_bits, right_bits],
                "candidate": actual,
                "reference": expected,
                "category": category,
                "ulp_distance": ulps,
            }
        )
    return {
        "schema_version": 2,
        "policy": "PortableLibmV1",
        "candidate_results": candidate_results,
        "candidate": "libm 0.2.16, default features disabled",
        "reference": "Python platform math, nonfinite results normalized to missing",
        "host": {
            "system": platform.system(),
            "machine": platform.machine(),
            "python": platform.python_version(),
        },
        "qualification": "blocked-by-mismatches" if mismatches else "not-qualified",
        "default_functions_remain_unsupported": list(FUNCTIONS),
        "counts": counts,
        "mismatches": mismatches,
    }


def main():
    """Write observations separately from expected truth and print the capability gate."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    output = subprocess.check_output(
        [
            "cargo",
            "run",
            "--offline",
            "--quiet",
            "-p",
            "yamaa-core",
            "--example",
            "math_probe",
        ],
        cwd=WORKSPACE,
        text=True,
    )
    report = assess(output.splitlines())
    for name, count in report["counts"].items():
        if count["samples"] != 10011:
            raise AssertionError(f"incomplete {name} sample: {count}")
        print(f"{name}: {count}")
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(
        f"Qualification: {report['qualification']}; default EXP/LN/POWER remain unsupported"
    )
    print(f"Assessment report: {args.output}")


if __name__ == "__main__":
    main()
