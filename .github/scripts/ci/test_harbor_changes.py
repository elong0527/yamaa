"""Keep PR Oracle coverage scoped without losing shared changes or renamed cases."""

import json
import os
import subprocess
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

import harbor_changes
from harbor_changes import changed_paths, select_benchmarks

AVAILABLE = ("adam-adsl-age-group", "adam-adtte-dor", "sdtm-dm-arms")


class HarborChanges(unittest.TestCase):
    def test_data_prompts_and_solutions_select_only_their_benchmarks(self):
        selection = select_benchmarks(
            "pull_request",
            [
                "benchmarks/adam-adtte-dor/input/rs.csv",
                "benchmarks/adam-adtte-dor/expected/adtte.csv",
                "evaluations/harbor/prompts/sdtm-dm-arms/brief.md",
                "evaluations/harbor/solutions/adam-adtte-dor/result.sas",
            ],
            AVAILABLE,
        )
        self.assertEqual(selection["benchmarks"], ["adam-adtte-dor", "sdtm-dm-arms"])
        self.assertFalse(selection["full"])

    def test_shared_runtime_and_dependency_changes_select_every_benchmark(self):
        for path in (
            "evaluations/harbor/build.py",
            "evaluations/harbor/grade.py",
            "evaluations/harbor/Dockerfile",
            "evaluations/harbor/system-r.md",
            "evaluations/harbor/leaderboards/system-r.md",
            "evaluations/harbor/licenses/opensas/LICENSE-APACHE-2.0.txt",
            "python/pyproject.toml",
            "python/uv.lock",
            ".github/workflows/harbor-oracle.yml",
            ".github/scripts/ci/harbor_changes.py",
        ):
            with self.subTest(path=path):
                selection = select_benchmarks("pull_request", [path], AVAILABLE)
                self.assertEqual(selection["benchmarks"], sorted(AVAILABLE))
                self.assertTrue(selection["full"])
                self.assertEqual(selection["shared"], [path])

    def test_docs_tests_and_non_harbor_benchmarks_do_not_run_docker_cases(self):
        selection = select_benchmarks(
            "pull_request",
            [
                "evaluations/harbor/README.md",
                "evaluations/harbor/prompts/README.md",
                "evaluations/harbor/solutions/opensas.md",
                "python/tests/test_harbor_preflight.py",
                "benchmarks/schema-functions/input/data.csv",
                "docs/index.md",
            ],
            AVAILABLE,
        )
        self.assertEqual(selection["benchmarks"], [])
        self.assertFalse(selection["full"])

    def test_removed_cases_are_excluded_from_current_available_benchmarks(self):
        selection = select_benchmarks(
            "pull_request", ["benchmarks/removed/expected/data.csv"], AVAILABLE
        )
        self.assertEqual(selection["benchmarks"], [])

    def test_schedule_and_dispatch_keep_full_coverage(self):
        for event in ("schedule", "workflow_dispatch"):
            selection = select_benchmarks(event, [], AVAILABLE)
            self.assertEqual(selection["benchmarks"], sorted(AVAILABLE))
            self.assertTrue(selection["full"])

    def test_unknown_event_cannot_silently_skip_coverage(self):
        with self.assertRaises(ValueError):
            select_benchmarks("push", [], AVAILABLE)

    def test_actions_outputs_and_saved_selection_match_the_coverage_scope(self):
        cases = (
            (
                "pull_request",
                ["benchmarks/adam-adtte-dor/input/rs.csv"],
                ["adam-adtte-dor"],
                False,
            ),
            ("pull_request", ["evaluations/harbor/README.md"], [], False),
            ("pull_request", ["evaluations/harbor/grade.py"], sorted(AVAILABLE), True),
            ("schedule", [], sorted(AVAILABLE), True),
        )
        for event_name, paths, names, full in cases:
            with (
                self.subTest(event=event_name, paths=paths),
                tempfile.TemporaryDirectory() as directory,
            ):
                root = Path(directory)
                for name in AVAILABLE:
                    (root / "benchmarks" / name).mkdir(parents=True)
                    prompt = root / "evaluations/harbor/prompts" / name / "full.md"
                    prompt.parent.mkdir(parents=True)
                    prompt.write_text("prompt\n")
                event = root / "event.json"
                event.write_text(
                    json.dumps(
                        {
                            "pull_request": {
                                "base": {"sha": "base"},
                                "head": {"sha": "head"},
                            }
                        }
                    )
                )
                manifest, output, summary = (
                    root / name for name in ("selected.json", "output", "summary")
                )
                with (
                    patch.object(harbor_changes, "ROOT", root),
                    patch.object(harbor_changes, "changed_paths", return_value=paths),
                    patch("sys.argv", ["harbor_changes.py", "--out", str(manifest)]),
                    patch.dict(
                        os.environ,
                        {
                            "GITHUB_EVENT_NAME": event_name,
                            "GITHUB_EVENT_PATH": str(event),
                            "GITHUB_OUTPUT": str(output),
                            "GITHUB_STEP_SUMMARY": str(summary),
                        },
                    ),
                ):
                    harbor_changes.main()
                self.assertEqual(json.loads(manifest.read_text()), names)
                self.assertEqual(
                    output.read_text(),
                    f"run={str(bool(names)).lower()}\nfull={str(full).lower()}\n",
                )
                if not names:
                    self.assertIn("Docker verification is skipped", summary.read_text())

    def test_git_selection_excludes_new_base_changes_and_keeps_renamed_cases(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)

            def git(*args):
                return subprocess.check_output(
                    ["git", "-C", str(root), *args], text=True
                ).strip()

            git("init", "--quiet", "-b", "main")
            git("config", "user.name", "CI fixture")
            git("config", "user.email", "fixture@example.test")
            old = root / "benchmarks/old-case/input/data.csv"
            old.parent.mkdir(parents=True)
            old.write_text("original\n")
            git("add", ".")
            git("commit", "--quiet", "-m", "base")
            git("checkout", "--quiet", "-b", "pr")
            new = root / "benchmarks/new-case/input/data.csv"
            new.parent.mkdir(parents=True)
            git("mv", str(old.relative_to(root)), str(new.relative_to(root)))
            git("commit", "--quiet", "-m", "rename")
            head = git("rev-parse", "HEAD")
            git("checkout", "--quiet", "main")
            (root / "evaluations").mkdir()
            (root / "evaluations/new-base-only.txt").write_text("unrelated\n")
            git("add", ".")
            git("commit", "--quiet", "-m", "advance base")
            paths = changed_paths(root, git("rev-parse", "HEAD"), head)
            self.assertEqual(
                set(paths),
                {
                    "benchmarks/old-case/input/data.csv",
                    "benchmarks/new-case/input/data.csv",
                },
            )


if __name__ == "__main__":
    unittest.main()
