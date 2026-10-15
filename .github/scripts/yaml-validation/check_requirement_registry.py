#!/usr/bin/env python3
"""Compatibility entry point for the merged requirement-registry check.

The duplicate/dangling-ID scan now lives in ``check_rule_rewrite.py`` as
``requirement_registry_errors`` (the legacy historical-ID scan was dropped).
This module remains only because ``.github/workflows/pr.yml`` invokes it by
path; it delegates to the merged implementation and adds no checks of its own.
"""

import argparse
import sys
from pathlib import Path

from check_rule_rewrite import requirement_registry_errors

check = requirement_registry_errors


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--root",
        type=Path,
        default=Path(__file__).resolve().parents[3],
        help="Repository root directory",
    )
    args = parser.parse_args()
    errors = check(args.root)
    for error in errors:
        print(error)
    if errors:
        print(f"FAIL: {len(errors)} requirement-registry violation(s).")
        return 1
    print(
        "PASS: current requirements resolve; historical IDs stay out of active sources."
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
