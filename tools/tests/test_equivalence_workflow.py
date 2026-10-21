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
    def decide(
        self,
        path,
        *,
        event="pull_request",
        base_path=None,
        merge=False,
        rename_from=None,
        job="equivalence",
    ):
        workflow = yaml.load(
            (ROOT / ".github/workflows/ci.yml").read_text(encoding="utf-8"),
            Loader=UniqueKeyLoader,
        )
        [step] = [
            step for step in workflow["jobs"][job]["steps"] if step.get("id") == "paths"
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
            if rename_from:
                base = commit(rename_from)
                (work / path).parent.mkdir(parents=True, exist_ok=True)
                git("mv", rename_from, path)
            for changed in (path,) if isinstance(path, str) else path:
                head = commit(changed)
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
            "python/README.md",
            "rust/README.md",
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
            ".cargo/config.toml",
            ".cargo/config",
            "R/.gitattributes",
            "new-build-input/settings.toml",
            "docs/tools/generator.py",
            ".github/workflows/ci.yml",
            ".github/actions/setup/action.yml",
        ):
            with self.subTest(path=path):
                self.assertEqual(self.decide(path)[0], "run=true")

    def test_docs_only_diff_skips_with_a_notice(self):
        for path in (
            "README.md",
            "CONTRIBUTING.md",
            "AGENTS.md",
            "docs/release.md",
            "docs/articles/api.md",
        ):
            with self.subTest(path=path):
                decision, notice = self.decide(path)
                self.assertEqual(decision, "run=false")
                self.assertIn("::notice::", notice)
                self.assertIn("equivalence gate did not run", notice)

    def test_changes_only_on_the_base_do_not_trigger_the_gate(self):
        self.assertEqual(
            self.decide("CONTRIBUTING.md", base_path="rust/src/lib.rs")[0],
            "run=false",
        )

    def test_mixed_documentation_and_build_changes_run_the_gate(self):
        self.assertEqual(
            self.decide(("docs/release.md", ".cargo/config.toml"))[0], "run=true"
        )

    def test_moving_code_into_documentation_still_runs_the_gate(self):
        self.assertEqual(
            self.decide("docs/former-code.md", rename_from="tools/gate.py")[0],
            "run=true",
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


class ConversionWorkflowTests(unittest.TestCase):
    """The conversion job runs on Python for a pull request whose diff reaches
    the 1.0 front end, the converter, the 1.0 schemas or the gate, including
    the conformance planner and runner it drives, and on both hosts on
    dispatch or its weekly schedule."""

    def decide(self, path, **arguments):
        decision, notice = EquivalenceWorkflowTests.decide(
            self, path, job="conversion", **arguments
        )
        return dict(line.split("=", 1) for line in decision.splitlines()), notice

    def test_the_gate_s_inputs_run_it_on_python(self):
        for path in (
            "rust/crates/yamaa-core/src/front_end/v1/spec/checks.rs",
            "tools/migrate/excepted.py",
            "yaml/v1/schema_spec.yaml",
            "tools/commands/conversion.py",
            "tools/conformance/public_conformance.py",
            "tools/surface.py",
            "python/tests/installed/conversion_define.py",
            "python/tests/installed/public_conformance.py",
        ):
            with self.subTest(path=path):
                self.assertEqual(
                    self.decide(path)[0], {"run": "true", "hosts": "python"}
                )

    def test_other_changes_pass_with_a_notice(self):
        for path in (
            "rust/crates/yamaa-core/src/specification/mod.rs",
            "yaml/schema.yaml",
            "benchmarks/example/expected/data.csv",
            "README.md",
        ):
            with self.subTest(path=path):
                decision, notice = self.decide(path)
                self.assertEqual(decision, {"hosts": "python", "run": "false"})
                self.assertIn("conversion gate did not run", notice)

    def test_dispatch_and_the_schedule_run_both_hosts(self):
        for event in ("workflow_dispatch", "schedule"):
            with self.subTest(event=event):
                self.assertEqual(
                    self.decide("README.md", event=event)[0],
                    {"run": "true", "hosts": "python,r"},
                )

    def test_the_weekly_schedule_alone_runs_the_job(self):
        workflow = yaml.load(
            (ROOT / ".github/workflows/ci.yml").read_text(encoding="utf-8"),
            Loader=UniqueKeyLoader,
        )
        triggers = workflow.get("on", workflow.get(True))
        schedules = [entry["cron"] for entry in triggers["schedule"]]
        self.assertIn("23 4 * * 1", schedules)
        self.assertIn(
            "github.event.schedule == '23 4 * * 1'",
            workflow["jobs"]["conversion"]["if"],
        )
        self.assertNotIn("schedule", workflow["jobs"]["equivalence"]["if"])


if __name__ == "__main__":
    unittest.main()
