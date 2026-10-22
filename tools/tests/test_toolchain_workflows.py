"""The lanes that build with a toolchain other than the pinned one, and the
weekly lanes that Main health reports.

Cargo's guide to continuous integration asks a project that declares a
rust-version to verify it in CI, to test with the toolchains beside stable,
and to keep testing the latest dependencies. The workspace rust-version is
the floor that the source packages, CRAN and embedders build with, and every
other job builds with the pinned toolchain, so CI's minimum-toolchain lint is
the check that fails when a change needs a newer Rust. The Next toolchain
workflow tests beta, and Dependency graphs resolves the newest graphs, weekly
on main, where Main health reports a failing run.
"""

import unittest

import yaml
from tools.commands import rust_tests
from tools.paths import ROOT
from tools.validation._repository_yaml import UniqueKeyLoader

WORKFLOWS = ROOT / ".github/workflows"
ACTIVATE_MINIMUM = "python -m tools.ci.toolchain --minimum --install"
# The repository lint's Clippy command over the whole workspace, which each
# lane runs with RUSTFLAGS=-D warnings, so that rustc's warnings fail it too.
CLIPPY = (
    "cargo clippy --locked --manifest-path rust/Cargo.toml --workspace "
    "--all-targets --all-features -- -D warnings"
)
# The same command without the R binding, whose build script runs R. CI's
# job runs it, as the repository lint does, because only the jobs that check
# the R host install R (test_host_bindings).
CLIPPY_BUT_R = CLIPPY.replace("--workspace ", "--workspace --exclude yamaa-r ")
# Workflows that run on main on a schedule without an issue from Main health:
# the README's Release checks badge shows the weekly native run.
BADGED = {"Native qualification"}


def workflow(name):
    return yaml.load(
        (WORKFLOWS / name).read_text(encoding="utf-8"), Loader=UniqueKeyLoader
    )


def triggers(parsed):
    return parsed.get("on", parsed.get(True))


def commands(job):
    """The command lines of a job's run steps, in order."""
    return [
        line.strip()
        for step in job["steps"]
        for line in step.get("run", "").splitlines()
        if line.strip()
    ]


def setup_inputs(job):
    [setup] = [
        step for step in job["steps"] if step.get("uses") == "./.github/actions/setup"
    ]
    return setup["with"]


class MinimumToolchainJobs(unittest.TestCase):
    def test_the_rust_lane_lints_with_the_minimum_after_its_pinned_tests(self):
        job = workflow("ci.yml")["jobs"]["rust"]
        self.assertNotIn("if", job)
        self.assertNotIn("needs", job)
        self.assertIn("timeout-minutes", job)
        run = commands(job)
        self.assertLess(run.index("make test-rust"), run.index(ACTIVATE_MINIMUM))
        self.assertLess(run.index(ACTIVATE_MINIMUM), run.index(CLIPPY_BUT_R))
        [activate] = [
            step for step in job["steps"] if step.get("run") == ACTIVATE_MINIMUM
        ]
        self.assertIn("steps.paths.outputs.minimum == 'true'", activate["if"])
        self.assertIn("!cancelled()", activate["if"])
        [lint] = [step for step in job["steps"] if step.get("run") == CLIPPY_BUT_R]
        self.assertEqual(lint["env"], {"RUSTFLAGS": "-D warnings"})
        self.assertNotIn("cargo update", "\n".join(run))

    def test_release_compatibility_repeats_the_lint_with_the_minimum(self):
        # The release lane adds the embedder workspace and the minimal format
        # builds; its Clippy command stays the pull-request job's.
        job = workflow("release-compatibility.yml")["jobs"]["rust-minimum"]
        run = commands(job)
        self.assertLess(run.index(ACTIVATE_MINIMUM), run.index(CLIPPY))
        [lint] = [step for step in job["steps"] if CLIPPY in step.get("run", "")]
        self.assertEqual(lint["env"], {"RUSTFLAGS": "-D warnings"})

    def test_the_ci_job_installs_no_r_and_the_release_lane_installs_r(self):
        # No step imports the Python package, so neither job builds the
        # development package. CI's job leaves the R binding out and installs
        # no R; the release lane lints the binding too, whose extendr build
        # script runs R.
        ci = workflow("ci.yml")["jobs"]["rust"]
        self.assertEqual(setup_inputs(ci), {"package": "tools"})
        release = workflow("release-compatibility.yml")["jobs"]["rust-minimum"]
        self.assertEqual(setup_inputs(release), {"r": "runtime", "package": "false"})


class NextToolchainWorkflow(unittest.TestCase):
    def test_beta_runs_weekly_on_main_and_on_dispatch_alone(self):
        parsed = workflow("next-toolchain.yml")
        self.assertEqual(parsed["name"], "Next toolchain")
        self.assertEqual(set(triggers(parsed)), {"schedule", "workflow_dispatch"})
        self.assertEqual(list(parsed["jobs"]), ["beta"])

    def test_beta_lints_and_tests_the_workspace_with_the_committed_lock(self):
        job = workflow("next-toolchain.yml")["jobs"]["beta"]
        self.assertIn("timeout-minutes", job)
        self.assertEqual(setup_inputs(job), {"r": "runtime", "package": "false"})
        run = commands(job)
        activate = "python -m tools.ci.toolchain --channel beta --install"
        self.assertEqual(run[0], activate)
        self.assertLess(run.index(activate), run.index(CLIPPY))
        self.assertLess(
            run.index(CLIPPY), run.index("python -m tools.commands.rust_tests")
        )
        # The workspace tests leave the bindings out and keep the lock.
        self.assertIn("--locked", rust_tests.command())
        for text in ("cargo update", "cargo fmt", "cargo test"):
            self.assertNotIn(text, "\n".join(run))


class DependencyGraphsSchedule(unittest.TestCase):
    def test_every_lane_runs_weekly_on_main_and_on_dispatch(self):
        parsed = workflow("dependency-graphs.yml")
        self.assertEqual(set(triggers(parsed)), {"schedule", "workflow_dispatch"})
        for name, job in parsed["jobs"].items():
            with self.subTest(job=name):
                self.assertNotIn("if", job)

    def test_the_weekly_run_and_a_dispatch_do_not_cancel_each_other(self):
        parsed = workflow("dependency-graphs.yml")
        self.assertTrue(parsed["concurrency"]["cancel-in-progress"])
        self.assertIn("${{ github.event_name }}", parsed["concurrency"]["group"])


class MainHealthReports(unittest.TestCase):
    def test_every_scheduled_workflow_but_the_badged_one_is_reported(self):
        reported = triggers(workflow("main-health.yml"))["workflow_run"]["workflows"]
        scheduled = {
            workflow(path.name)["name"]
            for path in sorted(WORKFLOWS.iterdir())
            if path.suffix in (".yml", ".yaml")
            and "schedule" in triggers(workflow(path.name))
        }
        self.assertEqual(BADGED & scheduled, BADGED)
        # Docs runs on each merge rather than on a schedule.
        self.assertEqual(set(reported), (scheduled - BADGED) | {"Docs"})
        self.assertEqual(reported, sorted(reported))


if __name__ == "__main__":
    unittest.main()
