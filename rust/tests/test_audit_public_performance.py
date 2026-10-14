"""Adversarial offline replay: original raw evidence, never collector summaries."""

import contextlib
import copy
import importlib.util
import json
import subprocess
import sys
import tempfile
import unittest
import zipfile
from pathlib import Path, PurePosixPath
from unittest import mock

TOOLS = Path(__file__).resolve().parents[1] / "tools"
with mock.patch.object(sys, "path", [str(TOOLS), *sys.path]):
    SPEC = importlib.util.spec_from_file_location(
        "performance_auditor", TOOLS / "audit_public_performance.py"
    )
    auditor = importlib.util.module_from_spec(SPEC)
    SPEC.loader.exec_module(auditor)


def write(path, value):
    path.write_text(json.dumps(value), encoding="utf-8")


def native_fixture(root, payload, branch="main"):
    run = {
        "id": 321,
        "run_attempt": 2,
        "head_sha": payload["source_revision"],
        "head_branch": branch,
        "repository": {"full_name": payload["repository"]},
        "head_repository": {"full_name": payload["repository"]},
        "path": ".github/workflows/native.yml",
        "event": "push",
        "status": "completed",
        "conclusion": "success",
    }
    jobs = [
        {
            "name": name,
            "status": "completed",
            "conclusion": "success",
            "run_id": 321,
            "run_attempt": 2,
            "head_sha": payload["source_revision"],
            "started_at": "2026-10-13T01:00:00Z",
            "completed_at": "2026-10-13T01:20:00Z",
        }
        for name in sorted(auditor.native_admission.required_jobs())
    ]
    artifacts = [
        {
            "id": number + 1000,
            "name": name,
            "expired": False,
            "size_in_bytes": 100,
            "workflow_run": {"id": 321, "head_sha": payload["source_revision"]},
            "created_at": "2026-10-13T01:10:00Z",
        }
        for number, name in enumerate(
            sorted(auditor.native_admission.required_artifacts())
        )
    ]
    payload.update(
        native_run="native-run.json",
        native_jobs="native-jobs.json",
        native_artifacts="native-artifacts.json",
    )
    for key, value in (
        ("native_run", run),
        ("native_jobs", jobs),
        ("native_artifacts", artifacts),
    ):
        write(root / payload[key], value)
    return run, jobs, artifacts


class Replay(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.directory = self.root / "candidate-evidence"
        self.directory.mkdir()
        manifest = auditor.workloads.prepare(self.root / "authored", (10,), ("lookup",))
        self.case = manifest["cases"][0]
        self.authored = self.root / "authored" / self.case["name"]
        self.record = {
            "host": "python",
            "system": "Linux",
            "runtime_version": "3.12.0",
            "artifact_path": "/vanished/yamaa.whl",
            "package_version": "0.4.0",
            "engine_version": "0.4.0",
            "installed_origins": {
                "package": "/vanished/site/yamaa",
                "native": "/vanished/site/yamaa/_native.so",
            },
        }
        self.plan = {
            "host": "python",
            "provenance": self.record,
            "evaluator": "public",
            "phase_measurements_required": False,
            "measurement_campaign": {"collection": "/vanished/runner/collection.json"},
        }
        self.row = {
            "case": self.case["name"],
            "accepted": False,
            "truth_accepted": False,
            "distribution": [{"median_ns": 999999999}],
            "children": [],
        }
        duration = {"start_ns": 100, "end_ns": 110, "duration_ns": 10}
        expected = auditor.read(self.authored / self.case["expected_output"])
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
        before = {
            "outcome": "success",
            "diagnostics": [],
            "error": None,
            "unsupported": [],
            "nodes": [
                {
                    "specification": "spec.yaml",
                    "outcome": "success",
                    "diagnostics": [],
                    "unsupported": [],
                }
            ],
            "source_reads": [
                {
                    "path": source["path"],
                    "outcome": "captured",
                    "condition": None,
                    "snapshots_created": 1,
                }
                for source in self.case["source_files"]
            ],
        }
        content = (self.authored / "expected/out.csv").read_bytes()
        saved = dict(
            before,
            publication={"status": "success"},
            artifacts=[
                {
                    "name": "out",
                    "content": content.decode(),
                    "byte_length": len(content),
                    "row_count": 10,
                    "columns": ["ID", "TEXT"],
                    "profile": "csv",
                }
            ],
        )
        for mode, number, declarations in auditor.schedule(self.case, "candidate"):
            name = self.case["name"] + f"-{mode}-{number}"
            child = self.directory / name
            child.mkdir()
            self.row["children"].append(
                {
                    "child": name,
                    "mode": mode,
                    "failure": None,
                    "accepted": False,
                    "peak_rss": None,
                    "counts_toward_timing": False,
                }
            )
            staged, repetitions = [], []
            for declaration in declarations:
                original = (
                    PurePosixPath("/vanished/runner")
                    / name
                    / f"work-{declaration['index']}"
                )
                local = child / f"work-{declaration['index']}"
                local.mkdir()
                for member in self.case["files"]:
                    if not member["path"].startswith("expected/"):
                        (local / member["path"]).write_bytes(
                            (self.authored / member["path"]).read_bytes()
                        )
                staged.append(
                    dict(
                        declaration,
                        work_directory=str(original),
                        entry=str(original / "spec.yaml"),
                        environment=None,
                        output_name="out.csv",
                        artifact_names=["out.csv"],
                    )
                )
                prefix = f"rep-{declaration['index']}"
                write(child / (prefix + "-views.json"), views)
                write(child / (prefix + "-build.json"), before)
                for stage in ("save_1", "save_2"):
                    write(child / (prefix + "-" + stage + ".json"), saved)
                    (child / (prefix + "-" + stage + ".csv")).write_bytes(content)
                metadata = dict(
                    self.record,
                    report_runtime_version="3.12.0 complete version",
                    clock={
                        "provider": "time.perf_counter_ns",
                        "unit": "ns",
                        "monotonic": True,
                        "adjustable": False,
                    },
                )
                repetitions.append(
                    {
                        "declaration": staged[-1],
                        "status": "success",
                        "failure": None,
                        "host_view_access_counts": {
                            "output": 2,
                            "issues": 1,
                            "verification_log": 1,
                            "warning_log": 1,
                        },
                        "evidence_capture_failures": [],
                        "metadata": metadata,
                        "native_call_order": ["public_build", "metadata_after_build"],
                        "unavailable": auditor.lifecycle.unavailable(False),
                        "rss": {
                            "provider": "resource.getrusage(RUSAGE_SELF).ru_maxrss",
                            "raw": 100,
                            "bytes": 102400,
                            "original_unit": "KiB",
                            "scope": "whole-child-lifetime; cumulative across warm repetitions",
                        },
                        "durations": {
                            key: duration for key in auditor.lifecycle.BOUNDARIES[1:]
                        },
                        "views": prefix + "-views.json",
                        "save_returns": [True, True],
                        "build_artifact_presence": {"out.csv": False},
                        "reports": {
                            "build": prefix + "-build.json",
                            "save_1": prefix + "-save_1.json",
                            "save_2": prefix + "-save_2.json",
                        },
                        "saved_artifacts": {
                            stage: {"out.csv": prefix + "-" + stage + ".csv"}
                            for stage in ("save_1", "save_2")
                        },
                    }
                )
            request = {
                "protocol": auditor.lifecycle.PROTOCOL,
                "child": name,
                "host": "python",
                "provenance": self.record,
                "output": str(PurePosixPath("/vanished/runner") / name),
                "evaluator": "public",
                "phase_measurements": False,
                "observe_callbacks": False,
                "reference_root_build": False,
                "reference_schema": None,
                "repetitions": staged,
            }
            write(child / "request.json", request)
            write(
                child / "response.json",
                {
                    "protocol": auditor.lifecycle.PROTOCOL,
                    "child": name,
                    "pid": 123,
                    "failure": None,
                    "import_duration": duration,
                    "repetitions": repetitions,
                },
            )
            write(
                child / "process.json",
                {
                    "pid": 123,
                    "exit_code": 0,
                    "launcher": "fresh small broker; controller fork RSS excluded",
                    "peak_rss": {
                        "provider": "os.wait4(child_pid).ru_maxrss",
                        "raw": 100,
                        "bytes": 102400,
                        "original_unit": "KiB",
                        "scope": auditor.child_peak_rss.SCOPE,
                        "system": "Linux",
                    },
                },
            )
            write(
                child / "compilation-probe.json",
                {
                    "provider": "process-name inventory",
                    "compilers": [],
                    "clear": True,
                    "atomic_lock": False,
                },
            )
            (child / "stdout.txt").touch()
            (child / "stderr.txt").touch()
        self.cold = self.directory / (self.case["name"] + "-cold-0")
        self.warm = self.directory / (self.case["name"] + "-warm-0")

    def replay(self):
        return auditor.admit_case(
            self.directory,
            self.plan,
            self.row,
            self.case,
            self.authored,
            self.record,
            "candidate",
            self.root,
        )

    def change(self, child, filename, mutate):
        value = auditor.read(child / filename)
        mutate(value)
        write(child / filename, value)

    def test_recomputes_original_distributions_after_runner_paths_disappear(self):
        result = self.replay()
        self.assertEqual(result["status"], "pass")
        self.assertEqual(
            result["metrics"]["cold.public_build.median_ns"]["raw"], [10] * 10
        )
        self.assertEqual(
            result["metrics"]["warm.public_build.median_ns"]["raw"], [10] * 30
        )
        self.assertEqual(
            result["metrics"]["warm.child_peak_rss.median_bytes"]["count"], 1
        )
        self.assertEqual(result["unavailable"], list(auditor.lifecycle.UNAVAILABLE))

    def test_missing_duplicate_unexpected_children_refuse(self):
        original = copy.deepcopy(self.row["children"])
        for changed in (
            original[:-1],
            original + [original[-1]],
            [original[0], original[0], *original[2:]],
        ):
            self.row["children"] = changed
            with self.assertRaises(ValueError):
                self.replay()

    def test_only_exact_three_retained_warmups_admit(self):
        self.change(
            self.warm,
            "request.json",
            lambda value: value["repetitions"][2].update(warmup=False),
        )
        with self.assertRaisesRegex(ValueError, "warmup/repetition schedule"):
            self.replay()

    def test_failed_attempt_pid_mismatch_and_source_mismatch_refuse(self):
        original = auditor.read(self.cold / "process.json")
        for field, value in (("exit_code", 1), ("pid", 124)):
            write(self.cold / "process.json", dict(original, **{field: value}))
            with self.assertRaisesRegex(ValueError, "failed child/PID"):
                self.replay()
        write(self.cold / "process.json", original)
        self.change(
            self.cold, "request.json", lambda value: value.update(provenance={})
        )
        with self.assertRaisesRegex(ValueError, "source/installation"):
            self.replay()

    def test_collector_cannot_hide_original_failures(self):
        self.row["children"][1]["failure"] = {"message": "failed originally"}
        with self.assertRaisesRegex(ValueError, "original failed attempt"):
            self.replay()

    def test_raw_failure_wrong_getter_counts_and_campaign_child_cannot_be_hidden(self):
        original = auditor.read(self.cold / "response.json")
        changed = copy.deepcopy(original)
        changed["repetitions"][0]["failure"] = {
            "class": "DomainError",
            "message": "original failed attempt",
        }
        write(self.cold / "response.json", changed)
        with self.assertRaisesRegex(ValueError, "original raw repetition failed"):
            self.replay()
        changed = copy.deepcopy(original)
        changed["repetitions"][0]["host_view_access_counts"]["output"] = 1
        write(self.cold / "response.json", changed)
        with self.assertRaisesRegex(ValueError, "host getter counts"):
            self.replay()
        write(self.cold / "response.json", original)
        self.change(
            self.cold,
            "request.json",
            lambda value: value.update(output="/another-attempt/" + self.cold.name),
        )
        with self.assertRaisesRegex(ValueError, "another campaign collection"):
            self.replay()

    def test_absent_raw_member_and_traversal_refuse(self):
        (self.cold / "rep-0-save_1.csv").unlink()
        with self.assertRaises(OSError):
            self.replay()
        with self.assertRaisesRegex(ValueError, "escapes evidence"):
            auditor.relative(self.cold, "../../authored/manifest.json")

    def test_wrong_typed_cells_saved_bytes_and_input_bytes_refuse(self):
        views = auditor.read(self.cold / "rep-0-views.json")
        changed = copy.deepcopy(views)
        changed["output"]["rows"][1][1] = {"type": "float", "value": "8000000000000000"}
        write(self.cold / "rep-0-views.json", changed)
        with self.assertRaisesRegex(ValueError, "independent truth"):
            self.replay()
        write(self.cold / "rep-0-views.json", views)
        content = (self.cold / "rep-0-save_1.csv").read_bytes()
        (self.cold / "rep-0-save_1.csv").write_bytes(b"altered\n")
        with self.assertRaisesRegex(ValueError, "independently authored truth"):
            self.replay()
        (self.cold / "rep-0-save_1.csv").write_bytes(content)
        (self.cold / "work-0/spec.yaml").write_bytes(b"{}")
        with self.assertRaisesRegex(ValueError, "input/specification bytes changed"):
            self.replay()

    def test_invented_zero_metric_clock_and_rss_refuse(self):
        original = auditor.read(self.cold / "response.json")
        changed = copy.deepcopy(original)
        changed["repetitions"][0]["durations"]["public_build"] = {
            "start_ns": 100,
            "end_ns": 100,
            "duration_ns": 0,
        }
        write(self.cold / "response.json", changed)
        with self.assertRaisesRegex(ValueError, "zero/nonfinite"):
            self.replay()
        changed = copy.deepcopy(original)
        changed["repetitions"][0]["metadata"]["clock"]["adjustable"] = True
        write(self.cold / "response.json", changed)
        with self.assertRaisesRegex(ValueError, "actual Python clock"):
            self.replay()
        write(self.cold / "response.json", original)
        self.change(
            self.cold,
            "process.json",
            lambda value: value["peak_rss"].update(raw=0, bytes=0),
        )
        with self.assertRaisesRegex(ValueError, "child peak RSS"):
            self.replay()

    def test_failed_warmup_and_missing_boundary_refuse(self):
        self.change(
            self.warm,
            "response.json",
            lambda value: value["repetitions"][0].update(status="failure"),
        )
        with self.assertRaisesRegex(ValueError, "repetition failed"):
            self.replay()

    def test_malformed_complete_report_runtime_refuses(self):
        original = auditor.read(self.cold / "response.json")
        for runtime in (None, "", 123):
            changed = copy.deepcopy(original)
            changed["repetitions"][0]["metadata"]["report_runtime_version"] = runtime
            write(self.cold / "response.json", changed)
            with self.assertRaisesRegex(ValueError, "complete-report runtime identity"):
                self.replay()


class Budgets(unittest.TestCase):
    def setUp(self):
        self.lane = auditor.tuples()[0]
        self.value = {
            "protocol": "public-performance-budgets/1",
            "status": "approved",
            "source_revision": "a" * 40,
            "decision": {
                "issue": 1973,
                "item": 10,
                "reference": "retained item-10 decision",
            },
            "rules": [
                {
                    "tuple": "*",
                    "case": "*",
                    "metric": "cold.public_build.median_ns",
                    "comparison": "absolute",
                    "limit": 12,
                }
            ],
            "dispositions": [
                {
                    "tuple": "*",
                    "case": "*",
                    "metric": "baseline:*",
                    "decision": "incomparable",
                    "reason": "full graph has no corresponding previous-release lifecycle",
                    "reference": "item-10 disposition",
                }
            ],
        }
        self.cell = {
            "tuple": self.lane,
            "case": "producer-1000-1",
            "candidate": {
                "status": "pass",
                "unavailable": [],
                "metrics": {"cold.public_build.median_ns": {"median": 10}},
            },
            "baseline": {"status": "missing"},
            "producer_root_build_baseline": {
                "status": "pass",
                "metrics": {"cold.public_build.median_ns": {"median": 2}},
            },
        }

    def test_absent_and_unapproved_never_pass(self):
        self.assertEqual(
            auditor.evaluate_budgets(None, [self.cell])["status"], "not_evaluated"
        )
        self.value["status"] = "unapproved"
        self.assertEqual(
            auditor.evaluate_budgets(self.value, [self.cell])["status"], "not_evaluated"
        )

    def test_explicit_limit_and_every_baseline_gap_disposition_required(self):
        auditor.validate_budgets(self.value, "a" * 40)
        self.assertEqual(
            auditor.evaluate_budgets(self.value, [self.cell])["status"], "pass"
        )
        self.value["dispositions"] = []
        self.assertEqual(
            auditor.evaluate_budgets(self.value, [self.cell])["status"], "incomplete"
        )

    def test_no_like_for_unlike_producer_root_build_ratio(self):
        self.value["rules"][0]["comparison"] = "baseline-ratio"
        self.assertEqual(
            auditor.evaluate_budgets(self.value, [self.cell])["status"], "incomplete"
        )

    def test_comparable_ratio_and_fail_are_distinct(self):
        self.cell["case"] = "lookup-1000-1"
        self.cell["baseline"] = {
            "status": "pass",
            "metrics": {"cold.public_build.median_ns": {"median": 2}},
        }
        self.value["rules"][0].update(comparison="baseline-ratio", limit=4)
        self.assertEqual(
            auditor.evaluate_budgets(self.value, [self.cell])["status"], "fail"
        )

    def test_missing_or_overlapping_thresholds_do_not_pass(self):
        self.value["rules"].append(copy.deepcopy(self.value["rules"][0]))
        self.assertEqual(
            auditor.evaluate_budgets(self.value, [self.cell])["status"], "incomplete"
        )
        self.value["rules"] = []
        self.assertEqual(
            auditor.evaluate_budgets(self.value, [self.cell])["status"], "incomplete"
        )

    def test_unversioned_wrong_source_null_nan_extra_and_unknown_decision_refuse(self):
        variants = []
        for field, value in (
            ("protocol", "public-performance-budgets/2"),
            ("source_revision", "b" * 40),
            ("extra", True),
        ):
            variants.append(dict(self.value, **{field: value}))
        for limit in (None, 0, -1, float("nan"), True):
            value = copy.deepcopy(self.value)
            value["rules"][0]["limit"] = limit
            variants.append(value)
        for value in variants:
            with self.subTest(value=value), self.assertRaises(ValueError):
                auditor.validate_budgets(value, "a" * 40)


class Inventory(unittest.TestCase):
    def test_exact_approved_matrix_and_generator_inventory(self):
        self.assertEqual(len(auditor.tuples()), 21)
        self.assertEqual(len(auditor.case_names()), 27)
        self.assertEqual(
            len(
                {
                    json.dumps(lane, sort_keys=True) + case
                    for lane in auditor.tuples()
                    for case in auditor.case_names()
                }
            ),
            567,
        )

    def test_duplicate_json_members_refuse(self):
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "input.json"
            path.write_text('{"status":"fail","status":"pass"}')
            with self.assertRaisesRegex(ValueError, "duplicate JSON"):
                auditor.read(path)

    def test_check_writes_both_reports_before_failing(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            write(root / "input.json", {"protocol": "incorrect"})
            process = subprocess.run(
                [
                    sys.executable,
                    str(TOOLS / "audit_public_performance.py"),
                    "--input",
                    str(root / "input.json"),
                    "--output",
                    str(root / "report"),
                    "--check",
                ],
                check=False,
                capture_output=True,
                text=True,
            )
            self.assertEqual(process.returncode, 1)
            self.assertFalse(
                auditor.read(root / "report/performance-audit.json")["accepted"]
            )
            self.assertTrue((root / "report/performance-audit.md").is_file())


class ActionsAdmission(unittest.TestCase):
    def setUp(self):
        self.payload = {"source_revision": "a" * 40, "repository": "elong0527/yamaa"}
        self.run = {
            "id": 123,
            "run_attempt": 2,
            "head_branch": "main",
            "head_sha": "a" * 40,
            "repository": {"full_name": "elong0527/yamaa"},
            "head_repository": {"full_name": "elong0527/yamaa"},
            "path": ".github/workflows/performance.yml",
            "event": "workflow_dispatch",
            "status": "in_progress",
            "conclusion": None,
        }
        self.entry = {
            "tuple": auditor.tuples()[0],
            "run_attempt": 2,
            "artifact_id": 456,
            "artifact_name": "performance-python-ubuntu-24.04-3.12-wheel",
        }
        self.artifact = {
            "id": 456,
            "name": self.entry["artifact_name"],
            "expired": False,
            "size_in_bytes": 100,
            "workflow_run": {"id": 123, "head_sha": "a" * 40},
            "created_at": "2026-10-14T01:10:00Z",
        }
        self.job = {
            "name": "performance-python (ubuntu-24.04, 3.12, wheel)",
            "run_id": 123,
            "run_attempt": 2,
            "head_sha": "a" * 40,
            "status": "completed",
            "conclusion": "success",
            "started_at": "2026-10-14T01:00:00Z",
            "completed_at": "2026-10-14T01:20:00Z",
        }

    def test_successful_producer_allows_pending_current_audit_job(self):
        auditor.validate_run(self.run, self.payload)
        auditor.admit_artifact(self.entry, [self.artifact], self.run, [self.job])

    def test_terminal_failed_or_cancelled_attempt_never_admits_retained_bytes(self):
        for conclusion in ("failure", "cancelled", "timed_out", None):
            run = dict(self.run, status="completed", conclusion=conclusion)
            with (
                self.subTest(conclusion=conclusion),
                self.assertRaisesRegex(ValueError, "failed/cancelled"),
            ):
                auditor.validate_run(run, self.payload)
        for conclusion in ("failure", "cancelled", None):
            job = dict(self.job, conclusion=conclusion)
            with (
                self.subTest(job=job),
                self.assertRaisesRegex(ValueError, "producing job"),
            ):
                auditor.admit_artifact(self.entry, [self.artifact], self.run, [job])

    def test_wrong_source_job_attempt_empty_expired_and_ambiguous_artifact_refuse(self):
        for job in (
            dict(self.job, run_attempt=1),
            dict(self.job, head_sha="b" * 40),
            dict(self.job, status="in_progress"),
        ):
            with self.assertRaises(ValueError):
                auditor.admit_artifact(self.entry, [self.artifact], self.run, [job])
        for artifact in (
            dict(self.artifact, expired=True),
            dict(self.artifact, size_in_bytes=0),
            dict(self.artifact, created_at="2026-10-13T01:10:00Z"),
        ):
            with self.assertRaises(ValueError):
                auditor.admit_artifact(self.entry, [artifact], self.run, [self.job])
        with self.assertRaisesRegex(ValueError, "ambiguous artifact"):
            auditor.admit_artifact(
                self.entry,
                [self.artifact, dict(self.artifact, id=789)],
                self.run,
                [self.job],
            )

    def test_nonpositive_run_id_wrong_source_and_sampled_event_refuse(self):
        for run in (
            dict(self.run, id=0),
            dict(self.run, head_sha="b" * 40),
            dict(self.run, event="pull_request"),
        ):
            with self.assertRaises(ValueError):
                auditor.validate_run(run, self.payload)

    def test_always_retains_all_567_gaps_and_budget_errors_stay_separate(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            payload = dict(
                self.payload,
                protocol=auditor.PROTOCOL,
                run="run.json",
                artifacts="artifacts.json",
                entries=[],
            )
            write(root / "input.json", payload)
            write(root / "run.json", self.run)
            write(root / "artifacts.json", [])
            write(root / "bad-budget.json", {"protocol": "wrong"})
            result = auditor.audit(root / "input.json", root, root / "bad-budget.json")
            self.assertEqual(len(result["cells"]), 567)
            self.assertEqual(result["coverage"]["missing"], 567)
            self.assertEqual(result["harness_correctness"]["status"], "incomplete")
            self.assertEqual(result["budgets"]["status"], "fail")
            self.assertFalse(result["accepted"])

    def test_malformed_campaign_still_writes_both_reports_and_all_matrix_gaps(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            name = self.entry["artifact_name"]
            collection = root / name / "candidate-evidence"
            collection.mkdir(parents=True)
            write(
                collection / "collection.json",
                {
                    "protocol": "public-performance/1",
                    "purpose": "characterization",
                    "host": "python",
                    "sampling": auditor.SAMPLING,
                    "phase_measurements_required": False,
                    "evaluator": "public",
                    "measurement_scope": "full-lifecycle",
                    "source_revision": "a" * 40,
                    "measurement_campaign": {},
                    "provenance": {
                        "artifact_path": "/vanished/yamaa.whl",
                        "system": "Linux",
                        "source_revision": "a" * 40,
                        "artifact_reference": "github-actions:321/attempt-2/native-python-ubuntu-24.04-3.14/yamaa.whl",
                    },
                },
            )
            entry = dict(
                self.entry,
                campaign=name + "/machine-campaign.json",
                collections=[
                    {"role": "candidate", "path": name + "/candidate-evidence"}
                ],
            )
            payload = dict(
                self.payload,
                protocol=auditor.PROTOCOL,
                run="run.json",
                artifacts="artifacts.json",
                jobs="jobs.json",
                entries=[entry],
            )
            native_run, _, _ = native_fixture(root, payload)
            write(root / name / "native-run.json", native_run)
            write(root / "input.json", payload)
            write(root / "run.json", self.run)
            write(root / "artifacts.json", [self.artifact])
            write(root / "jobs.json", [dict(self.job, runner_name="fixture-runner")])
            write(
                root / name / "machine-campaign.json",
                {
                    "protocol": "public-performance-campaign/1",
                    "campaign_id": 123,
                    "created_at": "2026-10-14T01:05:00Z",
                    "measurement_machine": {"runner_name": "fixture-runner"},
                },
            )
            self.assertEqual(
                auditor.main(
                    [
                        "--input",
                        str(root / "input.json"),
                        "--output",
                        str(root / "report"),
                        "--check",
                    ]
                ),
                1,
            )
            report = auditor.read(root / "report/performance-audit.json")
            self.assertEqual(report["harness_correctness"]["status"], "fail")
            self.assertEqual(len(report["cells"]), 567)
            self.assertEqual(report["coverage"]["missing"], 567)
            self.assertIn(
                "replace", report["harness_correctness"]["errors"][0]["reason"]
            )
            self.assertTrue((root / "report/performance-audit.md").is_file())


class NativeSourceBinding(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.payload = {"repository": "elong0527/yamaa", "source_revision": "a" * 40}
        self.run, self.jobs, self.artifacts = native_fixture(self.root, self.payload)
        self.performance_run = {"head_branch": "main"}
        self.directory = (
            self.root
            / "performance-python-ubuntu-24.04-3.12-wheel"
            / "candidate-evidence"
        )
        self.directory.mkdir(parents=True)
        write(self.directory.parent / "native-run.json", self.run)
        self.record = {
            "artifact_path": "/vanished/yamaa.whl",
            "system": "Linux",
            "source_revision": "a" * 40,
            "artifact_reference": "github-actions:321/attempt-2/native-python-ubuntu-24.04-3.14/yamaa.whl",
        }
        write(self.directory / "collection.json", {"provenance": self.record})

    def inputs(self):
        return auditor.native_inputs(self.payload, self.root, self.performance_run)

    def test_original_full_native_attempt_and_candidate_reference_re_admit(self):
        native = self.inputs()
        self.assertEqual(native["status"], "pass")
        result = auditor.native_collection(self.directory, auditor.tuples()[0], native)
        self.assertEqual(result["run_attempt"], 2)
        self.assertEqual(
            result["artifact_reference"], self.record["artifact_reference"]
        )
        self.assertIn(
            result["artifact_id"], [artifact["id"] for artifact in self.artifacts]
        )

    def test_absent_and_failed_native_metadata_never_admit(self):
        self.payload.pop("native_jobs")
        self.assertEqual(self.inputs()["status"], "incomplete")
        self.payload["native_jobs"] = "native-jobs.json"
        self.jobs[0]["run_attempt"] = 1
        write(self.root / "native-jobs.json", self.jobs)
        self.assertEqual(self.inputs()["status"], "fail")

    def test_outer_native_attempt_and_unattempted_or_wrong_candidate_references_refuse(
        self,
    ):
        native = self.inputs()
        write(self.directory.parent / "native-run.json", dict(self.run, run_attempt=1))
        with self.assertRaisesRegex(ValueError, "original native run/attempt"):
            auditor.native_collection(self.directory, auditor.tuples()[0], native)
        write(self.directory.parent / "native-run.json", self.run)
        for reference in (
            "github-actions:321/native-python-ubuntu-24.04-3.14/yamaa.whl",
            "github-actions:321/attempt-1/native-python-ubuntu-24.04-3.14/yamaa.whl",
        ):
            write(
                self.directory / "collection.json",
                {"provenance": dict(self.record, artifact_reference=reference)},
            )
            with self.assertRaisesRegex(ValueError, "native attempt reference"):
                auditor.native_collection(self.directory, auditor.tuples()[0], native)

    def test_invalid_or_duplicate_original_native_artifact_ids_refuse(self):
        self.artifacts[0]["id"] = self.artifacts[1]["id"]
        write(self.root / "native-artifacts.json", self.artifacts)
        self.assertEqual(self.inputs()["status"], "fail")


class Archives(unittest.TestCase):
    def test_original_archive_streams_one_safe_private_copy_and_provider_cleanup(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            entry = {
                "artifact_name": "performance-python-ubuntu-24.04-3.12-wheel",
                "archive": "original.zip",
            }
            with zipfile.ZipFile(root / "original.zip", "w") as archive:
                archive.writestr(
                    "candidate-evidence/collection.json", '{"original":true}'
                )
            original = (root / "original.zip").read_bytes()
            calls = []

            @contextlib.contextmanager
            def provider(actual_entry, manifest_root):
                self.assertEqual(actual_entry, entry)
                self.assertEqual(manifest_root, root)
                calls.append("opened")
                try:
                    yield root
                finally:
                    calls.append("closed")

            with auditor.entry_root(
                root, entry, root / "scratch", provider
            ) as evidence:
                self.assertEqual(
                    auditor.read(
                        evidence
                        / entry["artifact_name"]
                        / "candidate-evidence/collection.json"
                    ),
                    {"original": True},
                )
            self.assertEqual(calls, ["opened", "closed"])
            self.assertEqual((root / "original.zip").read_bytes(), original)

    def test_traversal_absolute_duplicate_and_symlink_archive_members_refuse(self):
        for names in (
            ("../escaped",),
            ("/absolute",),
            ("C:/absolute",),
            ("safe", "./safe"),
        ):
            with self.subTest(names=names), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                with zipfile.ZipFile(root / "bad.zip", "w") as archive:
                    for name in names:
                        archive.writestr(name, "unchanged")
                with (
                    self.assertRaises(ValueError),
                    auditor.entry_root(
                        root,
                        {"archive": "bad.zip", "artifact_name": "artifact"},
                        root / "scratch",
                        None,
                    ),
                ):
                    pass
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            with zipfile.ZipFile(root / "bad.zip", "w") as archive:
                member = zipfile.ZipInfo("linked")
                member.external_attr = 0o120777 << 16
                archive.writestr(member, "/outside")
            with (
                self.assertRaisesRegex(ValueError, "symlink"),
                auditor.entry_root(
                    root,
                    {"archive": "bad.zip", "artifact_name": "artifact"},
                    root / "scratch",
                    None,
                ),
            ):
                pass


class InstallationAdmission(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.repository = self.root / "source"
        (self.repository / "rust").mkdir(parents=True)
        (self.repository / "rust/Cargo.toml").write_text(
            '[workspace.package]\nversion = "0.4.0"\n'
        )
        for command in (
            ["git", "init", "-q"],
            ["git", "add", "."],
            [
                "git",
                "-c",
                "user.name=fixture",
                "-c",
                "user.email=fixture@localhost",
                "commit",
                "-qm",
                "synthetic source fixture",
            ],
        ):
            subprocess.run(
                command, cwd=self.repository, check=True, capture_output=True
            )
        revision = subprocess.check_output(
            ["git", "rev-parse", "HEAD"], cwd=self.repository, text=True
        ).strip()
        installation = self.root / "installation"
        installation.mkdir()
        self.record = {
            "source_revision": revision,
            "artifact_reference": "synthetic-fixture/yamaa.whl",
            "artifact_path": "/vanished/yamaa.whl",
            "package_form": "wheel",
            "host": "python",
            "runtime_version": "3.12.1",
            "package_version": "0.4.0",
            "engine_version": "0.4.0",
            "toolchain": None,
            "cpu": None,
            "system": "Linux",
            "installed_origins": {
                "package": "/vanished/site/yamaa",
                "native": "/vanished/site/yamaa/_native.so",
            },
            "installation_evidence": "/vanished/installation.json",
        }
        write(installation / "declared.json", self.record)
        write(
            installation / "installation.json",
            {
                "evidence": {
                    key: self.record[key]
                    for key in ("source_revision", "artifact_reference", "package_form")
                },
                "installation": {
                    "runtime_version": "3.12.1",
                    "host_package_version": "0.4.0",
                    "core_version": "0.4.0",
                    "package_location": "/vanished/site/yamaa/_native.so",
                },
                "build_association": "synthetic installation fixture; never live release evidence",
            },
        )
        (installation / "installed-native-library.bin").write_bytes(
            b"original native bytes"
        )
        self.wheel("yamaa", "0.4.0")

    def wheel(self, name, version):
        with zipfile.ZipFile(self.root / "installation/yamaa.whl", "w") as wheel:
            wheel.writestr("yamaa/_native.so", b"original native bytes")
            wheel.writestr(
                "yamaa-0.4.0.dist-info/METADATA", f"Name: {name}\nVersion: {version}\n"
            )

    def replay(self):
        return auditor.admit_installation(
            self.root,
            {
                "provenance": self.record,
                "source_revision": self.record["source_revision"],
            },
            auditor.tuples()[0],
            "candidate",
            self.repository,
            self.root / "scratch",
        )

    def test_original_wheel_native_and_package_identity_replayed_without_live_install(
        self,
    ):
        self.assertEqual(self.replay(), self.record)

    def test_only_approved_prior_python_version_can_be_a_baseline(self):
        with self.assertRaisesRegex(ValueError, "prior Python 0.3.0"):
            auditor.admit_installation(
                self.root,
                {
                    "provenance": self.record,
                    "source_revision": self.record["source_revision"],
                },
                auditor.tuples()[0],
                "baseline",
                self.repository,
                self.root / "scratch",
            )

    def test_changed_native_package_version_or_pinned_core_version_refuse(self):
        self.wheel("other-package", "0.4.0")
        with self.assertRaisesRegex(ValueError, "wheel package/version"):
            self.replay()
        self.wheel("yamaa", "0.3.0")
        with self.assertRaisesRegex(ValueError, "wheel package/version"):
            self.replay()
        self.wheel("yamaa", "0.4.0")
        (self.root / "installation/installed-native-library.bin").write_bytes(
            b"substituted native bytes"
        )
        with self.assertRaisesRegex(ValueError, "installed library differs"):
            self.replay()

    def test_replaced_original_declaration_and_wrong_source_identity_refuse(self):
        self.record["engine_version"] = "0.3.0"
        write(self.root / "installation/declared.json", self.record)
        with self.assertRaisesRegex(ValueError, "pinned source"):
            self.replay()

    def test_pinned_original_locks_replay_without_a_live_source_checkout(self):
        for name in ("python/uv.lock", "benchmarks/schema-functions/r/renv.lock"):
            target = self.repository / name
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(b"original lock fixture\n")
        subprocess.run(["git", "add", "."], cwd=self.repository, check=True)
        subprocess.run(
            [
                "git",
                "-c",
                "user.name=fixture",
                "-c",
                "user.email=fixture@localhost",
                "commit",
                "-qm",
                "locks",
            ],
            cwd=self.repository,
            check=True,
        )
        revision = subprocess.check_output(
            ["git", "rev-parse", "HEAD"], cwd=self.repository, text=True
        ).strip()
        subprocess.run(
            ["git", "bundle", "create", str(self.root / "source.bundle"), "HEAD"],
            cwd=self.repository,
            check=True,
            capture_output=True,
        )
        subprocess.run(
            [
                "git",
                "clone",
                "--bare",
                str(self.root / "source.bundle"),
                str(self.root / "retained.git"),
            ],
            check=True,
            capture_output=True,
        )
        self.repository.rename(self.root / "unavailable-source")
        auditor.regenerate(
            self.root / "independent",
            {"family": "callback", "driving_rows": 10},
            self.root / "retained.git",
            revision,
            self.root / "scratch",
        )
        self.assertEqual(
            (self.root / "independent/callback-10-1/python/uv.lock").read_bytes(),
            b"original lock fixture\n",
        )


class CollectionRefusals(unittest.TestCase):
    def test_frontier_and_scope_mismatch_do_not_read_or_admit_timing(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            for purpose in ("capacity-frontier", "correctness-smoke"):
                write(
                    root / "collection.json",
                    {"protocol": "public-performance/1", "purpose": purpose},
                )
                with self.assertRaisesRegex(ValueError, "frontier/correctness smoke"):
                    auditor.admit_collection(
                        root, auditor.tuples()[0], "candidate", "a" * 40, {}, root, root
                    )
            write(
                root / "collection.json",
                {
                    "protocol": "public-performance/1",
                    "purpose": "characterization",
                    "host": "python",
                    "phase_measurements_required": False,
                    "sampling": auditor.SAMPLING,
                    "evaluator": "python-reference",
                    "measurement_scope": "producer-root-build",
                },
            )
            with self.assertRaisesRegex(ValueError, "scope differs"):
                auditor.admit_collection(
                    root, auditor.tuples()[0], "candidate", "a" * 40, {}, root, root
                )


if __name__ == "__main__":
    unittest.main()
