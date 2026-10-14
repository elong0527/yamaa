"""Check sampled PR and complete main/issue qualification coverage."""

import fnmatch
import itertools
import json
import os
import re
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

import native_matrix


class NativeCoverage(unittest.TestCase):
    def test_actions_outputs_are_valid_matrices_and_plain_runner_names(self):
        for event in ("pull_request", "push", "workflow_dispatch"):
            with tempfile.TemporaryDirectory() as directory:
                output = Path(directory) / "output"
                summary = Path(directory) / "summary"
                environment = dict(
                    os.environ,
                    GITHUB_EVENT_NAME=event,
                    GITHUB_REF="refs/heads/main",
                    GITHUB_RUN_ID="38146574656",
                    GITHUB_OUTPUT=str(output),
                    GITHUB_STEP_SUMMARY=str(summary),
                )
                subprocess.run(
                    [sys.executable, native_matrix.__file__, "plan"],
                    env=environment,
                    check=True,
                )
                values = dict(
                    line.split("=", 1) for line in output.read_text().splitlines()
                )
                self.assertIn(values["compare-os"], native_matrix.PLATFORMS)
                self.assertEqual(values["full"], str(event != "pull_request").lower())
                self.assertEqual(values["run-rust"], str(event == "push").lower())
                self.assertEqual(
                    len(json.loads(values["qualify"])["include"]),
                    4 if event == "pull_request" else 18,
                )
                self.assertIn(
                    "PR native coverage" if event == "pull_request" else "Full",
                    summary.read_text(),
                )

    def test_main_keeps_every_supported_route(self):
        coverage = native_matrix.plan("push")
        routes = {
            (row["os"], row["python"], row["form"])
            for row in coverage["qualify"]["include"]
        }
        self.assertEqual(
            routes,
            set(
                itertools.product(
                    native_matrix.PLATFORMS, native_matrix.PYTHONS, native_matrix.FORMS
                )
            ),
        )
        self.assertEqual(len(coverage["build"]["include"]), 3)
        self.assertEqual(coverage["r"]["os"], list(native_matrix.PLATFORMS))
        self.assertTrue(coverage["full"])
        self.assertTrue(coverage["run-rust"])

    def test_dispatch_qualifies_the_same_installed_matrix_before_merge(self):
        dispatch = native_matrix.plan("workflow_dispatch")
        main = native_matrix.plan("push")
        self.assertEqual(
            {key: value for key, value in dispatch.items() if key != "run-rust"},
            {key: value for key, value in main.items() if key != "run-rust"},
        )
        self.assertFalse(dispatch["run-rust"])

    def test_explicit_qualification_push_keeps_every_main_check(self):
        main = native_matrix.plan("push", "refs/heads/main")
        branch = native_matrix.plan("push", "refs/heads/qualification/issue-1996")
        self.assertEqual(branch, main)
        self.assertTrue(branch["run-rust"])
        for reference in (
            "refs/heads/feature/example",
            "refs/tags/qualification/issue-1996",
        ):
            with self.subTest(reference=reference), self.assertRaises(ValueError):
                native_matrix.plan("push", reference)

    def test_workflow_push_trigger_and_full_job_guards_agree_with_plan(self):
        workflow = (Path(__file__).parents[2] / "workflows/native.yml").read_text()
        # Match the workflow's explicitly selected push branch patterns:
        # main and selected issue branches must qualify, ordinary branches do not.
        branches = re.search(r"branches: \[(.*?)\]", workflow).group(1)
        patterns = [value.strip().strip("'\"") for value in branches.split(",")]
        for branch, selected in (
            ("main", True),
            ("qualification/issue-1996", True),
            ("qualification/issue-2005-predicates", True),
            ("feature/example", False),
        ):
            with self.subTest(branch=branch):
                self.assertEqual(
                    any(fnmatch.fnmatchcase(branch, p) for p in patterns), selected
                )
                if selected:
                    self.assertTrue(
                        native_matrix.plan("push", "refs/heads/" + branch)["run-rust"]
                    )
        self.assertNotIn("outputs.run-rust == 'true' && github.ref", workflow)
        self.assertIn(
            "continue-on-error: ${{ github.ref == 'refs/heads/main' }}", workflow
        )

    def test_pr_checks_both_installation_forms_on_linux_and_python_endpoints(self):
        coverage = native_matrix.plan("pull_request")
        self.assertEqual(
            {
                (row["os"], row["python"], row["form"])
                for row in coverage["qualify"]["include"]
            },
            {
                ("ubuntu-24.04", "3.12", "wheel"),
                ("ubuntu-24.04", "3.12", "source"),
                ("ubuntu-24.04", "3.14", "wheel"),
                ("ubuntu-24.04", "3.14", "source"),
            },
        )
        self.assertEqual(len(coverage["qualify"]["include"]), 4)
        self.assertEqual(
            coverage["build"]["include"],
            [{"os": "ubuntu-24.04", "python": "3.14"}],
        )
        self.assertEqual(coverage["compare-os"], "ubuntu-24.04")
        self.assertFalse(coverage["full"])
        self.assertEqual(coverage["r"]["os"], ["ubuntu-24.04"])
        self.assertTrue(coverage["run-r"])
        self.assertFalse(coverage["run-rust"])

    def test_comparison_uses_only_selected_evidence_and_requires_r_on_every_os(self):
        for platform in native_matrix.PLATFORMS:
            matrix = {"include": [{"os": platform, "python": "3.13", "form": "source"}]}
            with patch("native_matrix.subprocess.run") as run:
                native_matrix.compare_cohorts(matrix, Path("/repo"), "revision")
            self.assertEqual(run.call_count, 2)
            for call in run.call_args_list:
                command = call.args[0]
                self.assertIn("cohort-input/source", command)
                self.assertIn("--r-evidence", command)
                self.assertIn("cohort-input/r-source", command)
                self.assertTrue(call.kwargs["check"])

    def test_comparison_reads_single_and_multiple_artifact_download_layouts(self):
        matrices = (
            {"include": [{"os": "ubuntu-24.04", "python": "3.13", "form": "source"}]},
            {
                "include": [
                    {"os": "ubuntu-24.04", "python": "3.13", "form": form}
                    for form in native_matrix.FORMS
                ]
            },
            native_matrix.plan("push")["qualify"],
        )
        probe = """import argparse
import json
from pathlib import Path

parser = argparse.ArgumentParser()
for option in ("fixtures", "python-evidence", "r-evidence", "output"):
    parser.add_argument("--" + option, type=Path, required=True)
parser.add_argument("--source-revision", required=True)
args = parser.parse_args()
python = json.loads((args.python_evidence / "supplemental/supplemental.json").read_text())
r = json.loads((args.r_evidence / "supplemental.json").read_text())
assert python["revision"] == r["revision"] == args.source_revision
args.output.mkdir(parents=True)
(args.output / "observed.json").write_text(json.dumps([python, r]))
"""
        for matrix in matrices:
            with (
                self.subTest(matrix=matrix),
                tempfile.TemporaryDirectory() as directory,
            ):
                root = Path(directory).resolve()
                tools = root / "repo/rust/tools"
                tools.mkdir(parents=True)
                for tool in ("qualify_public_cohort.py", "qualify_producer_cohort.py"):
                    (tools / tool).write_text(probe)
                downloads = root / "cohort-input"
                # Match the action's ZIP extraction: one match is flattened;
                # multiple matches each retain their artifact-name directory.
                python_archives = {
                    f"native-qualification-{row['os']}-{row['python']}-{row['form']}": row
                    for row in matrix["include"]
                }
                for name, row in python_archives.items():
                    extracted = (
                        downloads if len(python_archives) == 1 else downloads / name
                    )
                    supplemental = extracted / row["form"] / "supplemental"
                    supplemental.mkdir(parents=True)
                    (supplemental / "supplemental.json").write_text(
                        json.dumps({"revision": "revision", "route": row})
                    )
                platforms = {row["os"] for row in matrix["include"]}
                for platform in platforms:
                    extracted = (
                        downloads
                        if len(platforms) == 1
                        else downloads / f"native-r-qualification-{platform}"
                    )
                    supplemental = extracted / "r-source"
                    supplemental.mkdir(parents=True)
                    (supplemental / "supplemental.json").write_text(
                        json.dumps({"revision": "revision", "os": platform})
                    )
                previous = Path.cwd()
                try:
                    os.chdir(root)
                    native_matrix.compare_cohorts(matrix, root / "repo", "revision")
                finally:
                    os.chdir(previous)
                observations = list((root / "public-cohort").rglob("observed.json"))
                self.assertEqual(len(observations), 2 * len(matrix["include"]))
                for observation in observations:
                    python, r = json.loads(observation.read_text())
                    self.assertEqual(python["route"]["os"], r["os"])

    def test_failed_cohort_comparison_fails_the_job(self):
        matrix = native_matrix.plan("pull_request")["qualify"]
        with (
            patch(
                "native_matrix.subprocess.run",
                side_effect=subprocess.CalledProcessError(1, "gate"),
            ),
            self.assertRaises(subprocess.CalledProcessError),
        ):
            native_matrix.compare_cohorts(matrix, Path("/repo"), "revision")

    def test_unknown_event_cannot_silently_reduce_coverage(self):
        with self.assertRaises(ValueError):
            native_matrix.plan("schedule")


if __name__ == "__main__":
    unittest.main()
