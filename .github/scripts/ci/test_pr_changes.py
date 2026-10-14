"""Keep PR routing conservative without compiling runtimes for prose changes."""

import json
import os
import re
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

import pr_changes


class PRChanges(unittest.TestCase):
    def test_only_the_fast_workflow_runs_on_pull_requests(self):
        workflows = Path(__file__).resolve().parents[2] / "workflows"
        triggered = {
            path.name
            for path in workflows.glob("*.yml")
            if re.search(r"(?m)^  pull_request:\s*$", path.read_text())
        }
        self.assertEqual(triggered, {"pr.yml"})

    def test_prose_only_skips_both_runtime_checks(self):
        paths = [
            "README.md",
            "docs/index.md",
            "rules/execution/lookup.md",
            "rust/README.md",
            "rust/planning/plan.md",
            "rust/measurements/run.json",
            "python/README.md",
            "R/yamaa/README.md",
            "benchmarks/example/README.md",
            ".github/ci.md",
        ]
        self.assertEqual(pr_changes.coverage(paths), {"rust": False, "python": False})

    def test_shared_runtime_and_ci_changes_check_both(self):
        for path in (
            "rust/crates/yamaa-core/src/lib.rs",
            "rust/Cargo.lock",
            "rust/crates/yamaa-adapters/tests/fixtures/source.md",
            "rust/tests/project-programs/python/pyproject.toml",
            "yaml/schema.yaml",
            "benchmarks/example/expected/result.csv",
            ".github/workflows/native.yml",
            ".github/scripts/ci/pr_changes.py",
            ".github/scripts/commands/rust_tests.py",
            "Makefile",
        ):
            with self.subTest(path=path):
                self.assertEqual(
                    pr_changes.coverage([path]), {"rust": True, "python": True}
                )

    def test_facade_and_harness_changes_check_python(self):
        for path in (
            "python/src/yamaa/_domain.py",
            "python/uv.lock",
            "python/yamaa_build_backend.py",
            "R/yamaa/R/domain.R",
            "R/yamaa/src/entrypoint.c",
            "evaluations/harbor/build.py",
            ".github/scripts/yaml-validation/editorial.py",
        ):
            with self.subTest(path=path):
                self.assertEqual(
                    pr_changes.coverage([path]), {"rust": False, "python": True}
                )

    def test_real_diff_includes_deleted_and_renamed_runtime_files(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)

            def git(*args):
                return subprocess.check_output(
                    ["git", *args], cwd=root, text=True
                ).strip()

            git("init", "-q", "-b", "main")
            git("config", "user.email", "ci@example.invalid")
            git("config", "user.name", "CI fixture")
            (root / "python").mkdir()
            (root / "python/old.py").write_text("pass\n")
            (root / "python/deleted.py").write_text("pass\n")
            git("add", ".")
            git("commit", "-qm", "base")
            git("checkout", "-qb", "feature")
            (root / "python/old.py").rename(root / "notes.md")
            (root / "python/deleted.py").unlink()
            git("add", "-A")
            git("commit", "-qm", "change")
            git("checkout", "-q", "main")
            (root / "README.md").write_text("unrelated main change\n")
            git("add", "README.md")
            git("commit", "-qm", "advance main")
            base = git("rev-parse", "HEAD")
            git("merge", "--no-ff", "-qm", "PR merge", "feature")
            checkout = root / "checkout"
            git("clone", "-q", "--depth=2", root.as_uri(), str(checkout))
            event = root / "event.json"
            event.write_text(json.dumps({"pull_request": {"base": {"sha": base}}}))
            output = root / "output"
            summary = root / "summary"
            subprocess.run(
                [sys.executable, pr_changes.__file__],
                cwd=checkout,
                check=True,
                env=dict(
                    os.environ,
                    GITHUB_EVENT_NAME="pull_request",
                    GITHUB_EVENT_PATH=str(event),
                    GITHUB_OUTPUT=str(output),
                    GITHUB_STEP_SUMMARY=str(summary),
                ),
            )
            self.assertEqual(output.read_text(), "rust=false\npython=true\n")
            self.assertIn("Full installed/platform qualification", summary.read_text())

    def test_old_event_base_requires_complete_checkout_ancestry(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)

            def git(*args):
                return subprocess.check_output(
                    ["git", *args], cwd=root, text=True
                ).strip()

            git("init", "-q", "-b", "main")
            git("config", "user.email", "ci@example.invalid")
            git("config", "user.name", "CI fixture")
            (root / "README.md").write_text("base\n")
            git("add", "README.md")
            git("commit", "-qm", "event base")
            event_base = git("rev-parse", "HEAD")
            git("checkout", "-qb", "feature")
            (root / "notes.md").write_text("feature prose\n")
            git("add", "notes.md")
            git("commit", "-qm", "feature")
            git("checkout", "-q", "main")
            (root / "python").mkdir()
            (root / "python/changed.py").write_text("pass\n")
            git("add", "python/changed.py")
            git("commit", "-qm", "runtime change after event")
            (root / "README.md").write_text("later main prose\n")
            git("add", "README.md")
            git("commit", "-qm", "main advances before checkout")
            git("merge", "--no-ff", "-qm", "synthetic PR merge", "feature")

            event = root / "event.json"
            event.write_text(
                json.dumps({"pull_request": {"base": {"sha": event_base}}})
            )
            for depth in (2, None):
                with self.subTest(depth=depth):
                    checkout = root / f"checkout-{depth}"
                    clone_args = ["clone", "-q"]
                    if depth is not None:
                        clone_args.append(f"--depth={depth}")
                    git(*clone_args, root.as_uri(), str(checkout))
                    output = root / f"output-{depth}"
                    result = subprocess.run(
                        [sys.executable, pr_changes.__file__],
                        cwd=checkout,
                        check=False,
                        capture_output=True,
                        text=True,
                        env=dict(
                            os.environ,
                            GITHUB_EVENT_NAME="pull_request",
                            GITHUB_EVENT_PATH=str(event),
                            GITHUB_OUTPUT=str(output),
                            GITHUB_STEP_SUMMARY=str(root / f"summary-{depth}"),
                        ),
                    )
                    if depth == 2:
                        self.assertNotEqual(result.returncode, 0)
                        self.assertIn(event_base, result.stderr)
                        self.assertFalse(output.exists())
                    else:
                        self.assertEqual(result.returncode, 0, result.stderr)
                        # The event base includes the intervening runtime change;
                        # substituting the current merge parent would omit it.
                        self.assertEqual(
                            output.read_text(), "rust=false\npython=true\n"
                        )
                        unavailable = "f" * 40
                        event.write_text(
                            json.dumps({"pull_request": {"base": {"sha": unavailable}}})
                        )
                        output.unlink()
                        missing = subprocess.run(
                            [sys.executable, pr_changes.__file__],
                            cwd=checkout,
                            check=False,
                            capture_output=True,
                            text=True,
                            env=dict(
                                os.environ,
                                GITHUB_EVENT_NAME="pull_request",
                                GITHUB_EVENT_PATH=str(event),
                                GITHUB_OUTPUT=str(output),
                                GITHUB_STEP_SUMMARY=str(root / "missing-summary"),
                            ),
                        )
                        self.assertNotEqual(missing.returncode, 0)
                        self.assertIn(unavailable, missing.stderr)
                        self.assertFalse(output.exists())

    def test_smoke_jobs_fetch_event_base_ancestry(self):
        workflow = (
            Path(__file__).resolve().parents[2] / "workflows/pr.yml"
        ).read_text()
        for job in ("rust", "python"):
            with self.subTest(job=job):
                section = re.search(
                    rf"(?ms)^  {job}:\n(.*?)(?=^  \S|\Z)", workflow
                ).group(1)
                self.assertIn("fetch-depth: 0", section)

    def test_main_populates_both_caches_without_a_pr_event_file(self):
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / "output"
            summary = Path(directory) / "summary"
            subprocess.run(
                [sys.executable, pr_changes.__file__],
                check=True,
                env=dict(
                    os.environ,
                    GITHUB_EVENT_NAME="push",
                    GITHUB_OUTPUT=str(output),
                    GITHUB_STEP_SUMMARY=str(summary),
                ),
            )
            self.assertEqual(output.read_text(), "rust=true\npython=true\n")


if __name__ == "__main__":
    unittest.main()
