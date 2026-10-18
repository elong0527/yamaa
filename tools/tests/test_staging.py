"""Ensure the R archive has real shared sources and cannot overwrite prior work."""

import contextlib
import csv
import io
import json
import tempfile
import tomllib
import unittest
from pathlib import Path
from unittest import mock

from tools.staging import stage_r_package as staging


class StagingTests(unittest.TestCase):
    def test_shared_sources_are_self_contained(self):
        """The staged archive owns sources and fixture bytes without checkout links."""
        with tempfile.TemporaryDirectory() as directory:
            destination = Path(directory) / "yamaa"
            with contextlib.redirect_stdout(io.StringIO()):
                staging.stage(destination)
            self.assertTrue((destination / "DESCRIPTION").is_file())
            embedded = destination / "src/rust"
            for source in (staging.WORKSPACE / "crates").rglob("*"):
                if (
                    source.is_file()
                    and source.name != "Cargo.lock"
                    and not any(
                        part in {"target", "__pycache__"} for part in source.parts
                    )
                    and source.suffix != ".pyc"
                ):
                    copied = embedded / source.relative_to(staging.WORKSPACE)
                    self.assertFalse(copied.is_symlink())
                    self.assertEqual(source.read_bytes(), copied.read_bytes())
            self.assertTrue((embedded / "rust-toolchain.toml").is_file())
            for source in (staging.WORKSPACE / "vendor").rglob("*"):
                if source.is_file():
                    copied = embedded / source.relative_to(staging.WORKSPACE)
                    self.assertFalse(copied.is_symlink())
                    self.assertEqual(source.read_bytes(), copied.read_bytes())
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
            self.assertEqual(
                (embedded / "Cargo.lock").read_bytes(),
                (staging.WORKSPACE / "Cargo.lock").read_bytes(),
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

            # Both copies retain the fixture-owned facts and requirement citations.
            source_preparation = (
                staging.WORKSPACE
                / "crates/yamaa-adapters/tests/fixtures/inheritance_preparation.tsv"
            )
            retained_preparation = (
                destination / "inst/specification-original/inheritance-preparation.tsv"
            )
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
