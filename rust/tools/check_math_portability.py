"""Require exact portable-policy observations across the supported native CI matrix.

This compares implementations with each other, never generates expected truth,
and does not assert historical Python parity or complete mathematical accuracy.
"""

import argparse
import json
from pathlib import Path

from assess_math import FUNCTIONS, decode, normalized_bits

EXPECTED_HOSTS = {
    (system, machine, version)
    for system, machine in (
        ("Darwin", "arm64"),
        ("Linux", "x86_64"),
        ("Windows", "AMD64"),
    )
    for version in ("3.12", "3.14")
}


def observations(report):
    """Validate finite/missing observations and preserve every exact input/result bit."""
    if report["schema_version"] != 2 or report["policy"] != "PortableLibmV1":
        raise ValueError("wrong assessment schema or numerical policy")
    result = {}
    for row in report["candidate_results"]:
        key = row["case"], row["function"]
        if key in result or row["function"] not in FUNCTIONS:
            raise ValueError(f"duplicate or unknown sample {key}")
        inputs = row["inputs"]
        if len(inputs) != 2 or any(normalized_bits(decode(v)) != v for v in inputs):
            raise ValueError(f"noncanonical or nonfinite inputs for {key}")
        value = row["result"]
        if value != "missing" and normalized_bits(decode(value)) != value:
            raise ValueError(f"noncanonical or nonfinite result for {key}")
        result[key] = tuple(inputs), value
    return result


def compare(reports):
    """Reject any sample-set, input, value, missingness or zero-sign difference."""
    if len(reports) < 2:
        raise ValueError("at least two reports are required")
    expected = observations(reports[0])
    if not expected:
        raise ValueError("empty assessment")
    for report in reports[1:]:
        actual = observations(report)
        if actual.keys() != expected.keys():
            raise AssertionError("portable sample sets differ")
        for key, value in expected.items():
            if actual[key] != value:
                raise AssertionError(
                    f"portable math differs at {key}: {value} != {actual[key]}"
                )
    return len(expected)


def main():
    """Require every OS/Python report and complete per-function samples before comparison."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("directory", type=Path)
    args = parser.parse_args()
    reports = [
        json.loads(p.read_text(encoding="utf-8"))
        for p in sorted(args.directory.glob("*/math-assessment.json"))
    ]
    hosts = []
    for report in reports:
        host = report["host"]
        hosts.append(
            (host["system"], host["machine"], ".".join(host["python"].split(".")[:2]))
        )
        rows = observations(report)
        for name in FUNCTIONS:
            if sum(key[1] == name for key in rows) != 10011:
                raise AssertionError(f"incomplete {name} sample")
    if len(hosts) != len(EXPECTED_HOSTS) or set(hosts) != EXPECTED_HOSTS:
        raise AssertionError(f"incomplete or duplicate native matrix: {hosts}")
    count = compare(reports)
    print(
        f"PortableLibmV1: {count} exact observations match across {len(reports)} hosts/versions"
    )
    print(
        "This is sample portability, not historical Python parity or complete accuracy qualification"
    )


if __name__ == "__main__":
    main()
