"""Check that the support modules the Rust integration suites copy stay equal.

Each engine crate owns its integration tests' support tree, and no crate's
tests import another crate's test source (#2158). The adapters' tests build
the same hand-written documents, AST roots and project compilers as core's,
so `rust/crates/yamaa-adapters/tests/support/` holds copies of four modules of
`rust/crates/yamaa-core/tests/support/`. Each copy must stay byte for byte
equal to core's, so a change to one is made to both (#3569).
"""

import argparse
import sys
from pathlib import Path

from tools.paths import ROOT

OWNER = Path("rust/crates/yamaa-core/tests/support")
COPY = Path("rust/crates/yamaa-adapters/tests/support")
SHARED = (
    "document.rs",
    "project_call_compiler.rs",
    "project_codelist_compiler.rs",
    "roots.rs",
)


def check(root):
    """The copies that are missing or differ from core's module."""
    errors = []
    for name in SHARED:
        owner, copy = root / OWNER / name, root / COPY / name
        for path in (owner, copy):
            if not path.is_file():
                errors.append(f"{path.relative_to(root)} is missing")
        if owner.is_file() and copy.is_file():
            if owner.read_bytes() != copy.read_bytes():
                errors.append(
                    f"{COPY / name} differs from {OWNER / name}; "
                    "make the same change to both"
                )
    return errors


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=ROOT)
    errors = check(parser.parse_args(argv).root)
    for error in errors:
        print(f"ERROR: {error}")
    if errors:
        return 1
    print(
        f"PASS: the adapters' copies of {len(SHARED)} core test support "
        "modules are equal to core's."
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
