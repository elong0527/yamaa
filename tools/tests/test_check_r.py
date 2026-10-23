"""R CMD check --as-cran may report only the notes that cran-comments.md explains."""

import contextlib
import io
import re
import tempfile
import unittest
from pathlib import Path
from unittest import mock

from tools.commands import check_r
from tools.paths import ROOT

HEAD = """\
* using log directory '/tmp/check/yamaa.Rcheck'
* using R version 4.4.3 (2025-02-28)
* using options '--no-manual --as-cran'
* checking for file 'yamaa/DESCRIPTION' ... OK
* this is package 'yamaa' version '0.5.0'
* checking CRAN incoming feasibility ... NOTE
Maintainer: 'Yilong Zhang <elong0527@gmail.com>'

New submission

Size of tarball: 14539245 bytes
* checking package namespace information ... OK
"""
SIZE = """\
* checking installed package size ... {status}
  installed size is 24.2Mb
  sub-directories of 1Mb or more:
    libs  23.6Mb
"""
TAIL = """\
* checking Rust compilation ... OK
* checking tests ... [44s/45s] OK
  Running 'column_ceilings.R' [13s/13s]
  Running 'compact_yaml.R'
* checking for detritus in the temp directory ... OK
* DONE
Status: {status}
"""
BASHISMS = """\
* checking top-level files ... WARNING
A complete check needs the 'checkbashisms' script.
See section 'Configure and cleanup' in the 'Writing R Extensions'
manual.
"""


def log(*parts, status):
    return "".join(parts) + TAIL.format(status=status)


class FindingsTests(unittest.TestCase):
    def test_the_explained_notes_pass(self):
        for text in (
            # R 4.4 reports the installed size as a note.
            log(HEAD, SIZE.format(status="NOTE"), status="2 NOTEs"),
            # R 4.6.1 and R-devel report it as INFO, which R does not count.
            log(HEAD, SIZE.format(status="INFO"), status="1 NOTE"),
            # Without remote access the incoming check skips "New submission".
            log(HEAD.replace("New submission\n", ""), status="1 NOTE"),
        ):
            with self.subTest(text=text):
                self.assertEqual(check_r.unexpected(text), [])

    def test_every_other_finding_fails(self):
        timestamps = (
            "* checking for future file timestamps ... NOTE\n"
            "unable to verify current time\n"
        )
        spelling = HEAD.replace(
            "Size of tarball",
            "Possibly misspelled words in DESCRIPTION:\n  Yamaa (2:8)\n\nSize of tarball",
        )
        for text, problem in (
            (
                log(HEAD, timestamps, status="2 NOTEs"),
                "NOTE: checking for future file timestamps",
            ),
            (
                log(spelling, status="1 NOTE"),
                "Possibly misspelled words in DESCRIPTION:",
            ),
            (
                log(
                    HEAD.replace("feasibility ... NOTE", "feasibility ... WARNING"),
                    status="1 WARNING",
                ),
                "WARNING: checking CRAN incoming feasibility",
            ),
            (
                log(
                    HEAD,
                    SIZE.format(status="NOTE").replace("libs", "doc "),
                    status="2 NOTEs",
                ),
                "NOTE: checking installed package size: doc ",
            ),
            # Test inputs that R installed would be a regression (#3105).
            (
                log(
                    HEAD,
                    SIZE.format(status="NOTE") + "    specification-original   4.1Mb\n",
                    status="2 NOTEs",
                ),
                "NOTE: checking installed package size: specification-original",
            ),
            (
                log(HEAD, BASHISMS, status="1 WARNING, 1 NOTE"),
                "WARNING: checking top-level files",
            ),
            (
                log(
                    HEAD,
                    "* checking whether package 'yamaa' can be installed ...\n ERROR\n"
                    "Installation failed.\n",
                    status="1 ERROR, 1 NOTE",
                ),
                "ERROR: checking whether package 'yamaa' can be installed",
            ),
            (log(HEAD, status="2 NOTEs"), "the Status line counts"),
        ):
            with self.subTest(problem=problem):
                problems = check_r.unexpected(text)
                self.assertTrue(any(problem in line for line in problems), problems)

    def test_a_missing_checkbashisms_is_the_one_tolerated_warning(self):
        text = log(HEAD, BASHISMS, status="1 WARNING, 1 NOTE")
        self.assertEqual(check_r.unexpected(text, checkbashisms=False), [])
        # A UTF-8 locale quotes with typographic quotes.
        quoted = BASHISMS.replace(
            "section 'Configure and cleanup' in the 'Writing R Extensions'",
            "section \u2018Configure and cleanup\u2019 in the \u2018Writing R Extensions\u2019",
        )
        self.assertNotEqual(quoted, BASHISMS)
        self.assertEqual(
            check_r.unexpected(
                log(HEAD, quoted, status="1 WARNING, 1 NOTE"), checkbashisms=False
            ),
            [],
        )
        # A bashism that the script reports is never tolerated.
        found = BASHISMS.replace(
            "A complete check needs the 'checkbashisms' script.",
            "Found the following shell scripts with possible bashisms:",
        )
        self.assertNotEqual(
            check_r.unexpected(
                log(HEAD, found, status="1 WARNING, 1 NOTE"), checkbashisms=False
            ),
            [],
        )

    def test_a_log_without_a_status_line_is_refused(self):
        with self.assertRaisesRegex(ValueError, "no Status line"):
            check_r.findings(HEAD)

    def test_the_report_fails_on_unexplained_findings(self):
        with self.assertRaisesRegex(RuntimeError, "does not explain"):
            check_r.report(log(HEAD, BASHISMS, status="1 WARNING, 1 NOTE"))


class TarballTests(unittest.TestCase):
    def test_a_tarball_above_the_requested_limit_fails(self):
        limit = check_r.TARBALL_LIMIT
        for size, allowed in ((limit, True), (limit + 1, False)):
            with self.subTest(size=size):
                text = log(HEAD.replace("14539245", str(size)), status="1 NOTE")
                self.assertEqual(check_r.unexpected(text) == [], allowed)
        with tempfile.TemporaryDirectory() as directory:
            archive = Path(directory) / "yamaa_0.5.0.tar.gz"
            with archive.open("wb") as stream:
                stream.truncate(limit + 1)
            with self.assertRaisesRegex(RuntimeError, "above the"):
                check_r.check(archive, Path(directory) / "check")

    def test_cran_comments_requests_the_limit(self):
        comments = (ROOT / "R/yamaa/cran-comments.md").read_text(encoding="utf-8")
        self.assertIn(
            f"we request a limit of {check_r.TARBALL_LIMIT // 1_000_000} MB",
            " ".join(comments.split()),
        )


class CranCommentsTests(unittest.TestCase):
    def test_the_expected_notes_are_the_ones_cran_comments_explains(self):
        comments = (ROOT / "R/yamaa/cran-comments.md").read_text(encoding="utf-8")
        section = comments.split("## R CMD check results", 1)[1].split("\n## ", 1)[0]
        explained = re.findall(
            r"^- `checking (.+?) \.\.\. NOTE`", section, re.MULTILINE
        )
        self.assertEqual(sorted(explained), sorted(check_r.EXPECTED_NOTES))


class EnvironmentTests(unittest.TestCase):
    def test_the_installation_flags_are_known_and_remote_services_are_off(self):
        configuration = {
            "CC": "gcc -std=gnu2x",
            "CPPFLAGS": "-D_FORTIFY_SOURCE=2",
            "CPICFLAGS": "-fpic",
            "CFLAGS": "-O2 -Werror=format-security -march=x86-64-v2",
            "LDFLAGS": "-Wl,-z,relro",
        }
        with mock.patch.object(
            check_r, "r_config", side_effect=lambda name, _: configuration[name]
        ):
            environment = check_r.check_environment(
                {
                    "PATH": "/usr/bin",
                    "_R_CHECK_COMPILATION_FLAGS_KNOWN_": "-Wextra -fpic",
                }
            )
            self.assertEqual(
                environment["_R_CHECK_COMPILATION_FLAGS_KNOWN_"].split(),
                [
                    "-Wextra",
                    "-fpic",
                    "gcc",
                    "-std=gnu2x",
                    "-D_FORTIFY_SOURCE=2",
                    "-O2",
                    "-Werror=format-security",
                    "-march=x86-64-v2",
                    "-Wl,-z,relro",
                ],
            )
            self.assertEqual(environment["_R_CHECK_CRAN_INCOMING_REMOTE_"], "false")
            self.assertEqual(environment["_R_CHECK_SYSTEM_CLOCK_"], "false")
            self.assertEqual(environment["OPENBLAS_NUM_THREADS"], "1")
            for name, value in check_r.CRAN_SETTINGS.items():
                self.assertEqual(environment[name], value)
            # A caller may turn the remote checks on.
            environment = check_r.check_environment(
                {"PATH": "/usr/bin", "_R_CHECK_CRAN_INCOMING_REMOTE_": "true"}
            )
            self.assertEqual(environment["_R_CHECK_CRAN_INCOMING_REMOTE_"], "true")
            # DESCRIPTION is spell checked where aspell is installed.
            with tempfile.TemporaryDirectory() as directory:
                self.assertNotIn(
                    "_R_CHECK_CRAN_INCOMING_USE_ASPELL_",
                    check_r.check_environment({"PATH": directory}),
                )
                aspell = Path(directory) / "aspell"
                aspell.write_text("#!/bin/sh\n")
                aspell.chmod(0o755)
                self.assertEqual(
                    check_r.check_environment({"PATH": directory})[
                        "_R_CHECK_CRAN_INCOMING_USE_ASPELL_"
                    ],
                    "true",
                )

    def test_checks_run_outside_the_checkout(self):
        with tempfile.NamedTemporaryFile(suffix=".tar.gz") as archive:
            with self.assertRaisesRegex(ValueError, "outside the checkout"):
                check_r.check(Path(archive.name), ROOT / "build/r-check")


class FakeCheckTests(unittest.TestCase):
    """Run check() against a fake R CMD check, which writes given outputs."""

    def setUp(self):
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        self.root = Path(directory.name)
        self.archive = self.root / "yamaa_0.5.0.tar.gz"
        self.archive.write_bytes(b"archive")

    def run_check(self, returncode, outputs):
        """Run check() against a fake R CMD check that writes `outputs`, by
        path below yamaa.Rcheck, and exits with `returncode`."""
        commands = []
        # Each check needs a work directory of its own.
        work = Path(tempfile.mkdtemp(dir=self.root)) / "check"

        def fake_run(command, cwd, env, check):
            commands.append((command, check))
            directory = Path(cwd) / "yamaa.Rcheck"
            (directory / "tests").mkdir(parents=True)
            for name, text in outputs.items():
                (directory / name).write_text(text, encoding="utf-8")
            (directory / "00check.log").write_text(
                log(HEAD, status="1 NOTE"), encoding="utf-8"
            )
            return check_r.subprocess.CompletedProcess(command, returncode)

        printed = io.StringIO()
        with (
            mock.patch.object(check_r, "check_environment", return_value={}),
            mock.patch.object(check_r, "has_checkbashisms", return_value=True),
            mock.patch.object(
                check_r.subprocess, "check_output", return_value="revision\n"
            ),
            mock.patch.object(check_r.subprocess, "run", side_effect=fake_run),
            contextlib.redirect_stdout(printed),
        ):
            try:
                check_r.check(self.archive, work)
                failure = None
            except (check_r.subprocess.CalledProcessError, RuntimeError) as error:
                failure = error
        return commands, printed.getvalue(), failure


class FailingTestsTests(FakeCheckTests):
    """Every test file runs, and a failing file's whole output is printed."""

    def test_r_cmd_check_runs_every_test_file(self):
        commands, _, failure = self.run_check(0, {"tests/lint.Rout": "fine\n"})
        self.assertEqual(
            commands,
            [
                (
                    [
                        "R",
                        "CMD",
                        "check",
                        "--as-cran",
                        "--no-manual",
                        "--no-stop-on-test-error",
                        str(self.archive.resolve()),
                    ],
                    False,
                )
            ],
        )
        self.assertIsNone(failure)

    def test_a_failing_check_prints_each_failing_test_file_whole(self):
        _, printed, failure = self.run_check(
            1,
            {
                "tests/lint.Rout": "lint passed\n",
                "tests/schema.Rout.fail": "line 1\nError: the schema is one line\n",
                "tests/selftest.Rout.fail": "Error: every case passes\n",
            },
        )
        self.assertEqual(
            printed,
            "===== schema.Rout.fail =====\n"
            "line 1\nError: the schema is one line\n"
            "===== end of schema.Rout.fail =====\n"
            "===== selftest.Rout.fail =====\n"
            "Error: every case passes\n"
            "===== end of selftest.Rout.fail =====\n",
        )
        self.assertEqual(failure.returncode, 1)


# Reports in the forms that the sanitizers document: UndefinedBehaviorSanitizer
# names the source location and carries on, and with print_summary=0 prints no
# summary line; AddressSanitizer stops the program.
UBSAN_LINE = """\
readstat_sas7bdat_read.c:512:23: runtime error: signed integer overflow: \
2147483647 + 1 cannot be represented in type 'int'
"""
UBSAN_REPORT = f"""\
{UBSAN_LINE}\
SUMMARY: UndefinedBehaviorSanitizer: undefined-behavior readstat_sas7bdat_read.c:512:23
"""
ASAN_REPORT = """\
==4242==ERROR: AddressSanitizer: heap-buffer-overflow on address 0x602000000014
READ of size 4 at 0x602000000014 thread T0
SUMMARY: AddressSanitizer: heap-buffer-overflow readstat_bits.c:42:12 in readstat_int32
"""


class SanitizerReportsTests(FakeCheckTests):
    """A sanitizer report in an output of the check fails it, and the output
    is printed whole, once."""

    def test_a_report_fails_a_check_that_r_passed(self):
        # R passes a test, an example or an installation that
        # UndefinedBehaviorSanitizer reported on, since the program carried on.
        for output, report in (
            ("00install.out", UBSAN_REPORT),
            ("yamaa-Ex.Rout", UBSAN_REPORT),
            ("tests/lint.Rout", UBSAN_REPORT),
            ("tests/lint.Rout", UBSAN_LINE),
        ):
            with self.subTest(output=output, summary=report == UBSAN_REPORT):
                text = f"> library(yamaa)\n{report}> q()\n"
                _, printed, failure = self.run_check(
                    0, {output: text, "tests/schema.Rout": "schema passed\n"}
                )
                name = Path(output).name
                self.assertEqual(
                    printed,
                    f"===== {name} =====\n{text}===== end of {name} =====\n",
                )
                self.assertEqual(
                    str(failure),
                    "these outputs of the check hold a sanitizer report: "
                    f"yamaa.Rcheck/{output}",
                )

    def test_a_failed_test_with_a_report_is_printed_once(self):
        text = f"> yamaa::yamaa_selftest()\n{ASAN_REPORT}"
        _, printed, failure = self.run_check(
            1, {"tests/selftest.Rout.fail": text, "00install.out": "* DONE (yamaa)\n"}
        )
        self.assertEqual(
            printed,
            "===== selftest.Rout.fail =====\n"
            f"{text}"
            "===== end of selftest.Rout.fail =====\n",
        )
        self.assertEqual(failure.returncode, 1)


if __name__ == "__main__":
    unittest.main()
