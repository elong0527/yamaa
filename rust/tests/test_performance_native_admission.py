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
            {"name": name, "status": "completed", "conclusion": "success"}
            for name in admission.required_jobs()
        ]
        self.artifacts = [
            {
                "name": name,
                "expired": False,
                "size_in_bytes": 10,
                "workflow_run": {"head_sha": self.revision},
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
