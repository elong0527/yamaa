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

import yaml

ROOT = Path(__file__).resolve().parents[3]
COMMANDS = ROOT / ".github/scripts/commands"


class CommandEntryPoints(unittest.TestCase):
    def test_r_grammar_replay_installs_codec_in_its_selected_library_first(self):
        grammar = yaml.safe_load((ROOT / ".github/workflows/r.yml").read_text())
        native = yaml.safe_load((ROOT / ".github/workflows/native.yml").read_text())
        steps = grammar["jobs"]["installed-grammar"]["steps"]
        names = [step.get("name") for step in steps]
        codec_index = names.index("Install pinned R fixture JSON codec")
        replay_index = names.index(
            "Replay shared decisions through the installed public R facade"
        )
        self.assertLess(codec_index, replay_index)
        codec, replay = steps[codec_index], steps[replay_index]
        self.assertEqual(codec["env"]["R_LIBS"], replay["env"]["R_LIBS"])
        self.assertEqual(codec["env"]["R_LIBS"], "${{ runner.temp }}/grammar-r-library")
        native_codec = next(
            step
            for step in native["jobs"]["r"]["steps"]
            if step.get("name") == "Install pinned R fixture JSON codec"
        )
        self.assertEqual(codec["run"], native_codec["run"])
        self.assertIn('version <- "2.0.0"', codec["run"])
        self.assertIn('lib = Sys.getenv("R_LIBS")', codec["run"])
        self.assertIn(
            'stopifnot(as.character(utils::packageVersion("jsonlite")) == version)',
            codec["run"],
        )

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
                {s["staged_path"] for s in suites} | {"host-fixtures"},
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

    def test_altered_shared_truth_refuses_before_any_installed_suite_runs(self):
        spec = importlib.util.spec_from_file_location(
            "supplemental_inventory", ROOT / "rust/tools/supplemental_inventory.py"
        )
        inventory = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(inventory)
        catalog = inventory.load_catalog(
            ROOT, ROOT / "rust/qualification/supplemental.json"
        )
        with tempfile.TemporaryDirectory() as temporary:
            destination = Path(temporary) / "staged-r"
            staged = self.command(
                "installed.py", "stage", "--runtime", "r", "--staged-root", destination
            )
            self.assertEqual(staged.returncode, 0, staged.stderr)
            for original, relative in inventory.host_fixture_files(catalog):
                self.assertEqual(
                    (ROOT / original).read_bytes(),
                    (destination / relative).read_bytes(),
                )
            truth = destination / "host-fixtures/cases.json"
            # A valid JSON document with identical semantic values still has
            # different retained bytes and must fail before any child runs.
            truth.write_bytes(truth.read_bytes() + b" ")
            with (
                patch.object(
                    inventory,
                    "installed_metadata",
                    return_value={
                        "package_location": str(Path(temporary) / "installed")
                    },
                ),
                patch.object(inventory.subprocess, "run") as run,
                self.assertRaisesRegex(ValueError, "staged host fixture differs"),
            ):
                inventory.run_suites(
                    ROOT,
                    catalog,
                    runtime="r",
                    staged_root=destination,
                    output=Path(temporary) / "evidence",
                    evidence="owned-command-control",
                )
            run.assert_not_called()

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
        section = re.search(r"(?ms)^  push:\n(.*?)(?=^  \S|\Z)", native).group(1)
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
            with self.subTest(path=path):
                self.assertEqual(
                    pr_changes.coverage([path]), {"rust": True, "python": True}
                )
        deploy = (ROOT / ".github/workflows/deploy-docs.yml").read_text()
        self.assertIn("- '.github/scripts/commands/docs.py'", deploy)


if __name__ == "__main__":
    unittest.main()
