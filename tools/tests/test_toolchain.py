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

    def test_repository_checks_install_their_lint_format_and_source_components(self):
        # Clippy and rustfmt lint and format; rust-src holds the standard
        # library's sources, from which the third-party notices list the
        # crates that it links.
        install = ["rustup", "toolchain", "install", "1.99.1", "--profile", "minimal"]
        for declared, components in (
            ({}, ["clippy", "rustfmt", "rust-src"]),
            # A declared component is installed once, before the check components.
            (
                {"components": ["rustfmt", "llvm-tools"]},
                ["rustfmt", "llvm-tools", "clippy", "rust-src"],
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
