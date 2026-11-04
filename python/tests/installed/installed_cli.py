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
