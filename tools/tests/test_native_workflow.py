"""native.yml runs the jobs and steps and uploads the artifacts a release requires.

The release selector names the native jobs, their critical steps and the
artifacts it publishes. A rename or matrix change in the workflow that the
selector did not follow would pass native qualification and fail only once a
release tag is pushed.
"""

import itertools
import re
import unittest
from pathlib import Path
from unittest.mock import patch

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
                "job": job,
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

    def test_each_rust_lane_runs_only_the_optimized_workspace_tests(self):
        # Pull request and scheduled CI run the development-profile workspace
        # tests and Clippy on Linux, so native runs the workspace tests once,
        # with the release-test profile, on each platform (#3106).
        for lane in self.coverage["rust"]["include"]:
            job = "native-rust ({os}, {python})".format(**lane)
            commands = [
                line.strip()
                for step in self.jobs[job]["definitions"]
                for line in step.get("run", "").splitlines()
            ]
            tests = [command for command in commands if "cargo test" in command]
            with self.subTest(job=job):
                self.assertTrue(tests)
                for command in tests:
                    self.assertIn("--profile release-test", command)
                for command in commands:
                    self.assertNotIn("cargo clippy", command)
                    self.assertNotIn("tools.commands.rust_tests", command)

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

    def test_each_linux_wheel_builds_in_its_manylinux_container(self):
        for lane in self.coverage["build"]["include"]:
            job = "native-dist ({os}, {python})".format(**lane)
            [build] = [
                step
                for step in self.jobs[job]["definitions"]
                if step.get("name") == "Build wheel and source archive"
            ]
            with self.subTest(job=job):
                self.assertNotIn("if", build)
                self.assertIn(
                    'if [ "${RUNNER_OS}" = Linux ]; then\n'
                    '    python -m tools.ci.manylinux --out "${RUNNER_TEMP}/native-dist"\n'
                    "  else\n"
                    '    maturin build --release --locked --out "${RUNNER_TEMP}/native-dist"\n',
                    build["run"],
                )

    def test_each_linux_wheel_is_qualified_on_glibc_2_28(self):
        linux = {
            lane["os"]
            for lane in self.coverage["build"]["include"]
            if lane["os"].startswith("ubuntu-")
        }
        self.assertEqual(set(self.coverage["glibc"]["os"]), linux)
        for platform in self.coverage["glibc"]["os"]:
            job = f"native-glibc ({platform})"
            with self.subTest(job=job):
                # AlmaLinux 8 has glibc 2.28, pinned by a dated tag.
                self.assertRegex(
                    self.jobs[job]["job"]["container"], r"almalinux:8\.10-[0-9]{8}"
                )
                self.assertEqual(
                    [
                        name
                        for name in self.jobs[job]["downloads"]
                        if name.startswith("native-python-")
                    ],
                    [f"native-python-{platform}-{native.BUILD_PYTHON}"],
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

    def test_cran_checks_run_on_the_published_r_source_archive(self):
        # CRAN's incoming checks judge the bytes that a release publishes.
        r_source = f"native-r-{native.SOURCE_PLATFORM}"
        jobs = [
            f"native-cran ({platform}, R {version})"
            for platform in self.coverage["cran"]["os"]
            for version in self.coverage["cran"]["r"]
        ] + [
            f"native-cran ({platform})"
            for platform in self.coverage["cran_container"]["platform"]
        ]
        jobs += [
            f"native-cran ({platform}, R {version}, no Suggests)"
            for platform in self.coverage["cran_no_suggests"]["os"]
            for version in self.coverage["cran_no_suggests"]["r"]
        ]
        self.assertEqual(set(self.coverage["cran"]["r"]), {"devel", "release", "4.4"})
        for job in jobs:
            with self.subTest(job=job):
                self.assertEqual(
                    [
                        name
                        for name in self.jobs[job]["downloads"]
                        if name.startswith("native-r-")
                    ],
                    [r_source],
                )
                [check] = [
                    step
                    for step in self.jobs[job]["definitions"]
                    if step.get("name") == "Check the R source package as CRAN does"
                ]
                self.assertIn("-m tools.commands.check_r --archive", check["run"])
                # Without a network: connections go to a refusing proxy.
                self.assertEqual(check["env"]["http_proxy"], "http://127.0.0.1:9")

    def test_only_the_no_suggests_lane_checks_without_the_suggested_packages(self):
        # CRAN's noSuggests check is R-devel on Linux with
        # _R_CHECK_DEPENDS_ONLY_; with _R_CHECK_FORCE_SUGGESTS_ false, R
        # reports the missing Suggests instead of stopping. Every other CRAN
        # lane installs them and sets neither, so R stops there when one is
        # missing rather than letting the tests skip what needs it.
        settings = {
            "_R_CHECK_DEPENDS_ONLY_": "true",
            "_R_CHECK_FORCE_SUGGESTS_": "false",
        }
        lanes = {
            f"native-cran ({platform}, R {version}, no Suggests)"
            for platform in self.coverage["cran_no_suggests"]["os"]
            for version in self.coverage["cran_no_suggests"]["r"]
        }
        self.assertEqual(lanes, {"native-cran (ubuntu-24.04, R devel, no Suggests)"})
        for job in [name for name in self.jobs if name.startswith("native-cran ")]:
            steps = self.jobs[job]["definitions"]
            names = [step.get("name") for step in steps]
            check = steps[names.index("Check the R source package as CRAN does")]
            with self.subTest(job=job):
                chosen = {name: check["env"].get(name) for name in settings}
                if job not in lanes:
                    self.assertEqual(chosen, dict.fromkeys(settings))
                    continue
                self.assertEqual(chosen, settings)
                self.assertLess(
                    names.index("Confirm that no suggested package is installed"),
                    names.index("Check the R source package as CRAN does"),
                )
                for step in steps:
                    self.assertNotIn("setup-r-dependencies", step.get("uses", ""))
                    self.assertNotRegex(step.get("run", ""), r"install\.packages|pak::")

    def test_every_python_lane_is_compared_beside_r_reports_of_its_os(self):
        # Linux aarch64 has no R lane yet; its lanes use Linux x86_64's.
        commands = []
        with patch.object(
            native.subprocess,
            "run",
            side_effect=lambda command, **kwargs: commands.append(command),
        ):
            native.compare_cohorts(self.coverage["qualify"], ROOT, "a" * 40)
        self.assertEqual(len(commands), len(self.coverage["qualify"]["include"]))
        for lane, command in zip(
            self.coverage["qualify"]["include"], commands, strict=True
        ):
            r_platform = native.R_EVIDENCE.get(lane["os"], lane["os"])
            with self.subTest(lane=lane):
                self.assertIn(r_platform, native.R_PLATFORMS)
                evidence = command[command.index("--r-evidence") + 1]
                self.assertEqual(
                    Path(evidence),
                    Path("cohort-input")
                    / f"native-r-qualification-{r_platform}"
                    / "r-source",
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

    def test_every_cran_job_keeps_its_check_installation_and_test_logs(self):
        # A failed CRAN check is diagnosed from its artifact. In a container
        # job, ${{ runner.temp }} names the runner's directory, which the
        # runner maps to the container's only at the start of an input, so
        # there the paths start from the directory that the check step names.
        files = ["00check.log", "00install.out", "tests/*.Rout*"]
        jobs = [name for name in self.jobs if name.startswith("native-cran ")]
        self.assertTrue(jobs)
        for job in jobs:
            [upload] = [
                step
                for step in self.jobs[job]["definitions"]
                if step.get("name") == "Preserve the CRAN check"
            ]
            paths = [
                path.strip().partition("/yamaa.Rcheck/")
                for path in upload["with"]["path"].splitlines()
            ]
            directory = (
                "${{ steps.check.outputs.directory }}"
                if "container" in self.jobs[job]["job"]
                else "${{ runner.temp }}/cran-check"
            )
            with self.subTest(job=job):
                self.assertEqual([path[2] for path in paths], files)
                self.assertEqual({path[0] for path in paths}, {directory})

    def test_every_cran_job_refuses_connections_only_while_it_checks(self):
        # The installs before the check reach package mirrors; the package
        # build and check alone run behind the refusing proxy. Environment
        # names are case-insensitive on Windows.
        proxy = {"http_proxy", "https_proxy", "all_proxy", "no_proxy"}
        jobs = [name for name in self.jobs if name.startswith("native-cran ")]
        self.assertTrue(jobs)
        for job in jobs:
            with self.subTest(job=job):
                inherited = {name.lower() for name in self.jobs[job]["context"]["env"]}
                self.assertFalse(proxy & inherited)
                for step in self.jobs[job]["definitions"]:
                    names = {name.lower() for name in step.get("env", {})}
                    checks = (
                        step.get("name") == "Check the R source package as CRAN does"
                    )
                    self.assertEqual(proxy & names, proxy if checks else set())


if __name__ == "__main__":
    unittest.main()
