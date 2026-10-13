"""`python -m yamaa.style`: check or fix specification style from a shell.

Exit status 0 means no finding remains, 1 means at least one does, and 2
means a file could not be read or parsed, or the arguments were invalid.
"""

from __future__ import annotations

import argparse
import sys
from collections.abc import Sequence
from pathlib import Path

import yaml

from yamaa.style import (
    _FINDINGS as FINDINGS,
)
from yamaa.style import (
    _check_file,
    _field_orders,
    _fix_file,
    _specification_files,
)


def _names(text: str | None) -> set[str] | None:
    if text is None:
        return None
    return {name.strip() for name in text.split(",") if name.strip()}


def main(argv: Sequence[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        prog="python -m yamaa.style",
        description="Check yamaa specifications against the style contract.",
    )
    parser.add_argument("paths", nargs="+", type=Path, metavar="PATH")
    parser.add_argument(
        "--fix", action="store_true", help="apply the proved layout fixes in place"
    )
    parser.add_argument("--select", help="comma-separated findings to report")
    parser.add_argument("--ignore", help="comma-separated findings to skip")
    arguments = parser.parse_args(argv)

    select = _names(arguments.select)
    ignore = _names(arguments.ignore) or set()
    unknown = sorted(((select or set()) | ignore) - FINDINGS.keys())
    if unknown:
        parser.error(f"unknown style finding: {', '.join(unknown)}")
    chosen = (select if select is not None else set(FINDINGS)) - ignore

    orders = _field_orders()
    files = _specification_files(arguments.paths, orders["root_class"])
    failed = False
    remaining = 0
    affected = 0
    fixed = 0
    for path in files:
        try:
            if arguments.fix:
                changed, findings = _fix_file(path, select=chosen, orders=orders)
                fixed += changed
            else:
                findings = _check_file(path, select=chosen, orders=orders)
        except (OSError, UnicodeDecodeError, yaml.YAMLError) as error:
            reason = str(error).splitlines()[0] if str(error) else type(error).__name__
            print(f"{path}: cannot read specification: {reason}", file=sys.stderr)
            failed = True
            continue
        for finding in findings:
            print(finding.render())
        remaining += len(findings)
        affected += bool(findings)

    summary = f"{remaining} style finding(s) in {affected} of {len(files)} file(s)"
    if arguments.fix:
        summary += f"; fixed {fixed} file(s)"
    print(summary, file=sys.stderr)
    if failed:
        return 2
    return 1 if remaining else 0


if __name__ == "__main__":
    sys.exit(main())
