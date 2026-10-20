"""The command line's conformance runner skeleton (#3060), over stand-in binaries."""

import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

from tools.conformance import public_conformance as judge
from tools.paths import ROOT

RUNNER = ROOT / "rust/tests/public_conformance_cli.py"
SPEC = """schema_version: "1.0"
domain: TEST
keys: [ID]
input: {SRC: input.csv}
output: {path: result.csv, columns: [ID]}
columns:
  - {name: ID, type: int, derivation: SRC.ID}
"""
METADATA = "lifecycle: draft\nstandard: TEST\ndomain: TEST\n"
# Prints what it was asked and where, then refuses `run` as the binary does
# today.
NOT_IMPLEMENTED = """import json, os, sys
arguments = sys.argv[1:]
if arguments == ["--version"]:
    sys.stdout.write("yamaa 0.0.0\\n")
    sys.exit(0)
sys.stdout.write(json.dumps({"arguments": arguments, "cwd": os.getcwd()}))
sys.stderr.write("yamaa: run is not implemented yet\\n")
sys.exit(2)
"""
# Refuses every command as a usage error, which is not a refusal of a command
# not implemented yet.
USAGE = """import sys
if sys.argv[1:] == ["--version"]:
    sys.stdout.write("yamaa 0.0.0\\n")
    sys.exit(0)
sys.stderr.write("yamaa: run: missing PATH\\nrun 'yamaa --help' for usage\\n")
sys.exit(2)
"""


@unittest.skipIf(os.name == "nt", "a stand-in binary is a script run by its #! line")
class CommandLineRunner(unittest.TestCase):
    def setUp(self):
        temp = tempfile.TemporaryDirectory()
        self.addCleanup(temp.cleanup)
        self.root = Path(temp.name).resolve()
        self.fixtures = self.root / "fixtures"
        plain = self.fixtures / "plain"
        (plain / "expected").mkdir(parents=True)
        (plain / "expected/result.csv").write_text("ID\n1\n")
        functions = self.fixtures / "functions"
        functions.mkdir()
        (functions / "project.yaml").write_text(
            'schema_version: "1.0"\nenvironments:\n  python: python/environment.yaml\n'
        )
        for case in (plain, functions):
            (case / "spec.yaml").write_text(SPEC)
            (case / "benchmark.yaml").write_text(METADATA)
        self.plan = self.root / "plan"
        judge.write_json(self.plan / "plan.json", judge.plan(self.fixtures))

    def binary(self, name, program):
        path = self.root / name
        path.write_text(f"#!{sys.executable}\n{program}")
        path.chmod(0o755)
        return path

    def runner(self, binary, output):
        return subprocess.run(
            [
                sys.executable,
                "-I",
                str(RUNNER),
                "--plan",
                str(self.plan),
                "--fixtures",
                str(self.fixtures),
                "--binary",
                str(binary),
                "--output",
                str(output),
                "--source-revision",
                "test",
            ],
            capture_output=True,
            text=True,
            check=False,
        )

    @staticmethod
    def summary(result):
        return json.loads(result.stdout[result.stdout.index("{") :])

    @staticmethod
    def record(output, name):
        return json.loads((output / "records" / f"{name}.json").read_text())

    def test_each_call_runs_in_its_staged_case_and_each_exclusion_is_named(self):
        output = self.root / "observed"
        result = self.runner(self.binary("yamaa", NOT_IMPLEMENTED), output)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(
            self.summary(result),
            {
                "runtime": "cli",
                "excluded": 1,
                "not_implemented": 1,
                "observed": 0,
                "error": 0,
            },
        )
        run = json.loads((output / "run.json").read_text())
        self.assertEqual((run["runtime"], run["version"]), ("cli", "yamaa 0.0.0\n"))
        # The project-function benchmark is recorded with its reason, not
        # skipped silently.
        self.assertEqual(
            self.record(output, "functions"),
            {
                "example": "functions",
                "excluded": judge.CLI_FUNCTION_EXCLUSION,
                "calls": [],
                "error": None,
            },
        )
        [call] = self.record(output, "plain")["calls"]
        case = output / "work/plain"
        asked = json.loads(call["stdout"])
        self.assertEqual(asked["arguments"], ["run", "spec.yaml"])
        self.assertEqual(Path(asked["cwd"]).resolve(), case)
        self.assertEqual((call["exit_status"], call["not_implemented"]), (2, True))
        # Expected truth never enters the staged case.
        self.assertTrue((case / "spec.yaml").is_file())
        self.assertFalse((case / "expected").exists())
        self.assertFalse((case / "benchmark.yaml").exists())

    def test_a_usage_error_is_not_mistaken_for_a_command_not_implemented(self):
        output = self.root / "observed"
        result = self.runner(self.binary("usage", USAGE), output)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.summary(result)["observed"], 1)
        [call] = self.record(output, "plain")["calls"]
        self.assertEqual((call["exit_status"], call["not_implemented"]), (2, False))

    def test_the_output_must_be_fresh(self):
        output = self.root / "observed"
        output.mkdir()
        result = self.runner(self.binary("yamaa", NOT_IMPLEMENTED), output)
        self.assertEqual(result.returncode, 2)
        self.assertIn("output must be a fresh directory", result.stderr)


if __name__ == "__main__":
    unittest.main()
