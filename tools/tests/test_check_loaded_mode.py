"""The loaded-mode check refuses the test-only loaded mode in every file that
builds or publishes a shipped package, in the native workflow but its loaded
job, and in the default features of the boundary's crates."""

import tempfile
import unittest
from pathlib import Path

from tools.paths import ROOT
from tools.tests.support import write
from tools.validation import check_loaded_mode

REFUSED = "of the test-only loaded mode, which no shipped build enables"
NATIVE = check_loaded_mode.NATIVE
# A native workflow whose loaded job alone selects the loaded mode.
WORKFLOW = """\
on:
  workflow_dispatch:
env:
  NATIVE_BUILD_PYTHON: '3.14'
jobs:
  dist:
    steps:
      - run: python -m tools.ci.manylinux
  loaded:
    steps:
      - env:
          YAMAA_ENGINE: loaded
        run: cargo build -p yamaa-r --no-default-features --features dynamic-engine
"""
BOUNDARY = "rust/crates/yamaa-boundary/Cargo.toml"
ENGINE = "rust/crates/yamaa-engine/Cargo.toml"
PYTHON = "rust/crates/yamaa-python/Cargo.toml"
R = "rust/crates/yamaa-r/Cargo.toml"
BINDING = """\
[features]
default = ["linked-engine"]
linked-engine = ["dep:yamaa-engine"]
dynamic-engine = ["yamaa-boundary/loader"]
"""
MANIFESTS = {
    BOUNDARY: '[features]\nloader = ["dep:libc"]\n',
    ENGINE: "[features]\ndynamic-engine = []\n",
    PYTHON: BINDING,
    R: BINDING,
}


class LoadedModeTests(unittest.TestCase):
    def setUp(self):
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        self.root = Path(directory.name)
        for relative in check_loaded_mode.SHIPPED:
            write(self.root, relative, "# a shipped build\n")
        write(self.root, NATIVE, WORKFLOW)
        for relative, text in MANIFESTS.items():
            write(self.root, relative, text)

    def refused(self, cases):
        """Check each case of file, its whole text and the errors it gets,
        with every other file as setUp wrote it."""
        for relative, text, errors in cases:
            with self.subTest(file=relative, errors=errors):
                path = self.root / relative
                original = path.read_text(encoding="utf-8")
                path.write_text(text, encoding="utf-8")
                try:
                    self.assertEqual(check_loaded_mode.check(self.root), errors)
                finally:
                    path.write_text(original, encoding="utf-8")

    def test_the_repository_passes(self):
        self.assertEqual(check_loaded_mode.check(ROOT), [])

    def test_the_loaded_job_alone_selects_the_loaded_mode(self):
        self.assertEqual(check_loaded_mode.check(self.root), [])

    def test_a_shipped_build_that_names_the_loaded_mode_is_refused(self):
        shipped = "# a shipped build\n"
        self.refused(
            [
                (
                    "python/pyproject.toml",
                    shipped + 'features = ["extension-module", "dynamic-engine"]\n',
                    [f"python/pyproject.toml:2: names dynamic-engine, {REFUSED}"],
                ),
                (
                    "R/yamaa/src/Makevars.in",
                    shipped + "\tcargo build -p yamaa-r --features dynamic-engine\n",
                    [f"R/yamaa/src/Makevars.in:2: names dynamic-engine, {REFUSED}"],
                ),
                (
                    ".github/workflows/release.yml",
                    shipped + "        env:\n          YAMAA_ENGINE: loaded\n",
                    [f".github/workflows/release.yml:3: names YAMAA_ENGINE, {REFUSED}"],
                ),
                (
                    "tools/ci/manylinux.py",
                    shipped + 'FEATURES = "extension-module,dynamic-engine"\n',
                    [f"tools/ci/manylinux.py:2: names dynamic-engine, {REFUSED}"],
                ),
            ]
        )

    def test_the_native_workflow_selects_the_loaded_mode_in_its_loaded_job_alone(self):
        self.refused(
            [
                (
                    NATIVE,
                    WORKFLOW.replace(
                        "  NATIVE_BUILD_PYTHON: '3.14'\n",
                        "  NATIVE_BUILD_PYTHON: '3.14'\n  YAMAA_ENGINE: loaded\n",
                    ),
                    [f"{NATIVE}: its env names YAMAA_ENGINE, {REFUSED}"],
                ),
                (
                    NATIVE,
                    WORKFLOW.replace(
                        "python -m tools.ci.manylinux",
                        "maturin build --features extension-module,dynamic-engine",
                    ),
                    [f"{NATIVE}: its dist job names dynamic-engine, {REFUSED}"],
                ),
            ]
        )

    def test_a_default_feature_that_enables_the_loaded_mode_is_refused(self):
        self.refused(
            [
                (
                    ENGINE,
                    '[features]\ndefault = ["dynamic-engine"]\ndynamic-engine = []\n',
                    [
                        f"{ENGINE}: the default features enable dynamic-engine, {REFUSED}"
                    ],
                ),
                (
                    # Through another feature of the manifest.
                    BOUNDARY,
                    '[features]\ndefault = ["checks"]\nchecks = ["loader"]\n'
                    'loader = ["dep:libc"]\n',
                    [f"{BOUNDARY}: the default features enable loader, {REFUSED}"],
                ),
                (
                    PYTHON,
                    BINDING.replace(
                        'default = ["linked-engine"]',
                        'default = ["linked-engine", "dynamic-engine"]',
                    ),
                    [
                        f"{PYTHON}: the default features enable dynamic-engine, "
                        f"{REFUSED}",
                        f"{PYTHON}: the default features enable "
                        f"yamaa-boundary/loader, {REFUSED}",
                        f'{PYTHON}: the default features are ["linked-engine", '
                        '"dynamic-engine"], not ["linked-engine"]: every shipped '
                        "package links its engine",
                    ],
                ),
                (
                    R,
                    BINDING.replace('default = ["linked-engine"]\n', ""),
                    [
                        f'{R}: the default features are [], not ["linked-engine"]: '
                        "every shipped package links its engine"
                    ],
                ),
            ]
        )

    def test_a_missing_file_is_refused(self):
        for relative in ("R/yamaa/src/Makevars.win.in", NATIVE, R):
            with self.subTest(file=relative):
                path = self.root / relative
                original = path.read_text(encoding="utf-8")
                path.unlink()
                try:
                    self.assertEqual(
                        check_loaded_mode.check(self.root), [f"{relative} is missing"]
                    )
                finally:
                    path.write_text(original, encoding="utf-8")


if __name__ == "__main__":
    unittest.main()
