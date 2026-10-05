"""Report reference behavior against independent regex truth without rewriting it.

This is a migration assessment, not an engine qualification or approval gate.
Known discrepancies remain visible; no fixture is generated from observations.
"""

import csv
import json
from pathlib import Path

from yamaa.regex import REGEX_CONTRACT_VERSION, RegexError, compile_pattern, full_match

ROOT = Path(__file__).resolve().parents[1]
TRUTH = ROOT / "crates/yamaa-adapters/tests/fixtures/regex_contract.tsv"


def observe(pattern, subject):
    """Capture public reference acceptance, captures and full-match behavior."""
    try:
        compiled = compile_pattern(pattern)
        match = compiled.search(subject)
        return {
            "accepted": True,
            "group_count": compiled.groups,
            "full_match": full_match(pattern, subject),
            "match": None if match is None else [match.group(0), *match.groups()],
        }
    except RegexError:
        return {"accepted": False}


def assess():
    """Compare authored expectations and return every observation, including mismatches."""
    cases = []
    with TRUTH.open(encoding="ascii", newline="") as stream:
        for row in csv.DictReader(stream, delimiter="\t", quoting=csv.QUOTE_NONE):
            pattern = json.loads(row["pattern"])
            subject = json.loads(row["subject"])
            expected = json.loads(row["expected"])
            actual = observe(pattern, subject)
            cases.append(
                {
                    "id": row["id"],
                    "pattern": pattern,
                    "subject": subject,
                    "expected": expected,
                    "reference": actual,
                    "equal": actual == expected,
                }
            )
    return {
        "contract_version": REGEX_CONTRACT_VERSION,
        "qualification": "not-qualified",
        "cases": cases,
        "mismatches": sum(not case["equal"] for case in cases),
    }


if __name__ == "__main__":
    print(json.dumps(assess(), ensure_ascii=True, indent=2))
