"""Each host's packages ship the component inventory of their trimmed lock."""

import contextlib
import io
import shutil
import tempfile
import unittest
from pathlib import Path
from unittest import mock

from tools.packaging import generate_component_inventory as inventory
from tools.packaging import generate_third_party_notices as notices

PARSER_COMMIT = "1583482c4f08f3c14caea0e2f1a8526950fb0eff"
READSTAT_COMMIT = "104ba03a8da116eb8c094abc18bc2530b733eda9"
PURPOSES = {
    "pyo3": "the Python binding's interface to CPython",
    "yamaa-adapters": "the engine's codecs",
    "yamaa-core": "the language engine",
    "yamaa-saphyr-parser": "the YAML parser",
    "zstd-sys": "transitive dependency of zstd-safe: builds the zstd library",
}


class Workspace:
    """A checkout with crate packages as `cargo metadata` describes them."""

    def __init__(self, root):
        self.root = root
        self.workspace = root / "rust"
        self.packages = []

    def crate(self, name, version, license, directory=None, **fields):
        if directory is None:
            source = notices.CRATES_IO
            directory = self.root / "registry" / f"{name}-{version}"
        else:
            source = None
            directory = self.workspace / directory
        directory.mkdir(parents=True, exist_ok=True)
        package = {
            "name": name,
            "version": version,
            "license": license,
            "source": source,
            "manifest_path": str(directory / "Cargo.toml"),
            "repository": None,
            **fields,
        }
        self.packages.append(package)
        return directory

    def kept(self):
        return {(p["name"], p["version"], p["source"]) for p in self.packages}

    def metadata(self):
        return {"packages": self.packages}


def workspace(root):
    """The engine crates, the vendored parser and ReadStat, pyo3 and zstd-sys."""
    checkout = Workspace(root)
    checkout.crate("yamaa-core", "0.5.0", "MIT AND Unicode-3.0", "crates/yamaa-core")
    adapters = checkout.crate("yamaa-adapters", "0.5.0", "MIT", "crates/yamaa-adapters")
    (adapters / "vendor/readstat").mkdir(parents=True)
    (adapters / "vendor/readstat/README.md").write_text(
        "# ReadStat source integration\n\nThis directory vendors the MIT-licensed "
        "SAS reader subset of ReadStat 1.1.9,\nupstream commit "
        f"`{READSTAT_COMMIT}` from\n<https://github.com/WizardMac/ReadStat>, with "
        "the later fixes.\n"
    )
    parser = checkout.crate(
        "yamaa-saphyr-parser",
        "0.2.1",
        "MIT OR Apache-2.0",
        "vendor/saphyr-parser",
        repository="https://github.com/saphyr-rs/saphyr",
    )
    (parser / "PROVENANCE.md").write_text(
        "# saphyr-parser 0.2.1\n\nSource: the crates.io release.\nThe upstream "
        f"release commit is\n`{PARSER_COMMIT}`.\n"
    )
    checkout.crate("pyo3", "0.27.2", "MIT/Apache-2.0")
    checkout.crate("zstd-sys", "2.1.2+zstd.1.5.7", "BSD-3-Clause")
    return checkout


class Checkout(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.checkout = workspace(self.root)
        patches = mock.patch.multiple(
            notices,
            ROOT=self.root,
            WORKSPACE=self.checkout.workspace,
            PYTHON=self.root / "python",
            R_INST=self.root / "R/yamaa/inst",
        )
        patches.start()
        self.addCleanup(patches.stop)

    def rows(self, purposes=PURPOSES):
        return inventory.components(
            self.checkout.kept(), self.checkout.metadata(), purposes
        )


class ComponentsTest(Checkout):
    def test_rows_name_each_locked_crate_and_the_vendored_c_code(self):
        rows = {row.name: row for row in self.rows()}
        self.assertEqual(
            list(rows),
            [
                "libzstd",
                "pyo3",
                "ReadStat",
                "yamaa-adapters",
                "yamaa-core",
                "yamaa-saphyr-parser",
                "zstd-sys",
            ],
        )
        # Cargo's legacy separator becomes an SPDX OR.
        self.assertEqual(
            rows["pyo3"],
            inventory.Component(
                "pyo3", "0.27.2", "MIT OR Apache-2.0", "crates.io", PURPOSES["pyo3"]
            ),
        )
        self.assertEqual(rows["yamaa-core"].source, "rust/crates/yamaa-core")
        self.assertEqual(rows["yamaa-core"].license, "MIT AND Unicode-3.0")
        self.assertEqual(
            rows["yamaa-saphyr-parser"].source,
            "rust/vendor/saphyr-parser, from https://github.com/saphyr-rs/saphyr "
            f"at {PARSER_COMMIT}",
        )
        readstat = rows["ReadStat"]
        self.assertEqual((readstat.version, readstat.license), ("1.1.9", "MIT"))
        self.assertEqual(
            readstat.source,
            "rust/crates/yamaa-adapters/vendor/readstat, from "
            f"https://github.com/WizardMac/ReadStat at {READSTAT_COMMIT}",
        )
        zstd = rows["libzstd"]
        self.assertEqual(
            (zstd.version, zstd.license, zstd.source),
            (
                "1.5.7",
                "BSD-3-Clause OR GPL-2.0-only",
                "bundled in zstd-sys 2.1.2+zstd.1.5.7",
            ),
        )

    def test_the_inventory_is_a_header_and_one_tab_separated_row_each(self):
        text = inventory.render(self.rows())
        lines = text.split("\n")
        self.assertEqual(lines[0], "name\tversion\tlicense\tsource\tpurpose")
        self.assertEqual(lines[-1], "")
        self.assertIn(
            "pyo3\t0.27.2\tMIT OR Apache-2.0\tcrates.io\t"
            "the Python binding's interface to CPython",
            lines,
        )
        self.assertTrue(all(line.count("\t") == 4 for line in lines[:-1]))
        self.assertEqual(
            inventory.listed(text),
            {(row.name, row.version) for row in self.rows()},
        )
        for field in ("", "two\tcells", "two\nlines"):
            row = inventory.Component("cc", "1.2.0", "MIT", "crates.io", field)
            with (
                self.subTest(field=field),
                self.assertRaisesRegex(ValueError, "cc 1.2.0: the purpose must be"),
            ):
                inventory.render([row])

    def test_licenses_are_spdx_expressions(self):
        for declared, expression in (
            ("MIT OR Apache-2.0", "MIT OR Apache-2.0"),
            ("BSD-3-Clause/MIT", "BSD-3-Clause OR MIT"),
            ("(MIT/Apache-2.0) AND Unicode-3.0", "(MIT OR Apache-2.0) AND Unicode-3.0"),
        ):
            with self.subTest(declared=declared):
                self.assertEqual(
                    inventory.license_expression(declared, "crate 1.0.0"), expression
                )
        for declared, message in (
            (None, "declares no license"),
            (" ", "declares no license"),
            ("MIT OR", "malformed license expression"),
        ):
            with (
                self.subTest(declared=declared),
                self.assertRaisesRegex(ValueError, message),
            ):
                inventory.license_expression(declared, "crate 1.0.0")

    def test_a_crate_without_a_purpose_or_provenance_stops_generation(self):
        missing = {name: text for name, text in PURPOSES.items() if name != "pyo3"}
        with self.assertRaisesRegex(ValueError, "pyo3 0.27.2: .* states no purpose"):
            self.rows(missing)
        with self.assertRaisesRegex(ValueError, "pyo3 0.27.2: .* states no purpose"):
            self.rows(dict(PURPOSES, pyo3=" "))
        provenance = self.checkout.workspace / "vendor/saphyr-parser/PROVENANCE.md"
        original = provenance.read_text()
        provenance.write_text("Taken from the 0.2.1 release.\n")
        with self.assertRaisesRegex(ValueError, "must name the upstream commit"):
            self.rows()
        provenance.write_text(original)
        readme = (
            self.checkout.workspace / "crates/yamaa-adapters/vendor/readstat/README.md"
        )
        readme.write_text("This directory vendors ReadStat.\n")
        with self.assertRaisesRegex(ValueError, "must name the vendored release"):
            self.rows()

    def test_unsupported_sources_and_names_stop_generation(self):
        zstd = next(p for p in self.checkout.packages if p["name"] == "zstd-sys")
        zstd["version"] = "2.1.2"
        with self.assertRaisesRegex(ValueError, "names no bundled zstd release"):
            self.rows()
        zstd["version"] = "2.1.2+zstd.1.5.7"
        # A crate named like a vendored component would make two rows alike.
        self.checkout.crate("libzstd", "1.5.7", "BSD-3-Clause")
        with self.assertRaisesRegex(ValueError, "libzstd 1.5.7: listed twice"):
            self.rows(dict(PURPOSES, libzstd="a crate"))
        self.checkout.packages.pop()
        self.checkout.crate(
            "forked", "1.0.0", "MIT", source="git+https://example.org/forked"
        )
        with self.assertRaisesRegex(ValueError, "unsupported source"):
            self.rows(dict(PURPOSES, forked="a crate from git"))


class CheckTest(Checkout):
    def setUp(self):
        super().setUp()
        self.generated = {
            "python": inventory.render(self.rows()),
            "r": inventory.render([row for row in self.rows() if row.name != "pyo3"]),
        }
        self.paths = inventory.committed()
        for host, path in self.paths.items():
            path.parent.mkdir(parents=True)
            path.write_text(self.generated[host], encoding="utf-8")
        self.project = {"license-files": ["LICENSE.txt", "COMPONENTS.tsv"]}

    def test_current_inventories_listed_as_license_files_pass(self):
        self.assertEqual(
            self.paths,
            {
                "python": self.root / "python/COMPONENTS.tsv",
                "r": self.root / "R/yamaa/inst/COMPONENTS.tsv",
            },
        )
        self.assertEqual(inventory.check(self.generated, self.paths, self.project), [])

    def test_stale_missing_or_unlisted_inventories_fail(self):
        # The R inventory still lists a crate that its bundle dropped, and the
        # Python one misses a crate that its bundle gained.
        self.paths["r"].write_text(self.generated["python"], encoding="utf-8")
        python = self.generated["python"].replace(
            "pyo3\t0.27.2\tMIT OR Apache-2.0\tcrates.io\t"
            "the Python binding's interface to CPython\n",
            "",
        )
        self.paths["python"].write_text(python, encoding="utf-8")
        differs = (
            "differs from the inventory that the {} bundle's trimmed lock, "
            "tools/packaging/crate_purposes.toml and the vendored sources give"
        )
        self.assertEqual(
            inventory.check(self.generated, self.paths, {"license-files": []}),
            [
                "python/pyproject.toml: license-files must list COMPONENTS.tsv, so "
                "that the Python packages carry it",
                "python/COMPONENTS.tsv: no row for pyo3 0.27.2, which the python "
                "bundle ships",
                "python/COMPONENTS.tsv: " + differs.format("python"),
                "R/yamaa/inst/COMPONENTS.tsv: lists pyo3 0.27.2, which the r "
                "bundle no longer ships",
                "R/yamaa/inst/COMPONENTS.tsv: " + differs.format("r"),
            ],
        )
        # A changed purpose, license or source changes no row's identity.
        self.paths["python"].write_text(
            self.generated["python"].replace("the language engine", "the engine")
        )
        self.paths["r"].unlink()
        self.assertEqual(
            inventory.check(self.generated, self.paths, self.project),
            [
                "python/COMPONENTS.tsv: " + differs.format("python"),
                "R/yamaa/inst/COMPONENTS.tsv: missing",
            ],
        )

    def run_main(self, *arguments):
        output, errors = io.StringIO(), io.StringIO()
        with (
            mock.patch.object(inventory, "generate", return_value=self.generated),
            mock.patch.object(inventory, "committed", return_value=self.paths),
            mock.patch.object(
                inventory.notices, "python_project", return_value=self.project
            ),
            mock.patch("sys.argv", ["generate_component_inventory", *arguments]),
            contextlib.redirect_stdout(output),
            contextlib.redirect_stderr(errors),
        ):
            try:
                inventory.main()
            except SystemExit as stopped:
                return stopped.code, output.getvalue(), errors.getvalue()
        return 0, output.getvalue(), errors.getvalue()

    def test_main_checks_writes_or_prints_the_inventories(self):
        code, output, _ = self.run_main()
        self.assertEqual(code, 0)
        self.assertEqual(
            output.splitlines(),
            [
                "python/COMPONENTS.tsv: the 7 components of the python bundle",
                "R/yamaa/inst/COMPONENTS.tsv: the 6 components of the r bundle",
            ],
        )
        self.paths["python"].write_text("name\tversion\n", encoding="utf-8")
        code, _, errors = self.run_main()
        self.assertEqual(code, 1)
        self.assertIn("python/COMPONENTS.tsv: no row for pyo3 0.27.2", errors)
        self.assertTrue(
            errors.endswith(
                "Run: python -m tools.packaging.generate_component_inventory --write\n"
            )
        )
        code, output, _ = self.run_main("--write")
        self.assertEqual(
            (code, output.split(":")[0]), (0, "Wrote python/COMPONENTS.tsv")
        )
        self.assertEqual(
            self.paths["python"].read_bytes(), self.generated["python"].encode()
        )
        # Any host's inventory goes to standard output byte for byte.
        printed = io.TextIOWrapper(io.BytesIO(), newline="\r\n")
        with (
            mock.patch.object(inventory, "generate", return_value=self.generated),
            mock.patch("sys.argv", ["generate_component_inventory", "--host", "r"]),
            contextlib.redirect_stdout(printed),
        ):
            inventory.main()
        printed.flush()
        self.assertEqual(printed.buffer.getvalue(), self.generated["r"].encode())


class GraphTest(unittest.TestCase):
    @unittest.skipUnless(shutil.which("cargo"), "cargo is unavailable")
    def test_each_host_lists_its_bundle_and_the_vendored_c_code(self):
        generated = inventory.generate(["python", "r"])
        for host, binding, other in (
            ("python", "yamaa-python", "yamaa-r"),
            ("r", "yamaa-r", "yamaa-python"),
        ):
            with self.subTest(host=host):
                lines = generated[host].splitlines()
                self.assertEqual(lines[0], "\t".join(inventory.COLUMNS))
                rows = {}
                for line in lines[1:]:
                    name, version, license_, source, _ = line.split("\t")
                    rows.setdefault(name, []).append((version, license_, source))
                for name in ("yamaa-core", "yamaa-adapters", "yamaa", binding, "cc"):
                    self.assertIn(name, rows)
                self.assertNotIn(other, rows)
                self.assertEqual(rows["cc"][0][2], "crates.io")
                ((_, _, parser),) = rows["yamaa-saphyr-parser"]
                self.assertTrue(
                    parser.startswith(
                        "rust/vendor/saphyr-parser, from "
                        "https://github.com/saphyr-rs/saphyr at "
                    ),
                    parser,
                )
                ((_, _, source),) = rows["ReadStat"]
                self.assertTrue(
                    source.startswith("rust/crates/yamaa-adapters/vendor/readstat")
                )
                ((zstd_sys, _, _),) = rows["zstd-sys"]
                ((zstd, _, bundled),) = rows["libzstd"]
                self.assertEqual(zstd_sys.partition("+zstd.")[2], zstd)
                self.assertEqual(bundled, f"bundled in zstd-sys {zstd_sys}")


if __name__ == "__main__":
    unittest.main()
