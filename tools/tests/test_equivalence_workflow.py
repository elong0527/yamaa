"""Exercise the workflow's path decision against real Git histories."""

import os
import subprocess
import tempfile
import unittest
from pathlib import Path

import yaml
from tools.paths import ROOT
from tools.validation._repository_yaml import UniqueKeyLoader


class EquivalenceWorkflowTests(unittest.TestCase):
    def decide(self, path, *, event="pull_request", base_path=None, merge=False):
        workflow = yaml.load(
            (ROOT / ".github/workflows/ci.yml").read_text(encoding="utf-8"),
            Loader=UniqueKeyLoader,
        )
        [step] = [
            step
            for step in workflow["jobs"]["equivalence"]["steps"]
            if step.get("id") == "paths"
        ]
        with tempfile.TemporaryDirectory() as directory:
            work = Path(directory)

            def git(*arguments):
                return subprocess.check_output(
                    ["git", "-c", "user.name=Test", "-c", "user.email=test@example.org"]
                    + list(arguments),
                    cwd=work,
                    text=True,
                    stderr=subprocess.PIPE,
                ).strip()

            def commit(changed):
                file = work / changed
                file.parent.mkdir(parents=True, exist_ok=True)
                file.write_text("changed\n", encoding="utf-8")
                git("add", "--", changed)
                git("commit", "--quiet", "-m", "Change a path")
                return git("rev-parse", "HEAD")

            git("init", "--quiet")
            git("commit", "--quiet", "--allow-empty", "-m", "Base")
            base = git("rev-parse", "HEAD")
            head = commit(path)
            if base_path:
                git("checkout", "--quiet", "--detach", base)
                base = commit(base_path)
            checkout = head
            if merge:
                checkout = git(
                    "commit-tree",
                    git("rev-parse", f"{head}^{{tree}}"),
                    "-p",
                    base,
                    "-p",
                    head,
                    "-m",
                    "Synthetic merge",
                )
            git("checkout", "--quiet", "--detach", checkout)
            output = work / "github-output"
            result = subprocess.run(
                [
                    "bash",
                    "--noprofile",
                    "--norc",
                    "-e",
                    "-o",
                    "pipefail",
                    "-c",
                    step["run"],
                ],
                cwd=work,
                env=os.environ
                | {
                    "EVENT_NAME": event,
                    "BASE_SHA": base,
                    "HEAD_SHA": head,
                    "GITHUB_OUTPUT": str(output),
                },
                capture_output=True,
                text=True,
                check=True,
            )
            return output.read_text(encoding="utf-8").strip(), result.stdout

    def test_engine_and_gate_inputs_run_equivalence(self):
        for path in (
            "rust/crates/example/src/lib.rs",
            "yaml/schema.yaml",
            "benchmarks/example/expected/data.csv",
            "python/src/yamaa/__init__.py",
            "python/tests/installed/example.py",
            "python/pyproject.toml",
            "python/uv.lock",
            "R/yamaa/DESCRIPTION",
            "tools/migrate/example.py",
            "tools/conformance/example.py",
            "tools/commands/equivalence.py",
            "tools/commands/r_tests.py",
            "tools/commands/check_r.py",
            "tools/commands/python_tests.py",
            "tools/commands/setup.py",
            "tools/staging/stage_r_package.py",
            "tools/staging/engine_bundle.py",
            "tools/packaging/check_cargo_lock.py",
            "tools/ci/toolchain.py",
            "tools/benchmark.py",
            "tools/paths.py",
            "pyproject.toml",
            "Makefile",
            ".gitattributes",
            ".github/workflows/ci.yml",
            ".github/actions/setup/action.yml",
        ):
            with self.subTest(path=path):
                self.assertEqual(self.decide(path)[0], "run=true")

    def test_docs_only_diff_skips_with_a_notice(self):
        decision, notice = self.decide("CONTRIBUTING.md")
        self.assertEqual(decision, "run=false")
        self.assertIn("::notice::", notice)
        self.assertIn("equivalence gate did not run", notice)

    def test_changes_only_on_the_base_do_not_trigger_the_gate(self):
        self.assertEqual(
            self.decide("CONTRIBUTING.md", base_path="rust/src/lib.rs")[0],
            "run=false",
        )

    def test_dispatch_runs_even_without_engine_changes(self):
        self.assertEqual(
            self.decide("CONTRIBUTING.md", event="workflow_dispatch")[0], "run=true"
        )

    def test_a_synthetic_merge_cannot_pass_as_exact_head_evidence(self):
        with self.assertRaises(subprocess.CalledProcessError) as stopped:
            self.decide("python/pyproject.toml", merge=True)
        self.assertIn("::error::", stopped.exception.stdout)
        self.assertIn("pull-request head", stopped.exception.stdout)


if __name__ == "__main__":
    unittest.main()
