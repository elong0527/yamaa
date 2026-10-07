#!/usr/bin/env python3
"""Require production Rust requirement literals to name current normative rules.

The Rust diagnostic integration test separately reaches every typed registry cause
through evaluation, conversion, resource classification or original-document compilation.
Public condition names are deliberately not
unique keys: several existing failure causes share a condition spelling.
"""

import argparse
import re
from pathlib import Path

DEFINITION = re.compile(r"^\*\*(REQ-[0-9]{4,})\.\*\*", re.MULTILINE)
LITERAL = re.compile(r'"(REQ-[0-9]{4,})"')


def violations(root: Path) -> list[str]:
    """Check all crate source mappings, including those not yet in the registry."""
    defined = {
        identifier
        for path in (root / "rules").rglob("*.md")
        for identifier in DEFINITION.findall(path.read_text(encoding="utf-8"))
    }
    if not defined:
        return ["no normative requirement definitions found"]
    sources = sorted((root / "rust" / "crates").glob("*/src/**/*.rs"))
    if not sources:
        return ["no Rust crate sources found"]
    errors = []
    for path in sources:
        for number, line in enumerate(
            path.read_text(encoding="utf-8").splitlines(), start=1
        ):
            for identifier in LITERAL.findall(line):
                if identifier not in defined:
                    errors.append(
                        f"{path.relative_to(root).as_posix()}:{number}: "
                        f"undefined requirement {identifier}"
                    )
    return errors


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[2])
    errors = violations(parser.parse_args().root)
    for error in errors:
        print(error)
    if errors:
        return 1
    print("PASS: Rust requirement literals name current normative rules.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
