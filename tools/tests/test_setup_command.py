"""The environment that `make setup` and the CI setup action install."""

import tomllib
import unittest
from unittest import mock

from tools.commands import setup
from tools.paths import ROOT


def commands(*arguments):
    """The commands that setup runs where rustup is absent."""
    with (
        mock.patch.object(setup.shutil, "which", return_value=None),
        mock.patch.object(setup.subprocess, "run") as run,
        mock.patch.object(setup.subprocess, "check_output", return_value="python\n"),
    ):
        setup.main(["--uv", "uv", *arguments])
    return [call.args[0] for call in run.call_args_list]


class SetupCommand(unittest.TestCase):
    def test_the_default_builds_the_development_package(self):
        sync, tools = commands()
        self.assertEqual(sync[:2], ["uv", "sync"])
        self.assertNotIn("--no-install-package", sync)
        self.assertEqual(
            tools,
            ["uv", "pip", "install", "--python", "python"]
            + ["--no-deps", "--editable", str(ROOT)],
        )

    def test_without_package_syncs_the_same_environment_without_the_package(self):
        # The CI r job builds the R package's own engine and imports no Python
        # package, so it leaves the development package out.
        package = tomllib.loads(
            (ROOT / "python/pyproject.toml").read_text(encoding="utf-8")
        )["project"]["name"]
        default = commands()
        without = commands("--without-package")
        self.assertEqual(without[0], [*default[0], "--no-install-package", package])
        self.assertEqual(without[1:], default[1:])


if __name__ == "__main__":
    unittest.main()
