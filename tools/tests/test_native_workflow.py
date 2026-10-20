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
from tools.commands import rust_tests
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

    def test_each_rust_lane_leaves_out_the_host_bindings_and_installs_no_r(self):
        # The bindings have no Rust tests. The workspace tests leave them out,
        # as `make test-rust` does, so no step builds PyO3 or extendr, and the
        # job installs no R and sets no interpreter for PyO3 (#3106).
        for lane in self.coverage["rust"]["include"]:
            job = "native-rust ({os}, {python})".format(**lane)
            steps = self.jobs[job]["definitions"]
            tests = [
                line.strip()
                for step in steps
                for line in step.get("run", "").splitlines()
                if "cargo test" in line
            ]
            with self.subTest(job=job):
                self.assertTrue(tests)
                for command in tests:
                    self.assertEqual(
                        re.findall(r"--exclude (\S+)", command),
                        list(rust_tests.BINDINGS),
                    )
                self.assertNotIn(
                    "r-lib/actions/setup-r",
                    [step.get("uses", "").split("@")[0] for step in steps],
                )
                self.assertNotIn("PYO3_PYTHON", yaml.safe_dump(steps))

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
                    '    maturin build --profile dist --locked --out "${RUNNER_TEMP}/native-dist"\n',
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
        sources = {
            "python": ("native-python-", python_source),
            "r": ("native-r-", r_source),
        }
        for job, host in self.offline_source_jobs():
            prefix, source = sources[host]
            with self.subTest(job=job):
                self.assertEqual(
                    [
                        name
                        for name in self.jobs[job]["downloads"]
                        if name.startswith(prefix)
                    ],
                    [source],
                )

    def offline_source_jobs(self):
        """List each offline source lane's job name with its host."""
        return [
            (f"native-offline-source ({platform}, {host})", host)
            for platform in self.coverage["offline_source"]["os"]
            for host in self.coverage["offline_source"]["host"]
        ]

    def test_both_source_archives_install_offline_on_every_platform(self):
        # Each platform that a wheel is built for also builds and installs
        # both published source archives with the network disabled (#3030
        # D55), Linux aarch64 included.
        self.assertEqual(
            sorted(self.coverage["offline_source"]["os"]),
            sorted(lane["os"] for lane in self.coverage["build"]["include"]),
        )
        self.assertEqual(
            sorted(self.coverage["offline_source"]["host"]), ["python", "r"]
        )

    def test_offline_source_lanes_refuse_the_network_while_they_install_and_run(self):
        # The steps that build the archive and run the installed package go
        # behind a proxy that refuses every connection (macOS and Windows) and
        # run every container without a network and with a read-only root
        # (Linux). The steps before them reach package mirrors.
        proxy = {
            "http_proxy": "http://127.0.0.1:9",
            "HTTPS_PROXY": "http://127.0.0.1:9",
            "ALL_PROXY": "http://127.0.0.1:9",
            "NO_PROXY": "",
        }
        names = {name.lower() for name in proxy}
        required = release.required_jobs(self.coverage)
        for job, _ in self.offline_source_jobs():
            with self.subTest(job=job):
                inherited = {name.lower() for name in self.jobs[job]["context"]["env"]}
                self.assertFalse(names & inherited)
                offline = []
                for step in self.jobs[job]["definitions"]:
                    environment = step.get("env", {})
                    chosen = {
                        name: value
                        for name, value in environment.items()
                        if name.lower() in names
                    }
                    if step.get("name") not in required[job]:
                        self.assertEqual(chosen, {})
                        continue
                    offline.append(step["name"])
                    self.assertEqual(chosen, proxy)
                    self.assertNotIn("if", step)
                    containers = [
                        line
                        for line in step["run"].replace("\\\n", " ").splitlines()
                        if "docker run" in line
                    ]
                    self.assertTrue(containers)
                    for line in containers:
                        self.assertIn("--network none", line)
                        self.assertIn("--read-only", line)
                self.assertEqual(offline, list(required[job]))

    def test_offline_source_lanes_build_as_the_install_guidance_says(self):
        # pip builds the Python source archive with the maturin installed
        # beforehand and no package index, and R CMD INSTALL builds the R
        # source package, once in a container on Linux and once on macOS and
        # Windows; both packages then run the conformance corpus and compare it.
        commands = {
            "python": "pip install --no-build-isolation --no-index --no-cache-dir "
            "native-dist/yamaa-*.tar.gz",
            "r": "R CMD INSTALL",
        }
        required = release.required_jobs(self.coverage)
        for job, host in self.offline_source_jobs():
            steps = {step.get("name"): step for step in self.jobs[job]["definitions"]}
            install, conformance = required[job]
            with self.subTest(job=job):
                self.assertEqual(steps[install]["run"].count(commands[host]), 2)
                self.assertIn("public_conformance compare", steps[conformance]["run"])

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

    def cohort_commands(self):
        """List the comparisons that the cohort job runs, without running them."""
        commands = []
        with patch.object(
            native.subprocess,
            "run",
            side_effect=lambda command, **kwargs: commands.append(command),
        ):
            native.compare_cohorts(self.coverage, ROOT, "a" * 40)
        return commands

    def test_every_python_lane_is_compared_beside_r_reports_of_its_os(self):
        # Linux aarch64 has no native-r lane yet; its lanes use Linux x86_64's.
        lanes = self.coverage["qualify"]["include"]
        commands = self.cohort_commands()
        self.assertEqual(len(commands), len(lanes) + len(self.r_binary_lanes()))
        for lane, command in zip(lanes, commands[: len(lanes)], strict=True):
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

    def r_binary_lanes(self):
        """List each R binary lane's platform and R version."""
        return list(
            itertools.product(
                self.coverage["r_binary"]["os"], self.coverage["r_binary"]["r"]
            )
        )

    def setup_r(self, job):
        """Return the one setup-r step of a native job."""
        [step] = [
            step
            for step in self.jobs[job]["definitions"]
            if step.get("uses", "").split("@")[0] == "r-lib/actions/setup-r"
        ]
        return step

    def test_r_binaries_cover_r_release_and_r_oldrel_on_windows_and_macos(self):
        # #3030 D48: R-release and R-oldrel on Windows x64 and macOS arm64,
        # as CRAN builds them; D54: Linux builds from source.
        self.assertEqual(
            sorted(self.coverage["r_binary"]["os"]), ["macos-15", "windows-2025"]
        )
        self.assertEqual(
            self.coverage["r_binary"]["r"], [native.R_RELEASE, native.R_OLDREL]
        )

        def version(text):
            return tuple(map(int, text.split(".")))

        self.assertLess(version(native.R_OLDREL), version(native.R_RELEASE))
        self.assertNotEqual(
            native.r_series(native.R_OLDREL), native.r_series(native.R_RELEASE)
        )

    def test_every_pinned_native_r_is_r_release(self):
        # R-release is declared once, in tools/ci/native.py: each native job
        # that pins its R, rather than taking it from its matrix, names that
        # version, and so do the Linux lanes' rocker/r-ver images.
        pinned, images = set(), set()
        for job in self.jobs.values():
            for step in job["definitions"]:
                if step.get("uses", "").split("@")[0] == "r-lib/actions/setup-r":
                    requested = step["with"]["r-version"]
                    if "matrix." not in requested:
                        pinned.add(requested)
                images.update(re.findall(r"rocker/r-ver:(\S+)", step.get("run", "")))
        self.assertEqual(pinned, {native.R_RELEASE})
        self.assertEqual(images, {native.R_RELEASE})

    def test_each_r_binary_builds_from_the_published_r_source_archive(self):
        r_source = f"native-r-{native.SOURCE_PLATFORM}"
        for platform, version in self.r_binary_lanes():
            job = f"native-r-binary ({platform}, R {version})"
            steps = {step.get("name"): step for step in self.jobs[job]["definitions"]}
            setup = self.setup_r(job)
            with self.subTest(job=job):
                self.assertEqual(
                    [
                        name
                        for name in self.jobs[job]["downloads"]
                        if name.startswith("native-")
                    ],
                    [r_source],
                )
                self.assertEqual(
                    expand(setup["with"]["r-version"], self.jobs[job]["context"]),
                    version,
                )
                build = steps["Build the R binary package"]
                self.assertIn("R CMD INSTALL --build", build["run"])
                # The build reads the vendored crates behind the refusing proxy.
                self.assertEqual(build["env"]["http_proxy"], "http://127.0.0.1:9")
                self.assertEqual(
                    self.jobs[job]["uploads"], [f"native-r-binary-{platform}-{version}"]
                )

    def test_each_r_binary_installs_without_rust_rtools_or_a_network(self):
        # Installed as a user installs it, on a runner whose Rust is removed
        # and that has no Rtools; the installation and every step after it
        # run behind a proxy that refuses every connection, and the steps
        # before it, which reach package mirrors, do not.
        proxy = {
            "http_proxy": "http://127.0.0.1:9",
            "HTTPS_PROXY": "http://127.0.0.1:9",
            "ALL_PROXY": "http://127.0.0.1:9",
            "NO_PROXY": "",
        }
        names = {name.lower() for name in proxy}
        for platform, version in self.r_binary_lanes():
            job = f"native-r-binary-install ({platform}, R {version})"
            steps = self.jobs[job]["definitions"]
            titles = [step.get("name") for step in steps]
            setup = self.setup_r(job)
            removal = titles.index("Remove Rust and Rtools from the runner")
            install = titles.index("Install the R binary package without a network")
            with self.subTest(job=job):
                # Its own platform and series' binary, and nothing to build.
                self.assertEqual(
                    [
                        name
                        for name in self.jobs[job]["downloads"]
                        if name.startswith("native-")
                    ],
                    [f"native-r-binary-{platform}-{version}"],
                )
                self.assertEqual(
                    expand(setup["with"]["r-version"], self.jobs[job]["context"]),
                    version,
                )
                self.assertEqual(setup["with"]["rtools-version"], "none")
                self.assertIs(setup["with"]["windows-path-include-rtools"], False)
                scripts = "\n".join(step.get("run", "") for step in steps)
                self.assertNotIn("tools.ci.toolchain", scripts)
                self.assertNotIn("rustup target", scripts)
                # rustup removes itself on Windows; the macOS image's Homebrew
                # rustup cannot, so Homebrew removes it there.
                macos, _, others = steps[removal]["run"].partition("elif")
                self.assertIn('[ "${RUNNER_OS}" = macOS ]', macos)
                self.assertIn(
                    "brew uninstall --force --ignore-dependencies rustup", macos
                )
                self.assertNotIn("brew uninstall", others)
                self.assertIn("rustup self uninstall -y", others)
                self.assertLess(removal, install)
                self.assertIn(
                    "install.packages(commandArgs(TRUE), repos = NULL",
                    steps[install]["run"],
                )
                # R installs, loads and runs the package with a PATH that
                # names no compiler, Rust tool or GNU runtime library.
                self.assertIn("> toolchain-free-path", steps[removal]["run"])
                self.assertIn("libgcc_s_seh-1.dll", steps[removal]["run"])
                clean = 'export PATH="$(cat toolchain-free-path)"'
                conformance = titles.index(
                    "Report installed R public benchmark conformance"
                )
                for index in (install, conformance):
                    self.assertIn(clean, steps[index]["run"])
                self.assertIn("-e 'library(yamaa)'", steps[install]["run"])
                inherited = {name.lower() for name in self.jobs[job]["context"]["env"]}
                self.assertFalse(names & inherited)
                for index, step in enumerate(steps):
                    chosen = {
                        name: value
                        for name, value in step.get("env", {}).items()
                        if name.lower() in names
                    }
                    offline = index >= install and "run" in step
                    self.assertEqual(chosen, proxy if offline else {})

    def test_each_r_binary_runs_the_corpus_and_the_suites_as_a_binary(self):
        required = release.required_jobs(self.coverage)
        for platform, version in self.r_binary_lanes():
            job = f"native-r-binary-install ({platform}, R {version})"
            steps = {step.get("name"): step for step in self.jobs[job]["definitions"]}
            conformance = steps["Report installed R public benchmark conformance"]
            suites = steps["Run installed R supplemental suites outside checkout"]
            with self.subTest(job=job):
                self.assertIn(
                    "Report installed R public benchmark conformance", required[job]
                )
                self.assertEqual(
                    conformance["env"]["PUBLIC_CONFORMANCE_PACKAGE_FORM"], "binary"
                )
                self.assertIn("public_conformance compare", conformance["run"])
                self.assertIn("--package-form binary", suites["run"])
                self.assertIn(
                    f"native-r-binary-qualification-{platform}-{version}",
                    self.jobs[job]["uploads"],
                )

    def test_every_r_binary_install_joins_the_cohort_comparison(self):
        # Each binary's reports are compared beside the wheel lane of its
        # platform: on Windows the binary is built through Rtools' GNU
        # toolchain and the wheel with MSVC.
        lanes = self.coverage["qualify"]["include"]
        commands = self.cohort_commands()[len(lanes) :]
        for (platform, version), command in zip(
            self.r_binary_lanes(), commands, strict=True
        ):
            with self.subTest(platform=platform, version=version):
                self.assertIn(
                    {"os": platform, "python": native.BUILD_PYTHON, "form": "wheel"},
                    lanes,
                )
                self.assertEqual(
                    Path(command[command.index("--python-evidence") + 1]),
                    Path("cohort-input")
                    / f"native-qualification-{platform}-{native.BUILD_PYTHON}-wheel"
                    / "wheel",
                )
                self.assertEqual(
                    Path(command[command.index("--r-evidence") + 1]),
                    Path("cohort-input")
                    / f"native-r-binary-qualification-{platform}-{version}"
                    / "r-binary",
                )
        cohort = self.jobs["public-cohort-complete-runs"]["job"]
        self.assertIn("r-binary-install", cohort["needs"])
        self.assertIn(
            "native-r-binary-qualification-*",
            [step.get("with", {}).get("pattern") for step in cohort["steps"]],
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
