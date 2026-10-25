"""Each host lane uses one fresh install and reports independent failures."""

import contextlib
import io
import subprocess
import tempfile
import unittest
from pathlib import Path
from unittest import mock

from tools.commands import host_checks


class HostChecks(unittest.TestCase):
    def setUp(self):
        self.stack = contextlib.ExitStack()
        self.addCleanup(self.stack.close)
        directory = self.stack.enter_context(tempfile.TemporaryDirectory())
        self.work = Path(directory)
        self.python = self.work / "fresh-venv/bin/python"
        self.archive = self.work / "yamaa.tar.gz"
        self.environment = {"R_LIBS": str(self.work / "fresh-r-library")}

        def replace(owner, name, **kwargs):
            return self.stack.enter_context(mock.patch.object(owner, name, **kwargs))

        self.plan = replace(host_checks.public_conformance, "main", return_value=0)
        replace(host_checks.subprocess, "check_output", return_value="head-revision\n")
        self.python_install = replace(
            host_checks.equivalence, "install_python", return_value=self.python
        )
        self.python_tests = replace(
            host_checks.python_tests, "run_tests", return_value=[]
        )
        self.python_equivalence = replace(
            host_checks.equivalence, "python_conformance", return_value=0
        )
        self.configure = replace(
            host_checks.r_tests, "test_configure", return_value=None
        )
        self.r_install = replace(
            host_checks.r_tests,
            "install",
            return_value=(self.archive, self.environment),
        )
        self.r_check = replace(host_checks.r_tests, "check_install", return_value=None)
        self.r_suites = replace(
            host_checks.r_tests, "run_installed_suites", return_value=None
        )
        self.r_equivalence = replace(
            host_checks.equivalence, "r_conformance", return_value=0
        )

    def test_python_tests_and_equivalence_judge_the_same_fresh_wheel(self):
        self.assertEqual(host_checks.run("python", self.work, "uv", True), [])
        self.python_install.assert_called_once_with(
            self.work / "install", "uv", tooling=True
        )
        self.python_tests.assert_called_once_with(self.python, self.work / "suites")
        self.python_equivalence.assert_called_once_with(
            self.work / "equivalence",
            self.python,
            self.work / "plan",
            "head-revision",
        )
        self.r_install.assert_not_called()

    def test_r_check_and_equivalence_judge_the_same_fresh_source_install(self):
        self.assertEqual(host_checks.run("r", self.work, "uv", True), [])
        self.configure.assert_called_once()
        self.r_install.assert_called_once_with(self.work / "install")
        self.r_check.assert_called_once_with(
            self.archive, self.environment, self.work / "check"
        )
        self.r_suites.assert_called_once_with(self.work / "suites", self.environment)
        self.r_equivalence.assert_called_once_with(
            self.work / "equivalence",
            self.environment,
            self.work / "plan",
            "head-revision",
        )
        self.python_install.assert_not_called()

    def test_ordinary_checks_without_equivalence_do_not_walk_the_benchmark_plan(self):
        for host in ("python", "r"):
            with self.subTest(host=host):
                self.assertEqual(host_checks.run(host, self.work, "uv", False), [])
        self.plan.assert_not_called()
        self.python_tests.assert_called_once()
        self.r_check.assert_called_once()
        self.r_suites.assert_called_once()
        self.python_equivalence.assert_not_called()
        self.r_equivalence.assert_not_called()

    def test_python_test_and_benchmark_failures_are_both_reported(self):
        self.python_tests.return_value = ["pytest"]
        self.python_equivalence.return_value = 1
        with contextlib.redirect_stdout(io.StringIO()):
            self.assertEqual(
                host_checks.run("python", self.work, "uv", True),
                ["Python tests", "Python equivalence"],
            )
        self.python_equivalence.assert_called_once()

    def test_r_benchmark_runs_even_when_configure_or_package_checks_fail(self):
        self.configure.side_effect = subprocess.CalledProcessError(1, ["pytest"])
        self.r_check.side_effect = RuntimeError("R check warning")
        self.r_equivalence.return_value = 1
        with contextlib.redirect_stdout(io.StringIO()):
            self.assertEqual(
                host_checks.run("r", self.work, "uv", True),
                ["R configure tests", "R package check", "R equivalence"],
            )
        self.r_equivalence.assert_called_once()

    def test_full_r_suite_failure_fails_ci_and_still_runs_the_cran_check_and_benchmarks(
        self,
    ):
        self.r_suites.side_effect = RuntimeError("an installed regression failed")
        with contextlib.redirect_stdout(io.StringIO()):
            self.assertEqual(
                host_checks.run("r", self.work, "uv", True),
                ["R installed suites"],
            )
        self.r_check.assert_called_once_with(
            self.archive, self.environment, self.work / "check"
        )
        self.r_equivalence.assert_called_once()

    def test_a_host_failure_exits_the_command_with_failure(self):
        self.python_tests.return_value = ["pytest"]
        with (
            contextlib.redirect_stdout(io.StringIO()),
            contextlib.redirect_stderr(io.StringIO()),
            self.assertRaises(SystemExit) as stopped,
        ):
            host_checks.main(["--host", "python"])
        self.assertEqual(stopped.exception.code, 1)

    def test_a_malformed_benchmark_plan_still_allows_the_ordinary_tests(self):
        self.plan.side_effect = ValueError("invalid benchmark metadata")
        for host, label in (("python", "Python"), ("r", "R")):
            with self.subTest(host=host), contextlib.redirect_stdout(io.StringIO()):
                self.assertEqual(
                    host_checks.run(host, self.work, "uv", True),
                    [f"{label} equivalence"],
                )
        self.python_tests.assert_called_once()
        self.r_check.assert_called_once()
        self.python_equivalence.assert_not_called()
        self.r_equivalence.assert_not_called()


if __name__ == "__main__":
    unittest.main()
