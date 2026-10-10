"""Draft installed native conformance checks; expected truth stays outside execution."""

import json
import shutil
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from pydantic import ValidationError
from yamaa import _native as yamaa_native
from yamaa.adapters.conformance import (
    ExampleReport,
    compare_example,
    compare_reports,
    execute_example,
)
from yamaa.io.project import ResourceFailure

ROOT = Path(__file__).parent


class NativeConformance(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.output = Path(self.directory.name)

    def run_example(self, name, backend="rust", example=None, tag="run"):
        return execute_example(
            example or ROOT / "benchmarks" / name,
            schema_root=ROOT / "yaml",
            output_dir=self.output / tag / backend / name,
            backend=backend,
        )

    def test_cohort_and_real_callbacks_match_independent_truth_and_reference(self):
        for name in (
            "adam-adlb-ordered-sum",
            "schema-lookup",
            "schema-window-functions",
            "schema-inheritance",
            "negative-zero-division",
            "negative-integer-overflow",
        ):
            with self.subTest(name=name):
                reference = self.run_example(name, "python")
                native = self.run_example(name)
                self.assertEqual(native.backend, "rust")
                self.assertEqual(
                    native.engine_version, yamaa_native.engine_info()["core_version"]
                )
                self.assertTrue(
                    compare_example(native, ROOT / "benchmarks" / name).passed
                )
                self.assertTrue(compare_reports(reference, native).passed)
                self.assertTrue(native.source_reads)
        self.assertFalse(yamaa_native.engine_info()["execution_supported"])

    def test_unsupported_run_reaches_neither_source_nor_activation(self):
        with (
            patch(
                "yamaa.adapters.native_conformance.load_source_tables",
                side_effect=AssertionError("source reached"),
            ) as sources,
        ):
            report = self.run_example("adam-adsl-bmi")
        self.assertEqual(report.outcome, "unsupported")
        self.assertIn(
            "numeric_function_POWER", [u.operation for u in report.unsupported]
        )
        self.assertEqual(report.source_reads, ())
        self.assertEqual(report.callbacks, ())
        sources.assert_not_called()

    def test_native_route_never_invokes_reference_executor(self):
        with patch(
            "yamaa.planning.workflow.execute_with_source_provider",
            side_effect=AssertionError("reference fallback"),
        ) as fallback:
            report = self.run_example("schema-lookup")
        self.assertEqual(report.outcome, "success", report.error)
        fallback.assert_not_called()

    def test_expected_directory_is_not_an_execution_input(self):
        name = "schema-lookup"
        example = self.output / name
        shutil.copytree(ROOT / "benchmarks" / name, example)
        before = self.run_example(name, example=example, tag="before")
        shutil.rmtree(example / "expected")
        after = self.run_example(name, example=example, tag="after")
        self.assertEqual(before, after)

    def test_ingestion_failure_retains_actual_source_reads(self):
        name = "schema-lookup"
        example = self.output / name
        shutil.copytree(ROOT / "benchmarks" / name, example)
        next((example / "input").glob("*.csv")).write_bytes(b"\xff")
        reference = self.run_example(name, "python", example)
        native = self.run_example(name, example=example)
        self.assertEqual(native.outcome, "failure", native.error)
        self.assertTrue(native.source_reads)
        self.assertTrue(compare_reports(reference, native).passed)
        self.assertFalse(native.artifacts)

    def test_infrastructure_failure_keeps_callback_and_source_prefix(self):
        with patch(
            "yamaa.adapters.native_datasets._output_table",
            side_effect=RuntimeError("materialization failed"),
        ):
            report = self.run_example("schema-lookup")
        self.assertEqual(report.outcome, "error")
        self.assertIn("materialization failed", report.error)
        self.assertEqual(report.callbacks, ())
        self.assertTrue(report.source_reads)
        self.assertFalse(report.artifacts)

    def test_completed_verifications_survive_materialization_failure(self):
        completed = self.run_example("adam-adlb-ordered-sum")
        self.assertEqual(completed.outcome, "success")
        self.assertEqual(len(completed.verifications), 2)
        for site in ("_output_table", "build_verification_log", "build_artifact"):
            with (
                self.subTest(site=site),
                patch(
                    f"yamaa.adapters.native_datasets.{site}",
                    side_effect=RuntimeError("materialization failed"),
                ),
            ):
                failed = self.run_example("adam-adlb-ordered-sum", tag=site)
                self.assertEqual(failed.outcome, "error")
                self.assertEqual(failed.verifications, completed.verifications)
                self.assertTrue(failed.source_reads)
                self.assertFalse(failed.artifacts)

    def test_reference_infrastructure_failure_keeps_source_prefix(self):
        with patch(
            "yamaa.adapters.conformance._published",
            side_effect=RuntimeError("publication failed"),
        ):
            report = self.run_example("schema-lookup", "python")
        self.assertEqual(report.outcome, "error")
        self.assertEqual(report.callbacks, ())
        self.assertTrue(report.source_reads)

    def test_capture_failure_is_visible_without_rereading(self):
        with patch(
            "yamaa.io.project.ProjectResources.capture",
            side_effect=ResourceFailure("resource_path_missing", "input/lb.csv"),
        ) as capture:
            report = self.run_example("adam-adlb-ordered-sum")
        self.assertEqual(report.outcome, "failure", report.error)
        self.assertEqual(len(report.source_reads), 1)
        self.assertEqual(report.source_reads[0].outcome, "failure")
        self.assertEqual(report.source_reads[0].snapshots_created, 0)
        self.assertEqual(capture.call_count, 1)

    def test_old_report_cannot_imply_empty_new_source_observations(self):
        payload = self.run_example("schema-lookup").model_dump(mode="json")
        payload.pop("source_reads")
        with self.assertRaises(ValidationError):
            ExampleReport.model_validate_json(json.dumps(payload))

    def test_unknown_backend_is_rejected_before_entry_reads(self):
        with (
            patch(
                "yamaa.adapters.conformance.entry_specification",
                side_effect=AssertionError("entry read"),
            ) as read,
            self.assertRaisesRegex(ValueError, "backend must be"),
        ):
            self.run_example("schema-functions", "other")
        read.assert_not_called()


if __name__ == "__main__":
    unittest.main()
