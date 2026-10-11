"""Check that PR sampling retains its dependencies and main retains all coverage."""

import itertools
import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

import native_matrix


class NativeCoverage(unittest.TestCase):
    def test_actions_outputs_are_valid_matrices_and_plain_runner_names(self):
        for event in ("pull_request", "push"):
            with tempfile.TemporaryDirectory() as directory:
                output = Path(directory) / "output"
                summary = Path(directory) / "summary"
                environment = dict(
                    os.environ,
                    GITHUB_EVENT_NAME=event,
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
                self.assertEqual(values["full"], str(event == "push").lower())
                self.assertEqual(
                    len(json.loads(values["qualify"])["include"]),
                    18 if event == "push" else 1,
                )
                self.assertIn(
                    "Full" if event == "push" else "PR sample", summary.read_text()
                )

    def test_main_keeps_every_supported_route(self):
        coverage = native_matrix.plan("push", "1")
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
        self.assertEqual(coverage["r"]["os"], list(native_matrix.R_PLATFORMS))
        self.assertTrue(coverage["full"])

    def test_new_pr_runs_sample_all_routes_and_retries_keep_the_choice(self):
        observed = set()
        for run_id in range(1000):
            coverage = native_matrix.plan("pull_request", str(run_id))
            self.assertEqual(coverage, native_matrix.plan("pull_request", str(run_id)))
            self.assertEqual(len(coverage["qualify"]["include"]), 1)
            selected = coverage["qualify"]["include"][0]
            observed.add(tuple(selected.values()))
            self.assertEqual(
                coverage["build"]["include"], [{"os": selected["os"], "python": "3.14"}]
            )
            self.assertEqual(coverage["compare-os"], selected["os"])
            self.assertFalse(coverage["full"])
            expected_r = [] if selected["os"] == "windows-latest" else [selected["os"]]
            self.assertEqual(coverage["r"]["os"], expected_r)
            self.assertEqual(coverage["run-r"], bool(expected_r))
        self.assertEqual(len(observed), 18)

    def test_comparison_uses_only_selected_evidence_and_requires_r_on_unix(self):
        for platform in native_matrix.PLATFORMS:
            matrix = {"include": [{"os": platform, "python": "3.13", "form": "source"}]}
            with patch("native_matrix.subprocess.run") as run:
                native_matrix.compare_cohorts(matrix, Path("/repo"), "revision")
            self.assertEqual(run.call_count, 2)
            for call in run.call_args_list:
                command = call.args[0]
                self.assertIn(
                    f"cohort-input/native-qualification-{platform}-3.13-source/source",
                    command,
                )
                self.assertEqual(
                    "--r-evidence" in command, platform != "windows-latest"
                )
                self.assertTrue(call.kwargs["check"])

    def test_failed_cohort_comparison_fails_the_job(self):
        matrix = native_matrix.plan("pull_request", "1")["qualify"]
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
            native_matrix.plan("workflow_dispatch", "1")


if __name__ == "__main__":
    unittest.main()
