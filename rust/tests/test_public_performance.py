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
