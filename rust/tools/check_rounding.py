"""Require compiled Rust rounding to match the independent exact REQ-0418 oracle."""

import subprocess
from pathlib import Path

from assess_rounding import bits, cases, rational_round

POLICIES = ("ReferenceSubset", "PortableLibmV1")


def verify(samples, lines):
    """Bind every output to its intended case, policy and inputs before comparing bits."""
    if len(lines) != len(samples) * len(POLICIES) or not samples:
        raise AssertionError("incomplete rounding observations")
    index = 0
    for case, value, digits in samples:
        for policy in POLICIES:
            fields = lines[index].split("\t")
            expected = [
                case,
                policy,
                bits(value),
                str(digits),
                rational_round(value, digits),
            ]
            if fields != expected:
                raise AssertionError(f"rounding mismatch: {fields} != {expected}")
            index += 1
    return index


def observe(samples):
    """Replay both compiled policies with exact input identities."""
    result = subprocess.run(
        [
            "cargo",
            "run",
            "--offline",
            "--locked",
            "--quiet",
            "--release",
            "-p",
            "yamaa-core",
            "--example",
            "rounding_probe",
        ],
        cwd=Path(__file__).resolve().parents[1],
        input="".join(
            f"{case}\t{bits(value)}\t{digits}\n" for case, value, digits in samples
        ),
        text=True,
        capture_output=True,
        check=True,
    )
    return result.stdout.splitlines()


def main():
    """Run actual compiled release evaluation under both policies, without tolerance."""
    samples = cases()
    count = verify(samples, observe(samples))
    print(
        f"Exact compiled rounding: {count} results match the rational oracle across both policies"
    )


if __name__ == "__main__":
    main()
