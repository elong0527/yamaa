"""Read-only transport admits authentic original records and preserves failures."""

import copy
import json
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

import export_performance_records as exporter


class ExistingPerformanceRecords(unittest.TestCase):
    def setUp(self):
        self.run = {
            "id": 123,
            "run_attempt": 1,
            "head_sha": exporter.SOURCE,
            "path": ".github/workflows/performance.yml",
            "repository": {"full_name": "owner/repo"},
            "head_repository": {"full_name": "owner/repo"},
            "status": "completed",
            "conclusion": "success",
        }

    def test_original_run_source_attempt_repository_and_workflow_are_bound(self):
        args = ("owner/repo", 123, 1, exporter.SOURCE, self.run["path"])
        exporter.validate_identity(self.run, *args)
        for key, value in (
            ("id", 124),
            ("run_attempt", 2),
            ("head_sha", "a" * 40),
            ("path", "other.yml"),
            ("repository", {"full_name": "other/repo"}),
            ("head_repository", {"full_name": "other/repo"}),
        ):
            with self.subTest(key=key), self.assertRaises(ValueError):
                exporter.validate_identity({**self.run, key: value}, *args)

    def test_terminal_failure_is_retained_without_a_success_rewrite(self):
        failed = {**self.run, "conclusion": "failure"}
        running = {**self.run, "status": "in_progress", "conclusion": None}
        with tempfile.TemporaryDirectory() as temporary:
            records = Path(temporary)
            with (
                patch.object(exporter, "api", side_effect=[running, failed]),
                patch.object(exporter.time, "sleep") as wait,
            ):
                result = exporter.terminal_snapshot(
                    "owner/repo", 123, 1, exporter.SOURCE, records, 1
                )
            self.assertEqual(result, failed)
            self.assertEqual(json.loads((records / "run.json").read_text()), failed)
            wait.assert_called_once_with(60)

    def test_wait_expiration_retains_the_actual_in_progress_snapshot(self):
        running = {**self.run, "status": "in_progress", "conclusion": None}
        with tempfile.TemporaryDirectory() as temporary:
            records = Path(temporary)
            with (
                patch.object(exporter, "api", return_value=running),
                patch.object(exporter.time, "monotonic", side_effect=[0, 61]),
                self.assertRaises(ValueError),
            ):
                exporter.terminal_snapshot(
                    "owner/repo", 123, 1, exporter.SOURCE, records, 1
                )
            self.assertEqual(json.loads((records / "run.json").read_text()), running)

    def test_supported_archive_binding_preserves_zip_bytes_and_original_fields(self):
        original = {
            "entries": [
                {
                    "artifact_id": 7,
                    "artifact_name": "owner-original",
                    "run_attempt": 1,
                    "tuple": {"host": "python"},
                }
            ]
        }
        artifacts = {
            "artifacts": [{"id": 7, "name": "owner-original", "size_in_bytes": 8}]
        }
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "raw").mkdir()
            archive = root / "raw/7.zip"
            archive.write_bytes(b"ZIP\x00\xff\n\r!")
            result = exporter.bind_archives(copy.deepcopy(original), root, artifacts)
            self.assertEqual(archive.read_bytes(), b"ZIP\x00\xff\n\r!")
            entry = result["entries"][0]
            self.assertEqual(entry.pop("archive"), "raw/7.zip")
            self.assertEqual(result, original)
            archive.write_bytes(b"truncated")
            with self.assertRaises(ValueError):
                exporter.bind_archives(copy.deepcopy(original), root, artifacts)
            archive.write_bytes(b"ZIP\x00\xff\n\r!")
            artifacts["artifacts"][0]["name"] = "other-owner"
            with self.assertRaises(ValueError):
                exporter.bind_archives(copy.deepcopy(original), root, artifacts)


if __name__ == "__main__":
    unittest.main()
