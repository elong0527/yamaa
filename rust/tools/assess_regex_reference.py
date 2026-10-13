"""Observe compiled regex against unchanged independent truth.

The filename is retained for callers. This component assessment does not qualify
installed hosts; historical reference comparisons are not rerun.
"""

import json

from check_regex import cases, check, replay


def assess():
    """Retain all 43 shared cases and 12 exact authored edge observations."""
    observations = [
        row
        for row in cases()
        if row[4] == "yaml" or not row[0].startswith(("scalar-", "count-"))
    ]
    evidence = []
    for (identity, pattern, subject, expected, kind), actual in zip(
        observations, replay(observations), strict=True
    ):
        try:
            check(expected, actual, kind)
        except (AssertionError, KeyError):
            equal = False
        else:
            equal = True
        evidence.append(
            {
                "id": identity,
                "pattern": pattern,
                "subject": subject,
                "expected": expected,
                "expectation_scope": "complete observation"
                if kind == "exact"
                else "shared regex contract fields",
                "candidate": actual,
                "equal": equal,
            }
        )
    return {
        "contract_version": "2.0.0",
        "consumer": "retained typed native regex fixture",
        "historical_reference": "not rerun; archived comparisons remain historical",
        "qualification": "not-qualified",
        "cases": evidence,
        "mismatches": sum(not row["equal"] for row in evidence),
    }


if __name__ == "__main__":
    print(json.dumps(assess(), ensure_ascii=True, indent=2))
