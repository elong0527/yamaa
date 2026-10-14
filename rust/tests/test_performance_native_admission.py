"""Refuse sampled, wrong-source and incomplete native performance inputs."""

import copy
import importlib.util
import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest import mock

SPEC = importlib.util.spec_from_file_location(
    "performance_native_admission",
    Path(__file__).resolve().parents[1] / "tools/admit_performance_native.py",
)
admission = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(admission)


class NativeAdmission(unittest.TestCase):
    def setUp(self):
        self.repository = "owner/project"
        self.revision = "a" * 40
        self.branch = "qualification/issue-1998"
        self.run = {
            "id": 42,
            "run_attempt": 2,
            "html_url": "https://github.com/owner/project/actions/runs/42",
            "repository": {"full_name": self.repository},
            "head_repository": {"full_name": self.repository},
            "head_sha": self.revision,
            "head_branch": self.branch,
            "path": ".github/workflows/native.yml",
            "event": "push",
            "status": "completed",
            "conclusion": "success",
        }
        self.jobs = [
            {
                "name": name,
                "status": "completed",
                "conclusion": "success",
                "run_id": 42,
                "run_attempt": 2,
                "head_sha": self.revision,
                "started_at": "2026-10-13T12:00:00Z",
                "completed_at": "2026-10-13T12:10:00Z",
            }
            for name in admission.required_jobs()
        ]
        self.artifacts = [
            {
                "name": name,
                "expired": False,
                "size_in_bytes": 10,
                "workflow_run": {"id": 42, "head_sha": self.revision},
                "created_at": "2026-10-13T12:05:00Z",
            }
            for name in admission.required_artifacts()
        ]

    def validate(self):
        admission.validate(
            self.run,
            self.jobs,
            self.artifacts,
            self.repository,
            self.revision,
            self.branch,
        )

    def test_full_exact_source_is_admitted(self):
        self.validate()
        self.run["event"] = "workflow_dispatch"
        self.validate()

    def test_fork_merge_pr_different_head_and_failed_run_are_refused(self):
        original = copy.deepcopy(self.run)
        for key, value in (
            ("repository", {"full_name": "other/project"}),
            ("head_repository", {"full_name": "fork/project"}),
            ("head_sha", "b" * 40),
            ("head_branch", "main"),
            ("event", "pull_request"),
            ("path", ".github/workflows/ci.yml"),
            ("status", "in_progress"),
            ("conclusion", "failure"),
        ):
            with self.subTest(key=key):
                self.run = dict(original, **{key: value})
                with self.assertRaisesRegex(ValueError, "exact trusted source"):
                    self.validate()

    def test_missing_windows_source_install_failed_audit_and_duplicate_job_refuse(self):
        name = "native-qualify (windows-latest, 3.13, source)"
        original = copy.deepcopy(self.jobs)
        self.jobs = [job for job in self.jobs if job["name"] != name]
        with self.assertRaisesRegex(ValueError, "full native job"):
            self.validate()
        self.jobs = copy.deepcopy(original)
        next(job for job in self.jobs if job["name"] == "native-run-evidence-audit")[
            "conclusion"
        ] = "failure"
        with self.assertRaisesRegex(ValueError, "full native job"):
            self.validate()
        self.jobs = original + [original[0]]
        with self.assertRaisesRegex(ValueError, "full native job"):
            self.validate()

    def test_expired_missing_or_different_source_artifacts_refuse(self):
        original = copy.deepcopy(self.artifacts)
        for replacement in (
            {"expired": True},
            {"size_in_bytes": 0},
            {"workflow_run": {"head_sha": "b" * 40}},
        ):
            self.artifacts = copy.deepcopy(original)
            self.artifacts[0].update(replacement)
            with self.assertRaisesRegex(ValueError, "native artifact"):
                self.validate()
        self.artifacts = original[1:]
        with self.assertRaisesRegex(ValueError, "native artifact"):
            self.validate()

    def test_other_run_source_attempt_or_missing_job_identity_refuse(self):
        original = copy.deepcopy(self.jobs)
        for field, wrong in (
            ("run_id", 43),
            ("run_attempt", 1),
            ("head_sha", "b" * 40),
        ):
            with self.subTest(field=field):
                self.jobs = copy.deepcopy(original)
                self.jobs[0][field] = wrong
                with self.assertRaisesRegex(ValueError, "full native job"):
                    self.validate()
                self.jobs[0].pop(field)
                with self.assertRaisesRegex(ValueError, "full native job"):
                    self.validate()

    def test_old_attempt_artifact_cannot_admit_a_same_source_rerun(self):
        original = copy.deepcopy(self.artifacts)
        for wrong in (
            {"workflow_run": {"id": 43, "head_sha": self.revision}},
            {"created_at": "2026-10-13T11:05:00Z"},
            {"created_at": "2026-10-13T12:11:00Z"},
        ):
            with self.subTest(wrong=wrong):
                self.artifacts = copy.deepcopy(original)
                self.artifacts[0].update(wrong)
                with self.assertRaisesRegex(ValueError, "native artifact"):
                    self.validate()

    def test_artifact_uses_its_producing_job_interval(self):
        # A late cohort receipt cannot legitimize a distribution left over
        # from an earlier attempt, even when both used the same source.
        job = next(
            job
            for job in self.jobs
            if job["name"] == "native-dist (ubuntu-24.04, 3.14)"
        )
        job.update(
            started_at="2026-10-13T12:06:00Z", completed_at="2026-10-13T12:10:00Z"
        )
        with self.assertRaisesRegex(ValueError, "producing job attempt"):
            self.validate()

    def test_missing_or_unzoned_timestamps_refuse(self):
        original = copy.deepcopy(self.jobs)
        for wrong in (None, "2026-10-13T12:00:00"):
            with self.subTest(wrong=wrong):
                self.jobs = copy.deepcopy(original)
                self.jobs[0]["started_at"] = wrong
                with self.assertRaisesRegex((TypeError, ValueError), "timestamp"):
                    self.validate()

    def test_failed_native_run_aborts_without_launching_measurements(self):
        with tempfile.TemporaryDirectory() as directory:
            args = SimpleNamespace(
                repository=self.repository,
                revision=self.revision,
                branch=self.branch,
                run_id="42",
                wait_seconds=7200,
                output=Path(directory) / "retained",
            )
            self.run["conclusion"] = "failure"
            with (
                mock.patch.object(admission, "api", return_value=self.run) as api,
                self.assertRaisesRegex(ValueError, "qualification failed"),
            ):
                admission.admit(args)
            self.assertEqual(api.call_count, 1)
            self.assertTrue((args.output / "native-run.json").is_file())

    def test_admitted_run_exports_selected_original_attempt(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            args = SimpleNamespace(
                repository=self.repository,
                revision=self.revision,
                branch=self.branch,
                run_id="42",
                wait_seconds=0,
                output=root / "retained",
            )
            export = root / "output"
            with (
                mock.patch.object(
                    admission,
                    "api",
                    side_effect=[
                        self.run,
                        [{"jobs": self.jobs}],
                        [{"artifacts": self.artifacts}],
                    ],
                ),
                mock.patch.dict(admission.os.environ, GITHUB_OUTPUT=str(export)),
            ):
                admission.admit(args)
            self.assertEqual(
                export.read_text(),
                f"run-id=42\nrun-attempt=2\nrevision={self.revision}\n",
            )

    def test_missing_native_run_expires_at_declared_wait_bound(self):
        with tempfile.TemporaryDirectory() as directory:
            args = SimpleNamespace(
                repository=self.repository,
                revision=self.revision,
                branch=self.branch,
                run_id="",
                wait_seconds=0,
                output=Path(directory) / "retained",
            )
            with (
                mock.patch.object(admission, "api", return_value={"workflow_runs": []}),
                mock.patch.object(admission.time, "sleep") as sleep,
                self.assertRaisesRegex(ValueError, "timed out"),
            ):
                admission.admit(args)
            sleep.assert_not_called()
