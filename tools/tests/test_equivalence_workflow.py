"""Exercise the workflow's gate jobs: their path decisions against real Git
histories, and the check that reports the equivalence gate's host jobs.

The equivalence host jobs and the conversion job decide by the same diff
whether a pull request runs their gate: every change but documentation runs
both (D124 on #3030). Each job holds its own copy of the step, so every case
runs on both. The equivalence gate runs one job per host, in parallel, and
the `equivalence` job passes only when both pass (#3386).
"""

import os
import re
import subprocess
import tempfile
import unittest
from pathlib import Path

import yaml
from tools.paths import ROOT
from tools.validation._repository_yaml import UniqueKeyLoader

# Changes to the engine, the hosts, the benchmarks, the repository tools, the
# build or either gate's own inputs: each runs both gates.
GATE_INPUTS = (
    "rust/crates/example/src/lib.rs",
    "rust/crates/yamaa-core/src/front_end/v1/spec/checks.rs",
    "rust/crates/yamaa-core/src/specification/phases.rs",
    "yaml/schema.yaml",
    "yaml/v1/schema_spec.yaml",
    "benchmarks/example/spec.yaml",
    "benchmarks/example/expected/data.csv",
    "python/src/yamaa/__init__.py",
    "python/tests/installed/example.py",
    "python/tests/installed/conversion_define.py",
    "python/tests/installed/public_conformance.py",
    "python/pyproject.toml",
    "python/uv.lock",
    "python/README.md",
    "rust/README.md",
    "R/yamaa/DESCRIPTION",
    "tools/migrate/example.py",
    "tools/migrate/excepted.py",
    "tools/conformance/example.py",
    "tools/conformance/public_conformance.py",
    "tools/surface.py",
    "tools/commands/equivalence.py",
    "tools/commands/conversion.py",
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
)
# Changes confined to documentation: neither gate runs.
DOCUMENTATION = (
    "README.md",
    "CONTRIBUTING.md",
    "AGENTS.md",
    "docs/release.md",
    "docs/articles/api.md",
)
# The events that run the equivalence gate; scheduled runs leave it out.
EVENTS = (
    "github.event_name == 'pull_request' || github.event_name == 'workflow_dispatch'"
)


def workflow():
    return yaml.load(
        (ROOT / ".github/workflows/ci.yml").read_text(encoding="utf-8"),
        Loader=UniqueKeyLoader,
    )


def paths_step(job):
    """The step that decides whether a job's gate runs."""
    [step] = [
        step for step in workflow()["jobs"][job]["steps"] if step.get("id") == "paths"
    ]
    return step


def diff_command(job):
    """The `git diff` whose output decides whether a job's gate runs."""
    [command] = re.findall(
        r"^changed=\$\(git diff .*?\)$",
        paths_step(job)["run"],
        flags=re.MULTILINE | re.DOTALL,
    )
    return command


class PathDecision:
    """The cases each gate's path decision passes. A subclass names its job,
    the gate its messages name, and what its decision outputs beside `run`,
    on a pull request and on a dispatch."""

    job = ""
    gate = ""
    pull_request_outputs = {}
    dispatch_outputs = {}

    def decide(
        self,
        path,
        *,
        event="pull_request",
        base_path=None,
        merge=False,
        rename_from=None,
    ):
        step = paths_step(self.job)
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
            lines = output.read_text(encoding="utf-8").splitlines()
            return dict(line.split("=", 1) for line in lines), result.stdout

    def on_pull_request(self, run):
        """The whole decision for a pull request whose gate runs or not."""
        return {"run": run} | self.pull_request_outputs

    def test_engine_and_gate_inputs_run_the_gate(self):
        for path in GATE_INPUTS:
            with self.subTest(path=path):
                self.assertEqual(self.decide(path)[0], self.on_pull_request("true"))

    def test_docs_only_diff_skips_with_a_notice(self):
        for path in DOCUMENTATION:
            with self.subTest(path=path):
                decision, notice = self.decide(path)
                self.assertEqual(decision, self.on_pull_request("false"))
                self.assertIn("::notice::", notice)
                self.assertIn(
                    "The diff changes only documentation, so the "
                    f"{self.gate} gate did not run.",
                    notice,
                )

    def test_changes_only_on_the_base_do_not_trigger_the_gate(self):
        self.assertEqual(
            self.decide("CONTRIBUTING.md", base_path="rust/src/lib.rs")[0],
            self.on_pull_request("false"),
        )

    def test_mixed_documentation_and_build_changes_run_the_gate(self):
        self.assertEqual(
            self.decide(("docs/release.md", ".cargo/config.toml"))[0],
            self.on_pull_request("true"),
        )

    def test_moving_code_into_documentation_still_runs_the_gate(self):
        self.assertEqual(
            self.decide("docs/former-code.md", rename_from="tools/gate.py")[0],
            self.on_pull_request("true"),
        )

    def test_dispatch_runs_even_without_engine_changes(self):
        self.assertEqual(
            self.decide("CONTRIBUTING.md", event="workflow_dispatch")[0],
            {"run": "true"} | self.dispatch_outputs,
        )

    def test_a_synthetic_merge_cannot_pass_as_exact_head_evidence(self):
        with self.assertRaises(subprocess.CalledProcessError) as stopped:
            self.decide("python/pyproject.toml", merge=True)
        self.assertIn("::error::", stopped.exception.stdout)
        self.assertIn(
            f"The {self.gate} gate must run on the pull-request head.",
            stopped.exception.stdout,
        )


class EquivalenceWorkflowTests(PathDecision, unittest.TestCase):
    """Both of the equivalence gate's host jobs run this one step."""

    job = "equivalence-host"
    gate = "equivalence"


class EquivalenceCheckTests(unittest.TestCase):
    """The equivalence gate runs its Python and R halves as parallel jobs,
    and the `equivalence` job reports them as the one check that branch
    protection and the merge tooling read (#3386)."""

    def test_each_host_runs_its_half_in_its_own_job(self):
        job = workflow()["jobs"]["equivalence-host"]
        self.assertEqual(job["name"], "equivalence (${{ matrix.host }})")
        self.assertEqual(
            job["strategy"], {"fail-fast": False, "matrix": {"host": ["python", "r"]}}
        )
        self.assertEqual(job["if"], EVENTS)
        [gate] = [
            step
            for step in job["steps"]
            if "make check-equivalence" in step.get("run", "")
        ]
        self.assertEqual(
            gate,
            {
                "if": "steps.paths.outputs.run == 'true'",
                "env": {"HOSTS": "${{ matrix.host }}"},
                "run": 'make check-equivalence HOSTS="$HOSTS"',
            },
        )

    def test_only_the_r_half_installs_what_cran_checks_use(self):
        [programs] = [
            step
            for step in workflow()["jobs"]["equivalence-host"]["steps"]
            if step.get("name") == "Install the programs that CRAN's checks use"
        ]
        self.assertEqual(
            programs["if"], "steps.paths.outputs.run == 'true' && matrix.host == 'r'"
        )

    def test_the_check_runs_wherever_its_host_jobs_run(self):
        """It waits for both host jobs, runs even when one fails or is
        cancelled, and is left out on the events that leave them out."""
        jobs = workflow()["jobs"]
        check = jobs["equivalence"]
        self.assertNotIn("name", check)
        self.assertEqual(check["needs"], "equivalence-host")
        self.assertEqual(check["if"], f"always() && ({EVENTS})")
        self.assertEqual(jobs["equivalence-host"]["if"], EVENTS)

    def test_the_check_passes_only_when_every_host_job_passes(self):
        [step] = workflow()["jobs"]["equivalence"]["steps"]
        self.assertEqual(
            step["env"], {"RESULT": "${{ needs.equivalence-host.result }}"}
        )
        for result, passes in (
            ("success", True),
            ("skipped", True),
            ("failure", False),
            ("cancelled", False),
        ):
            with self.subTest(result=result):
                completed = subprocess.run(
                    ["bash", "--noprofile", "--norc", "-e", "-o", "pipefail"]
                    + ["-c", step["run"]],
                    env=os.environ | {"RESULT": result},
                    capture_output=True,
                    text=True,
                )
                self.assertEqual(completed.returncode == 0, passes)
                self.assertEqual("::error::" in completed.stdout, not passes)


class ConversionWorkflowTests(PathDecision, unittest.TestCase):
    """The conversion job runs on Python for each pull request that runs the
    equivalence gate (D124), and on both hosts on dispatch or its weekly
    schedule."""

    job = "conversion"
    gate = "conversion"
    pull_request_outputs = {"hosts": "python"}
    dispatch_outputs = {"hosts": "python,r"}

    def test_the_two_gates_read_the_same_diff(self):
        self.assertEqual(diff_command("conversion"), diff_command("equivalence-host"))

    def test_the_weekly_schedule_runs_both_hosts(self):
        self.assertEqual(
            self.decide("README.md", event="schedule")[0],
            {"run": "true", "hosts": "python,r"},
        )

    def test_the_weekly_schedule_alone_runs_the_job(self):
        parsed = workflow()
        triggers = parsed.get("on", parsed.get(True))
        schedules = [entry["cron"] for entry in triggers["schedule"]]
        self.assertIn("23 4 * * 1", schedules)
        self.assertIn(
            "github.event.schedule == '23 4 * * 1'",
            parsed["jobs"]["conversion"]["if"],
        )
        for job in ("equivalence-host", "equivalence"):
            with self.subTest(job=job):
                self.assertNotIn("schedule", parsed["jobs"][job]["if"])


if __name__ == "__main__":
    unittest.main()
