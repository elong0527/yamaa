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


BSD = (
    "Copyright (c) Meta Platforms, Inc.\n\nRedistribution and use in source and "
    "binary forms are permitted.\nNeither the name of Facebook.\n"
)
# License texts with the phrases that the notices recognize them by.
TEXTS = {
    "MIT": "Permission is hereby granted, free of charge, to any person obtaining "
    "a copy\n",
    "Apache-2.0": "TERMS AND CONDITIONS FOR USE, REPRODUCTION, AND DISTRIBUTION\n",
}


class Workspace:
    """A checkout with crate packages as `cargo metadata` describes them."""

    def __init__(self, root):
        self.root = root
        self.workspace = root / "rust"
        self.packages, self.members = [], []

    def crate(self, name, version, license, directory=None, member=False, **fields):
        if directory is None:
            source = notices.CRATES_IO
            directory = self.root / "registry" / f"{name}-{version}"
        else:
            source = None
            directory = self.workspace / directory
        directory.mkdir(parents=True, exist_ok=True)
        package = {
            "id": f"{name}@{version}",
            "name": name,
            "version": version,
            "license": license,
            "source": source,
            "manifest_path": str(directory / "Cargo.toml"),
            "repository": None,
            **fields,
        }
        self.packages.append(package)
        if member:
            self.members.append(package["id"])
        return directory

    def kept(self):
        return {(p["name"], p["version"], p["source"]) for p in self.packages}

    def metadata(self):
        return {"packages": self.packages, "workspace_members": self.members}


def workspace(root):
    """The engine crates, the vendored parser and ReadStat, pyo3 and zstd-sys."""
    checkout = Workspace(root)
    checkout.crate(
        "yamaa-core", "0.5.0", "MIT AND Unicode-3.0", "crates/yamaa-core", True
    )
    adapters = checkout.crate(
        "yamaa-adapters", "0.5.0", "MIT", "crates/yamaa-adapters", True
    )
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
    # The notices' reviewed texts of the parser and zstd-sys must exist.
    for relative, license in notices.EXTRA_FILES["yamaa-saphyr-parser"].items():
        (parser / relative).parent.mkdir(exist_ok=True)
        (parser / relative).write_text(TEXTS[license])
    checkout.crate("pyo3", "0.27.2", "MIT/Apache-2.0")
    zstd = checkout.crate("zstd-sys", "2.1.2+zstd.1.5.7", "BSD-3-Clause")
    (zstd / "zstd").mkdir()
    (zstd / "zstd/LICENSE").write_text(BSD)
    (zstd / "zstd/COPYING").write_text("GNU GENERAL PUBLIC LICENSE\nVersion 2\n")
    return checkout


def notice_text(*crates):
    """A LICENSE-THIRD-PARTY.txt whose crate section lists the crates; its
    standard library section lists a crate of its own."""
    entries = "".join(
        f"Crate: {name} {version}\n    License: MIT\n\n" for name, version in crates
    )
    return (
        "yamaa third-party notices\n=========================\n\n"
        f"Crates\n------\n\n{entries}"
        "Rust standard library\n---------------------\n\n"
        "Crate: hashbrown 0.17.1\n    License: MIT\n"
    )


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
            CLI=self.root / "rust/crates/yamaa-cli",
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
        # Each row names the one license that yamaa relies on, never the
        # choice that the manifest declares.
        self.assertEqual(
            rows["pyo3"],
            inventory.Component("pyo3", "0.27.2", "MIT", "crates.io", PURPOSES["pyo3"]),
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
            ("1.5.7", "BSD-3-Clause", "bundled in zstd-sys 2.1.2+zstd.1.5.7"),
        )

    def test_the_inventory_is_a_header_and_one_tab_separated_row_each(self):
        text = inventory.render(self.rows())
        lines = text.split("\n")
        self.assertEqual(lines[0], "name\tversion\tlicense\tsource\tpurpose")
        self.assertEqual(lines[-1], "")
        self.assertIn(
            "pyo3\t0.27.2\tMIT\tcrates.io\tthe Python binding's interface to CPython",
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
        # No row may name a GNU license.
        row = inventory.Component("gnu", "1.0.0", "GPL-2.0-only", "crates.io", "x")
        with self.assertRaisesRegex(ValueError, r"names a GNU license \(GPL\)"):
            inventory.render([row])

    def test_licenses_are_the_ones_that_yamaa_relies_on(self):
        pyo3 = next(p for p in self.checkout.packages if p["name"] == "pyo3")
        for declared, relied in (
            ("MIT OR Apache-2.0", "MIT"),
            ("MIT OR Apache-2.0 OR LGPL-2.1-or-later", "MIT"),
            ("BSD-3-Clause/MIT", "MIT"),
            ("BSD-3-Clause AND (MIT/Apache-2.0)", "MIT AND BSD-3-Clause"),
        ):
            pyo3["license"] = declared
            with self.subTest(declared=declared):
                self.assertEqual(
                    {row.name: row.license for row in self.rows()}["pyo3"], relied
                )
        for declared, message in (
            (None, "declares no license"),
            ("MIT OR", "malformed license expression"),
            ("GPL-3.0-only", "review the license"),
        ):
            pyo3["license"] = declared
            with (
                self.subTest(declared=declared),
                self.assertRaisesRegex(ValueError, message),
            ):
                self.rows()

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

    def test_binary_packages_name_the_standard_library_that_they_link(self):
        rows = self.rows()
        linked = inventory.components(
            self.checkout.kept(), self.checkout.metadata(), PURPOSES, "1.99.1"
        )
        (added,) = set(linked) - set(rows)
        self.assertEqual(
            (added.name, added.version, added.license, added.source),
            (
                "rust-std",
                "1.99.1",
                "MIT",
                "https://github.com/rust-lang/rust at 1.99.1",
            ),
        )
        self.assertIn("the Rust standard library", added.purpose)
        self.assertEqual(
            [row.name for row in linked],
            sorted((row.name for row in linked), key=str.casefold),
        )
        # Only a pinned release names the standard library that a package links.
        with self.assertRaisesRegex(ValueError, "pins 'stable', not a Rust release"):
            inventory.components(
                self.checkout.kept(), self.checkout.metadata(), PURPOSES, "stable"
            )

    def test_the_source_archive_leaves_out_the_standard_library_row(self):
        wheels = inventory.render(
            inventory.components(
                self.checkout.kept(), self.checkout.metadata(), PURPOSES, "1.99.1"
            )
        )
        self.assertEqual(
            inventory.source_inventory(wheels), inventory.render(self.rows())
        )
        for text in (inventory.render(self.rows()), wheels + wheels.split("\n", 1)[1]):
            with (
                self.subTest(rows=text.count("\nrust-std\t")),
                self.assertRaisesRegex(
                    ValueError, "the wheels' inventory must hold one rust-std row"
                ),
            ):
                inventory.source_inventory(text)

    def test_an_installed_r_package_appends_the_standard_library_row(self):
        # The R package's configure step appends one rust-std row, for the
        # rustc that compiles the installation, to the inventory that R
        # installs; the R source package's own inventory names none.
        source = inventory.render(self.rows())
        installed = inventory.installed_inventory(source, "1.99.1")
        self.assertTrue(installed.startswith(source))
        self.assertEqual(
            installed[len(source) :].split("\t"),
            [
                "rust-std",
                "1.99.1",
                "MIT",
                "https://github.com/rust-lang/rust at 1.99.1",
                inventory.INSTALLED_PURPOSE + "\n",
            ],
        )
        self.assertIn(
            "toolchain that compiled this package", inventory.INSTALLED_PURPOSE
        )
        for text, release, error in (
            (installed, "1.99.1", "the R source package's inventory must hold no"),
            (source, "stable", "pins 'stable', not a Rust release"),
        ):
            with (
                self.subTest(release=release),
                self.assertRaisesRegex(ValueError, error),
            ):
                inventory.installed_inventory(text, release)

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
        # The command line's inventory names the standard library that its
        # binaries link, as the wheels' does.
        linked = inventory.components(
            self.checkout.kept(), self.checkout.metadata(), PURPOSES, "1.99.1"
        )
        self.generated = {
            "python": inventory.render(self.rows()),
            "r": inventory.render([row for row in self.rows() if row.name != "pyo3"]),
            "cli": inventory.render([row for row in linked if row.name != "pyo3"]),
        }
        self.paths = inventory.committed()
        for host, path in self.paths.items():
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(self.generated[host], encoding="utf-8")
        self.project = {"license-files": ["LICENSE.txt", "COMPONENTS.tsv"]}

    def test_current_inventories_listed_as_license_files_pass(self):
        self.assertEqual(
            self.paths,
            {
                "python": self.root / "python/COMPONENTS.tsv",
                "r": self.root / "R/yamaa/inst/COMPONENTS.tsv",
                "cli": self.root / "rust/crates/yamaa-cli/COMPONENTS.tsv",
            },
        )
        self.assertEqual(inventory.check(self.generated, self.paths, self.project), [])

    def test_stale_missing_or_unlisted_inventories_fail(self):
        # The R inventory still lists a crate that its bundle dropped, and the
        # Python one misses a crate that its bundle gained.
        self.paths["r"].write_text(self.generated["python"], encoding="utf-8")
        python = self.generated["python"].replace(
            "pyo3\t0.27.2\tMIT\tcrates.io\tthe Python binding's interface to CPython\n",
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

    def test_lines_that_are_not_rows_are_named_with_their_numbers(self):
        # A merge left its conflict markers around the pyo3 row of the Python
        # inventory, and a tab split the yamaa-core purpose of the R one.
        row = (
            "pyo3\t0.27.2\tMIT\tcrates.io\tthe Python binding's interface to CPython\n"
        )
        conflicted = (
            f"<<<<<<< HEAD\n{row}=======\n{row.replace('CPython', 'Python')}"
            ">>>>>>> main\n"
        )
        self.paths["python"].write_text(
            self.generated["python"].replace(row, conflicted), encoding="utf-8"
        )
        self.paths["r"].write_text(
            self.generated["r"].replace("the language engine", "the language\tengine"),
            encoding="utf-8",
        )
        not_a_row = "not a row of 5 tab-separated fields"
        differs = (
            "differs from the inventory that the {} bundle's trimmed lock, "
            "tools/packaging/crate_purposes.toml and the vendored sources give"
        )
        messages = [
            f"python/COMPONENTS.tsv:3: {not_a_row}",
            f"python/COMPONENTS.tsv:5: {not_a_row}",
            f"python/COMPONENTS.tsv:7: {not_a_row}",
            "python/COMPONENTS.tsv: " + differs.format("python"),
            f"R/yamaa/inst/COMPONENTS.tsv:5: {not_a_row}",
            "R/yamaa/inst/COMPONENTS.tsv: " + differs.format("r"),
        ]
        self.assertEqual(
            inventory.check(self.generated, self.paths, self.project), messages
        )
        code, _, errors = self.run_main()
        self.assertEqual(
            (code, errors),
            (
                1,
                "\n".join(messages)
                + "\nRun: python -m tools.packaging.generate_component_inventory "
                "--write\n",
            ),
        )

    def run_main(self, *arguments):
        output, errors = io.StringIO(), io.StringIO()
        with (
            mock.patch.object(inventory, "generate", return_value=self.generated),
            mock.patch.object(inventory, "committed", return_value=self.paths),
            mock.patch.object(
                inventory.notices, "python_project", return_value=self.project
            ),
            mock.patch.object(
                inventory.notices,
                "locked_metadata",
                return_value=self.checkout.metadata(),
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
                "rust/crates/yamaa-cli/COMPONENTS.tsv: the 7 components of the cli "
                "bundle",
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
            mock.patch.object(
                inventory, "generate", return_value=self.generated
            ) as generate,
            mock.patch.object(
                inventory.notices,
                "locked_metadata",
                return_value=self.checkout.metadata(),
            ),
            mock.patch("sys.argv", ["generate_component_inventory", "--host", "r"]),
            contextlib.redirect_stdout(printed),
        ):
            inventory.main()
        printed.flush()
        self.assertEqual(printed.buffer.getvalue(), self.generated["r"].encode())
        generate.assert_called_once_with(["r"], self.checkout.metadata())

    def test_main_names_the_notices_that_list_other_crates_than_the_inventory(self):
        # The Python and R notices list the crates that their inventories mark
        # as linked; the command line's also lists pyo3, which its inventory
        # does not list.
        shared = [("yamaa-saphyr-parser", "0.2.1"), ("zstd-sys", "2.1.2+zstd.1.5.7")]
        for host, crates in (
            ("python", [("pyo3", "0.27.2"), *shared]),
            ("r", shared),
            ("cli", [("pyo3", "0.27.2"), *shared]),
        ):
            path = notices.copies()[host] / notices.NOTICES
            path.write_text(notice_text(*crates), encoding="utf-8")
        unlisted = (
            "rust/crates/yamaa-cli/LICENSE-THIRD-PARTY.txt: lists pyo3 0.27.2, which "
            "rust/crates/yamaa-cli/COMPONENTS.tsv does not mark as linked"
        )
        code, _, errors = self.run_main()
        self.assertEqual((code, errors), (1, f"{unlisted}\n{inventory.NOTICE_HINT}\n"))
        self.assertIn("generate_third_party_notices --write", inventory.NOTICE_HINT)
        # A stale inventory as well gets both hints, the inventory's first.
        self.paths["r"].write_text(self.generated["python"], encoding="utf-8")
        code, _, errors = self.run_main()
        lines = errors.splitlines()
        self.assertEqual(code, 1)
        self.assertEqual(
            lines[-3:],
            [
                unlisted,
                "Run: python -m tools.packaging.generate_component_inventory --write",
                inventory.NOTICE_HINT,
            ],
        )


class NoticeTest(Checkout):
    """Each host's notices list the crates that its inventory marks as linked."""

    PYO3 = ("pyo3", "0.27.2")
    PARSER = ("yamaa-saphyr-parser", "0.2.1")
    ZSTD = ("zstd-sys", "2.1.2+zstd.1.5.7")

    def setUp(self):
        super().setUp()
        for directory in notices.copies().values():
            directory.mkdir(parents=True)

    def generated(self, purposes=PURPOSES):
        return {
            host: inventory.render(self.rows(purposes)) for host in notices.copies()
        }

    def write_notices(self, *crates):
        for directory in notices.copies().values():
            (directory / notices.NOTICES).write_text(notice_text(*crates))

    def test_linked_crates_are_the_third_party_rows_of_linked_purposes(self):
        metadata = self.checkout.metadata()
        # Not the workspace members, nor the C code that crates compile, nor
        # the standard library.
        text = inventory.render(
            inventory.components(self.checkout.kept(), metadata, PURPOSES, "1.99.1")
        )
        self.assertIn("\nrust-std\t", text)
        self.assertIn("\nReadStat\t", text)
        self.assertEqual(
            inventory.linked_crates(text, metadata), {self.PYO3, self.PARSER, self.ZSTD}
        )
        for start in inventory.UNLINKED:
            purposes = dict(PURPOSES, pyo3=f"{start} what pulls it in")
            with self.subTest(purpose=start):
                self.assertEqual(
                    inventory.linked_crates(
                        inventory.render(self.rows(purposes)), metadata
                    ),
                    {self.PARSER, self.ZSTD},
                )

    def test_each_notice_lists_the_crates_that_its_inventory_marks_as_linked(self):
        metadata = self.checkout.metadata()
        self.write_notices(self.PYO3, self.PARSER, self.ZSTD)
        self.assertEqual(inventory.notice_problems(self.generated(), metadata), [])
        # A crate that the inventory marks as not compiled, one that it does
        # not list, and a linked one that the notices leave out.
        self.write_notices(self.PYO3, self.PARSER, ("rogue", "1.0.0"))
        generated = self.generated(dict(PURPOSES, pyo3="not compiled: on wasm32"))
        expected = []
        for directory in ("python", "R/yamaa/inst", "rust/crates/yamaa-cli"):
            notice = f"{directory}/LICENSE-THIRD-PARTY.txt"
            inventory_path = f"{directory}/COMPONENTS.tsv"
            expected += [
                f"{notice}: lists pyo3 0.27.2, which {inventory_path} does not mark "
                "as linked",
                f"{notice}: lists rogue 1.0.0, which {inventory_path} does not mark "
                "as linked",
                f"{notice}: no entry for zstd-sys 2.1.2+zstd.1.5.7, which "
                f"{inventory_path} marks as linked",
            ]
        self.assertEqual(inventory.notice_problems(generated, metadata), expected)
        # A missing notice is the notices' generator's to report.
        for directory in notices.copies().values():
            (directory / notices.NOTICES).unlink()
        self.assertEqual(inventory.notice_problems(generated, metadata), [])

    def test_one_purpose_covers_every_release_of_a_name(self):
        # The inventory marks a name, so the notices need some release of it.
        self.checkout.crate("pyo3", "0.26.0", "MIT")
        metadata = self.checkout.metadata()
        generated = self.generated()
        self.assertIn("\npyo3\t0.26.0\t", generated["python"])
        self.write_notices(self.PYO3, self.PARSER, self.ZSTD)
        self.assertEqual(inventory.notice_problems(generated, metadata), [])
        self.write_notices(self.PARSER, self.ZSTD)
        problems = inventory.notice_problems(generated, metadata)
        self.assertEqual(
            problems[:2],
            [
                "python/LICENSE-THIRD-PARTY.txt: no entry for pyo3 0.26.0, which "
                "python/COMPONENTS.tsv marks as linked",
                "python/LICENSE-THIRD-PARTY.txt: no entry for pyo3 0.27.2, which "
                "python/COMPONENTS.tsv marks as linked",
            ],
        )
        # A release that the inventory does not list is still not linked.
        self.write_notices(("pyo3", "0.25.0"), self.PARSER, self.ZSTD)
        self.assertEqual(
            inventory.notice_problems(generated, metadata)[0],
            "python/LICENSE-THIRD-PARTY.txt: lists pyo3 0.25.0, which "
            "python/COMPONENTS.tsv does not mark as linked",
        )


class GraphTest(unittest.TestCase):
    @unittest.skipUnless(shutil.which("cargo"), "cargo is unavailable")
    def test_each_host_lists_its_bundle_and_the_vendored_c_code(self):
        # The wheels and the command-line binaries are built with the pinned
        # toolchain (#3120); the R source package is not.
        self.assertEqual(inventory.BINARY_HOSTS, {"python", "cli"})
        generated = inventory.generate(
            ["python", "r", "cli"], notices.locked_metadata()
        )
        for host, binding, others in (
            ("python", "yamaa-python", ("yamaa-r", "yamaa-cli")),
            ("r", "yamaa-r", ("yamaa-python", "yamaa-cli")),
            ("cli", "yamaa-cli", ("yamaa-python", "yamaa-r")),
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
                for other in others:
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
                # The wheels and the command-line binaries link the pinned
                # toolchain's standard library; the R source package links
                # whichever toolchain builds it.
                if host in inventory.BINARY_HOSTS:
                    release = inventory.toolchain.channel()
                    self.assertEqual(
                        rows["rust-std"],
                        [
                            (
                                release,
                                "MIT",
                                f"https://github.com/rust-lang/rust at {release}",
                            )
                        ],
                    )
                else:
                    self.assertNotIn("rust-std", rows)


if __name__ == "__main__":
    unittest.main()
