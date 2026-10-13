"""Ensure the R archive has real shared sources and cannot overwrite prior work."""

import collections
import contextlib
import csv
import importlib.util
import io
import json
import tempfile
import unittest
from pathlib import Path
from unittest import mock

spec = importlib.util.spec_from_file_location(
    "stage_r_package", Path(__file__).parents[1] / "tools/stage_r_package.py"
)
staging = importlib.util.module_from_spec(spec)
spec.loader.exec_module(staging)


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
            # Historical preparation expectations keep facts unchanged; only the
            # approved citation-only sidecar may fill a missing requirement.
            def changes(before, after):
                if isinstance(before, dict):
                    self.assertEqual(before.keys(), after.keys())
                    for key in before:
                        if key == "requirement" and before[key] != after[key]:
                            self.assertIsNone(before[key])
                            yield after[key]
                        else:
                            yield from changes(before[key], after[key])
                elif isinstance(before, list):
                    self.assertEqual(len(before), len(after))
                    for old, new in zip(before, after, strict=True):
                        yield from changes(old, new)
                else:
                    self.assertEqual(before, after)

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
            self.assertEqual(len(historical), len(normative))
            actual_changes = collections.Counter()
            for old, new in zip(historical, normative, strict=True):
                before, after = (
                    json.loads(old.pop("expected")),
                    json.loads(new.pop("expected")),
                )
                self.assertEqual(old, new)
                actual_changes.update(changes(before, after))
            self.assertEqual(dict(actual_changes), {"REQ-0285": 1})

    def test_missing_lock_refuses_before_staging(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            with mock.patch.object(staging, "WORKSPACE", root), self.assertRaisesRegex(
                FileNotFoundError, "canonical Cargo.lock"
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
