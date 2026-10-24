"""The tooling of the test-only loaded mode: the link mode that YAMAA_ENGINE
selects, a staged R package patched to build its binding and the engine
library that way, while the repository's own build files stay linked, and
the checks of native qualification's native-loaded jobs, which keep their
logs."""

import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path
from unittest import mock

from tools.commands import check_r, loaded, loaded_checks
from tools.paths import ROOT
from tools.tests import support

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


class LoadedChecks(unittest.TestCase):
    def setUp(self):
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        self.work = Path(directory.name) / "work"
        self.logs = Path(directory.name) / "logs"
        self.work.mkdir()

    def test_the_checks_refuse_the_linked_mode(self):
        with mock.patch.dict(loaded.os.environ, {"YAMAA_ENGINE": "linked"}):
            status, _, errors = support.run_main(
                loaded_checks.main, "--host", "python", "--logs", str(self.logs)
            )
        # The usage line names the program as it was started.
        self.assertEqual(
            (status, errors.splitlines()[-1].partition(" error: ")[2]),
            (2, "these checks are of the loaded mode: set YAMAA_ENGINE=loaded"),
        )

    def test_the_python_check_runs_the_installed_suites_with_the_fixture_engines(self):
        fixtures = self.work / "fixture-engines"
        with (
            mock.patch.dict(loaded_checks.os.environ),
            mock.patch.object(
                loaded_checks.equivalence,
                "install_python",
                autospec=True,
                return_value=Path("python"),
            ) as install,
            mock.patch.object(
                loaded_checks.loaded,
                "build_fixtures",
                autospec=True,
                return_value=fixtures,
            ) as build,
            mock.patch.object(
                loaded_checks.python_tests, "run_installed_suites", autospec=True
            ) as suites,
        ):
            failed = loaded_checks.run("python", self.work, "uv", self.logs)
            variable = loaded_checks.os.environ[loaded.FIXTURES_VARIABLE]
        self.assertEqual(
            (
                failed,
                install.call_args_list,
                build.call_args_list,
                variable,
                suites.call_args_list,
            ),
            (
                [],
                [mock.call(self.work / "install", "uv")],
                [mock.call(fixtures)],
                str(fixtures),
                [
                    mock.call(
                        Path("python"),
                        self.work / "suites",
                        log=self.logs / "installed-suites.log",
                    )
                ],
            ),
        )

    def test_the_r_check_keeps_its_logs_whether_it_passes_or_fails(self):
        # R CMD check's directory holds the check's log, the installation's
        # output and each test file's output, beside the files it checked.
        written = (
            "yamaa.Rcheck/00check.log",
            "yamaa.Rcheck/00install.out",
            "yamaa.Rcheck/tests/interface.R",
            "yamaa.Rcheck/tests/interface.Rout",
            "yamaa.Rcheck/tests/streaming.Rout.fail",
            "yamaa.Rcheck/yamaa/DESCRIPTION",
        )
        kept = [
            "yamaa.Rcheck/00check.log",
            "yamaa.Rcheck/00install.out",
            "yamaa.Rcheck/tests/interface.Rout",
            "yamaa.Rcheck/tests/streaming.Rout.fail",
        ]
        failure = subprocess.CalledProcessError(1, ["R", "CMD", "check"])
        for label, error, failed in (
            ("a passing check", None, []),
            ("a failing check", failure, ["R package check"]),
        ):
            with self.subTest(case=label):
                work, logs = self.work / label, self.logs / label

                def check(archive, environment, directory, error=error):
                    for name in written:
                        support.write(directory, name, name)
                    if error:
                        raise error

                with (
                    mock.patch.object(
                        loaded_checks.r_tests,
                        "install",
                        autospec=True,
                        return_value=(Path("yamaa_1.0.0.tar.gz"), {}),
                    ),
                    mock.patch.object(
                        loaded_checks.r_tests,
                        "check_install",
                        autospec=True,
                        side_effect=check,
                    ),
                ):
                    result = loaded_checks.run("r", work, "uv", logs)
                self.assertEqual(
                    (
                        result,
                        sorted(
                            path.relative_to(logs).as_posix()
                            for path in logs.rglob("*")
                            if path.is_file()
                        ),
                    ),
                    (failed, kept),
                )


if __name__ == "__main__":
    unittest.main()
