"""Pin how the executor equivalence gate composes the existing owners."""

import tempfile
import unittest
from pathlib import Path
from unittest import mock

from tools.ci.toolchain import channel
from tools.commands import equivalence
from tools.paths import ROOT


class GateRun:
    """Replace every expensive step and record what the gate asked of it."""

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

    def run(self):
        with (
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
            mock.patch.object(equivalence, "check") as self.check,
        ):
            equivalence.main([])


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
        executors, python_runner, test_r_runner, r_runner = (
            call.args[0] for call in gate.subprocess.call_args_list
        )
        self.assertEqual(
            executors,
            [
                "cargo",
                "test",
                "--locked",
                "-p",
                "yamaa-adapters",
                "-p",
                "yamaa",
                "--features",
                "yamaa-core/executor-comparison",
            ],
        )
        comparison = gate.subprocess.call_args_list[0].kwargs
        self.assertEqual(comparison["env"]["YAMAA_EXECUTOR"], "compare")
        self.assertIs(comparison["check"], True)
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
            test_r_runner[2], str(ROOT / "rust/tests/test_public_conformance.R")
        )
        self.assertEqual(r_runner[2], str(ROOT / "rust/tests/public_conformance.R"))
        r_environment = gate.subprocess.call_args_list[3].kwargs["env"]
        self.assertEqual(r_environment["R_LIBS"], "fresh-library")
        self.assertEqual(r_environment["PUBLIC_CONFORMANCE_PACKAGE_FORM"], "source")
        self.assertEqual(gate.suites.call_args.args[0], Path("venv/bin/python"))
        self.assertEqual(gate.check.call_args.args[0], gate.archive)
        self.assertIs(gate.check.call_args.args[2], gate.environment)

    def test_a_differing_attempt_stops_the_gate_before_any_host(self):
        """The executor comparison runs first; its failure builds no host."""
        failure = equivalence.subprocess.CalledProcessError(1, ["cargo", "test"])
        with (
            mock.patch.object(
                equivalence.subprocess, "run", side_effect=failure
            ) as run,
            mock.patch.object(
                equivalence.subprocess, "check_output", return_value="abc123\n"
            ),
            mock.patch.object(equivalence.public_conformance, "main") as conformance,
            mock.patch.object(equivalence, "install_python") as install_python,
            self.assertRaises(equivalence.subprocess.CalledProcessError),
        ):
            equivalence.main([])
        self.assertEqual(run.call_args.args[0][:2], ["cargo", "test"])
        conformance.assert_not_called()
        install_python.assert_not_called()

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


if __name__ == "__main__":
    unittest.main()
