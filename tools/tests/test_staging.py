"""Ensure the R archive has real shared sources and cannot overwrite prior work."""

import contextlib
import csv
import io
import json
import os
import tarfile
import tempfile
import tomllib
import unittest
from pathlib import Path
from unittest import mock

from tools.packaging import check_cargo_lock
from tools.staging import engine_bundle, vendor_crates
from tools.staging import stage_r_package as staging
from tools.staging import stage_specification_inputs as specification


class StagingTests(unittest.TestCase):
    def test_shared_sources_are_self_contained(self):
        """The staged archive owns sources and fixture bytes without checkout links."""
        with tempfile.TemporaryDirectory() as directory:
            destination = Path(directory) / "yamaa"
            with contextlib.redirect_stdout(io.StringIO()):
                staging.stage(destination)
            self.assertTrue((destination / "DESCRIPTION").is_file())
            # configure writes the Makevars from their templates at
            # installation, so the package carries only the templates.
            for name in ("configure", "configure.win", "cleanup", "cleanup.win"):
                self.assertTrue(os.access(destination / name, os.X_OK), name)
            self.assertTrue((destination / "tools/configure.R").is_file())
            for name in ("Makevars", "Makevars.win"):
                self.assertFalse((destination / "src" / name).exists(), name)
                self.assertTrue((destination / "src" / f"{name}.in").is_file(), name)
            # src/rust is the R engine bundle: its sources from the checkout,
            # its workspace manifest and a verified trimmed lock, beside the
            # compressed crates and their Cargo configuration.
            embedded = destination / "src/rust"
            expected = {
                relative: source.read_bytes()
                for relative, source in engine_bundle.sources("r")
            }
            expected["Cargo.toml"] = engine_bundle.workspace_manifest().encode()
            self.assertEqual(
                {
                    path.relative_to(embedded).as_posix()
                    for path in embedded.rglob("*")
                    if path.is_file()
                },
                set(expected) | {"Cargo.lock", "vendor.tar.xz", "vendor-config.toml"},
            )
            for relative, payload in expected.items():
                self.assertFalse((embedded / relative).is_symlink())
                self.assertEqual((embedded / relative).read_bytes(), payload)
            kept = engine_bundle.verify_lock(
                (embedded / "Cargo.lock").read_text(encoding="utf-8"),
                (staging.WORKSPACE / "Cargo.lock").read_text(encoding="utf-8"),
                engine_bundle.members("r"),
            )
            self.assertEqual(
                sorted(path.name for path in (embedded / "crates").iterdir()),
                sorted(engine_bundle.members("r")),
            )
            # No crate tests, nor the benchmark files that they compile in,
            # and no toolchain file anywhere (#3030 D55).
            self.assertEqual(list(embedded.rglob("tests")), [])
            self.assertEqual(
                {path.name for path in (destination / "src").iterdir()},
                {path.name for path in (staging.REPOSITORY / "R/yamaa/src").iterdir()}
                | {"rust"},
            )
            self.assertEqual(
                [
                    path
                    for path in destination.rglob("*")
                    if path.name in vendor_crates.TOOLCHAIN_FILES
                ],
                [],
            )
            # The installed R package carries the Python package notices.
            notices = tomllib.loads(
                (staging.REPOSITORY / "python/pyproject.toml").read_text(
                    encoding="utf-8"
                )
            )["project"]["license-files"]
            self.assertIn("LICENSE.txt", notices)
            self.assertIn("LICENSE-THIRD-PARTY.txt", notices)
            for name in notices:
                self.assertEqual(
                    (destination / "inst" / name).read_bytes(),
                    (staging.REPOSITORY / "python" / name).read_bytes(),
                )
            self.assertEqual(
                (destination / "inst/LICENSE.txt").read_bytes(),
                (staging.REPOSITORY / "LICENSE").read_bytes(),
            )
            for author in ("ChenYuheng", "Ethiraric"):
                self.assertIn(
                    (
                        staging.WORKSPACE
                        / f"vendor/saphyr-parser/LICENSES/{author}-MIT"
                    ).read_bytes(),
                    (destination / "inst/LICENSE-YAML.txt").read_bytes(),
                )
            self.assertFalse(
                any(
                    path.name.startswith(".")
                    for path in (embedded / "vendor/saphyr-parser").rglob("*")
                )
            )
            self.assertFalse((embedded / "target").exists())
            grammar = destination / "inst/grammar-original/grammar"
            for source in (staging.REPOSITORY / "yaml/grammar").glob("*.yaml"):
                self.assertEqual(
                    (grammar / source.name).read_bytes(), source.read_bytes()
                )
            self.assertTrue((destination / "tests/public_grammar.R").is_file())
            # The package builds offline from the crates.io crates of the
            # trimmed lock, and carries the script that bundled them and
            # their authors' names.
            registry = {(name, version) for name, version, source in kept if source}
            unpacked = Path(directory) / "unpacked"
            with tarfile.open(destination / staging.VENDOR_ARCHIVE, "r:xz") as archive:
                members = archive.getmembers()
                self.assertTrue(all(m.isfile() or m.isdir() for m in members))
                archive.extractall(unpacked, filter="data")
            crates = sorted((unpacked / vendor_crates.DIRECTORY).iterdir())
            self.assertEqual(
                {crate.name for crate in crates},
                {f"{name}-{version}" for name, version in registry},
            )
            for crate in crates:
                files = {
                    path.relative_to(crate).as_posix()
                    for path in crate.rglob("*")
                    if path.is_file()
                }
                record = json.loads((crate / ".cargo-checksum.json").read_text())
                self.assertEqual(set(record["files"]), files - {".cargo-checksum.json"})
                for path in crate.rglob("*"):
                    if path.is_file():
                        self.assertFalse(vendor_crates.is_binary(path), path)
                        self.assertNotIn(path.name, vendor_crates.TOOLCHAIN_FILES)
            self.assertEqual(
                (destination / staging.VENDOR_CONFIG).read_text(encoding="utf-8"),
                engine_bundle.CARGO_CONFIG,
            )
            self.assertEqual(
                (destination / staging.VENDOR_SCRIPT).read_bytes(),
                Path(vendor_crates.__file__).read_bytes(),
            )
            authors = (destination / staging.AUTHORS).read_text(encoding="utf-8")
            self.assertEqual(
                {
                    tuple(line.removeprefix("Crate: ").split(" "))
                    for line in authors.splitlines()
                    if line.startswith("Crate: ")
                },
                registry | {("yamaa-saphyr-parser", "0.2.1")},
            )
            original = destination / "inst/specification-original"
            for module in (original / "schema").glob("*.yaml"):
                self.assertEqual(
                    module.read_bytes(),
                    (staging.REPOSITORY / "yaml" / module.name).read_bytes(),
                )
            for name in (
                "negative-zero-division",
                "negative-integer-overflow",
                "adam-adlb-ordered-sum",
                "schema-lookup",
                "schema-window-functions",
                "schema-inheritance",
            ):
                files = (
                    (
                        "spec_study.yaml",
                        "spec_organization.yaml",
                        "spec_compound.yaml",
                        "input/lb.csv",
                        "expected/adlb.csv",
                    )
                    if name == "schema-inheritance"
                    else (
                        "spec.yaml",
                        "input/dm.csv",
                        "input/ae.csv",
                        "input/meddict.csv",
                        "expected/adsl.csv",
                    )
                    if name == "schema-lookup"
                    else ("spec.yaml", "input/vs.csv", "expected/advs.csv")
                    if name == "schema-window-functions"
                    else (
                        "spec.yaml",
                        "input/lb.csv",
                        "expected/adlb.csv"
                        if name == "adam-adlb-ordered-sum"
                        else "expected/error.yaml",
                    )
                )
                for relative in files:
                    self.assertEqual(
                        (original / "cases" / name / relative).read_bytes(),
                        (
                            staging.REPOSITORY / "benchmarks" / name / relative
                        ).read_bytes(),
                    )
                truth = (
                    staging.WORKSPACE
                    / "crates/yamaa-adapters/tests/fixtures/specs"
                    / (name + ".json")
                )
                self.assertEqual(
                    json.loads((original / "expected" / (name + ".json")).read_text()),
                    json.loads(truth.read_text()),
                )

            # Installed inputs are what the installed R suites read, and each
            # third-party fixture keeps its license beside it.
            resources = destination / "inst"
            for path in resources.rglob("*"):
                if path.is_file():
                    relative = path.relative_to(resources).as_posix()
                    left_out = False
                    for pattern in staging.UNREAD_BY_R_SUITES:
                        if staging.matches(relative, pattern.removeprefix("!")):
                            left_out = not pattern.startswith("!")
                    self.assertFalse(left_out, relative)
            self.assertFalse((resources / "host_fixtures.py").exists())
            self.assertFalse((resources / "define-fixtures/schemas").exists())
            self.assertEqual(
                [path.relative_to(resources) for path in resources.rglob("uv.lock")],
                [Path("project-base/negative-function-contract/python/uv.lock")],
            )
            projects = resources / "project-base"
            self.assertFalse((resources / "project-original").exists())
            for path in projects.rglob("*"):
                if path.is_file():
                    self.assertNotIn(b"yamaabenchmarks", path.read_bytes(), str(path))
            for case in ("negative-function-contract", "base-round"):
                lock = json.loads((projects / case / "r/renv.lock").read_text())
                self.assertEqual(set(lock["Packages"]), {"yamaa"})
            self.assertIn(
                "base::identity",
                (
                    projects / "negative-function-contract/r/environment.yaml"
                ).read_text(),
            )
            self.assertIn(
                "base::signif", (projects / "base-round/r/environment.yaml").read_text()
            )
            self.assertEqual(list(original.glob("*.json")), [])
            fixtures = staging.WORKSPACE / "crates/yamaa-adapters/tests/fixtures/sas"
            for name in ("LICENSE-haven.txt", "LICENSE-pandas.txt"):
                self.assertEqual(
                    (resources / "host-fixtures/sas" / name).read_bytes(),
                    (fixtures / name).read_bytes(),
                )
            self.assertFalse((resources / "host-fixtures/sas/README.md").exists())
            # The package passes the check that its built archive gets.
            built = Path(directory) / "yamaa_0.5.0.tar.gz"
            with tarfile.open(built, "w:gz", compresslevel=1) as output:
                output.add(destination, arcname="yamaa")
            self.assertEqual(check_cargo_lock.check_bundle_archive(built), ("r", kept))

    def test_shared_specification_inputs_keep_both_preparation_copies(self):
        """Both copies retain the fixture-owned facts and requirement citations."""
        with tempfile.TemporaryDirectory() as directory:
            destination = Path(directory) / "specification-original"
            specification.stage(destination)
            source_preparation = (
                staging.WORKSPACE
                / "crates/yamaa-adapters/tests/fixtures/inheritance_preparation.tsv"
            )
            retained_preparation = destination / "inheritance-preparation.tsv"
            self.assertEqual(
                retained_preparation.read_bytes(), source_preparation.read_bytes()
            )

            def preparation_rows(path):
                with path.open(encoding="ascii", newline="") as stream:
                    return list(csv.DictReader(stream, delimiter="\t"))

            historical = preparation_rows(retained_preparation)
            normative = preparation_rows(
                retained_preparation.with_name("normative-inheritance-preparation.tsv")
            )
            for rows in (historical, normative):
                for row in rows:
                    row["expected"] = json.loads(row["expected"])
            self.assertEqual(historical, normative)

    def test_unread_inputs_follow_gitignore_order_and_refuse_stale_patterns(self):
        with tempfile.TemporaryDirectory() as directory:
            resources = Path(directory)
            for name in (
                "cases/kept/spec.yaml",
                "cases/held/spec.yaml",
                "cases/held/input/data.csv",
                "pq/read.parquet",
                "pq/unread.parquet",
                "truth.json",
                "deep/truth.json",
            ):
                (resources / name).parent.mkdir(parents=True, exist_ok=True)
                (resources / name).write_text(name)
            staging.leave_out_unread_inputs(
                resources,
                ("cases/held/**", "pq/*", "!pq/read.*", "*.json"),
            )
            self.assertEqual(
                sorted(
                    path.relative_to(resources).as_posix()
                    for path in resources.rglob("*")
                ),
                [
                    "cases",
                    "cases/kept",
                    "cases/kept/spec.yaml",
                    "deep",
                    "deep/truth.json",
                    "pq",
                    "pq/read.parquet",
                ],
            )
            # A stale pattern is refused before any staged file is removed.
            for stale in ("cases/held/**", "!pq/unread.parquet"):
                with (
                    self.subTest(stale=stale),
                    self.assertRaisesRegex(ValueError, "matches no staged file"),
                ):
                    staging.leave_out_unread_inputs(resources, ("pq/*", stale))
                self.assertTrue((resources / "pq/read.parquet").is_file())

    def test_missing_lock_refuses_before_staging(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            with (
                mock.patch.object(staging, "WORKSPACE", root),
                self.assertRaisesRegex(FileNotFoundError, "canonical Cargo.lock"),
            ):
                staging.stage(root / "destination")
            self.assertFalse((root / "destination").exists())

    def test_existing_destination_is_preserved(self):
        with tempfile.TemporaryDirectory() as directory:
            destination = Path(directory)
            marker = destination / "keep.txt"
            marker.write_text("prior build")
            with self.assertRaises(FileExistsError):
                staging.stage(destination)
            self.assertEqual(marker.read_text(), "prior build")
