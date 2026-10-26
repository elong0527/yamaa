"""The pinned, minimum and beta Rust toolchains that CI activates."""

import contextlib
import io
import re
import tempfile
import unittest
from pathlib import Path
from unittest import mock

from tools.ci import toolchain
from tools.commands import repository
from tools.paths import ROOT
from tools.tests import support

# The commands that run the third-party notices generator, which reads the
# toolchain's rust-src: the generator itself, the Make targets whose
# repository check runs it (tools.commands.repository.CHECKS) and the
# dependency graphs lane, which regenerates the notices of its copy.
GENERATOR = re.compile(
    r"python -m tools\.packaging\.generate_third_party_notices\b"
    r"|\bmake (?:check-repository|quick|check)(?![\w-])"
    r"|python -m tools\.ci\.dependency_graphs\b"
)
SETUP_ACTION = "./.github/actions/setup"


def release(version):
    return tuple(map(int, version.split(".")))


def workflow_jobs():
    """Yield each workflow job, as workflow:job, with its steps."""
    for path in sorted((ROOT / ".github/workflows").iterdir()):
        if path.suffix in (".yml", ".yaml"):
            for name, job in support.load_yaml(path)["jobs"].items():
                yield f"{path.name}:{name}", job.get("steps", [])


def adds_rust_src(step):
    """Whether a step adds rust-src: the setup action given notices, or the
    toolchain module given --notices."""
    if step.get("uses") == SETUP_ACTION:
        return (step.get("with") or {}).get("notices") == "true"
    return any(
        "python -m tools.ci.toolchain" in line and "--notices" in line.split()
        for line in step.get("run", "").splitlines()
    )


class ToolchainTest(unittest.TestCase):
    def test_minimum_completes_the_workspace_rust_version(self):
        for declared, expected in (
            ("1.90", "1.90.0"),
            ("1.90.1", "1.90.1"),
            ("2", "2.0.0"),
        ):
            with (
                self.subTest(declared=declared),
                tempfile.TemporaryDirectory() as directory,
            ):
                workspace = Path(directory)
                (workspace / "Cargo.toml").write_text(
                    f'[workspace.package]\nrust-version = "{declared}"\n'
                )
                with mock.patch.object(toolchain, "WORKSPACE", workspace):
                    self.assertEqual(toolchain.minimum(), expected)

    def test_the_pinned_channel_is_not_older_than_the_minimum(self):
        self.assertGreaterEqual(
            release(toolchain.channel()), release(toolchain.minimum())
        )

    def test_the_declaration_names_only_what_a_source_build_needs(self):
        # rustup installs every component that rust-toolchain.toml lists for
        # every build in rust/; repository checks add theirs (install_command).
        # No source package carries the file (#3030 D55).
        self.assertEqual(
            toolchain.declaration(),
            {"channel": toolchain.channel(), "profile": "minimal"},
        )

    def test_the_r_package_states_the_rust_its_build_uses(self):
        # The R package carries no toolchain file, so it compiles with the
        # caller's toolchain, whose Cargo and rustc must each meet the
        # rust-version; the field uses the wording of CRAN's guidance for
        # packages using Rust, with both versions, since the two can be
        # packaged separately. configure reads the rust-version itself
        # (tools/tests/test_r_configure.py).
        description = (ROOT / "R/yamaa/DESCRIPTION").read_text(encoding="utf-8")
        field = re.search(
            r"^SystemRequirements:(.*(?:\n[ \t].*)*)", description, re.MULTILINE
        )
        self.assertIsNotNone(field)
        minimum = toolchain.minimum()
        self.assertEqual(
            " ".join(field.group(1).split()),
            f"Cargo (Rust's package manager) (>= {minimum}), rustc (>= {minimum})",
        )

    def test_repository_checks_install_their_lint_and_format_components(self):
        # Clippy and rustfmt lint and format. rust-src holds the standard
        # library's sources, from which the third-party notices list the
        # crates that it links, so it comes only with notices.
        install = ["rustup", "toolchain", "install", "1.99.1", "--profile", "minimal"]
        for declared, notices, components in (
            ({}, False, ["clippy", "rustfmt"]),
            ({}, True, ["clippy", "rustfmt", "rust-src"]),
            # A declared component is installed once, before the others.
            (
                {"components": ["rustfmt", "llvm-tools"]},
                True,
                ["rustfmt", "llvm-tools", "clippy", "rust-src"],
            ),
        ):
            with self.subTest(declared=declared, notices=notices):
                self.assertEqual(
                    toolchain.install_command(
                        {"channel": "1.99.1", "profile": "minimal", **declared},
                        notices,
                    ),
                    [
                        *install,
                        "--no-self-update",
                        *[
                            argument
                            for component in components
                            for argument in ("--component", component)
                        ],
                    ],
                )

    def test_install_adds_rust_src_only_with_notices(self):
        for arguments, notices in (
            (["--install"], False),
            (["--install", "--notices"], True),
        ):
            with (
                self.subTest(arguments=arguments),
                mock.patch.dict(toolchain.os.environ, {"GITHUB_ENV": ""}),
                mock.patch.object(toolchain.subprocess, "run") as run,
            ):
                toolchain.main(arguments)
                run.assert_called_once_with(
                    toolchain.install_command(toolchain.declaration(), notices),
                    check=True,
                )

    def test_only_the_jobs_that_run_the_notices_generator_add_rust_src(self):
        # D157 (#3030) adds rust-src for the notices generator alone; every
        # other job builds, tests and lints without it (#3528).
        self.assertIn("tools.packaging.generate_third_party_notices", repository.CHECKS)
        self.assertIn(
            '"-m", "tools.packaging.generate_third_party_notices"',
            (ROOT / "tools/ci/dependency_graphs.py").read_text(encoding="utf-8"),
        )
        generators, sources, named = set(), set(), []
        for job, steps in workflow_jobs():
            if any(GENERATOR.search(step.get("run", "")) for step in steps):
                generators.add(job)
            if any(adds_rust_src(step) for step in steps):
                sources.add(job)
            # rust-src comes only from NOTICE_COMPONENTS.
            named += [job for step in steps if "rust-src" in step.get("run", "")]
        self.assertEqual(sources, generators)
        self.assertEqual(named, [])
        # So that a pattern that matches nothing cannot pass.
        self.assertLessEqual(
            {
                "ci.yml:repository",
                "dependency-graphs.yml:graph",
                "native.yml:cli",
                "native.yml:dist",
            },
            generators,
        )

    def test_the_setup_action_adds_rust_src_only_when_given_notices(self):
        steps = support.load_yaml(".github/actions/setup/action.yml")["runs"]["steps"]
        activate = {step.get("name"): step for step in steps}[
            "Activate the workspace toolchain"
        ]
        self.assertEqual(activate["env"], {"NOTICES": "${{ inputs.notices }}"})
        self.assertEqual(
            activate["run"].splitlines(),
            [
                'if [ "$NOTICES" = true ]; then',
                "  python -m tools.ci.toolchain --install --notices",
                "else",
                "  python -m tools.ci.toolchain --install",
                "fi",
            ],
        )
        # The setup command keeps that toolchain: make setup adds rust-src,
        # which make quick needs.
        self.assertEqual(
            [
                step["run"]
                for step in steps
                if re.search(r"tools\.commands\.setup|make setup", step.get("run", ""))
            ],
            [
                "python -m tools.commands.setup --keep-toolchain",
                "python -m tools.commands.setup --keep-toolchain --without-package",
            ],
        )


def activate(*arguments):
    """Run the module's command with rustup mocked; return the RUSTUP_TOOLCHAIN
    that it writes to GITHUB_ENV and the rustup commands that it runs."""
    with tempfile.TemporaryDirectory() as directory:
        environment = Path(directory) / "github-env"
        with (
            mock.patch.dict(toolchain.os.environ, {"GITHUB_ENV": str(environment)}),
            mock.patch.object(toolchain.subprocess, "run") as run,
        ):
            toolchain.main(list(arguments))
        lines = environment.read_text(encoding="utf-8").splitlines()
    written = dict(line.split("=", 1) for line in lines)
    return written["RUSTUP_TOOLCHAIN"], [call.args[0] for call in run.call_args_list]


class ToolchainSelectionTest(unittest.TestCase):
    """Which toolchain a CI job activates: the pinned channel, the workspace
    rust-version that the minimum-toolchain jobs lint with, or another rustup
    channel, such as the beta that the Next toolchain workflow tests."""

    def test_the_default_activates_the_pinned_channel_and_installs_nothing(self):
        self.assertEqual(activate(), (toolchain.channel(), []))

    def test_minimum_activates_the_workspace_rust_version(self):
        activated, commands = activate("--minimum")
        self.assertEqual(activated, toolchain.minimum())
        self.assertRegex(activated, r"^\d+\.\d+\.\d+$")
        self.assertEqual(commands, [])

    def test_channel_activates_another_rustup_channel(self):
        self.assertEqual(activate("--channel", "beta"), ("beta", []))

    def test_minimum_and_channel_each_select_the_toolchain(self):
        with (
            contextlib.redirect_stderr(io.StringIO()) as stderr,
            self.assertRaises(SystemExit),
        ):
            activate("--minimum", "--channel", "beta")
        self.assertIn(
            "--minimum and --channel each select the toolchain", stderr.getvalue()
        )

    def test_install_adds_the_check_components_to_the_selected_toolchain(self):
        declared = toolchain.declaration()
        for arguments, channel in (
            ((), declared["channel"]),
            (("--minimum",), toolchain.minimum()),
            (("--channel", "beta"), "beta"),
            (
                ("--minimum", "--host", "x86_64-pc-windows-gnu"),
                f"{toolchain.minimum()}-x86_64-pc-windows-gnu",
            ),
        ):
            with self.subTest(arguments=arguments):
                activated, commands = activate("--install", *arguments)
                self.assertEqual(activated, channel)
                self.assertEqual(
                    commands,
                    [toolchain.install_command({**declared, "channel": channel})],
                )
                [command] = commands
                self.assertEqual(
                    command[:4], ["rustup", "toolchain", "install", channel]
                )
                for component in toolchain.CHECK_COMPONENTS:
                    self.assertIn(component, command)
                for component in toolchain.NOTICE_COMPONENTS:
                    self.assertNotIn(component, command)


if __name__ == "__main__":
    unittest.main()
