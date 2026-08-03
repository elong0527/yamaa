"""Run every example fixture and compare output to its expected CSV."""
from __future__ import annotations

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))

from runner import Spec  # noqa: E402
from validate import check_registries, example_specs, validate_spec  # noqa: E402


def diff_rows(got, want):
    """Return a list of human-readable differences between two row lists."""
    out = []
    if len(got) != len(want):
        out.append(f"row count: got {len(got)}, expected {len(want)}")
    for i, (g, w) in enumerate(zip(got, want)):
        for col in w:
            if g.get(col, "") != w[col]:
                out.append(f"row {i + 1} {col}: got {g.get(col, '')!r}, expected {w[col]!r}")
    return out


def main():
    failures = 0

    reg = check_registries()
    print(f"{'FAIL' if reg else 'ok  '} registries")
    for err in reg:
        print(f"       {err}")
    failures += len(reg)

    for spec_path in example_specs():
        name = spec_path.parent.name
        errs = validate_spec(spec_path)
        if errs:
            print(f"FAIL {name} (validation)")
            for err in errs:
                print(f"       {err}")
            failures += len(errs)
            continue
        try:
            spec = Spec(spec_path)
            got = spec.run()
            want = spec.expected()
        except Exception as exc:  # noqa: BLE001 - report, don't mask
            print(f"FAIL {name} (execution) {type(exc).__name__}: {exc}")
            failures += 1
            continue
        diffs = diff_rows(got, want)
        print(f"{'FAIL' if diffs else 'ok  '} {name} ({len(got)} rows)")
        for d in diffs[:12]:
            print(f"       {d}")
        failures += len(diffs)

    print(f"\n{'FAILED' if failures else 'PASSED'}: {failures} problem(s)")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
