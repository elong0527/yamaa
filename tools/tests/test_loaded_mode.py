"""The tooling of the test-only loaded mode: the link mode that YAMAA_ENGINE
selects, and a staged R package patched to build its binding and the engine
library that way, while the repository's own build files stay linked."""

import shutil
import tempfile
import unittest
from pathlib import Path
from unittest import mock

from tools.commands import check_r, loaded
from tools.paths import ROOT

R_SOURCE = ROOT / "R/yamaa/src"


class LinkMode(unittest.TestCase):
    def test_the_mode_is_linked_unless_yamaa_engine_says_loaded(self):
        cases = [
            ({}, "linked"),
            ({"YAMAA_ENGINE": "linked"}, "linked"),
            ({"YAMAA_ENGINE": "loaded"}, "loaded"),
        ]
        for environment, expected in cases:
            with (
                self.subTest(environment=environment),
                mock.patch.dict(loaded.os.environ, environment, clear=True),
            ):
                self.assertEqual(loaded.mode(), expected)

    def test_another_mode_is_refused(self):
        with (
            mock.patch.dict(loaded.os.environ, {"YAMAA_ENGINE": "static"}, clear=True),
            self.assertRaisesRegex(ValueError, "YAMAA_ENGINE must be linked or loaded"),
        ):
            loaded.mode()


class StagedRPackage(unittest.TestCase):
    def setUp(self):
        work = tempfile.TemporaryDirectory()
        self.addCleanup(work.cleanup)
        self.package = Path(work.name) / "yamaa"
        (self.package / "src").mkdir(parents=True)
        for name in ("Makevars.in", "Makevars.win.in"):
            shutil.copy2(R_SOURCE / name, self.package / "src" / name)

    def test_the_staged_package_builds_its_binding_and_engine_in_the_loaded_mode(self):
        loaded.patch_r_package(self.package)
        added = {}
        for name in ("Makevars.in", "Makevars.win.in"):
            before = (R_SOURCE / name).read_text(encoding="utf-8").splitlines()
            after = (
                (self.package / "src" / name).read_text(encoding="utf-8").splitlines()
            )
            changed = [line for line in after if line not in before]
            added[name] = [
                (
                    "-p yamaa-r --no-default-features --features dynamic-engine"
                    in line,
                    "rustc" in line
                    and "-p yamaa-engine --lib --crate-type cdylib --features dynamic-engine"
                    in line,
                    line.lstrip().startswith(("for library", "cp ")),
                )
                for line in changed
            ]
        # Each file builds its binding without the linked engine, then the
        # engine library, and copies it into src/, where R installs it.
        self.assertEqual(
            added,
            {
                "Makevars.in": [
                    (True, False, False),
                    (False, True, False),
                    (False, False, True),
                ],
                "Makevars.win.in": [
                    (True, False, False),
                    (False, True, False),
                    (False, False, True),
                ],
            },
        )

    def test_the_repository_build_files_stay_linked(self):
        loaded.patch_r_package(self.package)
        for name in ("Makevars.in", "Makevars.win.in"):
            with self.subTest(file=name):
                self.assertNotIn(
                    loaded.FEATURE, (R_SOURCE / name).read_text(encoding="utf-8")
                )


if __name__ == "__main__":
    unittest.main()


class LoadedPackageCheck(unittest.TestCase):
    # R CMD check's note on a library in libs/ that registers no routines,
    # as it prints it for the engine library.
    LOG = (
        "* checking compiled code ... NOTE\n"
        "File \u2018yamaa/{library}\u2019:\n"
        "  Found no calls to: \u2018R_registerRoutines\u2019, \u2018R_useDynamicSymbols\u2019\n"
        "\n"
        "It is good practice to register native routines and to disable symbol\n"
        "search.\n"
        "\n"
        "See \u2018Writing portable packages\u2019 in the \u2018Writing R Extensions\u2019 manual.\n"
        "* checking examples ... OK\n"
        "* DONE\n"
        "\n"
        "Status: 1 NOTE\n"
    )

    def test_only_the_loaded_mode_accepts_the_engine_library_note(self):
        loaded_notes = check_r.EXPECTED_NOTES | loaded.R_CHECK_NOTES
        cases = [
            ("libs/libyamaa_engine.so", loaded_notes, []),
            ("libs/x64/yamaa_engine.dll", loaded_notes, []),
            (
                "libs/yamaa.so",
                loaded_notes,
                ["NOTE: checking compiled code: File \u2018yamaa/libs/yamaa.so\u2019:"],
            ),
            (
                "libs/libyamaa_engine.so",
                check_r.EXPECTED_NOTES,
                [
                    "NOTE: checking compiled code",
                    "    File \u2018yamaa/libs/libyamaa_engine.so\u2019:",
                    "    Found no calls to: \u2018R_registerRoutines\u2019, "
                    "\u2018R_useDynamicSymbols\u2019",
                    "    It is good practice to register native routines and to "
                    "disable symbol",
                    "    search.",
                    "    See \u2018Writing portable packages\u2019 in the "
                    "\u2018Writing R Extensions\u2019 manual.",
                ],
            ),
        ]
        for library, expected, problems in cases:
            with self.subTest(library=library, loaded=expected is loaded_notes):
                self.assertEqual(
                    check_r.unexpected(
                        self.LOG.format(library=library), expected=expected
                    ),
                    problems,
                )
