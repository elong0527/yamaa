"""The four required lanes carry qualification without duplicate host jobs,
and a fifth lane, not required, checks both hosts in the loaded mode.

Real-history path decisions live in test_pr_changes; host installation reuse
and failure propagation live in test_host_checks. These cases check that
the workflow connects that selection to its checks.
"""

import unittest

from tools.ci import pr_changes
from tools.paths import ROOT
from tools.tests import support


class RequiredLanes(unittest.TestCase):
    def setUp(self):
        self.jobs = support.workflow("ci.yml")["jobs"]

    def test_every_lane_selects_its_work_before_installing_tools(self):
        self.assertEqual(
            set(self.jobs), {"repository", "rust", "python", "r", "loaded"}
        )
        for name, job in self.jobs.items():
            with self.subTest(job=name):
                # A failed selection fails its required check; it cannot be
                # hidden by a skipped job or a separate reporting job.
                self.assertNotIn("if", job)
                self.assertNotIn("needs", job)
                self.assertNotIn("name", job)
                self.assertFalse(job.get("continue-on-error", False))
                steps = job["steps"]
                self.assertEqual(
                    steps[0]["with"],
                    {
                        "persist-credentials": False,
                        "fetch-depth": 0,
                        "ref": "${{ github.event.pull_request.head.sha || github.sha }}",
                    },
                )
                self.assertEqual(
                    steps[1], {"uses": "./.github/actions/select-checks", "id": "paths"}
                )
                for step in steps[2:]:
                    self.assertFalse(step.get("continue-on-error", False))
                    self.assertIn("if", step)

    def test_the_selection_action_exports_every_decision_field(self):
        action = support.load_yaml(".github/actions/select-checks/action.yml")
        self.assertEqual(set(action["outputs"]), set(pr_changes.decision("schedule")))
        [selection] = action["runs"]["steps"]
        self.assertIn('tools.ci.pr_changes >> "$GITHUB_OUTPUT"', selection["run"])
        self.assertEqual(
            selection["env"],
            {
                "EVENT_NAME": "${{ github.event_name }}",
                "BASE_SHA": "${{ github.event.pull_request.base.sha }}",
                "HEAD_SHA": "${{ github.event.pull_request.head.sha }}",
            },
        )

    def test_both_hosts_run_their_tests_and_equivalence_in_parallel_lanes(self):
        for host in ("python", "r"):
            with self.subTest(host=host):
                job = self.jobs[host]
                [check] = [
                    step
                    for step in job["steps"]
                    if "tools.commands.host_checks" in step.get("run", "")
                ]
                self.assertIn(f"arguments=(--host {host})", check["run"])
                self.assertIn("arguments+=(--equivalence)", check["run"])
                self.assertEqual(
                    check["env"]["EQUIVALENCE"],
                    "${{ steps.paths.outputs.equivalence_" + host + " }}",
                )
                self.assertIn(f"steps.paths.outputs.{host} == 'true'", check["if"])
                [setup] = [
                    step
                    for step in job["steps"]
                    if step.get("uses") == "./.github/actions/setup"
                ]
                self.assertEqual(setup["with"]["package"], "tools")

    def test_only_the_lanes_that_check_the_r_package_install_what_cran_checks_use(
        self,
    ):
        programs = [
            (name, step["if"])
            for name, job in self.jobs.items()
            for step in job["steps"]
            if "devscripts aspell" in step.get("run", "")
        ]
        self.assertEqual(
            programs,
            [
                ("r", "steps.paths.outputs.r == 'true'"),
                ("loaded", "steps.paths.outputs.loaded == 'true'"),
            ],
        )

    def test_the_loaded_lane_checks_both_hosts_with_the_engine_loaded(self):
        # Each host check, the R one even when the Python one failed, runs
        # with YAMAA_ENGINE=loaded and without the equivalence gate, which
        # the required lanes run.
        checks = [
            (step["run"], step["env"], step["if"])
            for step in self.jobs["loaded"]["steps"]
            if "tools.commands.host_checks" in step.get("run", "")
        ]
        command = (
            "uv run --project python --no-sync python -m tools.commands.host_checks"
        )
        self.assertEqual(
            checks,
            [
                (
                    f"{command} --host python",
                    {"YAMAA_ENGINE": "loaded"},
                    "steps.paths.outputs.loaded == 'true'",
                ),
                (
                    f"{command} --host r",
                    {
                        "YAMAA_ENGINE": "loaded",
                        "YAMAA_R_CARGO_TARGET_DIR": "${{ github.workspace }}/rust/target",
                    },
                    "steps.paths.outputs.loaded == 'true' && "
                    "(success() || steps.python.outcome == 'failure')",
                ),
            ],
        )

    def test_workflow_lint_runs_inside_the_repository_lane(self):
        [lint] = [
            step
            for step in self.jobs["repository"]["steps"]
            if "actionlint" in step.get("run", "")
        ]
        self.assertEqual(lint["if"], "steps.paths.outputs.workflow_lint == 'true'")
        self.assertFalse((ROOT / ".github/workflows/workflow-lint.yml").exists())

    def test_documentation_uses_its_own_environment_without_a_host_build(self):
        [docs] = [
            step
            for step in self.jobs["repository"]["steps"]
            if "make docs-test docs" in step.get("run", "")
        ]
        self.assertEqual(docs["if"], "steps.paths.outputs.repository == 'docs'")
        self.assertIn("--no-install-package yamaa", docs["run"])


if __name__ == "__main__":
    unittest.main()
