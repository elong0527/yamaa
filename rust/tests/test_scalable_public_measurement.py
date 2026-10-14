"""Scalable collector refusal/replay controls; synthetic clocks are not samples."""

import copy
import gzip
import importlib.util
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock

TOOLS = Path(__file__).resolve().parents[1] / "tools"
with mock.patch.object(sys, "path", [str(TOOLS), *sys.path]):
    SPEC = importlib.util.spec_from_file_location(
        "scalable_measurement", TOOLS / "measure_scalable_public.py"
    )
    measure = importlib.util.module_from_spec(SPEC)
    SPEC.loader.exec_module(measure)


class RetainedReportEquality(unittest.TestCase):
    def test_original_multichunk_utf8_plain_and_gzip_bytes_match(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            original = ('{"rows":["' + "\u00e9" * 1_100_000 + '"]}').encode()
            plain = root / "ordinary.json"
            compressed = root / "held.json.gz"
            plain.write_bytes(original)
            compressed.write_bytes(gzip.compress(original, compresslevel=1))
            self.assertTrue(measure.retained_equal(plain, compressed))
            # A mutation after the first retained buffer must also be rejected.
            changed = bytearray(original)
            changed[1_048_579] = ord("a")
            compressed.write_bytes(gzip.compress(changed, compresslevel=9))
            self.assertFalse(measure.retained_equal(plain, compressed))
            compressed.write_bytes(gzip.compress(original + b" "))
            self.assertFalse(measure.retained_equal(plain, compressed))


class Replay(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.source = self.root / "source"
        self.manifest = measure.workloads.prepare(self.source, (20,), ("producer",))
        self.output = self.root / "evidence"
        self.output.mkdir()

    def test_replay_rejects_altered_expected_bytes_without_candidate_execution(self):
        case = self.source / "producer-20-1"
        (case / "expected/out.csv").write_bytes(b"ID,VALUE\nwrong,0\n")
        with self.assertRaisesRegex(ValueError, "bytes differ"):
            measure.verify_workload(self.source, case.name, self.output)

    def test_replay_rejects_changed_generator_even_when_case_bytes_match(self):
        (self.source / "independent-generator.py").write_bytes(b"replacement")
        with self.assertRaisesRegex(ValueError, "generator bytes differ"):
            measure.verify_workload(self.source, "producer-20-1", self.output)

    def test_complete_primary_and_both_saves_require_every_producer_artifact(self):
        case, authored = measure.verify_workload(
            self.source, "producer-20-1", self.output
        )
        child = self.root / "child"
        child.mkdir()
        expected = measure.lifecycle.read_json(authored / case["expected_output"])
        views = {
            "output": expected,
            "output_after_save_2": expected,
            "issues": {
                "columns": [
                    ["phase", "str"],
                    ["condition", "str"],
                    ["requirement", "str"],
                    ["spec_paths", "list[str]"],
                    ["context", "str"],
                ],
                "row_count": 0,
                "rows": [],
            },
            "verification_log": None,
            "warning_log": None,
        }
        measure.lifecycle.write_json(child / "views.json", views)
        before = {
            "outcome": "success",
            "diagnostics": [],
            "error": None,
            "unsupported": [],
            "artifacts": [],
            "nodes": [
                {
                    "specification": name,
                    "outcome": "success",
                    "diagnostics": [],
                    "unsupported": [],
                }
                for name in case["expected_nodes"]
            ],
            "source_reads": [
                {
                    "path": "seed.csv",
                    "outcome": "captured",
                    "condition": None,
                    "snapshots_created": 1,
                }
            ],
        }
        saved = dict(
            before,
            artifacts=[
                {
                    "name": Path(artifact["path"]).stem,
                    "profile": "csv",
                    "columns": ["ID", "VALUE"],
                    "row_count": 20,
                    "content": (authored / artifact["expected"]).read_text(),
                    "byte_length": (authored / artifact["expected"]).stat().st_size,
                }
                for artifact in case["expected_artifacts"]
            ],
            publication={"status": "success"},
        )
        reports = {}
        artifacts = {}
        for stage, report in (("build", before), ("save_1", saved), ("save_2", saved)):
            reports[stage] = stage + ".json"
            measure.lifecycle.write_json(child / reports[stage], report)
            if stage == "build":
                continue
            artifacts[stage] = {}
            for artifact in case["expected_artifacts"]:
                name = stage + "-" + artifact["path"]
                (child / name).write_bytes(
                    (authored / artifact["expected"]).read_bytes()
                )
                artifacts[stage][artifact["path"]] = name
        declaration = {"index": 0, "warmup": False}
        duration = {"start_ns": 1, "end_ns": 2, "duration_ns": 1}
        row = {
            "declaration": declaration,
            "status": "success",
            "evidence_capture_failures": [],
            "metadata": {},
            "native_call_order": ["public_build", "metadata_after_build"],
            "unavailable": measure.lifecycle.UNAVAILABLE,
            "rss": None,
            "durations": {key: duration for key in measure.lifecycle.BOUNDARIES[1:]},
            "views": "views.json",
            "save_returns": [True, True],
            "reports": reports,
            "saved_artifacts": artifacts,
            "build_artifact_presence": {
                a["path"]: False for a in case["expected_artifacts"]
            },
        }
        response = {"failure": None, "import_duration": duration, "repetitions": [row]}
        request = {"host": "python", "repetitions": [declaration]}
        with (
            mock.patch.object(measure.lifecycle, "actual_origins"),
            mock.patch.object(measure.lifecycle, "validate_rss"),
        ):
            measure.validate(
                case,
                authored,
                child,
                request,
                response,
                {"system": "synthetic"},
                Path("/unused"),
            )
            original = copy.deepcopy(response)
            for report in (
                dict(saved, artifacts=saved["artifacts"][:1]),
                dict(saved, nodes=before["nodes"][:1]),
                dict(saved, source_reads=[]),
            ):
                measure.lifecycle.write_json(child / "save_1.json", report)
                measure.lifecycle.write_json(child / "save_2.json", report)
                with self.assertRaises(ValueError):
                    measure.validate(
                        case,
                        authored,
                        child,
                        request,
                        response,
                        {"system": "synthetic"},
                        Path("/unused"),
                    )
            measure.lifecycle.write_json(child / "save_1.json", saved)
            measure.lifecycle.write_json(child / "save_2.json", saved)
            row["saved_artifacts"]["save_2"].pop("producer-0.csv")
            with self.assertRaises(KeyError):
                measure.validate(
                    case,
                    authored,
                    child,
                    request,
                    response,
                    {"system": "synthetic"},
                    Path("/unused"),
                )
            measure.lifecycle.write_json(
                child / "views.json", dict(views, output=dict(expected, row_count=19))
            )
            with self.assertRaisesRegex(ValueError, "copied output"):
                measure.validate(
                    case,
                    authored,
                    child,
                    request,
                    original,
                    {"system": "synthetic"},
                    Path("/unused"),
                )

    def test_failed_warmup_or_missing_clock_rejects_instead_of_becoming_a_sample(self):
        case, authored = measure.verify_workload(
            self.source, "producer-20-1", self.output
        )
        for response in (
            {"failure": {"message": "capacity refusal"}},
            {"failure": None, "import_duration": None},
        ):
            with self.assertRaises(ValueError):
                measure.validate(
                    case,
                    authored,
                    self.output,
                    {"host": "python"},
                    response,
                    {},
                    Path("/unused"),
                )


if __name__ == "__main__":
    unittest.main()
