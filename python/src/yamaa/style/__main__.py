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
    FINDINGS,
    check_file,
    discover_schema_root,
    field_orders,
    fix_file,
    specification_files,
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
    parser.add_argument(
        "--schema-root",
        type=Path,
        help="schema bundle directory (default: the one the engine finds)",
    )
    arguments = parser.parse_args(argv)

    select = _names(arguments.select)
    ignore = _names(arguments.ignore) or set()
    unknown = sorted(((select or set()) | ignore) - FINDINGS.keys())
    if unknown:
        parser.error(f"unknown style finding: {', '.join(unknown)}")
    chosen = (select if select is not None else set(FINDINGS)) - ignore

    try:
        root = arguments.schema_root or discover_schema_root(arguments.paths[0])
        orders = field_orders(root)
    except (OSError, ValueError) as error:
        print(f"error: {error}", file=sys.stderr)
        return 2

    files = specification_files(arguments.paths, orders["root_class"])
    failed = False
    remaining = 0
    affected = 0
    fixed = 0
    for path in files:
        try:
            if arguments.fix:
                changed, findings = fix_file(path, select=chosen, orders=orders)
                fixed += changed
            else:
                findings = check_file(path, select=chosen, orders=orders)
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
