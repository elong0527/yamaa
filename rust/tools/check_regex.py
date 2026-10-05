"""Replay independent R022 expectations through the Rust core observation probe.

Expected outcomes come from committed contracts and scalar/count arithmetic,
never from a generated native result or a reference implementation's answers.
"""

import csv
import json
import subprocess
from pathlib import Path

import yaml

WORKSPACE = Path(__file__).resolve().parents[1]


def exact(accepted=True, *, groups=0, full=False, match=None):
    """Spell one complete independently expected observation."""
    if not accepted:
        return {"accepted": False}
    return {"accepted": True, "group_count": groups, "full_match": full, "match": match}


def cases():
    """Combine committed YAML truth, authored edge cases and independent set/count cases."""
    observations = []
    contract = yaml.safe_load(
        (WORKSPACE.parent / "yaml/conformance/regex.yaml").read_text(encoding="ascii")
    )
    assert contract["contract_version"] == "2.0.0"
    for case in contract["cases"]:
        observations.append(
            (case["id"], case["pattern"], case.get("subject", ""), case, "yaml")
        )
    path = WORKSPACE / "crates/yamaa-adapters/tests/fixtures/regex_contract.tsv"
    with path.open(encoding="ascii", newline="") as stream:
        for row in csv.DictReader(stream, delimiter="\t", quoting=csv.QUOTE_NONE):
            observations.append(
                (
                    row["id"],
                    json.loads(row["pattern"]),
                    json.loads(row["subject"]),
                    json.loads(row["expected"]),
                    "exact",
                )
            )
    # REQ-0822..24 explicitly enumerate these sets, independent of any regex host.
    whitespace = {
        0x9,
        0xA,
        0xB,
        0xC,
        0xD,
        0x20,
        0xA0,
        0x1680,
        *range(0x2000, 0x200B),
        0x2028,
        0x2029,
        0x202F,
        0x205F,
        0x3000,
        0xFEFF,
    }
    word = {*range(48, 58), *range(65, 91), *range(97, 123), 95}
    digits = set(range(48, 58))
    points = sorted(
        {*range(128), *whitespace, 0x85, 0xE9, 0x301, 0x665, 0x1D400, 0x10FFFF}
    )
    for spelling, members, negative in (
        (r"\s", whitespace, False),
        (r"[\s]", whitespace, False),
        (r"\S", whitespace, True),
        (r"[\S]", whitespace, True),
        (r"[^\S]", whitespace, False),
        (r"\w", word, False),
        (r"\W", word, True),
        (r"\d", digits, False),
        (r"\D", digits, True),
        (".", {10, 13, 0x2028, 0x2029}, True),
        ("[]", set(), False),
        ("[^]", set(), True),
    ):
        for point in points:
            subject = chr(point)
            matches = (point in members) != negative
            observations.append(
                (
                    f"scalar-{spelling}-{point}",
                    f"^{spelling}$",
                    subject,
                    exact(full=matches, match=[subject] if matches else None),
                    "exact",
                )
            )
    for minimum in range(5):
        for maximum in range(minimum, 5):
            for length in range(7):
                for lazy in (False, True):
                    subject = "a" * length
                    suffix = "?" if lazy else ""
                    pattern = f"(a{{{minimum},{maximum}}}{suffix})"
                    if length < minimum:
                        found = None
                    else:
                        consumed = minimum if lazy else min(length, maximum)
                        found = ["a" * consumed] * 2
                    observations.append(
                        (
                            f"count-{minimum}-{maximum}-{length}-{lazy}",
                            pattern,
                            subject,
                            exact(
                                groups=1, full=minimum <= length <= maximum, match=found
                            ),
                            "exact",
                        )
                    )
    return observations


def check(expected, actual, kind):
    """Reject policy outcomes and compare exactly the observations each contract records."""
    if kind == "exact":
        assert actual == expected, (expected, actual)
        return
    if expected.get("invalid"):
        assert actual == {"accepted": False}, (expected, actual)
        return
    assert actual["accepted"] is True
    assert actual["full_match"] == expected["schema_pattern"]
    assert (actual["match"] is not None) == expected["matches"]
    extraction = expected["str_extract"]
    if extraction is None:
        assert actual["match"] is None
    else:
        assert actual["match"] is not None
        assert actual["match"][extraction["group"]] == extraction["value"]


def main():
    """Probe the actual compiled core; do not write or repair expected truth."""
    observations = cases()
    run = subprocess.run(
        [
            "cargo",
            "run",
            "--offline",
            "--quiet",
            "-p",
            "yamaa-adapters",
            "--example",
            "regex_probe",
        ],
        cwd=WORKSPACE,
        input="".join(
            json.dumps({"pattern": p, "subject": s}) + "\n"
            for _, p, s, _, _ in observations
        ),
        text=True,
        capture_output=True,
        check=True,
    )
    # JSON permits U+0085/U+2028 inside strings; only LF delimits probe records.
    responses = [json.loads(line) for line in run.stdout.split("\n") if line]
    assert len(responses) == len(observations)
    for (identifier, _, _, expected, kind), actual in zip(
        observations, responses, strict=True
    ):
        try:
            check(expected, actual, kind)
        except (AssertionError, KeyError) as error:
            raise AssertionError(
                f"{identifier}: expected {expected!r}; actual {actual!r}"
            ) from error
    print(
        f"Regex core: {len(observations)} independent observations passed (43 committed cases and 12 authored edges included)"
    )


if __name__ == "__main__":
    main()
