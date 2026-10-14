#!/usr/bin/env python3
"""Check directory-derived benchmark inventory and observed conformance coverage."""

import argparse
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT / "rust/tools"))
from public_conformance import (
    EXCLUDED,
    PROTOCOL,
    benchmark_directories,
    plan,
    read_json,
)


def check_inventory(payload, actual_examples, observations=None):
    errors = []
    if not isinstance(payload, dict) or payload.get("protocol") != PROTOCOL:
        return ["unknown benchmark inventory protocol"]
    entries = payload.get("examples")
    if not isinstance(entries, list):
        return ["benchmark inventory examples must be a list"]
    names = [entry["name"] for entry in entries]
    actual = set(actual_examples)
    for name in sorted(set(names)):
        if names.count(name) > 1:
            errors.append(f"duplicate case: {name}")
    for name in sorted(actual - set(names)):
        errors.append(f"missing case: {name}")
    for name in sorted(set(names) - actual):
        errors.append(f"unexpected case: {name}")
    for name in sorted(set(EXCLUDED) - actual):
        errors.append(f"exclusion has no authored case: {name}")
    active = set()
    for entry in entries:
        name = entry["name"]
        if entry.get("excluded") != EXCLUDED.get(name):
            errors.append(f"exclusion reason differs: {name}")
        if not entry.get("excluded"):
            active.add(name)
        hosts = entry.get("entries", {})
        if set(hosts) != {"python", "r"} or not all(hosts.values()):
            errors.append(f"missing public host entry: {name}")
    if observations is not None:
        if observations.get("protocol") != "public-conformance-ratchet/1":
            errors.append("unknown observed conformance protocol")
        if observations.get("benchmarks") != len(actual):
            errors.append("observed authored denominator differs")
        excluded = observations.get("exclusions", [])
        observed_exclusions = {row["example"]: row["reason"] for row in excluded}
        if len(excluded) != len(observed_exclusions) or observed_exclusions != EXCLUDED:
            errors.append("observed exclusions differ")
        for host in ("python", "r"):
            observed = observations.get(host, [])
            if len(observed) != len(set(observed)):
                errors.append(f"duplicate observed {host} case")
            for name in sorted(active - set(observed)):
                errors.append(f"missing observed {host} case: {name}")
            for name in sorted(set(observed) - active):
                errors.append(f"unexpected observed {host} case: {name}")
    return errors


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=ROOT)
    args = parser.parse_args()
    fixtures = args.root / "benchmarks"
    try:
        payload = plan(fixtures)
        errors = check_inventory(
            payload,
            [p.name for p in benchmark_directories(fixtures)],
            read_json(args.root / "rust/qualification/public-conformance.json"),
        )
        sys.path.insert(0, str(ROOT / ".github/scripts/benchmark-docs"))
        from build_index import render_index

        index = fixtures / "README.md"
        if index.read_text(encoding="utf-8") != render_index(
            fixtures, index.read_text(encoding="utf-8")
        ):
            errors.append(
                "stale benchmark index; run .github/scripts/benchmark-docs/build_index.py"
            )
    except (OSError, ValueError, KeyError, TypeError) as exc:
        errors = [str(exc)]
    if errors:
        for error in errors:
            print(f"ERROR: {error}")
        return 1
    active = sum(not entry["excluded"] for entry in payload["examples"])
    print(
        f"benchmark inventory ok: {len(payload['examples'])} authored, {active} active on Python and R, {len(EXCLUDED)} explicitly excluded"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
