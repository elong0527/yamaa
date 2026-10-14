"""Rollback retention refuses changed ownership and unsafe archive members."""

import copy
import importlib.util
import io
import json
import tempfile
import unittest
import zipfile
from pathlib import Path
from unittest import mock

spec = importlib.util.spec_from_file_location(
    "retain_python_rollback",
    Path(__file__).resolve().parents[1] / "tools/retain_python_rollback.py",
)
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


class PriorRetentionAdmission(unittest.TestCase):
    def setUp(self):
        self.expected = json.loads(module.RECORD.read_text())
        self.run = {
            "id": self.expected["run_id"],
            "head_sha": self.expected["head"],
            "run_attempt": 1,
            "status": "completed",
            "conclusion": "success",
        }
        self.artifacts = [
            {
                "id": item["id"],
                "name": item["name"],
                "size_in_bytes": item["bytes"],
                "expired": False,
                "workflow_run": {
                    "id": self.expected["run_id"],
                    "head_sha": self.expected["head"],
                },
            }
            for item in self.expected["artifacts"]
        ]

    def test_complete_approved_run_is_admitted(self):
        module.admit_run(self.run, self.artifacts, self.expected)

    def test_another_attempt_source_failed_or_incomplete_run_is_refused(self):
        for key, value in (
            ("run_attempt", 2),
            ("head_sha", "another-head"),
            ("status", "in_progress"),
            ("conclusion", "failure"),
        ):
            with (
                self.subTest(key=key),
                self.assertRaisesRegex(ValueError, "run/head/attempt"),
            ):
                module.admit_run(
                    {**self.run, key: value}, self.artifacts, self.expected
                )

    def test_missing_duplicate_expired_replaced_and_foreign_artifacts_are_refused(self):
        for change in (
            lambda items: items.pop(),
            lambda items: items.append(copy.deepcopy(items[0])),
            lambda items: items[0].update(expired=True),
            lambda items: items[0].update(id=1),
            lambda items: items[0]["workflow_run"].update(head_sha="foreign-head"),
        ):
            artifacts = copy.deepcopy(self.artifacts)
            change(artifacts)
            with self.assertRaises(ValueError):
                module.admit_run(self.run, artifacts, self.expected)

    def test_path_traversal_duplicate_and_symlink_members_are_refused(self):
        for names, symlink in (
            (["../escape"], False),
            (["/escape"], False),
            (["C:\\escape"], False),
            (["same", "same"], False),
            (["link"], True),
        ):
            data = io.BytesIO()
            with zipfile.ZipFile(data, "w") as archive:
                for name in names:
                    info = zipfile.ZipInfo(name)
                    if symlink:
                        info.external_attr = 0o120777 << 16
                    archive.writestr(info, b"bytes")
            with tempfile.TemporaryDirectory() as root, self.assertRaises(ValueError):
                module.extract(data.getvalue(), Path(root))

    def test_upload_guard_refuses_pr_context_before_network_or_mutation(self):
        prior = mock.Mock()
        with (
            mock.patch.dict(
                "os.environ",
                {
                    "GITHUB_REF": "refs/pull/1/merge",
                    "GITHUB_EVENT_NAME": "pull_request",
                },
            ),
            self.assertRaisesRegex(ValueError, "trusted qualification"),
        ):
            module.retain(Path("unused"), Path("unused"), prior)
        prior.github.assert_not_called()

    def test_actions_redirect_request_uses_json_media_type_and_get_only(self):
        prior = mock.Mock(REPOSITORY="elong0527/yamaa")
        opener = mock.MagicMock()
        opener.open.return_value.__enter__.return_value.read.return_value = b"zip bytes"
        with (
            mock.patch.dict("os.environ", {"GH_TOKEN": "unit-token"}),
            mock.patch("urllib.request.build_opener", return_value=opener),
        ):
            self.assertEqual(module.download_artifact(123, prior), b"zip bytes")
        request = opener.open.call_args.args[0]
        self.assertEqual(request.get_method(), "GET")
        self.assertEqual(request.get_header("Accept"), "application/vnd.github+json")
        self.assertIsNone(request.data)

    def test_existing_conflicting_asset_is_refused_without_upload_or_overwrite(self):
        prior = mock.Mock(REPOSITORY="elong0527/yamaa", RELEASE=411980272)
        existing = {"name": "platform.whl", "id": 123, "state": "uploaded"}
        release = {"assets": [existing]}
        prior.github.side_effect = [release, release, b"different original bytes"]
        with tempfile.TemporaryDirectory() as root:
            directory = Path(root)
            (directory / "platform.whl").write_bytes(b"new bytes")
            with (
                mock.patch.dict(
                    "os.environ",
                    {"GITHUB_REF": module.TRUSTED_REF, "GITHUB_EVENT_NAME": "push"},
                ),
                mock.patch("subprocess.run") as upload,
                self.assertRaisesRegex(ValueError, "never overwrite"),
            ):
                module.retain(directory, directory, prior)
            upload.assert_not_called()


if __name__ == "__main__":
    unittest.main()
