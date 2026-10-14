"""Exercise command boundaries that protect installed context and retained evidence."""

import ast
import importlib.util
import json
import os
import re
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

import pr_changes

ROOT = Path(__file__).resolve().parents[3]
COMMANDS = ROOT / ".github/scripts/commands"


class CommandEntryPoints(unittest.TestCase):
    def test_shared_rust_command_discovers_both_feature_gated_suites(self):
        spec = importlib.util.spec_from_file_location(
            "rust_test_commands", COMMANDS / "rust_tests.py"
        )
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        with patch.object(module.subprocess, "run") as run:
            module.main()
        commands = [call.args[0] for call in run.call_args_list]
        self.assertEqual(len(commands), 4)
        for command in commands:
            self.assertEqual(command[0], "cargo")
            self.assertEqual(command[2:4], ["test", "--locked"])
        for arguments in (
            ["-p", "yamaa-engine", "--features", "test-support"],
            [
                "-p",
                "yamaa-adapters",
                "--features",
                "arrow-c-data",
                "--test",
                "c_data_table",
            ],
        ):
            self.assertIn(arguments, [command[4:] for command in commands])

    def command(self, name, *arguments, **kwargs):
        return subprocess.run(
            [sys.executable, str(COMMANDS / name), *map(str, arguments)],
            text=True,
            capture_output=True,
            check=False,
            **kwargs,
        )

    def test_staging_uses_every_cataloged_suite_and_refuses_overwrite(self):
        catalog = json.loads(
            (ROOT / "rust/qualification/supplemental.json").read_text()
        )
        with tempfile.TemporaryDirectory() as temporary:
            destination = Path(temporary) / "staged-r"
            result = self.command(
                "installed.py", "stage", "--runtime", "r", "--staged-root", destination
            )
            self.assertEqual(result.returncode, 0, result.stderr)
            suites = [suite for suite in catalog["suites"] if suite["runtime"] == "r"]
            self.assertEqual(
                {p.name for p in destination.iterdir()},
                {s["staged_path"] for s in suites},
            )
            for suite in suites:
                self.assertEqual(
                    (destination / suite["staged_path"]).read_bytes(),
                    (ROOT / suite["path"]).read_bytes(),
                )
            first = destination / suites[0]["staged_path"]
            first.write_text("retained earlier bytes\n")
            repeated = self.command(
                "installed.py", "stage", "--runtime", "r", "--staged-root", destination
            )
            self.assertNotEqual(repeated.returncode, 0)
            self.assertEqual(first.read_text(), "retained earlier bytes\n")

    def test_installed_checks_refuse_checkout_and_missing_artifact_identity(self):
        checkout = self.command(
            "installed.py", "stage", "--runtime", "python", "--staged-root", ROOT
        )
        self.assertNotEqual(checkout.returncode, 0)
        self.assertIn("outside the checkout", checkout.stderr)
        with tempfile.TemporaryDirectory() as temporary:
            result = self.command(
                "installed.py",
                "run",
                "--runtime",
                "python",
                "--staged-root",
                temporary,
                "--output",
                Path(temporary) / "output",
                "--source-revision",
                "current",
                "--package-form",
                "wheel",
                "--artifact-reference",
                "wheel bytes",
                "--evidence",
                "local run",
            )
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("--host-artifact-reference", result.stderr)
            self.assertFalse((Path(temporary) / "output").exists())

    def test_r_check_requires_clean_status_even_when_r_exits_successfully(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            archive = root / "yamaa.tar.gz"
            archive.write_bytes(b"standalone archive control")
            executable = root / "R"
            executable.write_text(
                f"#!{sys.executable}\n"
                "import os\nfrom pathlib import Path\n"
                "Path('yamaa.Rcheck').mkdir()\n"
                "Path('yamaa.Rcheck/00check.log').write_text(os.environ['CONTROL_R_STATUS']+'\\n')\n"
            )
            executable.chmod(0o755)
            for status, passed in (("Status: OK", True), ("Status: 1 WARNING", False)):
                work = root / ("ok" if passed else "warning")
                result = self.command(
                    "check_r.py",
                    "--archive",
                    archive,
                    "--work-dir",
                    work,
                    env={
                        **os.environ,
                        "PATH": str(root) + os.pathsep + os.environ["PATH"],
                        "R_LIBS": str(root),
                        "CONTROL_R_STATUS": status,
                    },
                )
                self.assertEqual(result.returncode == 0, passed, result.stderr)
                self.assertEqual(
                    (work / "yamaa.Rcheck/00check.log").read_text(), status + "\n"
                )
                self.assertEqual(
                    (work / "source-revision.txt").read_text().strip(),
                    subprocess.check_output(
                        ["git", "-C", str(ROOT), "rev-parse", "HEAD"], text=True
                    ).strip(),
                )

    def test_command_changes_select_native_and_docs_workflows(self):
        native = (ROOT / ".github/workflows/native.yml").read_text()
        for event in ("push",):
            section = re.search(rf"(?ms)^  {event}:\n(.*?)(?=^  \S|\Z)", native).group(
                1
            )
            patterns = [
                ast.literal_eval(value)
                for value in re.findall(r"(?m)^      - (.+)$", section)
            ]
            self.assertIn(".github/scripts/commands/**", patterns)
            self.assertIn("Makefile", patterns)
        self.assertNotRegex(native, r"(?m)^  pull_request:")
        smoke = (ROOT / ".github/workflows/pr.yml").read_text()
        self.assertRegex(smoke, r"(?m)^  pull_request:")
        for path in (
            "Makefile",
            ".github/scripts/commands/rust_tests.py",
            ".github/scripts/commands/installed.py",
        ):
            self.assertEqual(
                pr_changes.coverage([path]), {"rust": True, "python": True}
            )
        deploy = (ROOT / ".github/workflows/deploy-docs.yml").read_text()
        self.assertIn("- '.github/scripts/commands/docs.py'", deploy)


if __name__ == "__main__":
    unittest.main()
