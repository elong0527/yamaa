"""Synthetic fixtures exercise original completion and additive replay boundaries."""

import json
import os
import sys
import tempfile
import unittest
import zipfile
from pathlib import Path
from unittest import mock

TOOLS = Path(__file__).resolve().parents[1] / "tools"
sys.path.insert(0, str(TOOLS))
import finalize_public_performance as finalize


def original_run():
    return {
        "id": 123,
        "run_attempt": 2,
        "head_sha": "a" * 40,
        "repository": {"full_name": "elong0527/yamaa"},
        "head_repository": {"full_name": "elong0527/yamaa"},
        "event": "push",
        "head_branch": "qualification/issue-1998",
        "path": ".github/workflows/performance.yml",
        "status": "completed",
        "conclusion": "success",
    }


def identity():
    return finalize.campaign_identity(original_run(), "elong0527/yamaa")


def retained_parts():
    rows = [
        {
            "asset_id": 11,
            "name": "original.zip.part-0001",
            "bytes": 3,
            "bytes_equal": True,
            "original_artifact_id": 42,
            "original_artifact_name": "tuple",
            "original_run_attempt": 2,
            "original_zip_bytes": 5,
            "segment_count": 2,
            "byte_offset": 0,
        },
        {
            "asset_id": 12,
            "name": "original.zip.part-0002",
            "bytes": 2,
            "bytes_equal": True,
            "original_artifact_id": 42,
            "original_artifact_name": "tuple",
            "original_run_attempt": 2,
            "original_zip_bytes": 5,
            "segment_count": 2,
            "byte_offset": 3,
        },
    ]
    release = {
        "assets": [
            {
                "id": row["asset_id"],
                "name": row["name"],
                "size": row["bytes"],
                "state": "uploaded",
            }
            for row in rows
        ]
    }
    return release, {"assets": rows}


class CompletionIdentity(unittest.TestCase):
    def test_untrusted_completion_repository_events_and_branches_are_refused_before_reads(
        self,
    ):
        with tempfile.TemporaryDirectory() as temporary:
            event_path = Path(temporary) / "event.json"
            environment = {
                "GITHUB_REPOSITORY": "elong0527/yamaa",
                "GITHUB_EVENT_NAME": "workflow_run",
                "GITHUB_EVENT_PATH": str(event_path),
            }
            for field, value in (
                ("event", "pull_request"),
                ("event", "workflow_dispatch"),
                ("head_branch", "main"),
                ("path", ".github/workflows/native.yml"),
                ("head_repository", {"full_name": "fork/yamaa"}),
                ("repository", {"full_name": "fork/yamaa"}),
            ):
                run = dict(original_run(), **{field: value})
                event_path.write_text(
                    json.dumps({"action": "completed", "workflow_run": run})
                )
                with (
                    self.subTest(field=field, value=value),
                    mock.patch.dict(os.environ, environment, clear=True),
                    mock.patch.object(finalize, "api") as api,
                    self.assertRaisesRegex(ValueError, "trusted"),
                ):
                    finalize.selected_campaign(None)
                api.assert_not_called()
            with (
                mock.patch.dict(
                    os.environ,
                    dict(environment, GITHUB_REPOSITORY="fork/yamaa"),
                    clear=True,
                ),
                self.assertRaisesRegex(ValueError, "untrusted campaign repository"),
            ):
                finalize.selected_campaign(None)

    def test_versioned_evidence_push_keeps_original_source_separate_from_auditor(self):
        with tempfile.TemporaryDirectory() as temporary:
            request = Path(temporary) / "request.json"
            request.write_text(
                json.dumps({"protocol": "public-performance-replay/1", **identity()})
            )
            environment = {
                "GITHUB_REPOSITORY": "elong0527/yamaa",
                "GITHUB_EVENT_NAME": "push",
                "GITHUB_REF": finalize.REPLAY_REF,
                "GITHUB_SHA": "b" * 40,
            }
            with mock.patch.dict(os.environ, environment, clear=True):
                selected, collection_only = finalize.selected_campaign(request)
            self.assertEqual(selected, identity())
            self.assertFalse(collection_only)
            for ref in (
                "refs/heads/main",
                "refs/heads/qualification/issue-1998",
                "refs/heads/fork",
            ):
                with (
                    mock.patch.dict(
                        os.environ, dict(environment, GITHUB_REF=ref), clear=True
                    ),
                    self.assertRaisesRegex(
                        ValueError, "explicit trusted evidence push"
                    ),
                ):
                    finalize.selected_campaign(request)

    def test_completion_wrong_attempt_is_retained_and_original_failed_verdict_is_never_relabelled(
        self,
    ):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            wrong = dict(original_run(), run_attempt=1)
            with (
                mock.patch.object(finalize, "api", return_value=wrong),
                self.assertRaisesRegex(ValueError, "differs"),
            ):
                finalize.completion_snapshot(identity(), root / "wrong.json")
            self.assertEqual(json.loads((root / "wrong.json").read_text()), wrong)
            failed = dict(original_run(), conclusion="failure")
            with mock.patch.object(finalize, "api", return_value=failed):
                actual = finalize.completion_snapshot(identity(), root / "failed.json")
            self.assertEqual(actual["conclusion"], "failure")
            running = dict(original_run(), status="in_progress", conclusion=None)
            with (
                mock.patch.object(finalize, "api", return_value=running),
                self.assertRaisesRegex(ValueError, "not completed"),
            ):
                finalize.completion_snapshot(identity(), root / "running.json")

    def test_completed_manifest_preserves_original_snapshot_and_index_bytes(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            snapshot = dict(original_run(), status="in_progress", conclusion=None)
            run_bytes = (json.dumps(snapshot, separators=(",", ":")) + "\n").encode()
            manifest = {
                "repository": "elong0527/yamaa",
                "source_revision": "a" * 40,
                "run": "run.json",
                "entries": [],
            }
            manifest_bytes = json.dumps(manifest).encode()
            (root / "run.json").write_bytes(run_bytes)
            (root / "manifest.json").write_bytes(manifest_bytes)
            completion = root / "original-completion.json"
            completion.write_text(json.dumps(original_run()))
            path = finalize.completed_manifest(root, identity(), completion)
            self.assertEqual(
                json.loads(path.read_text())["completion_run"], "completion-run.json"
            )
            self.assertEqual((root / "run.json").read_bytes(), run_bytes)
            self.assertEqual((root / "manifest.json").read_bytes(), manifest_bytes)
            self.assertEqual(
                (root / "completion-run.json").read_bytes(), completion.read_bytes()
            )


class DurableOriginals(unittest.TestCase):
    def retrieve(self, repository, endpoint, destination, binary=False):
        self.assertEqual(repository, "elong0527/yamaa")
        self.assertTrue(binary)
        destination.write_bytes(
            {"releases/assets/11": b"abc", "releases/assets/12": b"de"}[endpoint]
        )

    def test_durable_original_segments_reassemble_exact_bytes_after_actions_expiry(
        self,
    ):
        release, receipt = retained_parts()
        with tempfile.TemporaryDirectory() as temporary:
            original = Path(temporary) / "tuple.zip"
            with mock.patch.object(finalize, "download", side_effect=self.retrieve):
                finalize.retained_zip(
                    identity(), release, receipt, 42, "tuple", original
                )
            self.assertEqual(original.read_bytes(), b"abcde")
            self.assertEqual(list(original.parent.iterdir()), [original])

    def test_missing_wrong_attempt_reordered_and_unverified_segments_are_refused(self):
        release, receipt = retained_parts()
        variants = [
            {"assets": receipt["assets"][:1]},
            {
                "assets": [
                    dict(receipt["assets"][0], original_run_attempt=1),
                    receipt["assets"][1],
                ]
            },
            {
                "assets": [
                    receipt["assets"][0],
                    dict(receipt["assets"][1], byte_offset=4),
                ]
            },
            {
                "assets": [
                    dict(receipt["assets"][0], bytes_equal=False),
                    receipt["assets"][1],
                ]
            },
        ]
        for variant in variants:
            with (
                self.subTest(variant=variant),
                tempfile.TemporaryDirectory() as temporary,
                mock.patch.object(finalize, "download", side_effect=self.retrieve),
                self.assertRaises(ValueError),
            ):
                finalize.retained_zip(
                    identity(),
                    release,
                    variant,
                    42,
                    "tuple",
                    Path(temporary) / "tuple.zip",
                )

    def test_truncated_retrieval_is_preserved_separately_from_immutable_durable_assets(
        self,
    ):
        release, receipt = retained_parts()
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            manifest_root = root / "input"
            manifest_root.mkdir()
            entry = {
                "artifact_id": 42,
                "artifact_name": "tuple",
                "archive": "tuple.zip",
            }

            def truncated(repository, endpoint, destination, binary=False):
                destination.write_bytes(b"a")

            provider = finalize.durable_provider(identity(), release, receipt)
            with (
                mock.patch.object(finalize, "download", side_effect=truncated),
                self.assertRaisesRegex(ValueError, "truncated"),
                provider(entry, manifest_root),
            ):
                self.fail("truncated original must not reach independent extraction")
            self.assertEqual(
                (root / "refused-artifact-42/tuple.zip.part-0").read_bytes(), b"a"
            )
            self.assertEqual(receipt, retained_parts()[1])

    def test_auditor_content_refusal_does_not_accumulate_validated_original_zip_copies(
        self,
    ):
        release, receipt = retained_parts()
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            manifest_root = root / "input"
            manifest_root.mkdir()
            entry = {
                "artifact_id": 42,
                "artifact_name": "tuple",
                "archive": "tuple.zip",
            }
            provider = finalize.durable_provider(identity(), release, receipt)
            with (
                mock.patch.object(finalize, "download", side_effect=self.retrieve),
                self.assertRaisesRegex(ValueError, "raw content refused"),
                provider(entry, manifest_root) as downloaded,
            ):
                self.assertEqual((downloaded / "tuple.zip").read_bytes(), b"abcde")
                raise ValueError("raw content refused by independent auditor")
            self.assertFalse(downloaded.exists())
            self.assertEqual(list(root.iterdir()), [manifest_root])
            self.assertEqual(receipt, retained_parts()[1])

    def test_index_only_extracts_expected_original_data_and_refuses_archive_code_and_links(
        self,
    ):
        for unexpected in ("../run.json", "audit.py", "nested/run.json", "run.json"):
            with (
                self.subTest(unexpected=unexpected),
                tempfile.TemporaryDirectory() as temporary,
            ):
                root = Path(temporary)
                archive_path, output = root / "index.zip", root / "output"
                output.mkdir()
                with zipfile.ZipFile(archive_path, "w") as archive:
                    for name in finalize.INDEX_FILES:
                        if unexpected != "run.json" or name != "run.json":
                            archive.writestr(name, b"original data")
                    if unexpected == "run.json":
                        info = zipfile.ZipInfo("run.json")
                        info.external_attr = 0o120777 << 16
                        archive.writestr(info, b"run.json")
                    else:
                        archive.writestr(unexpected, b"untrusted")
                with self.assertRaisesRegex(ValueError, "unsafe"):
                    finalize.extract_index(archive_path, output)
                self.assertFalse((root / "run.json").exists())

    def test_authored_nul_index_member_cannot_be_truncated_into_expected_filename(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            archive_path, output = root / "index.zip", root / "output"
            output.mkdir()
            with zipfile.ZipFile(archive_path, "w") as archive:
                for name in finalize.INDEX_FILES - {"run.json"}:
                    archive.writestr(name, b"original data")
                archive.writestr("run.jsonX", b"original data")
            # The original ZIP bytes contain the NUL; creating ZipInfo with a
            # NUL directly would silently truncate it before writing the fixture.
            archive_path.write_bytes(
                archive_path.read_bytes().replace(b"run.jsonX", b"run.json\0")
            )
            with zipfile.ZipFile(archive_path) as archive:
                member = archive.infolist()[-1]
                self.assertEqual(member.filename, "run.json")
                self.assertEqual(member.orig_filename, "run.json\0")
            with self.assertRaisesRegex(ValueError, "unsafe"):
                finalize.extract_index(archive_path, output)
            self.assertFalse((output / "run.json").exists())


if __name__ == "__main__":
    unittest.main()
