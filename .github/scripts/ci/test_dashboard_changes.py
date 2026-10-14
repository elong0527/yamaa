import json
import os
import subprocess
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

import dashboard_changes
from dashboard_changes import affects_dashboard


def git(root, *args):
    return subprocess.check_output(["git", "-C", str(root), *args], text=True).strip()


def repository(root):
    git(root, "init", "--quiet", "-b", "main")
    git(root, "config", "user.name", "CI fixture")
    git(root, "config", "user.email", "fixture@example.test")


def commit(root, path, text):
    target = root / path
    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_text(text)
    git(root, "add", ".")
    git(root, "commit", "--quiet", "-m", path)
    return git(root, "rev-parse", "HEAD")


def select(root, name, event=None):
    output = root / "output"
    output.unlink(missing_ok=True)
    environment = {"GITHUB_EVENT_NAME": name, "GITHUB_OUTPUT": str(output)}
    if event is not None:
        (root / "event.json").write_text(json.dumps(event))
        environment["GITHUB_EVENT_PATH"] = str(root / "event.json")
    with patch.dict(os.environ, environment):
        if event is None:
            os.environ.pop("GITHUB_EVENT_PATH", None)
        cwd = os.getcwd()
        try:
            os.chdir(root)
            dashboard_changes.main()
        finally:
            os.chdir(cwd)
    return output.read_text()


class DashboardChanges(unittest.TestCase):
    def test_dispatch_and_unknown_push_bases_check_every_dashboard(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            repository(root)
            commit(root, "docs/index.md", "prose\n")
            for name, event in (
                ("workflow_dispatch", None),
                ("push", None),
                ("push", {"before": "0" * 40}),
                ("push", {"before": "1" * 40}),
            ):
                with self.subTest(name=name, event=event):
                    self.assertEqual(select(root, name, event), "dashboards=true\n")

    def test_main_push_checks_dashboards_only_when_their_sources_changed(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            repository(root)
            before = commit(root, "benchmarks/case/input/data.csv", "original\n")
            commit(root, "docs/index.md", "prose\n")
            self.assertEqual(
                select(root, "push", {"before": before}), "dashboards=false\n"
            )
            commit(root, "benchmarks/case/input/data.csv", "changed\n")
            self.assertEqual(
                select(root, "push", {"before": before}), "dashboards=true\n"
            )

    def test_source_changes_and_deletions_need_preview(self):
        for path in (
            "benchmarks/schema-functions/input/data.csv",
            "benchmarks/schema-functions/README.md",
            "evaluations/harbor/prompts/schema-functions/full.md",
            "evaluations/harbor/solutions/schema-functions/result.R",
            ".github/scripts/benchmark-docs/generate.py",
            ".github/scripts/ci/dashboard_changes.py",
            ".github/scripts/commands/docs.py",
            "Makefile",
            ".github/workflows/yaml-validation.yml",
            "docs/articles/benchmark.md",
            "docs/assets/giscus-yamaa.css",
            "mkdocs.yml",
            "rust/tools/public_conformance.py",
        ):
            self.assertTrue(affects_dashboard(path), path)

    def test_main_push_checks_dashboard_imports_and_authored_site_contracts(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            repository(root)
            commit(root, "README.md", "prose\n")
            for path in (
                "rust/tools/public_conformance.py",
                "mkdocs.yml",
                "docs/assets/giscus-yamaa.css",
                "docs/articles/benchmark.md",
            ):
                with self.subTest(path=path):
                    before = git(root, "rev-parse", "HEAD")
                    commit(root, path, "dashboard dependency\n")
                    self.assertEqual(
                        select(root, "push", {"before": before}), "dashboards=true\n"
                    )

    def test_runtime_and_unrelated_docs_do_not_need_preview(self):
        for path in ("rust/Cargo.toml", "python/src/yamaa/style.py", "docs/index.md"):
            self.assertFalse(affects_dashboard(path), path)

    def test_renaming_a_source_out_of_the_dashboard_still_needs_preview(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            repository(root)
            base = commit(root, "benchmarks/case/input/data.csv", "original\n")
            git(root, "checkout", "--quiet", "-b", "pr")
            (root / "docs").mkdir()
            git(root, "mv", "benchmarks/case/input/data.csv", "docs/moved.csv")
            git(root, "commit", "--quiet", "-m", "rename source out of benchmarks")
            event = {"pull_request": {"base": {"sha": base}, "head": {"sha": "head"}}}
            self.assertEqual(select(root, "pull_request", event), "dashboards=true\n")


if __name__ == "__main__":
    unittest.main()
