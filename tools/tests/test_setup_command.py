"""The environment that `make setup` and the CI setup action install."""

import tempfile
import tomllib
import unittest
from pathlib import Path
from unittest import mock

from tools.ci import toolchain
from tools.commands import setup
from tools.paths import ROOT


def commands(*arguments, rustup=None, package="python"):
    """The commands that setup runs, where rustup is absent unless `rustup`
    names it, and the installed package's directory is `package`."""
    with (
        mock.patch.object(setup.shutil, "which", return_value=rustup),
        mock.patch.object(setup.subprocess, "run") as run,
        mock.patch.object(
            setup.subprocess, "check_output", side_effect=["python\n", f"{package}\n"]
        ),
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

    def test_where_rustup_runs_setup_adds_what_make_quick_needs(self):
        # Clippy and rustfmt for its lint, and rust-src for its notices check.
        install, *rest = commands(rustup="rustup")
        self.assertEqual(
            install, toolchain.install_command(toolchain.declaration(), notices=True)
        )
        self.assertIn("rust-src", install)
        self.assertEqual(rest, commands())

    def test_the_loaded_mode_installs_a_build_that_links_no_engine(self):
        # The locked environment holds the linked build; the loaded mode
        # builds the wheel without its linked engine, installs it in that
        # build's place and puts the engine library beside its module.
        with (
            mock.patch.dict(setup.os.environ, {"YAMAA_ENGINE": "loaded"}),
            mock.patch.object(
                setup.loaded, "build_engine", return_value=Path("engine")
            ),
            mock.patch.object(setup.loaded, "install_beside") as beside,
        ):
            sync, tools, build, install = commands()
        self.assertEqual(
            (sync, tools, build[:4], build[-2:], install, beside.call_args.args),
            (
                *commands(),
                ["uv", "build", "python", "--wheel"],
                [
                    "--config-setting",
                    "build-args=--profile=dev --no-default-features "
                    "--features extension-module,dynamic-engine",
                ],
                [
                    "uv",
                    "pip",
                    "install",
                    "--python",
                    "python",
                    "--no-deps",
                    "--reinstall",
                ],
                ("python", Path("engine")),
            ),
        )

    def test_a_linked_setup_after_a_loaded_one_syncs_the_linked_build_again(self):
        # An engine library beside the module shows that a loaded build was
        # installed last; uv keeps an installed build it considers current.
        with tempfile.TemporaryDirectory() as package:
            library = Path(package, setup.loaded.LIBRARY)
            library.write_bytes(b"")
            sync, tools, again = commands(package=package)
            self.assertEqual(
                (again, library.exists()),
                ([*sync, "--reinstall-package", "yamaa"], False),
            )

    def test_keep_toolchain_adds_no_components(self):
        # CI's setup action installs the toolchain first, with rust-src only
        # for the jobs that run the notices generator.
        self.assertEqual(commands("--keep-toolchain", rustup="rustup"), commands())


if __name__ == "__main__":
    unittest.main()
