"""The host bindings stay out of the workspace tests and lint (#3106).

The bindings have no Rust tests; the installed Python suites and the R package
check test them. Building them compiles PyO3 and extendr, whose builds need
Python and R. So the workspace tests leave both out, the workspace Clippy
leaves out the R binding, and `make test-r`, which runs where R is installed,
lints it instead.
"""

import tomllib
import unittest
from unittest import mock

from tools.commands import repository, rust_tests
from tools.paths import ROOT, WORKSPACE
from tools.tests import support


def members():
    """Map each workspace member's package name to its directory and manifest."""
    workspace = tomllib.loads((WORKSPACE / "Cargo.toml").read_text(encoding="utf-8"))
    found = {}
    for member in workspace["workspace"]["members"]:
        directory = WORKSPACE / member
        manifest = tomllib.loads((directory / "Cargo.toml").read_text(encoding="utf-8"))
        found[manifest["package"]["name"]] = (directory, manifest)
    return found


def repository_commands(command):
    """The commands that `python -m tools.commands.repository <command>` runs."""
    commands = []
    with (
        mock.patch.object(
            repository, "run", lambda *arguments, **_: commands.append(arguments)
        ),
        mock.patch.object(repository.fuzz, "copy_lock"),
    ):
        repository.main([command])
    return commands


def recipe(target):
    """The recipe lines of a target in the root Makefile."""
    lines = (ROOT / "Makefile").read_text(encoding="utf-8").splitlines()
    body = []
    for line in lines[lines.index(f"{target}:") + 1 :]:
        if not line.startswith("\t"):
            break
        body.append(line.strip())
    return body


class HostBindings(unittest.TestCase):
    def test_the_bindings_are_the_members_without_rust_tests(self):
        crates = members()
        untested = {
            name
            for name, (_, manifest) in crates.items()
            if manifest.get("lib", {}).get("test", True) is False
        }
        self.assertEqual(untested, set(rust_tests.BINDINGS))
        for name in rust_tests.BINDINGS:
            directory, manifest = crates[name]
            with self.subTest(crate=name):
                self.assertIs(manifest["lib"]["doctest"], False)
                # Otherwise `--all-targets` checks the library again as a bench.
                self.assertIs(manifest["lib"]["bench"], False)
                # `test = false` would leave any of these unrun.
                for targets in ("tests", "benches", "examples"):
                    with self.subTest(targets=targets):
                        self.assertFalse((directory / targets).exists())
                for source in sorted((directory / "src").rglob("*.rs")):
                    self.assertNotRegex(
                        source.read_text(encoding="utf-8"), r"#\[(cfg\()?test"
                    )

    def test_the_workspace_tests_leave_out_both_bindings(self):
        self.assertEqual(
            rust_tests.command(),
            ["cargo", "test", "--locked", "--workspace"]
            + ["--exclude", "yamaa-python", "--exclude", "yamaa-r"],
        )

    def test_the_lint_leaves_the_r_binding_to_make_test_r(self):
        [workspace] = [
            command
            for command in repository_commands("lint")
            if command[:2] == ("cargo", "clippy") and "--workspace" in command
        ]
        exclusions = [
            workspace[index + 1]
            for index, argument in enumerate(workspace)
            if argument == "--exclude"
        ]
        self.assertEqual(exclusions, ["yamaa-r"])
        [binding] = repository_commands("lint-r-binding")
        self.assertEqual(binding[:5], ("cargo", "clippy", "--locked", "-p", "yamaa-r"))
        self.assertEqual(
            recipe("test-r"),
            [
                "$(TOOLS) tools.commands.repository lint-r-binding",
                "$(TOOLS) tools.commands.r_tests",
            ],
        )

    def test_the_lint_checks_each_crate_with_default_features_without_them(self):
        # Seam: the commands the lint runs, which only a mocked run observes.
        # The crates are the members whose manifests name default features,
        # but the R binding, which the lint leaves to make test-r (#3966).
        [featureless] = [
            command
            for command in repository_commands("lint")
            if "--no-default-features" in command
        ]
        self.assertEqual(
            featureless,
            (
                "cargo",
                "clippy",
                "--locked",
                "-p",
                "yamaa-adapters",
                "-p",
                "yamaa-application",
                "-p",
                "yamaa",
                "--all-targets",
                "--no-default-features",
                "--",
                "-D",
                "warnings",
            ),
            "feature-less Clippy",
        )

    def test_only_jobs_that_check_the_r_host_install_r(self):
        workflow = support.workflow("ci.yml")
        installs = {
            "r": ["check"],
        }
        for name, job in workflow["jobs"].items():
            setups = [
                step.get("with", {}).get("r", "none")
                for step in job["steps"]
                if step.get("uses") == "./.github/actions/setup"
            ]
            with self.subTest(job=name):
                self.assertEqual(setups, installs.get(name, ["none"]))
        commands = [step.get("run", "") for step in workflow["jobs"]["r"]["steps"]]
        self.assertTrue(any("tools.commands.host_checks" in line for line in commands))
        self.assertTrue(any("lint-r-binding" in line for line in commands))

    def test_split_lint_keeps_all_local_checks_without_building_rust_for_python_lint(
        self,
    ):
        python = repository_commands("lint-python")
        rust = repository_commands("lint-rust")
        self.assertEqual(repository_commands("lint"), python + rust)
        self.assertFalse(any(command[0] == "cargo" for command in python))
        self.assertTrue(any(command[:2] == ("cargo", "clippy") for command in rust))

    def test_documentation_validation_uses_no_compiler_or_installed_host(self):
        commands = repository_commands("docs")
        self.assertEqual(
            [command[2] for command in commands],
            [
                "tools.rules.check",
                "tools.validation.check_documentation",
                "tools.validation.check_site_examples",
            ],
        )


if __name__ == "__main__":
    unittest.main()
