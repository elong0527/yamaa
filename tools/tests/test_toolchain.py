"""The pinned and minimum Rust toolchains that CI activates."""

import re
import tempfile
import unittest
from pathlib import Path
from unittest import mock

from tools.ci import toolchain
from tools.paths import ROOT


def release(version):
    return tuple(map(int, version.split(".")))


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
        # Both source packages carry rust-toolchain.toml unchanged, and rustup
        # installs every component it lists during an R source installation.
        self.assertEqual(
            toolchain.declaration(),
            {"channel": toolchain.channel(), "profile": "minimal"},
        )

    def test_the_r_package_states_the_rust_its_build_uses(self):
        # Under rustup the R build runs Cargo inside the bundled workspace, so
        # the pinned channel compiles it; other builds need the rust-version.
        description = (ROOT / "R/yamaa/DESCRIPTION").read_text(encoding="utf-8")
        field = re.search(
            r"^SystemRequirements:(.*(?:\n[ \t].*)*)", description, re.MULTILINE
        )
        self.assertIsNotNone(field)
        requirements = " ".join(field.group(1).split())
        self.assertIn(f"rustc {toolchain.channel()} ", requirements)
        self.assertIn(f"rustc >= {toolchain.minimum()})", requirements)

    def test_repository_checks_install_their_lint_and_format_components(self):
        install = ["rustup", "toolchain", "install", "1.99.1", "--profile", "minimal"]
        for declared, components in (
            ({}, ["clippy", "rustfmt"]),
            # A declared component is installed once, before the check components.
            (
                {"components": ["rustfmt", "llvm-tools"]},
                ["rustfmt", "llvm-tools", "clippy"],
            ),
        ):
            with self.subTest(declared=declared):
                self.assertEqual(
                    toolchain.install_command(
                        {"channel": "1.99.1", "profile": "minimal", **declared}
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


if __name__ == "__main__":
    unittest.main()
