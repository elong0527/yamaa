#!/usr/bin/env python3
"""Check the #101 execution manifest against yaml/examples directories.

Fails on missing, duplicate, or stale entries, unknown statuses, and
executable entries without a declared runtime. Run from the repository root:

    python3 .github/scripts/yaml-validation/check_execution_manifest.py
"""

import sys
from pathlib import Path

import yaml

ROOT = Path(__file__).resolve().parents[3]
EXAMPLES = ROOT / "yaml" / "examples"
MANIFEST = EXAMPLES / "execution-manifest.yaml"


def main() -> int:
    manifest = yaml.safe_load(MANIFEST.read_text(encoding="utf-8"))
    entries = manifest.get("examples", {})
    if not isinstance(entries, dict):
        print("execution-manifest.yaml: 'examples' must be a mapping")
        return 1

    actual = sorted(
        d.name for d in EXAMPLES.iterdir()
        if d.is_dir() and list(d.glob("spec*.yaml"))
    )
    errors = []

    seen: set[str] = set()
    for name in entries:
        if name in seen:
            errors.append(f"duplicate entry: {name}")
        seen.add(name)

    for name in actual:
        if name not in entries:
            errors.append(f"missing entry: {name}")
    for name in entries:
        if name not in actual:
            errors.append(f"stale entry (no such example): {name}")

    for name, entry in entries.items():
        if not isinstance(entry, dict):
            errors.append(f"{name}: entry must be a mapping")
            continue
        status = entry.get("status")
        if status == "executable":
            if not entry.get("runtimes"):
                errors.append(f"{name}: executable entries must declare runtimes")
        elif status == "blocked":
            if not entry.get("blocked_by"):
                errors.append(f"{name}: blocked entries must declare blocked_by")
        else:
            errors.append(f"{name}: unknown status {status!r}")

    if errors:
        print(f"{len(errors)} execution-manifest problem(s):")
        for error in errors:
            print(f"  - {error}")
        return 1
    print(f"execution manifest ok: {len(entries)} examples, "
          f"{sum(1 for e in entries.values() if e.get('status') == 'executable')} executable")
    return 0


if __name__ == "__main__":
    sys.exit(main())
