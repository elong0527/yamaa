"""Every crate that a host's source archive ships has a stated purpose, and
every unconditional engine dependency that #3030 scopes has a stated reason."""

import contextlib
import io
import shutil
import tempfile
import unittest
from pathlib import Path
from unittest import mock

from tools.packaging import check_crate_purposes as purposes
from tools.staging import engine_bundle

GRAPHS = {
    "cli": {"yamaa-core", "parser", "cc"},
    "python": {"yamaa-core", "parser", "pyo3", "cc"},
    "r": {"yamaa-core", "parser", "extendr-api", "cc"},
}
PURPOSES = {
    "yamaa-core": "the language engine",
    "parser": "transitive dependency of yamaa-core: parsing",
    "pyo3": "the Python binding's interface to CPython",
    "extendr-api": "the R binding's interface to R",
    "cc": "build script: compiles the bundled C sources",
}
# The scoped crates that each engine crate depends on unconditionally, and
# the reasons for them.
DEPENDENCIES = {
    "yamaa": {"arrow-array"},
    "yamaa-adapters": {"arrow-array", "serde_json"},
    "yamaa-core": set(),
}
REASONS = {
    "yamaa": {"arrow-array": "the facade's public table is Arrow"},
    "yamaa-adapters": {
        "arrow-array": "every table on the always-on path is Arrow arrays",
        "serde_json": "the wire form of the core's JSON values",
    },
}
LABEL = "tools/packaging/crate_purposes.toml"
SCOPES = "#3030 section 2.9 scopes"


def no_reason(crate, name):
    """The message for an unconditional dependency without a reason."""
    return (
        f"{LABEL}: no reason why {crate} depends on {name} unconditionally, "
        f"although {SCOPES} {name} to the features or adapters that need it"
    )


def dependency(name, kind=None, optional=False, target=None):
    """A dependency as `cargo metadata` lists it under a package."""
    return {"name": name, "kind": kind, "optional": optional, "target": target}


class ProblemsTest(unittest.TestCase):
    def test_a_purpose_for_each_crate_of_either_host_passes(self):
        self.assertEqual(purposes.problems(PURPOSES, GRAPHS), [])

    def test_a_shipped_crate_without_a_purpose_fails(self):
        missing = {
            name: purpose
            for name, purpose in PURPOSES.items()
            if name not in {"pyo3", "cc"}
        }
        self.assertEqual(
            purposes.problems(missing, GRAPHS),
            [
                f"{LABEL}: no purpose for cc, which the cli, python and r bundles ship",
                f"{LABEL}: no purpose for pyo3, which the python bundle ships",
            ],
        )

    def test_a_purpose_for_a_crate_that_no_host_ships_fails(self):
        # A crate that a dependency change dropped from both bundles.
        stale = dict(PURPOSES, retired="transitive dependency of parser")
        self.assertEqual(
            purposes.problems(stale, GRAPHS),
            [f"{LABEL}: retired has a purpose, but no host's bundle ships it"],
        )
        # A purpose stays while either host ships its crate, and goes stale
        # when the last one stops.
        self.assertEqual(
            purposes.problems(PURPOSES, {**GRAPHS, "r": set()}),
            [f"{LABEL}: extendr-api has a purpose, but no host's bundle ships it"],
        )

    def test_an_empty_purpose_fails(self):
        for purpose, message in (
            ("", "is empty"),
            (" \n\t", "is empty"),
            (1, "must be a string"),
            ({"why": "parsing"}, "must be a string"),
        ):
            with self.subTest(purpose=purpose):
                self.assertEqual(
                    purposes.problems(dict(PURPOSES, parser=purpose), GRAPHS),
                    [f"{LABEL}: the purpose of parser {message}"],
                )


class ReasonProblemsTest(unittest.TestCase):
    def test_a_reason_for_each_unconditional_scoped_dependency_passes(self):
        self.assertEqual(purposes.reason_problems(REASONS, DEPENDENCIES), [])

    def test_an_unconditional_scoped_dependency_without_a_reason_fails(self):
        # A new unconditional edge, and an engine crate with no table at all.
        reasons = {"yamaa-adapters": {"arrow-array": "Arrow tables"}}
        self.assertEqual(
            purposes.reason_problems(reasons, DEPENDENCIES),
            [
                no_reason("yamaa", "arrow-array"),
                no_reason("yamaa-adapters", "serde_json"),
            ],
        )

    def test_a_reason_for_a_dependency_that_is_optional_or_gone_fails(self):
        # The change that moves serde_json behind a feature, or into the
        # adapters that use it, removes its reason.
        moved = {**DEPENDENCIES, "yamaa-adapters": {"arrow-array"}}
        self.assertEqual(
            purposes.reason_problems(REASONS, moved),
            [
                f"{LABEL}: a reason why yamaa-adapters depends on serde_json "
                "unconditionally, which it no longer does"
            ],
        )

    def test_a_reason_for_a_crate_that_is_not_scoped_fails(self):
        reasons = {
            **REASONS,
            "yamaa-adapters": {**REASONS["yamaa-adapters"], "serde": "derives"},
        }
        self.assertEqual(
            purposes.reason_problems(reasons, DEPENDENCIES),
            [
                f"{LABEL}: a reason why yamaa-adapters depends on serde "
                "unconditionally, but #3030 section 2.9 does not scope serde"
            ],
        )

    def test_the_reasons_of_a_crate_that_is_not_an_engine_crate_fail(self):
        reasons = {**REASONS, "yamaa-python": {"arrow-array": "the hand-off"}}
        self.assertEqual(
            purposes.reason_problems(reasons, DEPENDENCIES),
            [
                f"{LABEL}: [unconditional.yamaa-python] names no engine crate; "
                "the engine crates are yamaa, yamaa-adapters and yamaa-core"
            ],
        )

    def test_an_empty_reason_or_a_reason_that_is_not_a_table_fails(self):
        for reason, message in (
            ("", "is empty"),
            (" \n\t", "is empty"),
            (1, "must be a string"),
            ({"why": "tables"}, "must be a string"),
        ):
            with self.subTest(reason=reason):
                self.assertEqual(
                    purposes.reason_problems(
                        {**REASONS, "yamaa": {"arrow-array": reason}}, DEPENDENCIES
                    ),
                    [f"{LABEL}: the reason why yamaa depends on arrow-array {message}"],
                )
        self.assertEqual(
            purposes.reason_problems({**REASONS, "yamaa": "Arrow"}, DEPENDENCIES),
            [f"{LABEL}: [unconditional.yamaa] must be a table of reasons"],
        )


class DeclaredTest(unittest.TestCase):
    def test_arrow_flatbuffers_serde_json_and_toml_are_scoped(self):
        for name in ("arrow", "arrow-array", "arrow-ipc", "flatbuffers"):
            with self.subTest(name=name):
                self.assertTrue(purposes.scoped(name))
        for name in ("serde_json", "toml"):
            with self.subTest(name=name):
                self.assertTrue(purposes.scoped(name))
        for name in ("arrowhead", "serde", "toml_datetime", "parquet", "yamaa-core"):
            with self.subTest(name=name):
                self.assertFalse(purposes.scoped(name))

    def test_the_scoped_normal_dependencies_that_no_feature_selects(self):
        metadata = {
            "packages": [
                {"name": "yamaa-core", "dependencies": [dependency("regex")]},
                {
                    "name": "yamaa-adapters",
                    "dependencies": [
                        dependency("yamaa-core"),
                        dependency("arrow-array"),
                        dependency("serde_json"),
                        # Unscoped, optional, development and build-only
                        # dependencies do not count.
                        dependency("serde"),
                        dependency("parquet", optional=True),
                        dependency("arrow-cast", optional=True),
                        dependency("toml", kind="dev"),
                        dependency("flatbuffers", kind="build"),
                        # A platform's dependency counts.
                        dependency("arrow-ipc", target="cfg(unix)"),
                    ],
                },
                {
                    "name": "yamaa-application",
                    "dependencies": [
                        dependency("yamaa-adapters"),
                        dependency("serde_json"),
                    ],
                },
                {
                    "name": "yamaa",
                    "dependencies": [
                        dependency("yamaa-application"),
                        dependency("arrow"),
                        dependency("arrow-schema"),
                    ],
                },
                # A binding is not an engine crate.
                {
                    "name": "yamaa-python",
                    "dependencies": [dependency("yamaa"), dependency("arrow-array")],
                },
            ]
        }
        self.assertEqual(
            purposes.declared(metadata),
            {
                "yamaa-core": set(),
                "yamaa-adapters": {"arrow-array", "serde_json", "arrow-ipc"},
                "yamaa-application": {"serde_json"},
                "yamaa": {"arrow", "arrow-schema"},
            },
        )

    def test_a_missing_engine_crate_refuses(self):
        metadata = {
            "packages": [
                {"name": "yamaa-core", "dependencies": []},
                {"name": "yamaa-adapters", "dependencies": []},
                {"name": "yamaa-application", "dependencies": []},
            ]
        }
        with self.assertRaisesRegex(ValueError, "yamaa is not a workspace member"):
            purposes.declared(metadata)

    @unittest.skipUnless(shutil.which("cargo"), "cargo is unavailable")
    def test_the_engine_manifests_name_their_scoped_dependencies(self):
        dependencies = purposes.unconditional()
        self.assertEqual(sorted(dependencies), sorted(engine_bundle.ENGINE))
        # Arrow, serde_json and toml left the core, and Arrow left the
        # always-on adapters and facade: it comes with their arrow and parquet
        # features, while the adapters keep serde_json and toml (#3030 section
        # 2.9, #3696), and the application layer serde_json for its records
        # (#3761).
        self.assertEqual(dependencies["yamaa-core"], set())
        self.assertEqual(dependencies["yamaa"], set())
        self.assertEqual(dependencies["yamaa-adapters"], {"serde_json", "toml"})
        self.assertEqual(dependencies["yamaa-application"], {"serde_json"})
        for names in dependencies.values():
            self.assertTrue(all(map(purposes.scoped, names)), names)


class MainTest(unittest.TestCase):
    def main(self, read=PURPOSES, reasons=REASONS):
        """Run main over the fixtures; its exit code, stdout and stderr lines."""
        output, errors = io.StringIO(), io.StringIO()
        code = 0
        with (
            mock.patch.object(purposes, "read", return_value=read),
            mock.patch.object(purposes, "shipped", return_value=GRAPHS),
            mock.patch.object(purposes, "reasons", return_value=reasons),
            mock.patch.object(purposes, "unconditional", return_value=DEPENDENCIES),
            contextlib.redirect_stdout(output),
            contextlib.redirect_stderr(errors),
        ):
            try:
                purposes.main()
            except SystemExit as stopped:
                code = stopped.code
        return code, output.getvalue().splitlines(), errors.getvalue().splitlines()

    def test_main_counts_the_purposes_and_the_reasons(self):
        self.assertEqual(
            self.main(),
            (
                0,
                [
                    f"{LABEL}: a purpose for each of the 5 crates that the cli, "
                    "python and r bundles ship",
                    f"{LABEL}: a reason for each of the 3 unconditional "
                    f"dependencies of the engine crates on crates that {SCOPES}",
                ],
                [],
            ),
        )

    def test_main_reports_each_problem_and_how_to_state_a_purpose(self):
        read = {name: text for name, text in PURPOSES.items() if name != "pyo3"}
        code, output, lines = self.main(read=read)
        self.assertEqual((code, output), (1, []))
        self.assertEqual(
            lines[0], f"{LABEL}: no purpose for pyo3, which the python bundle ships"
        )
        self.assertIn("--workspace --target all -i <crate>", lines[1])
        self.assertEqual(len(lines), 2)

    def test_main_reports_a_missing_reason_and_how_to_state_one(self):
        reasons = {**REASONS, "yamaa": {}}
        code, output, lines = self.main(reasons=reasons)
        self.assertEqual((code, output), (1, []))
        self.assertEqual(lines[0], no_reason("yamaa", "arrow-array"))
        self.assertIn("under [unconditional.<engine crate>]", lines[1])
        self.assertEqual(len(lines), 2)


class ReadTest(unittest.TestCase):
    def write(self, directory, text):
        path = Path(directory) / "crate_purposes.toml"
        path.write_text(text, encoding="utf-8")
        return path

    def read(self, text):
        with tempfile.TemporaryDirectory() as directory:
            return purposes.read(self.write(directory, text))

    def reasons(self, text):
        with tempfile.TemporaryDirectory() as directory:
            return purposes.reasons(self.write(directory, text))

    def test_the_purposes_are_the_crates_table(self):
        self.assertEqual(
            self.read('# Why yamaa ships each crate.\n[crates]\ncc = "build script"\n'),
            {"cc": "build script"},
        )

    def test_the_reasons_are_the_unconditional_tables(self):
        text = (
            '[crates]\ncc = "build script"\n'
            '[unconditional.yamaa]\narrow-array = """\\\n  the facade\'s \\\n  table"""\n'
        )
        self.assertEqual(self.read(text), {"cc": "build script"})
        self.assertEqual(
            self.reasons(text), {"yamaa": {"arrow-array": "the facade's table"}}
        )
        # The reasons may be missing.
        self.assertEqual(self.reasons('[crates]\ncc = "build script"\n'), {})

    def test_anything_but_the_crates_and_unconditional_tables_refuses(self):
        for text in (
            'cc = "build script"\n',
            'cc = "build script"\n[crates]\nlibc = "C library bindings"\n',
            'crates = "build script"\n',
            "[crates]\n[other]\n",
            'unconditional = "Arrow"\n[crates]\n',
            '[unconditional.yamaa]\narrow-array = "Arrow"\n',
        ):
            for reader in (self.read, self.reasons):
                with (
                    self.subTest(text=text, reader=reader.__name__),
                    self.assertRaisesRegex(ValueError, "only a \\[crates\\] table"),
                ):
                    reader(text)


class ShippedTest(unittest.TestCase):
    @unittest.skipUnless(shutil.which("cargo"), "cargo is unavailable")
    def test_each_host_ships_its_trimmed_lock_with_members_and_build_crates(self):
        graphs = purposes.shipped()
        self.assertEqual(sorted(graphs), ["cli", "python", "r"])
        for host, binding, other in (
            ("python", "yamaa-python", "yamaa-r"),
            ("r", "yamaa-r", "yamaa-python"),
            ("cli", "yamaa-cli", "yamaa-python"),
        ):
            with self.subTest(host=host):
                names = graphs[host]
                # The engine crates, the binding and the vendored YAML parser;
                # a build-script crate and a procedural macro; a crate that
                # only Windows compiles and one that no supported target does.
                for name in (
                    "yamaa-core",
                    "yamaa-adapters",
                    "yamaa",
                    binding,
                    "yamaa-saphyr-parser",
                    "cc",
                    "syn",
                    "windows-sys",
                    "wasm-bindgen",
                ):
                    self.assertIn(name, names)
                self.assertNotIn(other, names)
        self.assertIn("pyo3", graphs["python"] - graphs["r"])
        self.assertIn("extendr-api", graphs["r"] - graphs["python"])
        # The command line depends on the facade alone.
        self.assertFalse({"pyo3", "extendr-api", "yamaa-r"} & graphs["cli"])


if __name__ == "__main__":
    unittest.main()
