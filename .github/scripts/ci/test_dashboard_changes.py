import json
import os
import subprocess
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

import dashboard_changes
from dashboard_changes import affects_dashboard


class DashboardChanges(unittest.TestCase):
    def test_source_changes_and_deletions_need_preview(self):
        for path in (
            "benchmarks/schema-functions/input/data.csv",
            "benchmarks/schema-functions/README.md",
            "evaluations/harbor/prompts/schema-functions/full.md",
            "evaluations/harbor/solutions/schema-functions/result.R",
            ".github/scripts/benchmark-docs/generate.py",
            ".github/scripts/ci/dashboard_changes.py",
            ".github/workflows/yaml-validation.yml",
            "docs/articles/benchmark.md",
        ):
            self.assertTrue(affects_dashboard(path), path)

    def test_runtime_and_unrelated_docs_do_not_need_preview(self):
        for path in ("rust/Cargo.toml", "python/src/yamaa/style.py", "docs/index.md"):
            self.assertFalse(affects_dashboard(path), path)

    def test_renaming_a_source_out_of_the_dashboard_still_needs_preview(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)

            def git(*args):
                return subprocess.check_output(
                    ["git", "-C", str(root), *args], text=True
                ).strip()

            git("init", "--quiet", "-b", "main")
            git("config", "user.name", "CI fixture")
            git("config", "user.email", "fixture@example.test")
            source = root / "benchmarks/case/input/data.csv"
            source.parent.mkdir(parents=True)
            source.write_text("original\n")
            git("add", ".")
            git("commit", "--quiet", "-m", "base")
            base = git("rev-parse", "HEAD")
            git("checkout", "--quiet", "-b", "pr")
            moved = root / "docs/moved.csv"
            moved.parent.mkdir(parents=True)
            git("mv", "benchmarks/case/input/data.csv", "docs/moved.csv")
            git("commit", "--quiet", "-m", "rename source out of benchmarks")
            event = root / "event.json"
            event.write_text(
                json.dumps(
                    {"pull_request": {"base": {"sha": base}, "head": {"sha": "head"}}}
                )
            )
            output = root / "output"
            with (
                patch.dict(
                    os.environ,
                    {
                        "GITHUB_EVENT_NAME": "pull_request",
                        "GITHUB_EVENT_PATH": str(event),
                        "GITHUB_OUTPUT": str(output),
                    },
                ),
            ):
                cwd = os.getcwd()
                try:
                    os.chdir(root)
                    dashboard_changes.main()
                finally:
                    os.chdir(cwd)
            self.assertEqual(output.read_text(), "dashboards=true\n")


if __name__ == "__main__":
    unittest.main()
