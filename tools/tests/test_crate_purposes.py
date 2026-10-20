"""Every crate that a host's source archive ships has a stated purpose."""

import contextlib
import io
import shutil
import tempfile
import unittest
from pathlib import Path
from unittest import mock

from tools.packaging import check_crate_purposes as purposes

GRAPHS = {
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
LABEL = "tools/packaging/crate_purposes.toml"


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
                f"{LABEL}: no purpose for cc, which the python and r bundles ship",
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
            purposes.problems(PURPOSES, {"python": GRAPHS["python"], "r": set()}),
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

    def test_main_reports_each_problem_and_how_to_state_a_purpose(self):
        missing = {name: text for name, text in PURPOSES.items() if name != "pyo3"}
        errors = io.StringIO()
        with (
            mock.patch.object(purposes, "read", return_value=missing),
            mock.patch.object(purposes, "shipped", return_value=GRAPHS),
            contextlib.redirect_stderr(errors),
            self.assertRaises(SystemExit) as stopped,
        ):
            purposes.main()
        self.assertEqual(stopped.exception.code, 1)
        lines = errors.getvalue().splitlines()
        self.assertEqual(
            lines[0], f"{LABEL}: no purpose for pyo3, which the python bundle ships"
        )
        self.assertIn("--workspace --target all -i <crate>", lines[1])


class ReadTest(unittest.TestCase):
    def read(self, text):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "crate_purposes.toml"
            path.write_text(text, encoding="utf-8")
            return purposes.read(path)

    def test_the_purposes_are_the_crates_table(self):
        self.assertEqual(
            self.read('# Why yamaa ships each crate.\n[crates]\ncc = "build script"\n'),
            {"cc": "build script"},
        )

    def test_anything_but_one_crates_table_refuses(self):
        for text in (
            'cc = "build script"\n',
            'cc = "build script"\n[crates]\nlibc = "C library bindings"\n',
            'crates = "build script"\n',
            "[crates]\n[other]\n",
        ):
            with (
                self.subTest(text=text),
                self.assertRaisesRegex(ValueError, "only a \\[crates\\] table"),
            ):
                self.read(text)


class ShippedTest(unittest.TestCase):
    @unittest.skipUnless(shutil.which("cargo"), "cargo is unavailable")
    def test_each_host_ships_its_trimmed_lock_with_members_and_build_crates(self):
        graphs = purposes.shipped()
        self.assertEqual(sorted(graphs), ["python", "r"])
        for host, binding, other in (
            ("python", "yamaa-python", "yamaa-r"),
            ("r", "yamaa-r", "yamaa-python"),
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


if __name__ == "__main__":
    unittest.main()
