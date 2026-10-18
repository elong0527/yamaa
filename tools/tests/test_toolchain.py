"""The pinned and minimum Rust toolchains that CI activates."""

import tempfile
import unittest
from pathlib import Path
from unittest import mock

from tools.ci import toolchain


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


if __name__ == "__main__":
    unittest.main()
