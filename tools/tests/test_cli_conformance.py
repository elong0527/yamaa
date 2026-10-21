"""The command line's conformance runner (#3060), over stand-in binaries."""

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
# Behaves as the binary does today: refuses `run SPEC` as not implemented,
# and for `run PROJECT --spec NAME` prints one result document, here naming
# what it was asked and where, and exits 1 as a run with findings does. Its
# save wrote, as absolute paths with forward slashes, one file below the case
# and one beside it; the paths that climb out of the case or have a backslash
# are not ones the binary prints.
BINARY = """import json, os, sys
arguments = sys.argv[1:]
if arguments == ["--version"]:
    sys.stdout.write("yamaa 0.0.0\\n")
    sys.exit(0)
if "--spec" not in arguments:
    sys.stderr.write("yamaa: run without --spec or --section is not implemented yet\\n")
    sys.exit(2)
case = os.getcwd().replace(os.sep, "/")
paths = [
    case + "/out/test-report.csv",
    case + "-beside.csv",
    case + "/../escape.csv",
    "out\\\\test.csv",
]
document = {
    "arguments": arguments,
    "cwd": os.getcwd(),
    "issues": [],
    "paths": paths,
    "status": "failed",
}
sys.stdout.write(json.dumps(document) + "\\n")
sys.exit(1)
"""
# Runs every command and prints JSON that is no result document.
NULL = """import sys
if sys.argv[1:] == ["--version"]:
    sys.stdout.write("yamaa 0.0.0\\n")
    sys.exit(0)
sys.stdout.write("null\\n")
sys.exit(0)
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
# Runs every command but prints text, not the JSON document asked for.
TEXT = """import sys
if sys.argv[1:] == ["--version"]:
    sys.stdout.write("yamaa 0.0.0\\n")
    sys.exit(0)
sys.stdout.write("yamaa run of TEST: succeeded, 1 row x 1 column, 0 issues\\n")
sys.exit(0)
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
        project = self.fixtures / "project"
        project.mkdir()
        (project / "project.yaml").write_text(
            'schema_version: "1.0"\nsdtm:\n  specs: [spec.yaml]\n'
        )
        functions = self.fixtures / "functions"
        functions.mkdir()
        (functions / "project.yaml").write_text(
            'schema_version: "1.0"\nenvironments:\n  python: python/environment.yaml\n'
        )
        for case in (plain, project, functions):
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
        result = self.runner(self.binary("yamaa", BINARY), output)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(
            self.summary(result),
            {
                "runtime": "cli",
                "excluded": 1,
                "not_implemented": 1,
                "observed": 1,
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
        # A call with a project runs in its staged case and asks for JSON,
        # whose document the record keeps in place of the standard output.
        [call] = self.record(output, "project")["calls"]
        case = output / "work/project"
        document = call["document"]
        self.assertEqual(
            document["arguments"],
            ["--format", "json", "run", "project.yaml", "--spec", "TEST"],
        )
        self.assertEqual(Path(document["cwd"]).resolve(), case)
        self.assertEqual(document["status"], "failed")
        self.assertEqual((call["exit_status"], call["not_implemented"]), (1, False))
        self.assertNotIn("stdout", call)
        # The record keeps a written path relative to the case's root (#3312);
        # one beside the case, one that climbs out of it, and one with a
        # backslash stay as printed, for the comparator to refuse.
        printed = Path(document["cwd"]).as_posix()
        self.assertEqual(
            document["paths"],
            [
                "out/test-report.csv",
                printed + "-beside.csv",
                printed + "/../escape.csv",
                "out\\test.csv",
            ],
        )
        # Expected truth never enters the staged case.
        self.assertTrue((case / "spec.yaml").is_file())
        self.assertFalse((case / "expected").exists())
        self.assertFalse((case / "benchmark.yaml").exists())
        # A call the binary does not run yet is recorded as such.
        [call] = self.record(output, "plain")["calls"]
        self.assertEqual(call["arguments"], ["run", "spec.yaml"])
        self.assertEqual(
            (call["exit_status"], call["not_implemented"], call["document"]),
            (2, True, None),
        )
        self.assertFalse((output / "work/plain/expected").exists())

    def test_a_usage_error_is_neither_a_command_not_implemented_nor_an_observation(
        self,
    ):
        output = self.root / "observed"
        result = self.runner(self.binary("usage", USAGE), output)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(
            {key: self.summary(result)[key] for key in ("observed", "error")},
            {"observed": 0, "error": 2},
        )
        record = self.record(output, "plain")
        self.assertEqual(
            record["error"],
            "ValueError: yamaa exited with status 2: yamaa: run: missing PATH\n"
            "run 'yamaa --help' for usage\n",
        )

    def test_a_run_must_print_one_result_document(self):
        output = self.root / "observed"
        result = self.runner(self.binary("text", TEXT), output)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.summary(result)["error"], 2)
        self.assertTrue(
            self.record(output, "project")["error"].startswith("JSONDecodeError: ")
        )
        output = self.root / "observed-null"
        result = self.runner(self.binary("null", NULL), output)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(
            {key: self.summary(result)[key] for key in ("observed", "error")},
            {"observed": 0, "error": 2},
        )
        self.assertEqual(
            self.record(output, "project")["error"],
            "ValueError: yamaa printed no result with a status, issues and paths",
        )

    def test_the_output_must_be_fresh(self):
        output = self.root / "observed"
        output.mkdir()
        result = self.runner(self.binary("yamaa", BINARY), output)
        self.assertEqual(result.returncode, 2)
        self.assertIn("output must be a fresh directory", result.stderr)


if __name__ == "__main__":
    unittest.main()
