"""Independent admission controls for a source-built legacy baseline."""

import copy
import importlib.util
import json
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock

TOOLS = Path(__file__).resolve().parents[1] / "tools"
with mock.patch.object(sys, "path", [str(TOOLS), *sys.path]):
    SPEC = importlib.util.spec_from_file_location("public_performance", TOOLS / "collect_public_performance.py")
    collector = importlib.util.module_from_spec(SPEC)
    SPEC.loader.exec_module(collector)


class ReferenceAdmission(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        root = Path(temporary.name)
        manifest = collector.workloads.prepare(root / "authored", (20,), ("aggregate",))
        self.case = manifest["cases"][0]
        self.authored = root / "authored" / self.case["name"]
        self.child = root / "child"
        self.child.mkdir()
        expected = collector.lifecycle.read_json(self.authored / self.case["expected_output"])
        self.views = {"output": expected, "output_after_save_2": expected,
                      "issues": {"row_count": 0, "rows": []},
                      "verification_log": None, "warning_log": None}
        collector.lifecycle.write_json(self.child / "views.json", self.views)
        declaration = {"index": 0, "warmup": False}
        duration = {"start_ns": 100, "end_ns": 110, "duration_ns": 10}
        self.actual = {"declaration": declaration, "status": "success",
            "evidence_capture_failures": [], "metadata": {}, "views": "views.json",
            "durations": {boundary: duration for boundary in collector.lifecycle.BOUNDARIES[1:]},
            "build_artifact_presence": {"out.csv": False},
            "saved_artifacts": {stage: {"out.csv": stage + ".csv"} for stage in ("save_1", "save_2")}}
        for stage in ("save_1", "save_2"):
            (self.child / (stage + ".csv")).write_bytes((self.authored / "expected/out.csv").read_bytes())
        self.response = {"failure": None, "import_duration": duration, "repetitions": [self.actual]}
        self.request = {"host": "python", "repetitions": [declaration]}

    def validate(self):
        with mock.patch.object(collector.lifecycle, "actual_origins"):
            collector.validate_reference(self.case, self.authored, self.child,
                                         self.request, self.response, {}, Path("/unused"))

    def test_complete_independent_truth_admits_and_failure_never_admits(self):
        self.validate()
        self.actual["status"] = "failure"
        self.actual["failure"] = {"class": "TypeError", "message": "unsupported legacy environment"}
        with self.assertRaisesRegex(ValueError, "reference repetition failed"):
            self.validate()

    def test_wrong_float_bits_and_saved_bytes_refuse(self):
        changed = copy.deepcopy(self.views)
        changed["output"]["rows"][0][1] = {"type": "float", "value": "3ff0000000000000"}
        collector.lifecycle.write_json(self.child / "views.json", changed)
        with self.assertRaisesRegex(ValueError, "independent truth"):
            self.validate()
        collector.lifecycle.write_json(self.child / "views.json", self.views)
        (self.child / "save_2.csv").write_bytes(b"wrong\n")
        with self.assertRaisesRegex(ValueError, "independent bytes"):
            self.validate()

    def test_missing_clock_and_premature_publication_refuse(self):
        before = self.actual["durations"].pop("public_build")
        with self.assertRaises(KeyError):
            self.validate()
        self.actual["durations"]["public_build"] = before
        self.actual["build_artifact_presence"]["out.csv"] = True
        with self.assertRaisesRegex(ValueError, "published before"):
            self.validate()


class RootBuildAdmission(ReferenceAdmission):
    def setUp(self):
        super().setUp()
        root = self.authored.parent.parent
        manifest = collector.workloads.prepare(root / "producer", (20,), ("producer",))
        self.case = manifest["cases"][0]
        self.authored = root / "producer" / self.case["name"]
        expected = collector.lifecycle.read_json(self.authored / self.case["expected_output"])
        self.views = {"output": expected, "output_after_build_repeat": expected,
                      "issues": {"row_count": 0, "rows": []},
                      "verification_log": None, "warning_log": None}
        collector.lifecycle.write_json(self.child / "views.json", self.views)
        for boundary in ("held_save_1", "held_save_2"):
            self.actual["durations"].pop(boundary)
        self.actual["save_returns"] = []
        self.actual["saved_files"] = {}
        self.actual["saved_artifacts"] = {}
        absent = {artifact["path"]: False for artifact in self.case["expected_artifacts"]}
        self.actual["build_artifact_presence"] = dict(absent)
        self.actual["root_build_artifact_presence"] = dict(absent)
        source = self.child / "_reference_domain.py"
        source.write_bytes(b"pinned original source")
        self.actual["build_invocation"] = {
            "callable": "yamaa._reference_domain.yamaa_domain", "signature": "retained actual signature",
            "parameters": ["entry_path", "schema_root"], "source_file": str(source),
            "schema_root": str(root / "retained-schema"), "environment_passed": False}
        self.request.update(reference_root_build=True, reference_schema=str(root / "retained-schema"))

    def validate(self):
        record = {"installed_origins": {"package": str(self.child)}, "source_revision": "previous-release"}
        with mock.patch.object(collector.lifecycle, "actual_origins"), \
             mock.patch.object(collector.lifecycle, "git_bytes", return_value=b"pinned original source"):
            collector.validate_reference(self.case, self.authored, self.child,
                                         self.request, self.response, record, Path("/unused"))

    def test_complete_independent_truth_admits_and_failure_never_admits(self):
        self.validate()
        self.actual["save_returns"] = [True]
        with self.assertRaisesRegex(ValueError, "entered or timed a Save"):
            self.validate()

    def test_wrong_float_bits_and_saved_bytes_refuse(self):
        changed = copy.deepcopy(self.views)
        changed["output"]["rows"][0][1] = {"type": "float", "value": "8000000000000000"}
        collector.lifecycle.write_json(self.child / "views.json", changed)
        with self.assertRaisesRegex(ValueError, "independent truth"):
            self.validate()

    def test_active_environment_or_different_callable_source_refuses(self):
        path = self.authored / self.case["environment"]["python"]
        value = collector.lifecycle.read_json(path)
        value["functions"] = {"active": {"function": "host.callable"}}
        collector.lifecycle.write_json(path, value)
        with self.assertRaisesRegex(ValueError, "active or unreviewed"):
            self.validate()
        value["functions"] = {}
        collector.lifecycle.write_json(path, value)
        Path(self.actual["build_invocation"]["source_file"]).write_bytes(b"different source")
        with self.assertRaisesRegex(ValueError, "pinned previous release"):
            self.validate()

    def test_timed_save_or_later_publication_refuses(self):
        self.actual["durations"]["held_save_1"] = {"start_ns": 1, "end_ns": 2, "duration_ns": 1}
        with self.assertRaisesRegex(ValueError, "entered or timed a Save"):
            self.validate()
        self.actual["durations"].pop("held_save_1")
        self.actual["root_build_artifact_presence"]["out.csv"] = True
        with self.assertRaisesRegex(ValueError, "published an artifact"):
            self.validate()
