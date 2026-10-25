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
from tools.ci import manylinux, native, release_assets
from tools.ci import select_release_artifacts as release
from tools.commands import rust_tests
from tools.paths import ROOT
from tools.tests import support

EXPRESSION = re.compile(r"\$\{\{(.*?)\}\}")
# The commands that stage an engine bundle, to build a source archive from it,
# to compare an archive with it or to read the crates of its trimmed lock.
STAGING = re.compile(
    r"python -m tools\.(?:staging\.(?:engine_bundle|stage_r_package)|packaging\."
    r"(?:check_cargo_lock|check_crate_purposes|generate_component_inventory))\b"
)
PYYAML = re.compile(r"python -m pip install .*\bpyyaml==(\S+)")


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
    workflow = support.workflow("native.yml")
    jobs = {}
    for job_id, job in workflow["jobs"].items():
        matrix = job.get("strategy", {}).get("matrix", {})
        if not all(isinstance(values, list) for values in matrix.values()) or (
            {"include", "exclude"} & set(matrix) and set(matrix) != {"include"}
        ):
            raise AssertionError(
                f"{job_id}: this test expands only plain matrices and lists of "
                "included lanes"
            )
        if "include" in matrix:
            lanes = [dict(lane) for lane in matrix["include"]]
        else:
            lanes = [
                dict(zip(matrix, values, strict=True))
                for values in itertools.product(*matrix.values())
            ]
        for lane in lanes:
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

    def test_the_conformance_archive_is_written_into_the_directory_it_uploads(self):
        # The writer checks the archive that it wrote against the checkout,
        # and the job uploads that directory alone, so the artifact holds the
        # one asset declared in it.
        [asset] = [
            asset
            for asset in release_assets.declared(self.coverage)
            if asset.kind == "conformance"
        ]
        steps = self.jobs[asset.producer]["definitions"]
        [build] = [
            step
            for step in steps
            if step.get("name") == "Build the conformance archive"
        ]
        [upload] = [
            step
            for step in steps
            if step.get("uses", "").split("@")[0] == "actions/upload-artifact"
        ]
        self.assertEqual(
            (
                build["run"],
                upload["with"]["path"],
                steps.index(build) < steps.index(upload),
            ),
            (
                "python -m tools.packaging.conformance_archive write "
                '"${RUNNER_TEMP}/conformance"',
                "${{ runner.temp }}/conformance",
                True,
            ),
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

    def test_each_job_installs_pyyaml_before_it_stages_an_engine_bundle(self):
        # Staging reads the selftest list, benchmarks/selftest.yaml, with
        # PyYAML. Where PyYAML is missing, `import yaml` finds the repository's
        # yaml/ directory, so each job installs it in an unconditional step
        # before its first staging command (#3347), and every job pins the
        # same release.
        pins, staging = set(), set()
        for job, details in self.jobs.items():
            installed = False
            for step in details["definitions"]:
                for line in step.get("run", "").splitlines():
                    pin = PYYAML.search(line)
                    if pin:
                        pins.add(pin.group(1))
                        installed = installed or "if" not in step
                    if STAGING.search(line):
                        staging.add(job)
                        with self.subTest(job=job, command=line.strip()):
                            self.assertTrue(installed)
        self.assertEqual(len(pins), 1, pins)
        for lane in self.coverage["build"]["include"]:
            self.assertIn("native-dist ({os}, {python})".format(**lane), staging)

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
        for job, host in self.offline_source_jobs() + self.offline_toolchain_jobs():
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

    def offline_toolchain_jobs(self):
        """List each offline toolchain lane's job name with its host."""
        return [
            (f"native-offline-toolchain ({platform}, {host})", host)
            for platform in self.coverage["offline_toolchain"]["os"]
            for host in self.coverage["offline_toolchain"]["host"]
        ]

    def offline_wheel_jobs(self):
        """List each offline wheel lane's job name with its host, always python."""
        return [
            (f"native-offline-wheel ({platform}, {version})", "python")
            for platform in self.coverage["offline_wheel"]["os"]
            for version in self.coverage["offline_wheel"]["python"]
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

    def test_offline_lanes_refuse_the_network_while_they_install_and_run(self):
        # The steps that install the standalone Rust (the toolchain lanes),
        # build the archive or install the wheel, and run the installed
        # package go behind a proxy that refuses every connection (macOS and
        # Windows) and run every container without a network and with a
        # read-only root (Linux). The steps before them reach package mirrors
        # and static.rust-lang.org.
        proxy = {
            "http_proxy": "http://127.0.0.1:9",
            "HTTPS_PROXY": "http://127.0.0.1:9",
            "ALL_PROXY": "http://127.0.0.1:9",
            "NO_PROXY": "",
        }
        names = {name.lower() for name in proxy}
        required = release.required_jobs(self.coverage)
        for job, _ in (
            self.offline_source_jobs()
            + self.offline_toolchain_jobs()
            + self.offline_wheel_jobs()
        ):
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
            install, _, conformance = required[job]
            with self.subTest(job=job):
                self.assertEqual(steps[install]["run"].count(commands[host]), 2)
                self.assertIn("public_conformance compare", steps[conformance]["run"])

    def test_the_offline_toolchain_route_is_proved_on_every_linux_platform(self):
        # #3030 D54: a Linux machine without a modern Rust carries Rust in from
        # its standalone installer, so both published source archives build
        # with it on each Linux platform that a wheel is built for.
        linux = sorted(
            lane["os"]
            for lane in self.coverage["build"]["include"]
            if lane["os"].startswith("ubuntu-")
        )
        self.assertEqual(linux, ["ubuntu-24.04", "ubuntu-24.04-arm"])
        self.assertEqual(sorted(self.coverage["offline_toolchain"]["os"]), linux)
        self.assertEqual(
            sorted(self.coverage["offline_toolchain"]["host"]), ["python", "r"]
        )

    def assert_verifies_the_minimum_rust_installer(self, run):
        """Check that a script downloads the standalone installer archive of
        the workspace's rust-version for the runner's target, and that gpg
        checks the signature beside it, with a keyring that holds the
        committed Rust signing key alone, before the archive is unpacked."""
        self.assertIn("from tools.ci.toolchain import minimum", run)
        self.assertIn('target="$(uname -m)-unknown-linux-gnu"', run)
        self.assertIn('installer="rust-${version}-${target}.tar.xz"', run)
        self.assertIn(
            'for file in "dist/${installer}" "dist/${installer}.asc"; do', run
        )
        self.assertIn('"https://static.rust-lang.org/${file}"', run)
        self.assertIn('export GNUPGHOME="${PWD}/gnupg"', run)
        self.assertIn(
            'gpg --batch --import "${GITHUB_WORKSPACE}/tools/ci/keys/rust-key.gpg.ascii"',
            run,
        )
        verify = (
            "gpg --batch --no-auto-key-retrieve --verify "
            '"${installer}.asc" "${installer}"'
        )
        self.assertLess(run.index(verify), run.index('tar -xJf "${installer}"'))

    def test_offline_toolchain_lanes_verify_the_installer_of_the_minimum_rust(self):
        # While the network is up, each lane downloads and verifies the
        # standalone installer of the minimum Rust. A replaced download must
        # not supply its own verification key.
        name = "Download and verify the standalone Rust installer"
        scripts = set()
        for job, _ in self.offline_toolchain_jobs():
            steps = self.jobs[job]["definitions"]
            names = [step.get("name") for step in steps]
            download = steps[names.index(name)]
            run = download["run"]
            scripts.add(run)
            with self.subTest(job=job):
                self.assertNotIn("if", download)
                self.assert_verifies_the_minimum_rust_installer(run)
                self.assertLess(
                    names.index(name),
                    names.index(
                        "Install Rust from its standalone installer without a network"
                    ),
                )
        # Every lane runs the same script.
        self.assertEqual(len(scripts), 1)

    def test_offline_toolchain_lanes_build_with_the_standalone_rust_alone(self):
        # The archive's own install.sh installs Rust into /opt/rust with no
        # rustup, in a container without a network. The build mounts it
        # read-only and finds no rustup and no other cargo or rustc, as the
        # install guidance describes, and the installed package runs the
        # conformance corpus. No step installs or activates the runner's Rust.
        commands = {
            "python": "pip install --no-build-isolation --no-index --no-cache-dir "
            "native-dist/yamaa-*.tar.gz",
            "r": "R CMD INSTALL --library=/work/library /work/yamaa_*.tar.gz",
        }
        offline = "for state in /sys/class/net/*/operstate; do"
        required = release.required_jobs(self.coverage)
        for job, host in self.offline_toolchain_jobs():
            steps = {step.get("name"): step for step in self.jobs[job]["definitions"]}
            toolchain, build, _, conformance = required[job]
            # Each command on one line, with single spaces.
            installation, compilation = (
                " ".join(steps[name]["run"].replace("\\\n", " ").split())
                for name in (toolchain, build)
            )
            scripts = "\n".join(step.get("run", "") for step in steps.values())
            with self.subTest(job=job):
                self.assertEqual(
                    toolchain,
                    "Install Rust from its standalone installer without a network",
                )
                self.assertIn(
                    '"rust-${STANDALONE_RUST_VERSION}-${STANDALONE_RUST_TARGET}'
                    '/install.sh" --prefix=/opt/rust '
                    '--components="rustc,cargo,rust-std-${STANDALONE_RUST_TARGET}" '
                    "--disable-ldconfig",
                    installation,
                )
                self.assertIn('--volume "${PWD}/rust:/opt/rust" ', installation)
                for tool in ("rustc", "cargo"):
                    with self.subTest(tool=tool):
                        self.assertIn(
                            f"/opt/rust/bin/{tool} --version | "
                            f'grep -F "{tool} ${{STANDALONE_RUST_VERSION}} "',
                            installation,
                        )
                self.assertIn('--volume "${PWD}/rust:/opt/rust:ro"', compilation)
                self.assertIn('test -z "$(command -v rustup)"', compilation)
                for tool in ("cargo", "rustc"):
                    with self.subTest(tool=tool):
                        self.assertIn(
                            f'test "$(command -v {tool})" = /opt/rust/bin/{tool}',
                            compilation,
                        )
                self.assertEqual(compilation.count(commands[host]), 1)
                self.assertLess(
                    compilation.index("command -v rustc"),
                    compilation.index(commands[host]),
                )
                self.assertIn("public_conformance compare", steps[conformance]["run"])
                # Each container that installs Rust or builds the archive first
                # finds no network interface but the loopback up.
                for run in (installation, compilation):
                    self.assertEqual(run.count(offline), run.count("docker run"))
                self.assertNotIn("python -m tools.ci.toolchain", scripts)
                self.assertNotIn("--print sysroot", scripts)
                self.assertNotIn("rustup", scripts.replace("command -v rustup", ""))

    def test_every_wheel_installs_offline_with_the_oldest_and_newest_cpython(self):
        # Each wheel that a release publishes is abi3 from the oldest
        # supported CPython, which its tag names, so that CPython and the
        # newest bound the supported interpreters that it serves (#3061).
        pythons = sorted(
            native.PYTHONS, key=lambda text: tuple(map(int, text.split(".")))
        )
        self.assertEqual(
            sorted(self.coverage["offline_wheel"]["os"]),
            sorted(lane["os"] for lane in self.coverage["build"]["include"]),
        )
        self.assertEqual(
            self.coverage["offline_wheel"]["python"], [pythons[0], pythons[-1]]
        )
        tag = "cp" + pythons[0].replace(".", "") + "-abi3-"
        for asset in release_assets.declared(self.coverage):
            if asset.kind == "wheel":
                with self.subTest(asset=asset.title):
                    self.assertIn(tag, asset.file)

    def test_offline_wheel_lanes_install_files_carried_in_as_the_guide_says(self):
        # While the network is up, each lane collects its platform's published
        # wheel and the runtime dependencies that python/uv.lock pins, as
        # wheels alone, into one directory: on Linux with the CPython of the
        # lane's version in the pinned manylinux_2_28 image. Then, without a
        # network, pip installs from that directory alone with the command of
        # the restricted-environment guide, into a new virtual environment,
        # and the installed package must import with the wheel's version:
        # once in the Linux container, and once on macOS and Windows.
        command = "--no-index --find-links yamaa-files yamaa"
        guide = (ROOT / "docs/articles/restricted-environments.md").read_text(
            encoding="utf-8"
        )
        self.assertIn(f"python -m pip install {command}\n", guide)
        download = (
            "-m pip download --no-cache-dir --only-binary :all: --no-deps "
            "--dest yamaa-files -r runtime-requirements.txt"
        )
        oldest = self.coverage["offline_wheel"]["python"][0]
        producers = release_assets.artifacts(release_assets.declared(self.coverage))
        required = release.required_jobs(self.coverage)
        for job, _ in self.offline_wheel_jobs():
            context = self.jobs[job]["context"]
            platform, version = context["matrix"]["os"], context["matrix"]["python"]
            steps = self.jobs[job]["definitions"]
            names = [step.get("name") for step in steps]
            collect = steps[names.index("Collect the wheel and its dependencies")]
            install = steps[names.index(required[job][0])]
            [setup] = [
                step
                for step in steps
                if step.get("uses", "").split("@")[0] == "actions/setup-python"
            ]
            # Each command on one line, with single spaces.
            collection, installation = (
                " ".join(step["run"].replace("\\\n", " ").split())
                for step in (collect, install)
            )
            [interpreter] = re.findall(r'interpreter="([^"]+)"', collection)
            with self.subTest(job=job):
                wheel = f"native-python-{platform}-{native.BUILD_PYTHON}"
                self.assertEqual(self.jobs[job]["downloads"], [wheel])
                self.assertIn(wheel, producers)
                self.assertEqual(
                    expand(setup["with"]["python-version"], context), version
                )
                self.assertEqual(
                    expand(collect["env"]["PYTHON_VERSION"], context), version
                )
                # The image's CPython, named as tools/ci/manylinux.py names
                # the oldest.
                self.assertEqual(
                    interpreter.replace(
                        "${PYTHON_VERSION/./}", oldest.replace(".", "", 1)
                    ),
                    manylinux.PYTHON,
                )
                self.assertIn("from tools.ci.manylinux import IMAGES", collection)
                self.assertLess(steps.index(collect), steps.index(install))
                self.assertIn(
                    "--locked --no-dev --no-emit-project --no-hashes "
                    "--output-file runtime-requirements.txt",
                    collection,
                )
                self.assertNotIn("--extra", collection)
                self.assertIn("cp native-dist/yamaa-*.whl yamaa-files/", collection)
                self.assertEqual(collection.count(download), 2)
                self.assertEqual(installation.count("-I -m venv venv"), 2)
                self.assertEqual(
                    installation.count(f"-m pip install --no-cache-dir {command}"), 2
                )
                self.assertEqual(
                    installation.count("-m pip list --disable-pip-version-check"), 2
                )
                self.assertEqual(
                    installation.count(
                        "sys.exit(None if v == sys.argv[1] else "
                        'f"pip installed yamaa {v}, not {sys.argv[1]}")\' '
                        '"${version}"'
                    ),
                    2,
                )

    def test_r_checks_cover_the_declared_floor_and_latest_patch(self):
        description = (ROOT / "R/yamaa/DESCRIPTION").read_text(encoding="utf-8")
        floor = re.search(r"^Depends: R \(>= ([0-9.]+)\)$", description, re.M)[1]
        workflow = support.workflow("R-CMD-check.yaml")
        regular = workflow["jobs"]["R-CMD-check"]["strategy"]["matrix"]["config"]
        self.assertEqual(
            {
                "exact_floor": self.coverage["cran"]["os"]
                if floor in self.coverage["cran"]["r"]
                else [],
                "latest_patch": [
                    lane["os"]
                    for lane in regular
                    if lane["r"] == native.r_series(floor)
                ],
            },
            {
                "exact_floor": ["ubuntu-24.04", "macos-15", "windows-2025"],
                "latest_patch": ["ubuntu-24.04"],
            },
            "the oldest admitted R patch is checked on every R platform, and its latest patch on Linux",
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
        jobs += [
            f"native-cran ({platform}, R {version}, manual)"
            for platform in self.coverage["cran_manual"]["os"]
            for version in self.coverage["cran_manual"]["r"]
        ]
        self.assertEqual(set(self.coverage["cran"]["r"]), {"devel", "release", "4.2.0"})
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
                    self.assertNotIn("--ignore-vignettes", check["run"])
                    continue
                self.assertEqual(chosen, settings)
                self.assertIn("--ignore-vignettes", check["run"])
                self.assertLess(
                    names.index("Confirm that no suggested package is installed"),
                    names.index("Check the R source package as CRAN does"),
                )
                for step in steps:
                    self.assertNotIn("setup-r-dependencies", step.get("uses", ""))
                    self.assertNotRegex(step.get("run", ""), r"install\.packages|pak::")

    def test_only_the_manual_lane_builds_and_validates_the_manuals(self):
        # CRAN's incoming checks build the PDF manual and validate the HTML
        # manual. One lane installs what they need before its check: pdflatex
        # with the LaTeX packages that R's Rd.sty loads, the Times fonts and
        # Inconsolata, which R's default manual fonts require, and HTML Tidy.
        # It runs check_r.py --manual, which fails unless R reports both
        # manual checks OK. Every other CRAN lane leaves the manuals out.
        packages = [
            "texlive-latex-base",
            "texlive-latex-recommended",
            "texlive-fonts-recommended",
            "texlive-fonts-extra",
            "tidy",
        ]
        lanes = {
            f"native-cran ({platform}, R {version}, manual)"
            for platform in self.coverage["cran_manual"]["os"]
            for version in self.coverage["cran_manual"]["r"]
        }
        self.assertEqual(lanes, {"native-cran (ubuntu-24.04, R devel, manual)"})
        for job in [name for name in self.jobs if name.startswith("native-cran ")]:
            steps = self.jobs[job]["definitions"]
            names = [step.get("name") for step in steps]
            check = names.index("Check the R source package as CRAN does")
            installed = [
                package
                for step in steps[:check]
                for package in packages
                if package in step.get("run", "").split()
            ]
            with self.subTest(job=job):
                self.assertEqual(
                    {
                        "manual": "--manual" in steps[check]["run"].split(),
                        "installed before the check": installed,
                    },
                    {
                        "manual": job in lanes,
                        "installed before the check": packages if job in lanes else [],
                    },
                )

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
        # Every platform that a wheel is built for, Linux aarch64 included
        # (#3030 D49), has a native-r lane of its own, and each Python lane
        # reads the reports that it uploads (D240 on #3673).
        lanes = self.coverage["qualify"]["include"]
        commands = self.cohort_commands()
        self.assertEqual(len(commands), len(lanes) + len(self.r_binary_lanes()))
        for lane, command in zip(lanes, commands[: len(lanes)], strict=True):
            artifact = f"native-r-qualification-{lane['os']}"
            job = self.jobs.get(f"native-r ({lane['os']})", {"uploads": []})
            with self.subTest(lane=lane):
                self.assertEqual(
                    {
                        "R evidence": Path(command[command.index("--r-evidence") + 1]),
                        "uploaded by the native-r job of its OS": artifact
                        in job["uploads"],
                    },
                    {
                        "R evidence": Path("cohort-input") / artifact / "r-source",
                        "uploaded by the native-r job of its OS": True,
                    },
                )

    def test_a_python_platform_without_a_native_r_lane_is_refused(self):
        # A Python lane is compared beside R reports of its own platform, never
        # beside another platform's or none.
        coverage = native.coverage()
        coverage["r"]["os"].remove("ubuntu-24.04-arm")
        with (
            patch.object(native.subprocess, "run"),
            self.assertRaisesRegex(
                ValueError, "the platform of Python lanes: ubuntu-24.04-arm$"
            ),
        ):
            native.compare_cohorts(coverage, ROOT, "a" * 40)

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

    def test_each_macos_r_binary_needs_no_macos_newer_than_the_arm64_floor(self):
        # R compiles entrypoint.c and links yamaa.so with clang, which takes
        # its minimum macOS from MACOSX_DEPLOYMENT_TARGET and otherwise from
        # the runner. The job sets the floor that configure.R gives Cargo when
        # the caller sets none, and the check reads the minimum that the
        # binary's yamaa.so declares and refuses one above that floor.
        configure = (ROOT / "R/yamaa/tools/configure.R").read_text(encoding="utf-8")
        [floor] = re.findall(
            r'Sys\.getenv\("MACOSX_DEPLOYMENT_TARGET", "([0-9.]+)"\)', configure
        )
        for platform, version in self.r_binary_lanes():
            job = f"native-r-binary ({platform}, R {version})"
            steps = {step.get("name"): step for step in self.jobs[job]["definitions"]}
            check = steps["Check the R binary package"]["run"]
            macos = check.partition('if [ "${RUNNER_OS}" = macOS ]; then')[2]
            with self.subTest(job=job):
                self.assertEqual(
                    self.jobs[job]["context"]["env"]["MACOSX_DEPLOYMENT_TARGET"], floor
                )
                for step in steps.values():
                    self.assertNotIn("MACOSX_DEPLOYMENT_TARGET", step.get("env", {}))
                self.assertIn("-C r-binary-check yamaa/libs/yamaa.so", macos)
                self.assertIn("otool -l r-binary-check/yamaa/libs/yamaa.so", macos)
                self.assertIn("""awk '$1 == "minos" { print $2 }'""", macos)
                self.assertIn(
                    """-e 'target <- Sys.getenv("MACOSX_DEPLOYMENT_TARGET")'""", macos
                )
                self.assertIn(
                    "stopifnot(package_version(minos) <= package_version(target))",
                    macos,
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
                selftest = titles.index("Run the installed package's selftest")
                conformance = titles.index(
                    "Report installed R public benchmark conformance"
                )
                for index in (install, selftest, conformance):
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

    def test_each_r_binary_install_finds_no_rust_on_the_runner_path(self):
        # A Rust tool that the removal left on the runner's own PATH, which
        # the steps that do not replace it inherit, fails the lane. So the
        # shell looks for one once both removal branches have closed, and
        # before the step reads that PATH to write the one that R gets, from
        # which a directory holding a Rust tool would be dropped unseen.
        for platform, version in self.r_binary_lanes():
            job = f"native-r-binary-install ({platform}, R {version})"
            steps = {step.get("name"): step for step in self.jobs[job]["definitions"]}
            script = steps["Remove Rust and Rtools from the runner"]["run"]
            with self.subTest(job=job):
                self.assertEqual(script.count('command -v "${tool}"'), 1)
                check = script.index('command -v "${tool}"')
                # The fi that closes the macOS and the other branch.
                uninstall = script.index("rustup self uninstall -y")
                removed = script.index("\nfi\n", uninstall)
                filtered = script.index('<<< "${PATH}"')
                self.assertLess(removed, check)
                self.assertLess(check, filtered)
                self.assertNotIn("PATH=", script[:check])
                self.assertIn("exit 1", script[check:filtered])
                # The shell would still report the rustup that it ran.
                self.assertLess(removed, script.index("hash -r"))
                self.assertLess(script.index("hash -r"), check)

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

    def test_each_install_without_a_network_gates_on_the_installed_selftest(self):
        # #3062: right after the installation, and before the conformance
        # corpus, each lane that installs a package without a network runs
        # the installed package's selftest with the host's own call. The call
        # prints the report, the facade's lines, to the log and fails the lane
        # unless the status is succeeded, and a release requires the step.
        # An offline source or wheel lane runs it twice: in the Linux
        # container, and on macOS and Windows. A wheel lane runs no corpus,
        # which native-qualify runs on every wheel, so its selftest ends it.
        selftest = "Run the installed package's selftest"
        calls = {
            "python": "import sys, yamaa; r = yamaa.selftest(); print(r); "
            'sys.exit(r.status != "succeeded")',
            "r": "r <- yamaa::yamaa_selftest(); print(r); "
            'quit(status = r$status != "succeeded")',
        }
        lanes = (
            [(job, host, 2) for job, host in self.offline_source_jobs()]
            + [(job, host, 1) for job, host in self.offline_toolchain_jobs()]
            + [
                (f"native-r-binary-install ({platform}, R {version})", "r", 1)
                for platform, version in self.r_binary_lanes()
            ]
            + [(job, host, 2) for job, host in self.offline_wheel_jobs()]
        )
        wheels = {job for job, _ in self.offline_wheel_jobs()}
        required = release.required_jobs(self.coverage)
        for job, host, runs in lanes:
            steps = self.jobs[job]["definitions"]
            names = [step.get("name") for step in steps]
            with self.subTest(job=job):
                self.assertEqual(names.count(selftest), 1)
                index = names.index(selftest)
                self.assertNotIn("if", steps[index])
                self.assertRegex(
                    names[index - 1], r"^Install the .+ without a network$"
                )
                if job in wheels:
                    self.assertEqual(index, len(names) - 1)
                else:
                    self.assertRegex(
                        names[index + 1],
                        r"^Report installed .+ public benchmark conformance$",
                    )
                position = required[job].index(selftest)
                self.assertEqual(
                    required[job][position - 1 : position + 2],
                    tuple(names[index - 1 : index + 2]),
                )
                script = " ".join(steps[index]["run"].replace("\\\n", " ").split())
                self.assertEqual(script.count(f"'{calls[host]}'"), runs)

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

    def test_each_cran_container_runs_the_steps_of_its_own_image(self):
        # The containers share one job. gcc16's image has Fedora's Rust;
        # clang-asan's has none, so that lane installs the minimum Rust, and
        # then confirms what the build used and what its sanitizers
        # instrumented. A step for one image runs on that image alone.
        own = {
            "gcc16": ["Install Fedora's Cargo and rustc, and Python"],
            "clang-asan": [
                "Install Python and xz, and name the symbolizer",
                "Install the minimum Rust from its standalone installer",
                "Confirm that the build used the minimum Rust",
                "Confirm that the sanitizers instrumented the crates' C",
            ],
        }
        self.assertEqual(self.coverage["cran_container"]["platform"], list(own))
        for platform in own:
            job = self.jobs[f"native-cran ({platform})"]
            conditions = {
                step["name"]: step["if"]
                for step in job["definitions"]
                if "matrix." in step.get("if", "")
            }
            with self.subTest(platform=platform):
                self.assertEqual(
                    {
                        name: evaluate(condition, job["context"])
                        for name, condition in conditions.items()
                    },
                    {
                        name: image == platform
                        for image, names in own.items()
                        for name in names
                    },
                )

    def test_cran_containers_install_declared_suggests_and_pandoc_before_checking(self):
        # The workflow seam must supply builders from DESCRIPTION as its
        # Suggests evolve, and provide Pandoc in each container's own OS.
        for platform, install in (
            ("gcc16", "dnf install -y cargo rust python3 pandoc"),
            (
                "clang-asan",
                "apt-get install --yes --no-install-recommends python3 xz-utils pandoc",
            ),
        ):
            steps = self.jobs[f"native-cran ({platform})"]["definitions"]
            names = [step.get("name") for step in steps]
            setup = steps[names.index("Install the package's Suggests")]["run"]
            with self.subTest(platform=platform):
                self.assertIn(install, "\n".join(s.get("run", "") for s in steps))
                self.assertIn('read.dcf("R/yamaa/DESCRIPTION")', setup)
                self.assertIn('"Suggests"', setup)
                self.assertIn("pak::pkg_install(packages", setup)
                self.assertIn("vapply(packages, requireNamespace", setup)
                self.assertNotIn('c("arrow", "jsonlite")', setup)
                self.assertLess(
                    names.index("Install the package's Suggests"),
                    names.index("Check the R source package as CRAN does"),
                )

    def test_the_sanitizer_lane_builds_with_the_minimum_rust_from_its_installer(self):
        # R-hub's clang-asan image carries no Rust. The lane installs the
        # workspace's rust-version from its standalone installer, verified as
        # native-offline-toolchain verifies it, with no rustup, and the
        # installation log must show that configure found that toolchain.
        steps = {
            step.get("name"): step
            for step in self.jobs["native-cran (clang-asan)"]["definitions"]
        }
        install = steps["Install the minimum Rust from its standalone installer"]["run"]
        confirm = steps["Confirm that the build used the minimum Rust"]["run"]
        self.assert_verifies_the_minimum_rust_installer(install)
        # Each command on one line, with single spaces.
        installation = " ".join(install.replace("\\\n", " ").split())
        self.assertIn(
            '"rust-${version}-${target}/install.sh" --prefix=/opt/rust '
            '--components="rustc,cargo,rust-std-${target}" --disable-ldconfig',
            installation,
        )
        self.assertLess(
            installation.index("--no-auto-key-retrieve --verify"),
            installation.index("/install.sh"),
        )
        for tool in ("rustc", "cargo"):
            with self.subTest(tool=tool):
                self.assertIn(
                    f'/opt/rust/bin/{tool} --version | grep -F "{tool} ${{version}} "',
                    installation,
                )
                self.assertIn(
                    f'grep -F "using {tool} ${{STANDALONE_RUST_VERSION}} " '
                    '"${RUNNER_TEMP}/cran-check/yamaa.Rcheck/00install.out"',
                    confirm,
                )
        self.assertIn('echo /opt/rust/bin >> "${GITHUB_PATH}"', installation)
        self.assertIn(
            'echo "STANDALONE_RUST_VERSION=${version}" >> "${GITHUB_ENV}"',
            installation,
        )
        self.assertNotIn("rustup", installation)
        order = [
            "Install Python and xz, and name the symbolizer",
            "Install the minimum Rust from its standalone installer",
            "Check the R source package as CRAN does",
            "Confirm that the build used the minimum Rust",
        ]
        self.assertEqual(sorted(order, key=list(steps).index), order)

    def test_the_sanitizer_lane_confirms_that_both_sanitizers_reach_the_crates_c(self):
        # The Makevars give Cargo R's C compiler, which carries the image's
        # -fsanitize flags. entrypoint.c calls neither sanitizer and the Rust
        # code is not instrumented, so calls to both in the installed shared
        # object show that the C of the crates was instrumented.
        steps = {
            step.get("name"): step
            for step in self.jobs["native-cran (clang-asan)"]["definitions"]
        }
        name = "Confirm that the sanitizers instrumented the crates' C"
        confirm = " ".join(steps[name]["run"].replace("\\\n", " ").split())
        self.assertIn(
            "nm -D --undefined-only "
            '"${RUNNER_TEMP}/cran-check/yamaa.Rcheck/yamaa/libs/yamaa.so"',
            confirm,
        )
        for symbols in ("' __asan_(report_)?(load|store)'", "' __ubsan_handle_'"):
            with self.subTest(symbols=symbols):
                self.assertIn(symbols, confirm)
        self.assertLess(
            list(steps).index("Check the R source package as CRAN does"),
            list(steps).index(name),
        )

    def cli_lanes(self):
        """List each command-line lane's platform and Rust target."""
        return [
            (lane["os"], lane["target"]) for lane in self.coverage["cli"]["include"]
        ]

    def test_each_cli_job_pairs_a_platform_with_its_rust_target(self):
        # #3030 D12, D49: static musl binaries for Linux x86_64 and aarch64,
        # the latter on an arm64 runner, the static C runtime for Windows x64,
        # and macOS arm64.
        self.assertEqual(
            sorted(self.cli_lanes()),
            [
                ("macos-15", "aarch64-apple-darwin"),
                ("ubuntu-24.04", "x86_64-unknown-linux-musl"),
                ("ubuntu-24.04-arm", "aarch64-unknown-linux-musl"),
                ("windows-2025", "x86_64-pc-windows-msvc"),
            ],
        )
        for prefix in ("native-cli", "native-cli-install"):
            names = [name for name in self.jobs if name.startswith(f"{prefix} (")]
            self.assertEqual(
                names,
                [
                    f"{prefix} ({platform}, {target})"
                    for platform, target in self.cli_lanes()
                ],
            )
            for name in names:
                with self.subTest(job=name):
                    job = self.jobs[name]
                    self.assertEqual(
                        expand(job["job"]["runs-on"], job["context"]),
                        job["context"]["matrix"]["os"],
                    )

    def test_each_cli_binary_builds_with_the_dist_profile_for_its_target(self):
        # The wheels' profile, release without the symbol table; never the R
        # build's release profile, which strips nothing. The toolchain is the
        # pinned one, installed as the other native jobs install it.
        build = (
            "cargo build --locked --profile dist --package yamaa-cli --bin yamaa "
            '--target "${TARGET}"'
        )
        for platform, target in self.cli_lanes():
            job = f"native-cli ({platform}, {target})"
            steps = {step.get("name"): step for step in self.jobs[job]["definitions"]}
            context = self.jobs[job]["context"]
            tools, binary = (
                steps["Install build tools"],
                steps["Build the command-line binary"],
            )
            with self.subTest(job=job):
                self.assertIn("python -m tools.ci.toolchain --install", tools["run"])
                self.assertIn('rustup target add "${TARGET}"', tools["run"])
                self.assertEqual(binary["working-directory"], "rust")
                self.assertEqual(binary["run"].count(build), 1)
                self.assertNotIn("--release", binary["run"])
                for step in (tools, binary, steps["Check the command-line binary"]):
                    self.assertEqual(expand(step["env"]["TARGET"], context), target)
                # Only the Windows target links the C runtime statically; the
                # musl targets are static already.
                self.assertEqual(
                    binary["env"]["CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUSTFLAGS"],
                    "-C target-feature=+crt-static",
                )
                self.assertNotIn("RUSTFLAGS", context["env"])
                # The Linux targets compile the bundled C sources with musl-gcc.
                self.assertIn(
                    "sudo apt-get install --yes --no-install-recommends musl-tools",
                    tools["run"],
                )
                self.assertIn('export "CC_${TARGET//-/_}=musl-gcc"', binary["run"])

    def test_each_cli_binary_is_checked_static_or_for_its_minimum_macos(self):
        configure = (ROOT / "R/yamaa/tools/configure.R").read_text(encoding="utf-8")
        [floor] = re.findall(
            r'Sys\.getenv\("MACOSX_DEPLOYMENT_TARGET", "([0-9.]+)"\)', configure
        )
        for platform, target in self.cli_lanes():
            job = f"native-cli ({platform}, {target})"
            steps = {step.get("name"): step for step in self.jobs[job]["definitions"]}
            check = steps["Check the command-line binary"]["run"]
            linux, _, rest = check.partition('elif [ "${RUNNER_OS}" = macOS ]; then')
            macos, _, windows = rest.partition("else")
            with self.subTest(job=job):
                # file and readelf: static, with no interpreter or library.
                self.assertIn('if [ "${RUNNER_OS}" = Linux ]; then', linux)
                self.assertIn("grep -E 'static-pie linked|statically linked'", linux)
                self.assertIn("grep INTERP; then exit 1; fi", linux)
                self.assertIn("grep NEEDED; then exit 1; fi", linux)
                # The minimum macOS that the binary declares, at most the
                # floor of the wheels and the R binaries (#3320).
                self.assertEqual(
                    self.jobs[job]["context"]["env"]["MACOSX_DEPLOYMENT_TARGET"], floor
                )
                self.assertIn("""awk '$1 == "minos" { print $2 }'""", macos)
                self.assertIn("sys.exit(minos > target)", macos)
                self.assertIn('"${minos}" "${MACOSX_DEPLOYMENT_TARGET}"', macos)
                # No C runtime DLL.
                self.assertIn("-dependents", windows)
                self.assertIn("vcruntime|msvcp|ucrtbase|api-ms-win-crt", windows)

    def test_each_cli_archive_is_checked_against_the_committed_notices(self):
        for platform, target in self.cli_lanes():
            job = f"native-cli ({platform}, {target})"
            steps = [step.get("name") for step in self.jobs[job]["definitions"]]
            runs = {
                step.get("name"): step.get("run", "")
                for step in self.jobs[job]["definitions"]
            }
            archive = runs["Archive the command-line binary"]
            check = runs["Check the command-line archive"].splitlines()
            with self.subTest(job=job):
                self.assertIn(
                    'python -m tools.packaging.cli_archive write "${TARGET}" '
                    '"${binary}" "${RUNNER_TEMP}/cli"',
                    archive,
                )
                # The generators prove the committed copies current first.
                self.assertEqual(
                    check,
                    [
                        "python -m tools.packaging.generate_third_party_notices",
                        "python -m tools.packaging.generate_component_inventory",
                        'python -m tools.packaging.cli_archive check "${RUNNER_TEMP}"/cli/*.zip',
                        'python -m tools.packaging.check_package_notices "${RUNNER_TEMP}"/cli/*.zip',
                    ],
                )
                self.assertLess(
                    steps.index("Check the command-line binary"),
                    steps.index("Archive the command-line binary"),
                )
                self.assertLess(
                    steps.index("Check the command-line archive"),
                    steps.index("Preserve the command-line archive"),
                )

    def test_each_cli_archive_is_unpacked_and_run_without_a_network(self):
        # D150 criterion 4 (#3030): nothing from the build but the archive.
        # The steps before the unpacking, which reach package mirrors and the
        # image registry, never run the binary. The unpacking and each step
        # after it that runs a command go behind a proxy that refuses every
        # connection (macOS and Windows) and run in a container without a
        # network and with a read-only root, as the runner's user, from the
        # image pulled beforehand (Linux).
        proxy = {
            "http_proxy": "http://127.0.0.1:9",
            "HTTPS_PROXY": "http://127.0.0.1:9",
            "ALL_PROXY": "http://127.0.0.1:9",
            "NO_PROXY": "",
        }
        names = {name.lower() for name in proxy}
        required = release.required_jobs(self.coverage)
        for platform, target in self.cli_lanes():
            job = f"native-cli-install ({platform}, {target})"
            definitions = self.jobs[job]["definitions"]
            titles = [step.get("name") for step in definitions]
            steps = {step.get("name"): step for step in definitions}
            unpack = titles.index("Unpack the command-line archive without a network")
            prepare = titles.index("Prepare the command-line conformance corpus")
            with self.subTest(job=job):
                self.assertEqual(self.jobs[job]["job"]["needs"], "cli")
                self.assertEqual(self.jobs[job]["downloads"], [f"native-cli-{target}"])
                self.assertEqual(
                    required[job],
                    (
                        "Unpack the command-line archive without a network",
                        "Run the command-line selftest",
                        "Report command-line public benchmark conformance",
                    ),
                )
                self.assertEqual(
                    [
                        step.get("name")
                        for step in definitions[unpack:]
                        if "run" in step
                    ],
                    list(required[job]),
                )
                # The image of the containers is pulled while the network is up.
                self.assertLess(prepare, unpack)
                pull = steps["Prepare the command-line conformance corpus"]["run"]
                self.assertIn("from tools.ci.manylinux import IMAGES", pull)
                self.assertIn('echo "OFFLINE_IMAGE=${image}"', pull)
                self.assertIn('docker pull "${image}"', pull)
                inherited = {name.lower() for name in self.jobs[job]["context"]["env"]}
                self.assertFalse(names & inherited)
                for index, step in enumerate(definitions):
                    chosen = {
                        name: value
                        for name, value in step.get("env", {}).items()
                        if name.lower() in names
                    }
                    if index < unpack or "run" not in step:
                        self.assertEqual(chosen, {})
                        self.assertNotIn("unpacked/", step.get("run", ""))
                        continue
                    self.assertEqual(chosen, proxy)
                    self.assertNotIn("if", step)
                    containers = [
                        line
                        for line in step["run"].replace("\\\n", " ").splitlines()
                        if "docker run" in line
                    ]
                    self.assertEqual(len(containers), 1)
                    self.assertEqual(containers[0].count("--network"), 1)
                    for option in (
                        "--network none",
                        "--read-only",
                        '--user "$(id -u):$(id -g)"',
                        '"${OFFLINE_IMAGE}"',
                    ):
                        self.assertIn(option, containers[0])
                # unzip restores the executable's mode, in the container on
                # Linux and on macOS; Expand-Archive unpacks on Windows.
                unpacking = definitions[unpack]["run"]
                self.assertEqual(
                    unpacking.count('unzip "${archives[0]}" -d unpacked'), 2
                )
                self.assertIn("Expand-Archive -LiteralPath", unpacking)
                # The version and the selftest of the same binary, which on
                # Linux sees the unpacked archive alone, read-only.
                run = steps["Run the command-line selftest"]["run"]
                self.assertIn('yamaa=("${binary}")', run)
                self.assertIn('--volume "${PWD}/unpacked:/work/unpacked:ro"', run)
                self.assertIn('"${OFFLINE_IMAGE}" "/work/${binary}")', run)
                self.assertIn('"${yamaa[@]}" --version | tee cli-version.txt', run)
                self.assertIn('if [ "${first}" != "yamaa ${version}" ]; then', run)
                self.assertTrue(run.rstrip().endswith('"${yamaa[@]}" selftest'))

    def test_each_cli_archive_runs_the_corpus_written_as_1_0_after_its_selftest(self):
        # #3060: while the network is up, the repository's tools copy the
        # corpus (D122) and plan it; after the selftest, the command line's
        # runner runs it through the unpacked binary and the comparator judges
        # the records. No Rust.
        required = release.required_jobs(self.coverage)
        directory = "${{ runner.temp }}/offline"
        for platform, target in self.cli_lanes():
            job = f"native-cli-install ({platform}, {target})"
            definitions = self.jobs[job]["definitions"]
            titles = [step.get("name") for step in definitions]
            steps = {step.get("name"): step for step in definitions}
            prepare = steps["Prepare the command-line conformance corpus"]
            conformance = steps["Report command-line public benchmark conformance"]
            upload = steps[
                "Preserve command-line public benchmark inventory and saved artifacts"
            ]
            script = " ".join(conformance["run"].replace("\\\n", " ").split())
            with self.subTest(job=job):
                self.assertEqual(
                    [step.get("uses", "").split("@")[0] for step in definitions],
                    [
                        "actions/download-artifact",
                        "",
                        "actions/checkout",
                        "actions/setup-python",
                        "",
                        "",
                        "",
                        "",
                        "",
                        "actions/upload-artifact",
                    ],
                )
                self.assertEqual(
                    definitions[1]["run"].split(),
                    "git config --global core.autocrlf false "
                    "git config --global core.symlinks true".split(),
                )
                self.assertIn(
                    "python -m pip install py-yaml12==0.2.0 pyyaml==6.0.3 "
                    "pyarrow==25.0.1",
                    steps["Install repository tools"]["run"],
                )
                self.assertIn(
                    "python -I -m tools.commands.cli_conformance --prepare-only "
                    "--output public-conformance-cli",
                    prepare["run"],
                )
                self.assertEqual(
                    titles[titles.index("Install repository tools") + 1 :][:4],
                    [
                        "Prepare the command-line conformance corpus",
                        *required[job],
                    ],
                )
                # The archive, the prepared corpus and the records share one
                # directory, which the Linux containers mount.
                self.assertEqual(definitions[0]["with"]["path"], f"{directory}/cli")
                for title in (
                    "Prepare the command-line conformance corpus",
                    *required[job],
                ):
                    self.assertEqual(steps[title]["working-directory"], directory)
                # The binary that passed the selftest, with Windows' suffix.
                self.assertIn(
                    'binary="unpacked/$(basename "${archives[0]}" .zip)/yamaa"', script
                )
                self.assertIn('binary="${binary}.exe"', script)
                self.assertIn(
                    'revision=$(git -C "${GITHUB_WORKSPACE}" rev-parse HEAD)', script
                )
                self.assertIn(
                    'evidence="${GITHUB_SERVER_URL}/${GITHUB_REPOSITORY}/actions/runs/'
                    '${GITHUB_RUN_ID}"',
                    script,
                )
                # The runner over the prepared copies: with the image's CPython
                # in the container on Linux, which mounts the checkout
                # read-only, and with the runner's Python on macOS and Windows.
                self.assertIn(
                    "corpus=(--plan public-conformance-cli/plan "
                    "--fixtures public-conformance-cli/benchmarks "
                    '--binary "${binary}" --output public-conformance-cli/observed '
                    '--source-revision "${revision}" --evidence "${evidence}")',
                    script,
                )
                self.assertIn('--volume "${GITHUB_WORKSPACE}:/checkout:ro"', script)
                self.assertIn(
                    '"${OFFLINE_IMAGE}" "${OFFLINE_PYTHON}" -I '
                    '/checkout/rust/tests/public_conformance_cli.py "${corpus[@]}"',
                    script,
                )
                self.assertIn(
                    'python -I "${GITHUB_WORKSPACE}/rust/tests/public_conformance_cli.py" '
                    '"${corpus[@]}"',
                    script,
                )
                self.assertTrue(
                    script.endswith(
                        "python -I -m tools.conformance.public_conformance compare "
                        "--fixtures public-conformance-cli/benchmarks "
                        "--observed public-conformance-cli/observed "
                        "--output public-conformance-cli/inventory.json"
                    )
                )
                self.assertEqual(
                    self.jobs[job]["uploads"], [f"public-conformance-cli-{target}"]
                )
                self.assertEqual(
                    upload["with"]["path"].splitlines(),
                    [
                        f"{directory}/public-conformance-cli/inventory.json",
                        f"{directory}/public-conformance-cli/observed",
                    ],
                )
                self.assertIn(
                    "steps.cli_conformance.outcome == 'failure'", upload["if"]
                )
                for step in definitions:
                    for tool in ("tools.ci.toolchain", "cargo ", "rustup"):
                        self.assertNotIn(tool, step.get("run", ""))

    def test_every_cli_archive_joins_the_cohort_comparison_of_its_wheel(self):
        # Each archive's saved outputs are compared with the complete reports
        # of its platform's wheel lane with the build Python and the R
        # reports beside them, in that lane's one comparison.
        joined = {
            Path(command[command.index("--cli-evidence") + 1]): command
            for command in self.cohort_commands()
            if "--cli-evidence" in command
        }
        self.assertEqual(
            sorted(joined),
            sorted(
                Path("cohort-input") / f"public-conformance-cli-{target}"
                for _, target in self.cli_lanes()
            ),
        )
        for platform, target in self.cli_lanes():
            command = joined[Path("cohort-input") / f"public-conformance-cli-{target}"]
            with self.subTest(platform=platform, target=target):
                self.assertEqual(
                    Path(command[command.index("--python-evidence") + 1]),
                    Path("cohort-input")
                    / f"native-qualification-{platform}-{native.BUILD_PYTHON}-wheel"
                    / "wheel",
                )
                self.assertEqual(
                    Path(command[command.index("--r-evidence") + 1]),
                    Path("cohort-input")
                    / f"native-r-qualification-{platform}"
                    / "r-source",
                )
        cohort = self.jobs["public-cohort-complete-runs"]["job"]
        self.assertIn("cli-install", cohort["needs"])
        self.assertIn(
            "public-conformance-cli-*",
            [step.get("with", {}).get("pattern") for step in cohort["steps"]],
        )

    def test_a_cli_target_without_a_wheel_lane_to_join_is_refused(self):
        coverage = native.coverage()
        coverage["cli"]["include"].append({"os": "freebsd-14", "target": "x86_64"})
        with (
            patch.object(native.subprocess, "run"),
            self.assertRaisesRegex(ValueError, "command-line targets: freebsd-14$"),
        ):
            native.compare_cohorts(coverage, ROOT, "a" * 40)

    def share_lanes(self):
        """List each share lane's platform and route."""
        return list(
            itertools.product(
                self.coverage["share"]["os"], self.coverage["share"]["route"]
            )
        )

    def test_the_corpus_runs_on_a_windows_share_by_unc_path_and_mapped_drive(self):
        # #3030 D15 admits a Windows network share, and a drive mapped to one,
        # for every file that a run reads or saves. The engine's walk serves
        # every host, so the wheel that a release publishes for Windows,
        # installed with the build Python, runs the corpus there by each
        # route, and a release requires each lane's steps.
        self.assertEqual(
            self.coverage["share"],
            {"os": ["windows-2025"], "route": ["unc-path", "mapped-drive"]},
        )
        required = release.required_jobs(self.coverage)
        producers = release_assets.artifacts(release_assets.declared(self.coverage))
        for platform, route in self.share_lanes():
            job = f"native-share ({platform}, {route})"
            details = self.jobs[job]
            names = [step.get("name") for step in details["definitions"]]
            [setup] = [
                step
                for step in details["definitions"]
                if step.get("uses", "").split("@")[0] == "actions/setup-python"
            ]
            wheel = f"native-python-{platform}-{native.BUILD_PYTHON}"
            with self.subTest(job=job):
                self.assertEqual(details["job"]["needs"], "dist")
                self.assertEqual(
                    expand(details["job"]["runs-on"], details["context"]), platform
                )
                self.assertIn(wheel, producers)
                self.assertEqual(
                    details["downloads"],
                    [wheel, f"native-project-{platform}-{native.BUILD_PYTHON}"],
                )
                self.assertEqual(
                    expand(setup["with"]["python-version"], details["context"]),
                    native.BUILD_PYTHON,
                )
                self.assertEqual(
                    required[job],
                    (
                        "Install the wheel",
                        "Share a directory over SMB",
                        "Report installed Python public benchmark conformance "
                        "on the share",
                    ),
                )
                self.assertEqual(
                    [name for name in names if name in required[job]],
                    list(required[job]),
                )
                self.assertEqual(
                    details["uploads"],
                    [f"public-conformance-python-share-{platform}-{route}"],
                )

    def test_each_share_lane_names_the_staged_cases_through_its_route_alone(self):
        # The runner stages and records each case in the shared directory, as
        # every lane stages it, and names it in every call through the lane's
        # route: the share's UNC path, or a drive that the same step maps to
        # the share, since a mapping belongs to the logon session that made
        # it. The comparator reads the cases where they were staged.
        # PowerShell stops on a failed cmdlet, and on a failed program only
        # where the step checks its exit status, so it checks each one.
        for platform, route in self.share_lanes():
            job = f"native-share ({platform}, {route})"
            details = self.jobs[job]
            steps = {step.get("name"): step for step in details["definitions"]}
            share = steps["Share a directory over SMB"]
            conformance = steps[
                "Report installed Python public benchmark conformance on the share"
            ]
            upload = steps[
                "Preserve Python public benchmark inventory and saved artifacts"
            ]
            script = conformance["run"]
            with self.subTest(job=job):
                for step in (share, conformance):
                    self.assertEqual(step["shell"], "pwsh")
                    self.assertNotIn("if", step)
                self.assertIn("(Join-Path $env:RUNNER_TEMP 'share')", share["run"])
                self.assertIn(
                    "New-SmbShare -Name yamaa-share -Path $directory.FullName",
                    share["run"],
                )
                self.assertIn(
                    r"Test-Path -LiteralPath '\\localhost\yamaa-share' "
                    "-PathType Container",
                    share["run"],
                )
                self.assertEqual(conformance["working-directory"], "${{ runner.temp }}")
                self.assertEqual(
                    expand(conformance["env"]["ROUTE"], details["context"]), route
                )
                # A branch for each route that native.py declares.
                for each in self.coverage["share"]["route"]:
                    self.assertEqual(script.count(f"$env:ROUTE -eq '{each}'"), 1)
                self.assertIn(r"$root = '\\localhost\yamaa-share\'", script)
                mapping = r"net use Y: '\\localhost\yamaa-share' /persistent:no"
                self.assertIn(mapping, script)
                self.assertIn("[System.IO.DriveType]::Network", script)
                self.assertIn(r"$root = 'Y:\'", script)
                runner = (
                    r"python -I (Join-Path $env:GITHUB_WORKSPACE "
                    r"'python\tests\installed\public_conformance.py') @corpus"
                )
                self.assertIn(
                    "'--output', 'share\\observed', '--output-as', \"${root}observed\"",
                    script,
                )
                self.assertIn("'--package-form', 'wheel'", script)
                compare = (
                    r"public_conformance compare --fixtures $benchmarks "
                    r"--observed 'share\observed' "
                    r"--output 'public-conformance\inventory.json'"
                )
                order = [
                    mapping,
                    "public_conformance plan --fixtures $benchmarks",
                    runner,
                    compare,
                ]
                self.assertEqual(sorted(order, key=script.index), order)
                programs = script.count("python -I") + script.count("net use ")
                self.assertEqual(
                    script.count("if ($LASTEXITCODE) { exit $LASTEXITCODE }"), programs
                )
                self.assertEqual(
                    upload["with"]["path"].splitlines(),
                    [
                        "${{ runner.temp }}/public-conformance",
                        "${{ runner.temp }}/share/observed",
                    ],
                )


if __name__ == "__main__":
    unittest.main()
