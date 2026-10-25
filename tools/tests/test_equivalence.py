"""Pin how the executor equivalence gate composes the existing owners."""

import contextlib
import tempfile
import unittest
from pathlib import Path
from unittest import mock

from tools.ci.toolchain import channel
from tools.commands import equivalence
from tools.paths import ROOT


class GateRun:
    """Replace every expensive step and record what the gate asked of it.

    Every run works in the same directory, so two runs' steps compare."""

    def __init__(self, statuses=(0, 0)):
        self.statuses = list(statuses)
        self.plans = []
        self.compared = []
        self.environment = {"R_LIBS": "fresh-library"}
        self.archive = Path("yamaa_0.5.0.tar.gz")

    def conformance(self, argv):
        if argv[0] == "plan":
            self.plans.append(argv)
            return 0
        self.compared.append(Path(argv[argv.index("--observed") + 1]).parent.name)
        return self.statuses.pop(0)

    def run(self, *argv):
        with (
            mock.patch.object(
                equivalence.tempfile,
                "TemporaryDirectory",
                return_value=contextlib.nullcontext("gate-work"),
            ),
            mock.patch.object(
                equivalence.public_conformance, "main", side_effect=self.conformance
            ),
            mock.patch.object(
                equivalence, "install_python", return_value=Path("venv/bin/python")
            ) as self.install_python,
            mock.patch.object(equivalence.subprocess, "run") as self.subprocess,
            mock.patch.object(
                equivalence.subprocess, "check_output", return_value="abc123\n"
            ),
            mock.patch.object(equivalence, "run_installed_suites") as self.suites,
            mock.patch.object(
                equivalence.r_tests,
                "install",
                return_value=(self.archive, self.environment),
            ) as self.install_r,
            mock.patch.object(
                equivalence.r_tests, "run_installed_suites"
            ) as self.r_suites,
            mock.patch.object(equivalence, "check") as self.check,
        ):
            equivalence.main(list(argv))

    def steps(self):
        """Each step that the run asked for after its plan, with its arguments."""
        return {
            "compared": self.compared,
            "commands": list(self.subprocess.call_args_list),
            "python installs": list(self.install_python.call_args_list),
            "python suites": list(self.suites.call_args_list),
            "r installs": list(self.install_r.call_args_list),
            "r suites": list(self.r_suites.call_args_list),
            "r checks": list(self.check.call_args_list),
        }


class EquivalenceGateTests(unittest.TestCase):
    def test_every_check_runs_on_the_fresh_installs(self):
        """Both hosts' conformance, the Python suites and R CMD check all run."""
        gate = GateRun()
        gate.run()
        self.assertEqual(len(gate.plans), 1)
        self.assertEqual(
            gate.plans[0][gate.plans[0].index("--fixtures") + 1],
            str(ROOT / "benchmarks"),
        )
        self.assertEqual(gate.compared, ["python", "r"])
        python_runner, project_install, test_r_runner, r_runner = (
            call.args[0] for call in gate.subprocess.call_args_list
        )
        self.assertEqual(
            python_runner[:3],
            [
                "venv/bin/python",
                "-I",
                str(ROOT / "python/tests/installed/public_conformance.py"),
            ],
        )
        self.assertEqual(
            python_runner[python_runner.index("--source-revision") + 1], "abc123"
        )
        self.assertEqual(
            project_install,
            [
                "R",
                "CMD",
                "INSTALL",
                "--library=fresh-library",
                str(ROOT / "rust/tests/project-programs/r"),
            ],
        )
        self.assertEqual(
            test_r_runner[2], str(ROOT / "rust/tests/test_public_conformance.R")
        )
        self.assertEqual(r_runner[2], str(ROOT / "rust/tests/public_conformance.R"))
        r_environment = gate.subprocess.call_args_list[3].kwargs["env"]
        self.assertEqual(r_environment["R_LIBS"], "fresh-library")
        self.assertEqual(r_environment["PUBLIC_CONFORMANCE_PACKAGE_FORM"], "source")
        self.assertEqual(gate.suites.call_args.args[0], Path("venv/bin/python"))
        self.assertEqual(gate.check.call_args.args[0], gate.archive)
        self.assertIs(gate.check.call_args.args[2], gate.environment)
        gate.r_suites.assert_called_once_with(
            Path("gate-work/r-suites"), gate.environment
        )

    def test_a_python_mismatch_fails_before_the_r_build(self):
        """A failing Python comparison stops the gate with a nonzero status."""
        gate = GateRun(statuses=(1,))
        with self.assertRaises(SystemExit) as stopped:
            gate.run()
        self.assertIn("Python", str(stopped.exception.code))
        gate.suites.assert_not_called()
        gate.install_r.assert_not_called()

    def test_an_r_mismatch_fails_the_gate(self):
        """A failing R comparison stops the gate before R CMD check."""
        gate = GateRun(statuses=(0, 1))
        with self.assertRaises(SystemExit) as stopped:
            gate.run()
        self.assertIn("R public", str(stopped.exception.code))
        gate.check.assert_not_called()

    def test_each_host_runs_its_half_alone(self):
        """`--hosts python` builds, compares and runs the suites of Python
        alone; `--hosts r` builds, compares and checks the R package alone."""
        python = GateRun(statuses=(0,))
        python.run("--hosts", "python")
        self.assertEqual(python.compared, ["python"])
        python.suites.assert_called_once()
        python.install_r.assert_not_called()
        python.check.assert_not_called()
        r = GateRun(statuses=(0,))
        r.run("--hosts", "r")
        self.assertEqual(r.compared, ["r"])
        r.install_python.assert_not_called()
        r.suites.assert_not_called()
        r.check.assert_called_once()

    def test_the_two_halves_are_the_whole_gate(self):
        """Run apart, as CI's two host jobs run them, the halves plan alike
        and ask for every step of the default run, and for nothing else."""
        whole = GateRun()
        whole.run()
        python = GateRun(statuses=(0,))
        python.run("--hosts", "python")
        r = GateRun(statuses=(0,))
        r.run("--hosts", "r")
        self.assertEqual(python.plans, whole.plans)
        self.assertEqual(r.plans, whole.plans)
        self.assertEqual(
            {name: python.steps()[name] + r.steps()[name] for name in whole.steps()},
            whole.steps(),
        )

    def test_a_failing_r_half_fails_alone(self):
        """The R half reports its own conformance failure."""
        gate = GateRun(statuses=(1,))
        with self.assertRaises(SystemExit) as stopped:
            gate.run("--hosts", "r")
        self.assertIn("R public", str(stopped.exception.code))
        gate.install_python.assert_not_called()
        gate.check.assert_not_called()

    def test_hosts_name_python_or_r(self):
        for hosts in ("julia", "python,julia", ","):
            with self.subTest(hosts=hosts), self.assertRaises(SystemExit):
                GateRun().run("--hosts", hosts)

    def test_python_installs_wheels_built_from_the_checkout(self):
        """The Python host is a new environment, never the development install."""
        with tempfile.TemporaryDirectory() as directory:
            work = Path(directory)
            (work / "dist").mkdir()
            for name in (
                "yamaa-0.5.0-cp312-abi3-linux_x86_64.whl",
                "yamaa_benchmarks-1.0.0-py3-none-any.whl",
            ):
                (work / "dist" / name).touch()
            with mock.patch.object(equivalence.subprocess, "run") as run:
                python = equivalence.install_python(work, "uv")
        commands = [call.args[0] for call in run.call_args_list]
        self.assertEqual(commands[0][:3], ("uv", "build", "python"))
        self.assertIn("build-args=--profile=dev", commands[0])
        self.assertEqual(
            commands[1][:3], ("uv", "build", "rust/tests/project-programs/python")
        )
        self.assertEqual(commands[2][:4], ("uv", "export", "--project", "python"))
        self.assertIn("--locked", commands[2])
        self.assertEqual(commands[3][:2], ("uv", "venv"))
        self.assertEqual(commands[4][:3], ("uv", "pip", "install"))
        self.assertIn("--no-deps", commands[4])
        self.assertEqual(
            [
                Path(argument).name
                for argument in commands[4]
                if argument.endswith(".whl")
            ],
            [
                "yamaa-0.5.0-cp312-abi3-linux_x86_64.whl",
                "yamaa_benchmarks-1.0.0-py3-none-any.whl",
            ],
        )
        self.assertTrue(python.is_relative_to(work / "venv"))
        for call in run.call_args_list:
            self.assertEqual(call.kwargs["env"]["RUSTUP_TOOLCHAIN"], channel())

    def test_the_python_lane_adds_locked_tooling_without_an_editable_host(self):
        with tempfile.TemporaryDirectory() as directory:
            with mock.patch.object(equivalence.subprocess, "run") as run:
                python = equivalence.install_python(Path(directory), "uv", tooling=True)
        commands = [call.args[0] for call in run.call_args_list]
        export = commands[2]
        self.assertIn("--locked", export)
        self.assertEqual(export[export.index("--group") + 1], "tooling")
        self.assertEqual(export[export.index("--no-emit-package") + 1], "yamaa-tools")
        self.assertEqual(
            commands[-1],
            (
                "uv",
                "pip",
                "install",
                "--python",
                str(python),
                "--no-deps",
                "-e",
                str(ROOT),
            ),
        )
        self.assertFalse(any("--editable" in command for command in commands[:-1]))


if __name__ == "__main__":
    unittest.main()
