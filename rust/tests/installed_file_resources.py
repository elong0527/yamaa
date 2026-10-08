"""Installed descriptor-backed source capture without reference semantic imports."""

import builtins
import csv
import gc
import json
import os
import shutil
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

import yamaa_native

ROOT = Path(__file__).with_name("specification-original")


class FileResources(unittest.TestCase):
    def setUp(self):
        original = builtins.__import__

        def reject(name, *args, **kwargs):
            if name.startswith((
                "yamaa.domain", "yamaa.runtime", "yamaa.planning", "yamaa.expressions",
                "yamaa.verification", "yamaa.submission", "yamaa.specification",
                "yamaa.io.artifact", "yamaa.io.source", "yamaa.io.csv", "yamaa.io.parquet",
            )) or name.split(".")[0] in {"yaml", "yaml12", "pyarrow", "polars"}:
                raise AssertionError("reference semantic import: " + name)
            return original(name, *args, **kwargs)

        guard = patch("builtins.__import__", side_effect=reject)
        guard.start()
        self.addCleanup(guard.stop)
        from yamaa.io._native_source_capture import SourceCapture
        from yamaa.io.project import ProjectResources, ResourceByteLimit, ResourceFailure
        self.capture_type = SourceCapture
        self.resources_type = ProjectResources
        self.byte_limit = ResourceByteLimit
        self.resource_failure = ResourceFailure

    def prepare(self):
        def no_parent(*_):
            self.fail("standalone entry reached inheritance authority")
        source = (ROOT / "cases/schema-lookup/spec.yaml").read_bytes()
        return yamaa_native._prepare_document("spec.yaml", source, no_parent, no_parent, no_parent)

    def test_real_files_preserve_complete_classified_capture_reports(self):
        with (ROOT / "source-capture.tsv").open(encoding="ascii") as stream:
            records = list(csv.DictReader(stream, delimiter="\t"))
        self.assertEqual(len(records), 8)
        for record in records:
            with self.subTest(kind=record["kind"], at=record["fail_at"], cached=record["cached"]):
                with tempfile.TemporaryDirectory() as directory:
                    root = Path(directory).resolve()
                    shutil.copytree(ROOT / "cases/schema-lookup/input", root / "input")
                    fail_at, cached = int(record["fail_at"]), bool(int(record["cached"]))
                    failed = root / ("input/dm.csv" if fail_at == 0 else "input/ae.csv")
                    failed.unlink()
                    if record["kind"] == "not_regular_file":
                        failed.mkdir()
                    resources = self.resources_type(root)
                    if fail_at and cached:
                        resources.capture("input/dm.csv")
                    before = resources.capture_reads
                    capture = self.capture_type(resources)
                    spec = self.prepare()
                    result = spec.build(capture, ("fixture-runtime", "fixture-engine", "schema-lookup", "spec.yaml", "."))
                    expected = json.loads(record["expected"])
                    self.assertEqual(json.loads(result.observations()), expected)
                    self.assertEqual(resources.capture_reads - before, int(fail_at > 0 and not cached))
                    self.assertIsNone(result.output())
                    del capture, resources, spec
                    gc.collect()
                    with self.assertRaisesRegex(ValueError, "cannot save a failed build"):
                        result.save(lambda *_: self.fail("failed file capture published"))
                    self.assertEqual(json.loads(result.observations()), expected)

    def test_real_file_inspection_fails_before_every_study_capture(self):
        with (ROOT / "source-inspection.tsv").open(encoding="ascii") as stream:
            records = list(csv.DictReader(stream, delimiter="\t"))
        self.assertEqual(len(records), 7)
        paths = {"DM": "input/dm.csv", "AE": "input/ae.csv", "MEDDRA": "input/meddict.csv"}
        for record in records:
            for cached in (False, True):
                with self.subTest(case=record["case"], cached=cached):
                    with tempfile.TemporaryDirectory() as directory:
                        root = Path(directory).resolve()
                        shutil.copytree(ROOT / "cases/schema-lookup/input", root / "input")
                        resources = self.resources_type(root)
                        if cached:
                            resources.capture("input/dm.csv")
                        before = resources.capture_reads
                        for name, kind in json.loads(record["failures"]).items():
                            location = root / paths[name]
                            location.unlink()
                            if kind == "not_regular_file":
                                location.mkdir()
                        capture = self.capture_type(resources)
                        events = []
                        def inspect(name, path):
                            events.append((name, path))
                            return capture.inspect(name, path)
                        spec = self.prepare()
                        with patch("yamaa.io.project.os.read", side_effect=AssertionError("metadata inspection read study bytes")):
                            result = spec.build(capture, ("fixture-runtime", "fixture-engine", "schema-lookup", "spec.yaml", "."), inspect=inspect)
                        self.assertEqual(events, list(paths.items()))
                        self.assertEqual(resources.capture_reads, before)
                        expected = json.loads(record["expected"])
                        self.assertEqual(json.loads(result.observations()), expected)
                        self.assertIsNone(result.output())
                        with self.assertRaisesRegex(ValueError, "cannot save a failed build"):
                            result.save(lambda *_: self.fail("metadata failure published"))

    def test_real_file_inspection_repeats_before_successful_cached_capture(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory).resolve()
            shutil.copytree(ROOT / "cases/schema-lookup/input", root / "input")
            resources = self.resources_type(root)
            capture = self.capture_type(resources)
            spec = self.prepare()
            expected = json.loads((ROOT / "expected/schema-lookup.json").read_text())
            expected["artifacts"] = []
            events = []
            def inspect(name, path):
                events.append(("inspect", name))
                return capture.inspect(name, path)
            def read(name, path, maximum):
                events.append(("capture", name))
                return capture(name, path, maximum)
            for created in (1, 0):
                events.clear()
                result = spec.build(read, ("fixture-runtime", "fixture-engine", "schema-lookup", "spec.yaml", "."), inspect=inspect)
                self.assertEqual(events, [(kind, name) for kind in ("inspect", "capture") for name in ("DM", "AE", "MEDDRA")])
                for observation in expected["source_reads"]:
                    observation["snapshots_created"] = created
                expected.update(runtime_version="fixture-runtime", engine_version="fixture-engine")
                self.assertEqual(json.loads(result.observations()), expected)
            self.assertEqual(resources.capture_reads, 3)

    def test_byte_limit_and_changed_cached_bytes_stay_opaque(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory).resolve()
            source = root / "source.csv"
            source.write_bytes(b"held")
            resources = self.resources_type(root)
            capture = self.capture_type(resources)
            with self.assertRaises(self.byte_limit):
                capture("SRC", "source.csv", 3)
            self.assertEqual(resources.capture_reads, 0)
            self.assertEqual(capture("SRC", "source.csv", 4), (b"held", True))
            self.assertEqual(capture("SRC", "source.csv", 4), (b"held", False))
            source.write_bytes(b"changed")
            with self.assertRaises(self.resource_failure) as caught:
                capture("SRC", "source.csv", 4)
            self.assertEqual(caught.exception.condition, "resource_path_content_changed")
            self.assertEqual(caught.exception.phase, "ingest")
            self.assertEqual(resources.capture_reads, 1)

    def test_lookup_file_capture_keeps_complete_result_and_exact_saved_bytes(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory).resolve()
            shutil.copytree(ROOT / "cases/schema-lookup/input", root / "input")
            resources = self.resources_type(root)
            capture = self.capture_type(resources)
            spec = self.prepare()
            result = spec.build(capture, ("fixture-runtime", "fixture-engine", "schema-lookup", "spec.yaml", "."))
            expected = json.loads((ROOT / "expected/schema-lookup.json").read_text())
            self.assertEqual(json.loads(result.observations()), dict(expected, artifacts=[]))
            self.assertEqual(resources.capture_reads, 3)
            for path in (root / "input").iterdir():
                path.unlink()
            del resources, capture, spec
            gc.collect()
            publications = []
            def publish(path, content):
                self.assertEqual(path, "adsl.csv")
                self.assertEqual(content, (ROOT / "cases/schema-lookup/expected/adsl.csv").read_bytes())
                pending = root / "pending"
                pending.write_bytes(content)
                os.replace(pending, root / path)
                publications.append((root / path).read_bytes())
            for _ in range(2):
                self.assertEqual(json.loads(result.save(publish)), expected)
            self.assertEqual(len(publications), 2)
            self.assertEqual(publications[0], publications[1])
            self.assertEqual(json.loads(result.observations()), dict(expected, artifacts=[]))

    def test_existing_containment_and_symlink_failures_stay_opaque(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory).resolve()
            (root / "source.csv").write_bytes(b"held")
            capture = self.capture_type(self.resources_type(root))
            with self.assertRaises(self.resource_failure) as caught:
                capture("SRC", "../outside.csv", 4)
            self.assertEqual(caught.exception.condition, "resource_path_outside_project")
            try:
                (root / "alias.csv").symlink_to("source.csv")
            except OSError:
                return  # Windows may lack symlink creation permission.
            with self.assertRaises(self.resource_failure) as caught:
                capture("SRC", "alias.csv", 4)
            self.assertEqual(caught.exception.condition, "resource_path_symlink")


if __name__ == "__main__":
    unittest.main()
