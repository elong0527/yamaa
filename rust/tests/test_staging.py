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
                if source.is_file() and source.suffix in {".rs", ".toml", ".txt"}:
                    copied = embedded / source.relative_to(staging.WORKSPACE)
                    self.assertFalse(copied.is_symlink())
                    self.assertEqual(source.read_bytes(), copied.read_bytes())
            self.assertTrue((embedded / "rust-toolchain.toml").is_file())
            self.assertFalse((embedded / "target").exists())
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
                files = ("spec_study.yaml", "spec_organization.yaml", "spec_compound.yaml", "input/lb.csv", "expected/adlb.csv") if name == "schema-inheritance" else ("spec.yaml", "input/dm.csv", "input/ae.csv", "input/meddict.csv", "expected/adsl.csv") if name == "schema-lookup" else ("spec.yaml", "input/vs.csv", "expected/advs.csv") if name == "schema-window-functions" else (
                    "spec.yaml", "input/lb.csv", "expected/adlb.csv" if name == "adam-adlb-ordered-sum" else "expected/error.yaml"
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
            for fixture in (
                "aggregate_syntax.tsv",
                "numeric_syntax.tsv",
                "predicate_syntax.tsv",
                "schema_transport.tsv",
                "schema_model.tsv",
                "schema_windows.tsv",
                "schema_composition.tsv",
                "schema_layer_admission.tsv",
                "schema_inheritance_dependencies.tsv",
                "yaml_transport.tsv",
                "regex_transport.tsv",
                "scalar_transport.tsv",
                "numeric_transport.tsv",
                "reference_binding.tsv",
                "reference_scope.tsv",
                "reference_intermediate.tsv",
                "reference_keys.tsv",
                "reference_match_values.tsv",
                "reference_relations.tsv",
            ):
                self.assertEqual(
                    (destination / "inst" / fixture).read_bytes(),
                    (
                        staging.WORKSPACE
                        / "crates/yamaa-adapters/tests/fixtures"
                        / fixture
                    ).read_bytes(),
                )

            # Sidecars may correct approved citations only, keeping captured facts intact.
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

            inventory = {
                "schema_model.tsv": {"REQ-0287": 33, "REQ-0285": 4, "REQ-0286": 4},
                "schema_transport.tsv": {"REQ-0281": 1},
                "schema_layer_admission.tsv": {"REQ-0285": 1},
                "schema_composition.tsv": {}, "schema_windows.tsv": {},
                "schema_inheritance_dependencies.tsv": {},
            }
            for fixture, expected_changes in inventory.items():
                def rows(name):
                    with (destination / "inst" / name).open(encoding="utf-8", newline="") as stream:
                        return list(csv.DictReader(stream, delimiter="\t"))
                historical = rows(fixture)
                normative = rows("normative-" + fixture)
                self.assertEqual(len(historical), len(normative))
                actual_changes = collections.Counter()
                for old, new in zip(historical, normative, strict=True):
                    before = json.loads(old.pop("expected"))
                    after = json.loads(new.pop("expected"))
                    self.assertEqual(old, new)
                    actual_changes.update(changes(before, after))
                self.assertEqual(dict(actual_changes), expected_changes, fixture)

            source_preparation = staging.WORKSPACE / "crates/yamaa-adapters/tests/fixtures/inheritance_preparation.tsv"
            retained_preparation = destination / "inst/specification-original/inheritance-preparation.tsv"
            self.assertEqual(retained_preparation.read_bytes(), source_preparation.read_bytes())
            def preparation_rows(path):
                with path.open(encoding="ascii", newline="") as stream:
                    return list(csv.DictReader(stream, delimiter="\t"))
            historical = preparation_rows(retained_preparation)
            normative = preparation_rows(retained_preparation.with_name("normative-inheritance-preparation.tsv"))
            self.assertEqual(len(historical), len(normative))
            actual_changes = collections.Counter()
            for old, new in zip(historical, normative, strict=True):
                before, after = json.loads(old.pop("expected")), json.loads(new.pop("expected"))
                self.assertEqual(old, new)
                actual_changes.update(changes(before, after))
            self.assertEqual(dict(actual_changes), {"REQ-0285": 1})

            for family in ("tables", "datasets"):
                fixtures = (
                    staging.WORKSPACE / "crates/yamaa-adapters/tests/fixtures" / family
                )
                for source in fixtures.iterdir():
                    self.assertEqual(
                        (destination / "inst" / family / source.name).read_bytes(),
                        source.read_bytes(),
                    )
            self.assertEqual(
                (destination / "inst/function_invocation.tsv").read_bytes(),
                (
                    staging.WORKSPACE
                    / "crates/yamaa-engine/tests/fixtures/function_invocation.tsv"
                ).read_bytes(),
            )
            self.assertEqual(
                (destination / "inst/dependency_analysis.tsv").read_bytes(),
                (
                    staging.WORKSPACE
                    / "crates/yamaa-core/tests/fixtures/dependency_analysis.tsv"
                ).read_bytes(),
            )
            self.assertEqual(
                (destination / "inst/column_dependencies.tsv").read_bytes(),
                (
                    staging.WORKSPACE
                    / "crates/yamaa-core/tests/fixtures/column_dependencies.tsv"
                ).read_bytes(),
            )

    def test_missing_lock_refuses_before_staging(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            with mock.patch.object(staging, "WORKSPACE", root):
                with self.assertRaisesRegex(FileNotFoundError, "canonical Cargo.lock"):
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
