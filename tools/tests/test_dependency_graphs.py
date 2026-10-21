"""The dependency lanes must select floors without changing release inputs."""

import json
import os
import re
import subprocess
import tempfile
import tomllib
import unittest
from pathlib import Path
from unittest import mock

from packaging.requirements import Requirement

from tools.ci import dependency_graphs
from tools.ci.dependency_graphs import (
    LOCKS,
    NOTICES,
    assert_msrv,
    minimum_cargo,
    minimum_python,
    pin_manifest,
    prepare,
    run_graph,
)
from tools.packaging import generate_third_party_notices as notice_generator
from tools.paths import ROOT

# Floors by interpreter in python/pyproject.toml. lxml's floor is the same on
# every interpreter; the others split at CPython 3.15.
EARLIER_FLOORS = {
    "lxml": "6.1.3",
    "polars": "1.35.2",
    "pyarrow": "23.0.1",
    "pydantic": "2.12.4",
}
PYTHON_315_FLOORS = {
    "lxml": "6.1.3",
    "polars": "2.0.0",
    "pyarrow": "26.0.0",
    "pydantic": "2.14.0",
}


def applicable(requirements, python):
    """Specifiers by package for the requirements that apply on one interpreter."""
    selected = {}
    for line in requirements:
        requirement = Requirement(line)
        if requirement.marker is None or requirement.marker.evaluate(
            {"python_version": python}
        ):
            selected.setdefault(requirement.name, []).append(str(requirement.specifier))
    return selected


class DependencyGraphsTests(unittest.TestCase):
    def test_cargo_minimum_syntax_fails_closed(self):
        for value, expected in [
            ("1.0.228", "=1.0.228"),
            (">=0.9.8, <2", "=0.9.8"),
            ("=57.3.0", "=57.3.0"),
        ]:
            with self.subTest(value=value):
                self.assertEqual(minimum_cargo(value), expected)
        for value in ["*", "1", "~1.2", ">1.2.3", "1.2.3 || 2.0.0"]:
            with self.subTest(value=value), self.assertRaises(ValueError):
                minimum_cargo(value)

    def test_pin_manifest_rejects_malformed_shapes_as_value_error(self):
        # Malformed Cargo.toml sections must follow the module's ValueError
        # convention, not escape as AttributeError/TypeError/KeyError.
        for content in [
            'target = "x"\n[dependencies]\nfoo = "1.2.3"\n',
            'dependencies = "x"\n',
            "dependencies = []\n",
            "[dependencies]\nfoo = 123\n",
            "[dependencies]\nfoo = []\n",
            "[dependencies]\nfoo = {}\n",
            "[dependencies]\nfoo = {version = 123}\n",
            '[target]\nx = "y"\n',
        ]:
            with self.subTest(content=content), tempfile.TemporaryDirectory() as td:
                path = Path(td) / "Cargo.toml"
                path.write_text(content)
                with self.assertRaises(ValueError):
                    pin_manifest(path)
        for value in (123, None, b"1.2.3"):
            with self.subTest(value=value), self.assertRaises(ValueError):
                minimum_cargo(value)
            with self.subTest(value=value, python=True), self.assertRaises(ValueError):
                minimum_python(value)

    def test_pin_manifest_pins_string_and_table_dependencies(self):
        with tempfile.TemporaryDirectory() as td:
            path = Path(td) / "Cargo.toml"
            path.write_text(
                '[dependencies]\nfoo = "1.2.3"\nbar = {version = ">=0.9.8, <2"}\n'
                'local = {path = "../local"}\n'
                'vendored = {version = ">=0.2.1, <0.3", path = "../../vendor/v"}\n'
                '[target.x.dependencies]\nbaz = "=2.0.0"\n'
            )
            pin_manifest(path)
            text = path.read_text()
            self.assertIn('foo = "=1.2.3"', text)
            self.assertIn('bar = {version = "=0.9.8"}', text)
            self.assertIn('baz = "=2.0.0"', text)
            self.assertIn('local = {path = "../local"}', text)
            # A vendored crate is a path dependency: its requirement stays.
            self.assertIn(
                'vendored = {version = ">=0.2.1, <0.3", path = "../../vendor/v"}', text
            )

    def test_python_minimum_preserves_markers_and_extras(self):
        self.assertEqual(
            minimum_python('example[extra]>=2.3; python_version >= "3.12"'),
            'example[extra]==2.3; python_version >= "3.12"',
        )
        with self.assertRaises(ValueError):
            minimum_python("example>2")

    def test_python_minimum_keeps_one_pin_per_marker_split_floor(self):
        pins = [
            minimum_python("example>=1.2; python_version < '3.15'"),
            minimum_python("example>=2.0; python_version >= '3.15'"),
        ]
        self.assertEqual(
            pins,
            [
                'example==1.2; python_version < "3.15"',
                'example==2.0; python_version >= "3.15"',
            ],
        )
        self.assertEqual(applicable(pins, "3.14"), {"example": ["==1.2"]})
        self.assertEqual(applicable(pins, "3.15"), {"example": ["==2.0"]})

    def test_minimum_stage_keeps_the_vendored_parser_and_exact_policy_pins(self):
        locks = {
            name: (ROOT / name).read_bytes()
            for name in ("rust/Cargo.lock", "python/uv.lock")
        }
        with tempfile.TemporaryDirectory() as temporary:
            stage = Path(temporary) / "graph"
            self.assertEqual(prepare(ROOT, stage, "minimum"), "1.82")
            self.assertTrue((stage / ".git").is_dir())
            self.assertEqual(
                subprocess.check_output(["git", "-C", str(stage), "rev-parse", "HEAD"]),
                subprocess.check_output(["git", "-C", str(ROOT), "rev-parse", "HEAD"]),
            )
            self.assertEqual(
                subprocess.check_output(
                    ["git", "-C", str(stage), "ls-files", "rust/Cargo.toml"]
                ),
                b"rust/Cargo.toml\n",
            )
            adapters = tomllib.loads(
                (stage / "rust/crates/yamaa-adapters/Cargo.toml").read_text()
            )["dependencies"]
            self.assertEqual(adapters["toml"]["version"], "=0.9.8")
            self.assertEqual(adapters["arrow-array"]["version"], "=55.2.0")
            parser = adapters["yamaa-saphyr-parser"]
            self.assertEqual(parser["version"], "=0.2.1")
            self.assertEqual(parser["path"], "../../vendor/saphyr-parser")
            self.assertNotIn("saphyr-parser", adapters)
            self.assertNotIn(
                "patch", tomllib.loads((stage / "rust/Cargo.toml").read_text())
            )
            core = tomllib.loads(
                (stage / "rust/crates/yamaa-core/Cargo.toml").read_text()
            )["dependencies"]
            self.assertEqual(core["libm"]["version"], "=0.2.16")
            self.assertFalse((stage / "rust/Cargo.lock").exists())
            self.assertEqual(
                (stage / "python/uv.lock").read_bytes(), locks["python/uv.lock"]
            )
            requirements = (stage / "requirements.in").read_text().splitlines()
            for python, floors in (
                ("3.12", EARLIER_FLOORS),
                ("3.14", EARLIER_FLOORS),
                ("3.15", PYTHON_315_FLOORS),
            ):
                selected = applicable(requirements, python)
                for name, floor in floors.items():
                    with self.subTest(python=python, package=name):
                        self.assertEqual(selected[name], ["==" + floor])
        self.assertEqual(locks, {name: (ROOT / name).read_bytes() for name in locks})

    def test_newest_stage_retains_supported_ranges(self):
        with tempfile.TemporaryDirectory() as temporary:
            stage = Path(temporary) / "graph"
            prepare(ROOT, stage, "newest")
            for path in (
                "rust/crates/yamaa-adapters/Cargo.toml",
                "python/pyproject.toml",
            ):
                self.assertEqual(
                    (stage / path).read_bytes(), (ROOT / path).read_bytes()
                )
            requirements = (stage / "requirements.in").read_text().splitlines()
            for python, floors in (
                ("3.14", EARLIER_FLOORS),
                ("3.15", PYTHON_315_FLOORS),
            ):
                selected = applicable(requirements, python)
                for name, floor in floors.items():
                    with self.subTest(python=python, package=name):
                        self.assertEqual(selected[name], [">=" + floor])
            self.assertFalse((stage / "rust/Cargo.lock").exists())

    def test_destination_cannot_change_source_or_overwrite_existing(self):
        for destination in (ROOT, ROOT / "nested-graph"):
            with self.subTest(destination=destination), self.assertRaises(ValueError):
                prepare(ROOT, destination, "minimum")
        with tempfile.TemporaryDirectory() as temporary, self.assertRaises(ValueError):
            prepare(ROOT, Path(temporary), "newest")

    def test_msrv_rejects_newer_packages_even_after_resolver_fallback(self):
        metadata = {
            "packages": [
                {
                    "name": "allowed",
                    "version": "1.0",
                    "rust_version": "1.82",
                    "edition": "2021",
                },
                {
                    "name": "unspecified",
                    "version": "1.0",
                    "rust_version": None,
                    "edition": "2018",
                },
            ]
        }
        assert_msrv(metadata, "1.82.0")
        for package, needed in (
            ({"rust_version": "1.83.0", "edition": "2021"}, "1.83.0"),
            # Edition 2024 needs Rust 1.85, declared rust-version or not.
            ({"rust_version": None, "edition": "2024"}, "1.85"),
        ):
            newer = {"name": "too-new", "version": "2.0", **package}
            with (
                self.subTest(package=package),
                self.assertRaisesRegex(ValueError, rf"too-new 2\.0 \(Rust {needed}\)"),
            ):
                assert_msrv({"packages": [*metadata["packages"], newer]}, "1.82")

    def lane_commands(self, source, destination, effect=None):
        """Run a newest lane with each command recorded instead of run."""
        commands = []

        def run(arguments, cwd=None, **_):
            commands.append((tuple(arguments), cwd))
            if effect is not None:
                effect(arguments)
            return subprocess.CompletedProcess(arguments, 0)

        def check_output(arguments, cwd=None, text=False, **_):
            commands.append((tuple(arguments), cwd))
            if arguments[:2] == ["cargo", "metadata"]:
                return json.dumps({"packages": []}).encode()
            return "" if text else b""

        with (
            mock.patch.object(dependency_graphs, "prepare", return_value="1.82"),
            mock.patch.object(dependency_graphs.shutil, "which", return_value="x"),
            mock.patch.object(dependency_graphs.subprocess, "run", run),
            mock.patch.object(
                dependency_graphs.subprocess, "check_output", check_output
            ),
        ):
            run_graph(source, destination, "newest", "3.12")
        return commands

    def test_lane_regenerates_its_notices_before_it_builds_or_stages(self):
        """Notice checks in a lane cover the graph that the lane resolved."""
        with tempfile.TemporaryDirectory() as temporary:
            destination = Path(temporary) / "graph"
            destination.mkdir()
            commands = self.lane_commands(ROOT, destination)
        python = str(
            destination
            / "python/.venv"
            / ("Scripts/python.exe" if os.name == "nt" else "bin/python")
        )
        install = ("uv", "pip", "install", "--python", python, "--no-deps")

        def step(*prefix):
            """The position of the one command that starts with prefix."""
            [index] = [
                index
                for index, (arguments, _) in enumerate(commands)
                if arguments[: len(prefix)] == prefix
            ]
            return index

        # The copy's own tools write the copy's notices from the resolved lock,
        # before the extension and the R package are built from that graph.
        generate = (python, "-m", "tools.packaging.generate_third_party_notices")
        notices = step(*generate)
        self.assertEqual(commands[notices], ((*generate, "--write"), destination))
        for earlier in (("cargo", "generate-lockfile"), (*install, "--editable")):
            self.assertLess(step(*earlier), notices)
        for later in (
            (*install, "--no-build-isolation"),
            (python, "-m", "tools.commands.python_tests"),
            (python, "-m", "tools.commands.r_tests"),
        ):
            self.assertLess(notices, step(*later))

    def test_lane_runs_the_workspace_tests_through_the_shared_command(self):
        """The lane's workspace tests leave out the host bindings, as CI's do."""
        with tempfile.TemporaryDirectory() as temporary:
            destination = Path(temporary) / "graph"
            destination.mkdir()
            commands = self.lane_commands(ROOT, destination)
        python = str(
            destination
            / "python/.venv"
            / ("Scripts/python.exe" if os.name == "nt" else "bin/python")
        )
        ran = [arguments for arguments, _ in commands]
        self.assertIn((python, "-m", "tools.commands.rust_tests"), ran)
        self.assertNotIn(
            ("cargo", "test", "--locked", "--workspace"),
            [arguments[:4] for arguments in ran],
        )

    def test_lane_refuses_to_change_the_checkout_notices(self):
        self.assertEqual(
            NOTICES,
            tuple(
                (directory / notice_generator.NOTICES).relative_to(ROOT).as_posix()
                for directory in notice_generator.copies().values()
            ),
        )
        with tempfile.TemporaryDirectory() as temporary:
            source, destination = Path(temporary) / "source", Path(temporary) / "graph"
            for path in LOCKS + NOTICES:
                (source / path).parent.mkdir(parents=True, exist_ok=True)
                (source / path).write_text("canonical\n")
            destination.mkdir()

            def write_checkout_notices(arguments):
                if "--write" in arguments:
                    (source / NOTICES[1]).write_text("lane\n")

            with self.assertRaisesRegex(RuntimeError, re.escape(NOTICES[1])):
                self.lane_commands(source, destination, write_checkout_notices)


if __name__ == "__main__":
    unittest.main()
