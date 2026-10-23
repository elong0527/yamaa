"""Check that no shipped build enables the test-only loaded mode.

A binding built with its `dynamic-engine` feature links no engine and loads
the engine library from beside its own module at runtime, and the engine
crate built with the same feature exports the entry that the loader looks
up (rust/interface/README.md). That mode serves tests alone, and
YAMAA_ENGINE selects it for the repository's host checks
(tools/commands/loaded.py). Every shipped package links its engine, so
neither the wheel nor the R library loads a library at runtime or exports a
new symbol. This check refuses either name in the files that build or
publish a shipped package: the Python package's pyproject.toml, the R
package's build files, the release workflows and the staging tools that
they run.
"""

import argparse
import sys
from pathlib import Path

from tools.paths import ROOT

NAMES = ("dynamic-engine", "YAMAA_ENGINE")
SHIPPED = (
    "python/pyproject.toml",
    "R/yamaa/DESCRIPTION",
    "R/yamaa/configure",
    "R/yamaa/configure.win",
    "R/yamaa/cleanup",
    "R/yamaa/cleanup.win",
    "R/yamaa/src/Makevars.in",
    "R/yamaa/src/Makevars.win.in",
    "R/yamaa/tools/configure.R",
    ".github/workflows/release.yml",
    ".github/workflows/release-rehearsal.yml",
    ".github/workflows/release-compatibility.yml",
    ".github/workflows/pypi-install.yml",
    "tools/staging/engine_bundle.py",
    "tools/staging/stage_r_package.py",
    "tools/staging/vendor_crates.py",
)


def check(root):
    """Each line of a shipped build's files that names the loaded mode, and
    each of those files that is missing."""
    errors = []
    for relative in SHIPPED:
        path = root / relative
        if not path.is_file():
            errors.append(f"{relative} is missing")
            continue
        lines = path.read_text(encoding="utf-8").splitlines()
        for number, line in enumerate(lines, start=1):
            for name in NAMES:
                if name in line:
                    errors.append(
                        f"{relative}:{number}: names {name}, of the test-only "
                        "loaded mode, which no shipped build enables"
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
        f"PASS: none of the {len(SHIPPED)} files of a shipped build names the loaded mode."
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
