"""Exercise the workflow's gate jobs: their path decisions against real Git
histories, and the equivalence gate's host jobs.

The equivalence host jobs and the conversion job decide by the same diff
whether a pull request runs their gate: every change but documentation runs
both (D124 on #3030). Each job holds its own copy of the step, so every case
runs on both. The equivalence gate runs one job per host, in parallel
(#3386), and those two jobs are its checks: no job reports them as one
(#3492).
"""

import functools
import os
import re
import subprocess
import tempfile
import unittest
from pathlib import Path

from tools.paths import ROOT
from tools.tests import support

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


@functools.cache
def workflow():
    """The CI workflow, read once: the path decisions read their step from it
    for each case."""
    return support.workflow("ci.yml")


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


# Each commit of a case's history; the fixed time keeps the revisions of one
# history the same whichever case writes it.
COMMITTER = "committer yamaa test <test@example.invalid> 0 +0000"


def history_stream(paths, *, base_path=None, merge=False, rename_from=None):
    """A `git fast-import` stream that writes a case's history and checks out
    its last commit on main, then prints the base and the head revision.

    Each changed path holds "changed". The history starts at an empty commit;
    a renamed path is committed first and becomes the base; each path then
    takes a commit of its own. A base path is a commit on the base, beside the
    head, and a merge is a synthetic merge of the head into the base.
    """
    lines = ["feature get-mark"]
    marks = 0

    def commit(parents, tree, message):
        nonlocal marks
        marks += 1
        if not parents:
            lines.append("reset refs/heads/case")
        lines.extend(
            [
                "commit refs/heads/case",
                f"mark :{marks}",
                COMMITTER,
                f"data {len(message)}",
                message,
            ]
        )
        lines.extend(
            f"{'from' if index == 0 else 'merge'} {parent}"
            for index, parent in enumerate(parents)
        )
        lines.append("deleteall")
        for path in sorted(tree):
            lines.extend([f"M 100644 inline {path}", "data 8", "changed"])
        return f":{marks}"

    base = commit([], set(), "Base")
    base_tree = {rename_from} if rename_from else set()
    if rename_from:
        base = commit([base], base_tree, "Change a path")
    head, tree = base, set()
    for changed in paths:
        tree = tree | {changed}
        head = commit([head], tree, "Change a path")
    if base_path:
        base = commit([base], base_tree | {base_path}, "Change a path")
    checkout = commit([base, head], tree, "Synthetic merge") if merge else head
    lines.extend(["reset refs/heads/main", f"from {checkout}", "", f"get-mark {base}"])
    lines.append(f"get-mark {head}")
    return "\n".join(lines) + "\n"


class PathDecision:
    """The cases each gate's path decision passes. A subclass names its job,
    the gate its messages name, and what its decision outputs beside `run`,
    on a pull request and on a dispatch.

    The cases share one repository: each writes its history into it with one
    `git fast-import` and points main, the branch checked out, at the commit
    the step runs on. The step reads only commits and HEAD, so the work tree
    is never checked out."""

    job = ""
    gate = ""
    pull_request_outputs = {}
    dispatch_outputs = {}

    @classmethod
    def setUpClass(cls):
        super().setUpClass()
        directory = tempfile.TemporaryDirectory()
        cls.addClassCleanup(directory.cleanup)
        cls.work = support.init(Path(directory.name) / "work")
        cls.output = Path(directory.name) / "github-output"

    def decide(
        self,
        path,
        *,
        event="pull_request",
        base_path=None,
        merge=False,
        rename_from=None,
    ):
        stream = history_stream(
            (path,) if isinstance(path, str) else path,
            base_path=base_path,
            merge=merge,
            rename_from=rename_from,
        )
        base, head = subprocess.run(
            ["git", "-C", str(self.work), "fast-import", "--quiet", "--force"],
            input=stream,
            capture_output=True,
            text=True,
            check=True,
        ).stdout.split()
        self.output.write_text("", encoding="utf-8")
        result = subprocess.run(
            [
                "bash",
                "--noprofile",
                "--norc",
                "-e",
                "-o",
                "pipefail",
                "-c",
                paths_step(self.job)["run"],
            ],
            cwd=self.work,
            env=os.environ
            | {
                "EVENT_NAME": event,
                "BASE_SHA": base,
                "HEAD_SHA": head,
                "GITHUB_OUTPUT": str(self.output),
            },
            capture_output=True,
            text=True,
            check=True,
        )
        lines = self.output.read_text(encoding="utf-8").splitlines()
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
    """The equivalence gate runs its Python and R halves as parallel jobs
    (#3386), the legs `equivalence (python)` and `equivalence (r)`, and those
    two checks are the gate (#3492)."""

    def test_each_host_runs_its_half_in_its_own_job(self):
        job = workflow()["jobs"]["equivalence-host"]
        # No matrix expression: GitHub appends each leg's host to the name,
        # and a run that skips the job shows the plain name.
        self.assertEqual(job["name"], "equivalence")
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

    def test_no_job_waits_for_the_legs(self):
        """A job that reported the legs as one check would wait for a runner
        after both passed, and hold up a merge that they already prove
        (#3492)."""
        for name, job in workflow()["jobs"].items():
            needs = job.get("needs", [])
            with self.subTest(job=name):
                self.assertNotIn(
                    "equivalence-host", [needs] if isinstance(needs, str) else needs
                )


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
        self.assertNotIn("schedule", parsed["jobs"]["equivalence-host"]["if"])


if __name__ == "__main__":
    unittest.main()
