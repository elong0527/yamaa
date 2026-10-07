"""Ensure the R archive has real shared sources and cannot overwrite prior work."""

import contextlib
import importlib.util
import io
import json
import tempfile
import unittest
from pathlib import Path

spec = importlib.util.spec_from_file_location(
    "stage_r_package", Path(__file__).parents[1] / "tools/stage_r_package.py"
)
staging = importlib.util.module_from_spec(spec)
spec.loader.exec_module(staging)


class StagingTests(unittest.TestCase):
    def test_shared_sources_are_self_contained(self):
        """The staged archive owns sources and fixture bytes without checkout links."""
        with tempfile.TemporaryDirectory() as directory:
            destination = Path(directory) / "yamaanative"
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
            self.assertFalse((embedded / "Cargo.lock").exists())
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
            ):
                files = ("spec.yaml", "input/dm.csv", "input/ae.csv", "input/meddict.csv", "expected/adsl.csv") if name == "schema-lookup" else ("spec.yaml", "input/vs.csv", "expected/advs.csv") if name == "schema-window-functions" else (
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
                    / "crates/yamaa-adapters/tests/fixtures/specifications"
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

    def test_existing_destination_is_preserved(self):
        with tempfile.TemporaryDirectory() as directory:
            destination = Path(directory)
            marker = destination / "keep.txt"
            marker.write_text("prior build")
            with self.assertRaises(FileExistsError):
                staging.stage(destination)
            self.assertEqual(marker.read_text(), "prior build")
