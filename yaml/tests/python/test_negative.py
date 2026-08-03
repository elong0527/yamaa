"""Every negative fixture must be rejected, with the expected message.

Each directory under tests/negative/ holds a spec.yaml violating one rule and an
expect.txt naming a substring the error must contain. This is the corpus both
implementations are held to: run_negative.R checks R against the same files, so
the two validators are compared rather than assumed to agree.
"""
from __future__ import annotations

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))

from validate import validate_spec  # noqa: E402

NEG = Path(__file__).resolve().parents[1] / "negative"


def cases():
    return sorted(p for p in NEG.iterdir() if (p / "spec.yaml").exists())


def main():
    failures = 0
    for case in cases():
        want = (case / "expect.txt").read_text().strip()
        errs = validate_spec(case / "spec.yaml")
        if not errs:
            print(f"FAIL {case.name}: accepted, expected rejection")
            failures += 1
        elif not any(want in e for e in errs):
            print(f"FAIL {case.name}: rejected for the wrong reason")
            print(f"       wanted substring: {want!r}")
            for e in errs[:3]:
                print(f"       got: {e}")
            failures += 1
        else:
            print(f"ok   {case.name}")
    print(f"\n{'FAILED' if failures else 'PASSED'}: {failures} of {len(cases())} negative cases wrong")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
