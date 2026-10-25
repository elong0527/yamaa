"""Check that the support modules the Rust integration suites copy stay equal.

Each engine crate owns its integration tests' support tree, and no crate's
tests import another crate's test source (#2158). The adapters' and the
application's tests build the same hand-written documents and AST roots as
core's, the application's tests also its project compilers, and its replay
reads the case form core's replay reads (#3798); the application's tests also
write the adapters' Parquet inputs and use their scratch directories and
inheritance helpers (#3761). Each copy must stay byte for byte equal to its
owner's module, so a change to one is made to every copy (#3569).
"""

import argparse
import sys
from pathlib import Path

from tools.paths import ROOT

CORE = Path("rust/crates/yamaa-core/tests/support")
ADAPTERS = Path("rust/crates/yamaa-adapters/tests/support")
APPLICATION = Path("rust/crates/yamaa-application/tests/support")
# Each owner's support tree, a tree that copies some of its modules, and the
# modules copied.
COPIES = (
    (CORE, ADAPTERS, ("document.rs", "roots.rs")),
    (
        CORE,
        APPLICATION,
        (
            "diagnostic/form.rs",
            "document.rs",
            "project_call_compiler.rs",
            "project_codelist_compiler.rs",
            "roots.rs",
        ),
    ),
    (ADAPTERS, APPLICATION, ("inheritance.rs", "parquet_files.rs", "scratch.rs")),
)


def check(root):
    """The copies that are missing or differ from their owner's module."""
    errors = []
    for owner_tree, copy_tree, names in COPIES:
        for name in names:
            owner, copy = root / owner_tree / name, root / copy_tree / name
            for path in (owner, copy):
                if not path.is_file():
                    errors.append(f"{path.relative_to(root)} is missing")
            if owner.is_file() and copy.is_file():
                if owner.read_bytes() != copy.read_bytes():
                    errors.append(
                        f"{copy_tree / name} differs from {owner_tree / name}; "
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
    copied = sum(len(names) for _, _, names in COPIES)
    print(f"PASS: the {copied} copied test support modules are equal to their owners'.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
