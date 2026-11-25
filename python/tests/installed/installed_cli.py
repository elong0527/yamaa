"""Installed `yamaa.cli()` and `python -m yamaa`: the command line run as the
Python host (`cli.entries` of rust/interface/bindings.yaml).

The commands, what they print and their exit statuses are the facade's `cli`
module, which the `yamaa` binary's own tests cover line by line; this checks
the seam the binary cannot reach: the command runs over this interpreter's
engine, so it names this host, reaches `sys.stdout` and `sys.stderr` as the
interpreter holds them, and calls the project functions installed here,
which the binary refuses.
"""

import contextlib
import inspect
import io
import json
import os
import platform
import shutil
import subprocess
import sys
import tempfile
import threading
import unittest
from pathlib import Path

import yamaa

STUDIES = Path(__file__).parent / "project-original"


def run(arguments):
    """`yamaa.cli(arguments)`: its exit status and what it wrote to each
    stream of the interpreter."""
    stdout, stderr = io.StringIO(), io.StringIO()
    with contextlib.redirect_stdout(stdout), contextlib.redirect_stderr(stderr):
        status = yamaa.cli(arguments)
    return status, stdout.getvalue(), stderr.getvalue()


def module(arguments, cwd=None):
    """`python -m yamaa` with `arguments`, in a fresh interpreter."""
    completed = subprocess.run(
        [sys.executable, "-m", "yamaa", *arguments],
        cwd=cwd,
        capture_output=True,
        text=True,
        check=False,
    )
    return completed.returncode, completed.stdout, completed.stderr


def module_bytes(arguments, encoding, cwd=None):
    """`python -m yamaa` with `arguments`, in a fresh interpreter whose
    standard streams have `encoding`: its status and the bytes of each."""
    completed = subprocess.run(
        [sys.executable, "-m", "yamaa", *arguments],
        cwd=cwd,
        capture_output=True,
        env={**os.environ, "PYTHONIOENCODING": encoding},
        check=False,
    )
    return completed.returncode, completed.stdout, completed.stderr


# A spec read as UTF-8 whose label no cp1252 or ASCII stream spells.
UTF8_SPEC = (
    "yamaa: '1.0'\ntype: spec\nencoding: utf-8\nmodel: SDTM\ndomain: DM\n"
    "keys: [USUBJID]\ninput: {RAW: {path: raw.csv}}\ncolumns:\n"
    '- {name: USUBJID, type: str, label: "\u03bcg/L \u2265 5", derivation: RAW.USUBJID}\n'
    "output: {path: out/dm.csv, columns: [USUBJID], report: out/report.csv}\n"
)


class CommandLine(unittest.TestCase):
    def test_cli_takes_the_arguments_and_is_exported(self):
        self.assertEqual(list(inspect.signature(yamaa.cli).parameters), ["arguments"])
        self.assertIn("cli", yamaa.__all__)

    def test_version_names_this_host_and_interpreter(self):
        status, stdout, stderr = run(["--format", "json", "--version"])
        self.assertEqual((status, stderr), (0, ""))
        info = yamaa.info()
        self.assertEqual(json.loads(stdout), info)
        self.assertEqual(info["host"], "python")
        self.assertEqual(info["host_version"], platform.python_version())
        status, stdout, stderr = run(["--version"])
        self.assertEqual((status, stderr), (0, ""))
        self.assertEqual(
            stdout,
            f"yamaa {info['version']}\n"
            f"language versions: {', '.join(info['language_versions'])}\n"
            f"features: {', '.join(info['features'])}\n"
            "host: python\n"
            f"host version: {info['host_version']}\n"
            f"platform: {info['platform']}\n",
        )

    def test_selftest_reports_this_host(self):
        status, stdout, stderr = run(["--format", "json", "selftest"])
        self.assertEqual(stderr, "")
        document = json.loads(stdout)
        self.assertEqual(document["info"], yamaa.selftest().info)
        self.assertEqual(status, 0 if document["status"] == "succeeded" else 1)

    def test_a_usage_error_reaches_standard_error_and_exits_2(self):
        status, stdout, stderr = run(["bogus"])
        self.assertEqual(
            (status, stdout, stderr),
            (2, "", "yamaa: unknown command bogus\nrun 'yamaa --help' for usage\n"),
        )
        self.assertEqual(
            module([]),
            (2, "", "yamaa: no command given\nrun 'yamaa --help' for usage\n"),
        )

    def test_the_module_prints_what_the_function_prints(self):
        status, stdout, stderr = run(["--help"])
        self.assertEqual((status, stderr), (0, ""))
        self.assertEqual(module(["--help"]), (0, stdout, ""))
        self.assertTrue(
            stdout.startswith("usage: yamaa [--format text|json] COMMAND [ARGUMENTS]\n")
        )

    def test_help_lists_no_flag_the_host_refuses(self):
        # --host and --interpreter are usage errors in a host (D400): its
        # run synopsis is the binary's without them.
        status, stdout, stderr = run(["--help"])
        self.assertEqual((status, stderr), (0, ""))
        runs = [line for line in stdout.splitlines() if line.startswith("  run ")]
        self.assertEqual(
            runs,
            ["  run PATH [--spec NAME | --section SECTION] [--threads N] [--verbose]"],
        )

    def test_the_module_writes_the_same_bytes_under_every_stream_encoding(self):
        # The seam: the interpreter's stream encoding, which only a fresh
        # interpreter sets. The command's UTF-8 bytes reach a redirect as
        # the binary writes them, whatever the encoding (D400).
        label = "\u03bcg/L \u2265 5".encode()
        with tempfile.TemporaryDirectory(prefix="yamaa-cli-") as directory:
            Path(directory, "u8spec.yaml").write_text(UTF8_SPEC, encoding="utf-8")
            printed = {
                encoding: module_bytes(["resolve", "u8spec.yaml"], encoding, directory)
                for encoding in ("utf-8", "cp1252", "ascii")
            }
        status, stdout, stderr = printed["utf-8"]
        self.assertEqual((status, stderr), (0, b""), stderr)
        self.assertIn(label, stdout)
        for encoding in ("cp1252", "ascii"):
            with self.subTest(encoding=encoding):
                self.assertEqual(printed[encoding], printed["utf-8"])

    def test_a_standard_output_that_cannot_be_written_exits_2(self):
        class Closed:
            def write(self, text):
                raise OSError("the reader is gone")

            def flush(self):
                pass

        stderr = io.StringIO()
        with contextlib.redirect_stdout(Closed()), contextlib.redirect_stderr(stderr):
            status = yamaa.cli(["--version"])
        self.assertEqual(
            (status, stderr.getvalue()),
            (2, "yamaa: cannot write to standard output: the reader is gone\n"),
        )

    def test_a_standard_output_without_flush_takes_the_output(self):
        # print() writes to a replacement with `write` alone; so does the
        # command, which has nothing to flush there.
        class Written:
            def __init__(self):
                self.text = ""

            def write(self, text):
                self.text += text

        expected = run(["--version"])
        stdout, stderr = Written(), io.StringIO()
        with contextlib.redirect_stdout(stdout), contextlib.redirect_stderr(stderr):
            status = yamaa.cli(["--version"])
        self.assertEqual((status, stdout.text, stderr.getvalue()), expected)

    def test_commands_on_several_threads_give_standard_output_back(self):
        # The seam: the hold of sys.stdout is process-wide while commands
        # detach, so overlapping commands must give back the stream the
        # first one held, not each other's sys.stderr.
        stdout, stderr = io.StringIO(), io.StringIO()
        statuses = []
        with contextlib.redirect_stdout(stdout), contextlib.redirect_stderr(stderr):
            threads = [
                threading.Thread(
                    target=lambda: statuses.append(
                        yamaa.cli(["--format", "json", "selftest"])
                    )
                )
                for _ in range(4)
            ]
            for thread in threads:
                thread.start()
            for thread in threads:
                thread.join()
            self.assertIs(sys.stdout, stdout)
        # Each command's document reaches the held standard output, even
        # when another command still holds it.
        documents = [json.loads(line) for line in stdout.getvalue().splitlines()]
        self.assertEqual((len(statuses), len(documents), stderr.getvalue()), (4, 4, ""))

    def test_a_standard_output_that_takes_no_bytes_exits_2(self):
        # A binary layer that takes nothing fails the write, as the
        # binary's write-all does, instead of truncating the output.
        class Stalled:
            class buffer:
                @staticmethod
                def write(data):
                    return 0

            def write(self, text):
                raise AssertionError("the bytes go to the binary layer")

        size = len(run(["--version"])[1].encode())
        stderr = io.StringIO()
        with contextlib.redirect_stdout(Stalled()), contextlib.redirect_stderr(stderr):
            status = yamaa.cli(["--version"])
        self.assertEqual(
            (status, stderr.getvalue()),
            (
                2,
                "yamaa: cannot write to standard output: "
                f"the stream took 0 of {size} bytes\n",
            ),
        )

    def test_the_arguments_must_be_a_list_of_text(self):
        for arguments in ("--help", None, ["--format", 1], 3):
            with self.subTest(arguments=arguments):
                with self.assertRaises(yamaa.Error):
                    yamaa.cli(arguments)

    def test_a_run_calls_the_project_functions_installed_here(self):
        # The binary refuses this run before it reads data, as
        # unsupported_operation (D110, D123): the Python host runs it with
        # the installed yamaa_benchmarks package, verifies the project's uv
        # lock first, and saves the authored bytes.
        with tempfile.TemporaryDirectory(prefix="yamaa-cli-") as directory:
            case = Path(directory).resolve() / "schema-functions"
            shutil.copytree(STUDIES / "schema-functions", case)
            expected = (case / "expected/test.csv").read_bytes()
            previous = Path.cwd()
            os.chdir(case)
            try:
                status, stdout, stderr = run(
                    ["--format", "json", "run", "project.yaml", "--spec", "TEST"]
                )
            finally:
                os.chdir(previous)
            self.assertEqual(stderr, "")
            document = json.loads(stdout)
            self.assertEqual(
                (status, document["status"], document["issues"], document["spec"]),
                (0, "succeeded", [], "TEST"),
                stdout,
            )
            self.assertEqual(
                document["paths"], [(case / "test.csv").as_posix()], stdout
            )
            self.assertEqual((case / "test.csv").read_bytes(), expected)
            (case / "test.csv").unlink()
            # The whole project through `python -m yamaa`, with the text
            # output: the summary line, the member line and the path written.
            status, stdout, stderr = module(["run", "project.yaml"], cwd=case)
            self.assertEqual((status, stderr), (0, ""), stdout)
            rows = len(expected.decode().splitlines()) - 1
            columns = len(expected.decode().splitlines()[0].split(","))
            self.assertEqual(
                stdout,
                "yamaa project run of project.yaml: succeeded, 1 specification, 0 issues\n"
                f"  TEST  succeeded  {rows} rows x {columns} columns, 0 issues\n"
                f"{(case / 'test.csv').as_posix()}\n",
            )
            self.assertEqual((case / "test.csv").read_bytes(), expected)

    def test_a_project_function_prints_to_standard_error(self):
        # While the command runs, sys.stdout is sys.stderr, as in the
        # binary's worker: a function that prints leaves the JSON document
        # whole (D400).
        import yamaa_benchmarks

        original = yamaa_benchmarks.project_ratio

        # The engine admits a callable by its own formals, so the stand-in
        # repeats them.
        def printing(numerator, denominator, decimals=2, adjust=0.0, as_percent=False):
            print("from project_ratio")
            return original(numerator, denominator, decimals, adjust, as_percent)

        with tempfile.TemporaryDirectory(prefix="yamaa-cli-") as directory:
            case = Path(directory).resolve() / "schema-functions"
            shutil.copytree(STUDIES / "schema-functions", case)
            previous = Path.cwd()
            os.chdir(case)
            yamaa_benchmarks.project_ratio = printing
            try:
                status, stdout, stderr = run(
                    ["--format", "json", "run", "project.yaml", "--spec", "TEST"]
                )
            finally:
                yamaa_benchmarks.project_ratio = original
                os.chdir(previous)
        self.assertEqual(
            (status, json.loads(stdout)["status"]), (0, "succeeded"), stdout
        )
        self.assertEqual(set(stderr.splitlines()), {"from project_ratio"})

    def test_verbose_progress_reaches_standard_error_as_it_happens(self):
        with tempfile.TemporaryDirectory(prefix="yamaa-cli-") as directory:
            case = Path(directory).resolve() / "schema-functions"
            shutil.copytree(STUDIES / "schema-functions", case)
            status, stdout, stderr = module(
                ["check", "project.yaml", "--verbose"], cwd=case
            )
            self.assertEqual(status, 0, stderr)
            # The summary names the path as given.
            self.assertEqual(
                stdout, "yamaa check of project.yaml: succeeded, 0 issues\n"
            )
            lines = stderr.splitlines()
            self.assertEqual(lines[0], "TEST: preparation start")
            self.assertEqual(lines[1], "TEST: preparation end: succeeded")
            self.assertEqual(lines[2], "name\tphase\tstarted_ms\telapsed_ms\toutcome")


if __name__ == "__main__":
    unittest.main()
