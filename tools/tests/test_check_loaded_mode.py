"""The loaded-mode check refuses the test-only loaded mode in every file that
builds or publishes a shipped package."""

import tempfile
import unittest
from pathlib import Path

from tools.paths import ROOT
from tools.tests.support import write
from tools.validation import check_loaded_mode


class LoadedModeTests(unittest.TestCase):
    def setUp(self):
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        self.root = Path(directory.name)
        for relative in check_loaded_mode.SHIPPED:
            write(self.root, relative, "# a shipped build\n")

    def test_the_repository_passes(self):
        self.assertEqual(check_loaded_mode.check(ROOT), [])

    def test_a_shipped_build_that_names_the_loaded_mode_is_refused(self):
        cases = [
            (
                "python/pyproject.toml",
                'features = ["extension-module", "dynamic-engine"]\n',
                "python/pyproject.toml:2: names dynamic-engine",
            ),
            (
                "R/yamaa/src/Makevars.in",
                "\tcargo build -p yamaa-r --features dynamic-engine\n",
                "R/yamaa/src/Makevars.in:2: names dynamic-engine",
            ),
            (
                ".github/workflows/release.yml",
                "        env:\n          YAMAA_ENGINE: loaded\n",
                ".github/workflows/release.yml:3: names YAMAA_ENGINE",
            ),
        ]
        for relative, line, located in cases:
            with self.subTest(file=relative):
                path = self.root / relative
                original = path.read_text(encoding="utf-8")
                path.write_text(original + line, encoding="utf-8")
                self.assertEqual(
                    check_loaded_mode.check(self.root),
                    [
                        f"{located}, of the test-only loaded mode, which no "
                        "shipped build enables"
                    ],
                )
                path.write_text(original, encoding="utf-8")

    def test_a_missing_shipped_file_is_refused(self):
        (self.root / "R/yamaa/src/Makevars.win.in").unlink()
        self.assertEqual(
            check_loaded_mode.check(self.root),
            ["R/yamaa/src/Makevars.win.in is missing"],
        )


if __name__ == "__main__":
    unittest.main()
