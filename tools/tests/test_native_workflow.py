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
from tools.ci import native, release_assets
from tools.ci import select_release_artifacts as release
from tools.paths import ROOT
from tools.validation._repository_yaml import UniqueKeyLoader

EXPRESSION = re.compile(r"\$\{\{(.*?)\}\}")


def evaluate(expression, context):
    """Evaluate the GitHub expression subset that native.yml names use.

    Operands are quoted strings and context properties such as matrix.os;
    the operators are ==, !=, && and ||, with GitHub's precedence and operand
    results. Anything else fails rather than being guessed.
    """
    tokens = re.findall(r"'[^']*'|==|!=|&&|\|\||[^\s=!&|]+", expression)
    if not tokens:
        raise AssertionError(f"this test cannot evaluate {expression!r}")

    def operand():
        token = tokens.pop(0) if tokens else ""
        if len(token) > 1 and token.startswith("'") and token.endswith("'"):
            return token[1:-1]
        scope, _, key = token.partition(".")
        if key not in context.get(scope, {}):
            raise AssertionError(f"this test cannot evaluate {expression!r}")
        return context[scope][key]

    def comparison():
        value = operand()
        while tokens[:1] in (["=="], ["!="]):
            equal = tokens.pop(0) == "=="
            value = (value == operand()) == equal
        return value

    def conjunction():
        value = comparison()
        while tokens[:1] == ["&&"]:
            tokens.pop(0)
            right = comparison()
            value = right if value else value
        return value

    def disjunction():
        value = conjunction()
        while tokens[:1] == ["||"]:
            tokens.pop(0)
            right = conjunction()
            value = value or right
        return value

    value = disjunction()
    if tokens:
        raise AssertionError(f"this test cannot evaluate {expression!r}")
    return value


def expand(template, context):
    """Fill a job or artifact name with one matrix lane's values."""
    return EXPRESSION.sub(
        lambda match: str(evaluate(match.group(1), context)), template
    )


def native_run_jobs():
    """Map each job a native run lists to its steps and artifact names."""
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
            context = {"matrix": lane, "env": dict(workflow.get("env", {}))}
            context["env"].update(
                {
                    key: expand(value, context)
                    for key, value in job.get("env", {}).items()
                }
            )
            name = expand(job.get("name", job_id), context)
            if name in jobs:
                raise AssertionError(f"more than one native job is named {name}")
            jobs[name] = {
                "steps": [],
                "uploads": [],
                "downloads": [],
                "definitions": job["steps"],
                "context": context,
            }
            for step in job["steps"]:
                jobs[name]["steps"].append(step.get("name"))
                action = step.get("uses", "").split("@")[0]
                if action not in (
                    "actions/upload-artifact",
                    "actions/download-artifact",
                ) or "name" not in step.get("with", {}):
                    continue
                if action == "actions/download-artifact" and "if" in step:
                    raise AssertionError(
                        f"{name}: this test reads no download condition"
                    )
                kind = "uploads" if action == "actions/upload-artifact" else "downloads"
                jobs[name][kind].append(expand(step["with"]["name"], context))
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
            names = self.jobs.get(job, {"steps": []})["steps"]
            for step in steps:
                with self.subTest(job=job, step=step):
                    self.assertEqual(names.count(step), 1)

    def test_each_release_artifact_is_uploaded_by_its_producing_job_alone(self):
        uploaders = {}
        for job, artifacts in self.jobs.items():
            for artifact in artifacts["uploads"]:
                uploaders.setdefault(artifact, []).append(job)
        producers = release_assets.artifacts(release_assets.declared(self.coverage))
        for artifact, producer in producers.items():
            with self.subTest(artifact=artifact):
                self.assertEqual(uploaders.get(artifact), [producer])

    def test_only_the_source_platform_keeps_its_python_source_archive(self):
        # A release artifact holds exactly its declared assets, so the other
        # Python build lanes drop the source archive they built as a check.
        sources = {
            asset.producer
            for asset in release_assets.declared(self.coverage)
            if asset.kind == "sdist"
        }
        for lane in self.coverage["build"]["include"]:
            job = "native-dist ({os}, {python})".format(**lane)
            with self.subTest(job=job):
                steps = self.jobs[job]["definitions"]
                removal = 'rm "${RUNNER_TEMP}"/native-dist/*.tar.gz'
                [step] = [step for step in steps if step.get("run") == removal]
                # After the distribution checks, before the upload.
                names = [item.get("name") for item in steps]
                self.assertLess(
                    names.index("Check Python distribution contents"),
                    steps.index(step),
                )
                self.assertLess(
                    steps.index(step), names.index("Preserve distributions")
                )
                self.assertEqual(
                    evaluate(step["if"], self.jobs[job]["context"]),
                    job not in sources,
                )

    def test_every_source_lane_installs_the_published_source_archives(self):
        # macOS and Windows install the Linux-built archives a release
        # publishes, rather than building their own.
        producers = release_assets.artifacts(release_assets.declared(self.coverage))
        linux = native.SOURCE_PLATFORM
        python_source = f"native-python-{linux}-{native.BUILD_PYTHON}"
        r_source = f"native-r-{linux}"
        self.assertLessEqual({python_source, r_source}, set(producers))
        for lane in self.coverage["qualify"]["include"]:
            job = "native-qualify ({os}, {python}, {form})".format(**lane)
            own = f"native-python-{lane['os']}-{native.BUILD_PYTHON}"
            with self.subTest(job=job):
                self.assertEqual(
                    [
                        name
                        for name in self.jobs[job]["downloads"]
                        if name.startswith("native-python-")
                    ],
                    [python_source if lane["form"] == "source" else own],
                )
        for platform in self.coverage["r"]["os"]:
            job = f"native-r ({platform})"
            with self.subTest(job=job):
                self.assertEqual(
                    [
                        name
                        for name in self.jobs[job]["downloads"]
                        if name.startswith("native-r-")
                    ],
                    [r_source],
                )

    def test_the_expression_subset_follows_github_operand_results(self):
        context = {"matrix": {"form": "source", "os": "macos-15"}, "env": {}}
        for expression, expected in (
            ("matrix.form == 'source' && 'ubuntu-24.04' || matrix.os", "ubuntu-24.04"),
            ("matrix.form == 'wheel' && 'ubuntu-24.04' || matrix.os", "macos-15"),
            ("matrix.os", "macos-15"),
            ("matrix.os != 'ubuntu-24.04'", True),
            ("matrix.form != 'source'", False),
        ):
            with self.subTest(expression=expression):
                self.assertEqual(evaluate(expression, context), expected)
        for expression in ("runner.os == 'Windows'", "matrix.form ! 'wheel'", "", "("):
            with (
                self.subTest(expression=expression),
                self.assertRaisesRegex(AssertionError, "cannot evaluate"),
            ):
                evaluate(expression, context)


if __name__ == "__main__":
    unittest.main()
