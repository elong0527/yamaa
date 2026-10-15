"""Check the directory-derived benchmark inventory.

Case membership, both host entries and explicit exclusions derive from the
benchmark directories themselves. Whether active cases pass is gated by the
native run's conformance comparison, not by recorded observations here.
"""

import argparse
import sys
from pathlib import Path

from tools.benchmark import load_metadata
from tools.conformance.public_conformance import (
    PROTOCOL,
    benchmark_directories,
    plan,
)
from tools.paths import ROOT


def check_inventory(payload, actual_examples):
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
    for entry in entries:
        name = entry["name"]
        if entry.get("excluded") != actual_examples.get(name):
            errors.append(f"exclusion reason differs: {name}")
        hosts = entry.get("entries", {})
        if set(hosts) != {"python", "r"} or not all(hosts.values()):
            errors.append(f"missing public host entry: {name}")
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
            {
                p.name: load_metadata(p).get("conformance", {}).get("excluded")
                for p in benchmark_directories(fixtures)
            },
        )
    except (OSError, ValueError, KeyError, TypeError) as exc:
        errors = [str(exc)]
    if errors:
        for error in errors:
            print(f"ERROR: {error}")
        return 1
    active = sum(not entry["excluded"] for entry in payload["examples"])
    print(
        f"benchmark inventory ok: {len(payload['examples'])} authored, {active} active on Python and R, {len(payload['examples']) - active} explicitly excluded"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
