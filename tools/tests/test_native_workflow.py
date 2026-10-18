"""native.yml runs the jobs and steps and uploads the artifacts a release requires.

The release selector names the native jobs, their critical steps and the
artifacts it publishes. A rename or matrix change in the workflow that the
selector did not follow would pass native qualification and fail only once a
release tag is pushed.
"""

import itertools
import re
import unittest

import yaml
from tools.ci import native
from tools.ci import select_release_artifacts as release
from tools.paths import ROOT
from tools.validation._repository_yaml import UniqueKeyLoader

MATRIX_VALUE = re.compile(r"\$\{\{\s*matrix\.([A-Za-z0-9_-]+)\s*\}\}")


def expand(template, lane):
    """Fill a job or artifact name with one matrix lane's values."""
    name = MATRIX_VALUE.sub(lambda match: str(lane[match.group(1)]), template)
    if "${{" in name:
        raise AssertionError(f"this test cannot expand the expression in {template}")
    return name


def native_run_jobs():
    """Map each job a native run lists to its step names and uploaded artifacts."""
    workflow = yaml.load(
        (ROOT / ".github/workflows/native.yml").read_text(encoding="utf-8"),
        Loader=UniqueKeyLoader,
    )
    jobs = {}
    for job_id, job in workflow["jobs"].items():
        matrix = job.get("strategy", {}).get("matrix", {})
        if {"include", "exclude"} & set(matrix) or not all(
            isinstance(values, list) for values in matrix.values()
        ):
            raise AssertionError(f"{job_id}: this test expands only plain matrices")
        for values in itertools.product(*matrix.values()):
            lane = dict(zip(matrix, values, strict=True))
            name = expand(job.get("name", job_id), lane)
            if name in jobs:
                raise AssertionError(f"more than one native job is named {name}")
            jobs[name] = (
                [step.get("name") for step in job["steps"]],
                [
                    expand(step["with"]["name"], lane)
                    for step in job["steps"]
                    if step.get("uses", "").startswith("actions/upload-artifact@")
                ],
            )
    return jobs


class NativeWorkflowContract(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.coverage = native.coverage()
        cls.jobs = native_run_jobs()

    def test_a_native_run_lists_exactly_the_jobs_a_release_requires(self):
        self.assertEqual(
            sorted(self.jobs), sorted(release.required_jobs(self.coverage))
        )

    def test_each_required_step_is_named_once_in_its_job(self):
        for job, steps in release.required_jobs(self.coverage).items():
            names = self.jobs.get(job, ([], []))[0]
            for step in steps:
                with self.subTest(job=job, step=step):
                    self.assertEqual(names.count(step), 1)

    def test_each_release_artifact_is_uploaded_by_its_producing_job_alone(self):
        uploaders = {}
        for job, (_, artifacts) in self.jobs.items():
            for artifact in artifacts:
                uploaders.setdefault(artifact, []).append(job)
        producers, _ = release.release_artifacts(self.coverage)
        for artifact, producer in producers.items():
            with self.subTest(artifact=artifact):
                self.assertEqual(uploaders.get(artifact), [producer])


if __name__ == "__main__":
    unittest.main()
