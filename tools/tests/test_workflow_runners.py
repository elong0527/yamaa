"""Every workflow job runs on a fixed runner image.

GitHub moves a -latest label to a newer image on its own schedule, which can
change the operating system and its toolchains, such as MSVC on Windows,
between two runs of one commit. So no workflow names one, and each runner
label that a job's expressions compare its runner with, such as a longer
Windows timeout, is one of the job's lanes: moving a lane to another image
cannot leave such a comparison on the old label.
"""

import re
import unittest

import yaml
from tools.paths import ROOT
from tools.validation._repository_yaml import UniqueKeyLoader

FLOATING = re.compile(r"\b(?:ubuntu|windows|macos)-latest\b")
MATRIX_VALUE = re.compile(r"\$\{\{\s*matrix\.([A-Za-z0-9_.-]+)\s*\}\}")


def workflow_files():
    return sorted(
        path
        for path in (ROOT / ".github/workflows").iterdir()
        if path.suffix in (".yml", ".yaml")
    )


def strings(node):
    """Yield every string under a parsed YAML node."""
    if isinstance(node, str):
        yield node
    elif isinstance(node, dict):
        for value in node.values():
            yield from strings(value)
    elif isinstance(node, list):
        for value in node:
            yield from strings(value)


def unknown_lanes(job_id, job):
    """Return the runner labels that a job compares but does not run on.

    A job runs on one label or on a matrix value, such as matrix.os or
    matrix.config.os; anything else fails rather than being guessed.
    """
    runs_on = job["runs-on"]
    reference = MATRIX_VALUE.fullmatch(runs_on) if isinstance(runs_on, str) else None
    if reference is None:
        if isinstance(runs_on, str) and "${{" not in runs_on:
            return set()
        raise AssertionError(f"{job_id}: this test cannot read runs-on {runs_on!r}")
    path = reference.group(1).split(".")
    matrix = job.get("strategy", {}).get("matrix")
    if not isinstance(matrix, dict):
        raise AssertionError(f"{job_id}: this test reads only a written matrix")
    rows = [{path[0]: value} for value in matrix.get(path[0], [])]
    lanes = set()
    for row in rows + matrix.get("include", []):
        value = row
        for key in path:
            value = value.get(key) if isinstance(value, dict) else None
        if value is not None:
            lanes.add(value)
    comparison = re.compile(
        r"matrix\." + re.escape(reference.group(1)) + r"\s*[!=]=\s*'([^']*)'"
    )
    return {
        label for text in strings(job) for label in comparison.findall(text)
    } - lanes


class WorkflowRunners(unittest.TestCase):
    def test_no_workflow_or_action_names_a_floating_runner_image(self):
        files = workflow_files() + sorted(
            (ROOT / ".github/actions").glob("*/action.y*ml")
        )
        found = [
            f"{path.relative_to(ROOT)}:{number}: {line.strip()}"
            for path in files
            for number, line in enumerate(
                path.read_text(encoding="utf-8").splitlines(), 1
            )
            if FLOATING.search(line)
        ]
        self.assertEqual(found, [])

    def test_each_runner_label_that_a_job_compares_is_one_of_its_lanes(self):
        for path in workflow_files():
            workflow = yaml.load(
                path.read_text(encoding="utf-8"), Loader=UniqueKeyLoader
            )
            for job_id, job in workflow["jobs"].items():
                # A job that calls a reusable workflow names no runner.
                if "uses" in job:
                    continue
                with self.subTest(workflow=path.name, job=job_id):
                    self.assertEqual(unknown_lanes(job_id, job), set())

    def test_a_comparison_left_on_a_moved_label_is_found(self):
        job = {
            "runs-on": "${{ matrix.config.os }}",
            "timeout-minutes": "${{ matrix.config.os == 'windows-2022' && 60 || 45 }}",
            "strategy": {
                "matrix": {
                    "config": [
                        {"os": "ubuntu-24.04", "r": "release"},
                        {"os": "windows-2025", "r": "release"},
                    ]
                }
            },
            "steps": [{"if": "matrix.config.os != 'ubuntu-24.04'", "run": "true"}],
        }
        self.assertEqual(unknown_lanes("check", job), {"windows-2022"})
        job["strategy"]["matrix"]["include"] = [{"config": {"os": "windows-2022"}}]
        self.assertEqual(unknown_lanes("check", job), set())
        for runs_on in ("${{ inputs.os }}", ["self-hosted", "linux"]):
            with (
                self.subTest(runs_on=runs_on),
                self.assertRaisesRegex(AssertionError, "cannot read"),
            ):
                unknown_lanes("check", {**job, "runs-on": runs_on})


if __name__ == "__main__":
    unittest.main()
