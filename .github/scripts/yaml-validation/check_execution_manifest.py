#!/usr/bin/env python3
"""Check the #101 execution manifest against yaml/examples directories.

Fails on missing, duplicate, or stale entries, unknown statuses, and
executable entries without a declared runtime. Run from the repository root:

    python3 .github/scripts/yaml-validation/check_execution_manifest.py
"""

import re
import sys
from pathlib import Path

import yaml

sys.path.insert(0, str(Path(__file__).resolve().parent))
from yamaa_validation.repository import UniqueKeyLoader


ROOT = Path(__file__).resolve().parents[3]
EXAMPLES = ROOT / "yaml" / "examples"
MANIFEST = EXAMPLES / "execution-manifest.yaml"
BLOCKED_BY_PATTERN = re.compile(r"#[1-9][0-9]*")


def load_manifest(path):
    try:
        with open(path, "r", encoding="utf-8") as handle:
            return yaml.load(handle, Loader=UniqueKeyLoader), None
    except (OSError, yaml.YAMLError) as exc:
        detail = getattr(exc, "problem", None) or str(exc)
        return None, f"execution-manifest.yaml: {detail}"


def check_manifest(manifest, actual_examples=None):
    if not isinstance(manifest, dict):
        return ["execution-manifest.yaml: root must be a mapping"]

    errors = []
    if manifest.get("version") != "1.0":
        errors.append('execution-manifest.yaml: \'version\' must be "1.0"')

    entries = manifest.get("examples")
    if not isinstance(entries, dict):
        errors.append("execution-manifest.yaml: 'examples' must be a mapping")
        return errors

    if actual_examples is not None:
        actual = set(actual_examples)
        for name in sorted(actual):
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
            blocker = entry.get("blocked_by")
            if not isinstance(blocker, str) or BLOCKED_BY_PATTERN.fullmatch(
                blocker
            ) is None:
                errors.append(
                    f"{name}: blocked entries must declare blocked_by as a "
                    "string matching #[1-9][0-9]*"
                )
        else:
            errors.append(f"{name}: unknown status {status!r}")
    return errors


def main() -> int:
    manifest, load_error = load_manifest(MANIFEST)
    if load_error:
        print(load_error)
        return 1

    actual = sorted(
        d.name for d in EXAMPLES.iterdir()
        if d.is_dir() and list(d.glob("spec*.yaml"))
    )
    errors = check_manifest(manifest, actual)

    if errors:
        print(f"{len(errors)} execution-manifest problem(s):")
        for error in errors:
            print(f"  - {error}")
        return 1
    entries = manifest["examples"]
    print(f"execution manifest ok: {len(entries)} examples, "
          f"{sum(1 for e in entries.values() if e.get('status') == 'executable')} executable")
    return 0


if __name__ == "__main__":
    sys.exit(main())
