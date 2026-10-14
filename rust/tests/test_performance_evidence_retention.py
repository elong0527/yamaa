"""Performance evidence survives missing tuples without accepting or replacing it."""

import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest import mock

TOOLS = Path(__file__).resolve().parents[1] / "tools"
sys.path.insert(0, str(TOOLS))
import index_public_performance as index
import replay_public_performance as replay
import retain_public_performance as retention


def selected_run(revision="a" * 40):
    return {
        "id": 123,
        "repository": {"full_name": "elong0527/yamaa"},
        "head_repository": {"full_name": "elong0527/yamaa"},
        "head_sha": revision,
        "run_attempt": 1,
        "path": ".github/workflows/performance.yml",
        "event": "push",
    }


class RetainedIndex(unittest.TestCase):
    def test_r_variant_segments_keep_twenty_one_logical_tuples_and_all_approved_cases(
        self,
    ):
        artifacts, jobs = [], []
        for lane, name in index.tuples():
            variants = index.R_VARIANTS if lane["host"] == "r" else (None,)
            for variant in variants:
                artifact_name = f"{name}-{variant}" if variant else name
                artifacts.append(
                    {
                        "id": len(artifacts) + 1,
                        "name": artifact_name,
                        "created_at": "2026-10-14T00:00:10Z",
                    }
                )
                jobs.append(
                    {
                        "name": index.producer_job(lane, variant),
                        "run_attempt": 1,
                        "status": "completed",
                        "started_at": "2026-10-14T00:00:00Z",
                        "completed_at": "2026-10-14T00:01:00Z",
                    }
                )
        result = index.manifest(
            selected_run(),
            artifacts,
            "elong0527/yamaa",
            "a" * 40,
            1,
            jobs=jobs,
            r_variants=True,
        )
        self.assertEqual(len(result["entries"]), 21)
        self.assertEqual(len(artifacts), 45)
        for entry in result["entries"]:
            if entry["tuple"]["host"] == "python":
                self.assertNotIn("segments", entry)
                self.assertIsNotNone(entry["artifact_id"])
                continue
            self.assertEqual(set(entry), {"tuple", "segments"})
            segments = entry["segments"]
            self.assertEqual([s["variant"] for s in segments], list(index.R_VARIANTS))
            cases = [case for segment in segments for case in segment["cases"]]
            self.assertEqual(len(cases), 27)
            self.assertEqual(len(set(cases)), 27)
            self.assertEqual(
                sum(
                    len(s["cases"])
                    for s in segments
                    if any(c["role"] == "baseline" for c in s["collections"])
                ),
                18,
            )
            self.assertEqual(
                sum(
                    len(s["cases"])
                    for s in segments
                    if any(
                        c["role"] == "producer-root-build-baseline"
                        for c in s["collections"]
                    )
                ),
                6,
            )
            self.assertTrue(all(s["artifact_id"] is not None for s in segments))
            callback = next(s for s in segments if s["variant"] == "callback-1")
            self.assertEqual(
                [c["role"] for c in callback["collections"]], ["candidate"]
            )

    def test_missing_wrong_segment_job_never_shrinks_the_r_tuple_or_uses_another_variant(
        self,
    ):
        lane = {"host": "r", "os": "ubuntu-24.04", "python": None, "form": "source"}
        name = "performance-r-ubuntu-24.04-source-window-10"
        artifact = {"id": 42, "name": name, "created_at": "2026-10-14T00:00:10Z"}
        job = {
            "name": index.producer_job(lane, "window-100"),
            "run_attempt": 1,
            "status": "completed",
            "started_at": "2026-10-14T00:00:00Z",
            "completed_at": "2026-10-14T00:01:00Z",
        }
        result = index.manifest(
            selected_run(),
            [artifact],
            "elong0527/yamaa",
            "a" * 40,
            1,
            jobs=[job],
            r_variants=True,
        )
        entry = next(e for e in result["entries"] if e["tuple"] == lane)
        self.assertEqual(len(entry["segments"]), 9)
        self.assertTrue(all(s["artifact_id"] is None for s in entry["segments"]))
        with self.assertRaisesRegex(ValueError, "unexpected R performance variant"):
            index.variant_cases("window-999")

    def test_rerun_does_not_select_a_unique_artifact_from_an_earlier_attempt(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            checkout = root / "checkout"
            checkout.mkdir()

            def git(*arguments):
                return subprocess.check_output(
                    ["git", *arguments], cwd=checkout, text=True
                ).strip()

            git("init", "-q")
            git("config", "user.name", "Fixture")
            git("config", "user.email", "fixture@example.invalid")
            (checkout / "truth").write_bytes(b"independent prior source")
            git("add", "truth")
            git("commit", "-qm", "baseline")
            git("tag", "yamaa-py-0.3.0")
            revision = git("rev-parse", "HEAD")
            args = SimpleNamespace(
                input=root / "input",
                checkout=checkout,
                repository="elong0527/yamaa",
                revision=revision,
                run_id=123,
                attempt=2,
                baseline_ref="yamaa-py-0.3.0",
                native_run_id="",
                native_attempt="",
            )
            run = dict(selected_run(revision), run_attempt=2)
            artifact = {
                "id": 77,
                "name": "performance-python-ubuntu-24.04-3.12-wheel",
                "created_at": "2026-10-14T00:00:10Z",
            }
            job = {
                "id": 88,
                "name": "performance-python (ubuntu-24.04, 3.12, wheel)",
                "run_attempt": 2,
                "status": "completed",
                "started_at": "2026-10-14T00:02:00Z",
                "completed_at": "2026-10-14T00:03:00Z",
            }
            with mock.patch.object(
                index,
                "api",
                side_effect=[run, [{"artifacts": [artifact]}], [{"jobs": [job]}]],
            ):
                value = index.index(args)
            self.assertEqual(len(value["entries"]), 21)
            self.assertIsNone(value["entries"][0]["artifact_id"])
            self.assertEqual(value["entries"][0]["run_attempt"], 2)
            self.assertEqual(
                json.loads((args.input / "artifacts.json").read_text()), [artifact]
            )
            self.assertEqual(json.loads((args.input / "jobs.json").read_text()), [job])

    def test_native_snapshot_reads_selected_attempt_and_preserves_original_artifact_ids(
        self,
    ):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            native_run = {"id": 11, "run_attempt": 2, "head_sha": "a" * 40}
            native_jobs = [{"id": 12, "run_attempt": 2}]
            native_artifacts = [
                {
                    "id": 13,
                    "name": "native-python-ubuntu-24.04-3.14",
                    "digest": "unneeded",
                    "created_at": "2026-10-14T01:00:00Z",
                }
            ]
            with mock.patch.object(
                index,
                "api",
                side_effect=[
                    native_run,
                    [{"jobs": native_jobs}],
                    [{"artifacts": native_artifacts}],
                ],
            ) as api:
                index.native_snapshot(root, "elong0527/yamaa", "11", "2")
            self.assertEqual(
                api.call_args_list[0].args,
                ("repos/elong0527/yamaa/actions/runs/11/attempts/2",),
            )
            self.assertIn("attempts/2/jobs", api.call_args_list[1].args[0])
            retained = json.loads((root / "native-artifacts.json").read_text())
            self.assertEqual(retained[0]["id"], 13)
            self.assertNotIn("digest", retained[0])
            self.assertEqual(
                json.loads((root / "native-run.json").read_text()), native_run
            )

    def test_missing_and_ambiguous_artifacts_keep_all_twenty_one_tuples_visible(self):
        lanes = list(index.tuples())
        artifacts = [
            {"id": number + 1, "name": name, "created_at": "2026-10-14T00:00:10Z"}
            for number, (_, name) in enumerate(lanes[1:])
        ]
        artifacts.append(dict(artifacts[0], id=999))
        jobs = [
            {
                "name": (
                    f"performance-python ({item['os']}, {item['python']}, {item['form']})"
                    if item["host"] == "python"
                    else f"performance-r ({item['os']}, source)"
                ),
                "run_attempt": 1,
                "status": "completed",
                "started_at": "2026-10-14T00:00:00Z",
                "completed_at": "2026-10-14T00:01:00Z",
            }
            for item, _ in lanes
        ]
        result = index.manifest(
            selected_run(), artifacts, "elong0527/yamaa", "a" * 40, 1, jobs=jobs
        )
        self.assertEqual(len(result["entries"]), 21)
        self.assertIsNone(result["entries"][0]["artifact_id"])
        self.assertIsNone(result["entries"][1]["artifact_id"])
        self.assertTrue(
            all(entry["artifact_id"] is not None for entry in result["entries"][2:])
        )
        self.assertTrue(
            all(len(entry["collections"]) == 3 for entry in result["entries"])
        )
        self.assertEqual(
            sum(entry["tuple"]["host"] == "r" for entry in result["entries"]), 3
        )

    def test_different_source_or_attempt_never_becomes_the_original_run(self):
        for field, value in (
            ("head_sha", "b" * 40),
            ("run_attempt", 2),
            ("event", "pull_request"),
            ("event", "workflow_call"),
            ("path", ".github/workflows/native.yml"),
        ):
            run = selected_run()
            run[field] = value
            with (
                self.subTest(field=field),
                self.assertRaisesRegex(ValueError, "exact source/attempt"),
            ):
                index.manifest(run, [], "elong0527/yamaa", "a" * 40, 1, jobs=[])

    def test_wrong_ambiguous_or_unfinished_job_never_labels_artifact_with_selected_attempt(
        self,
    ):
        item = {"host": "r", "os": "ubuntu-24.04", "python": None, "form": "source"}
        artifact = {
            "id": 77,
            "name": "performance-r-ubuntu-24.04-source",
            "created_at": "2026-10-14T00:00:10Z",
        }
        job = {
            "name": "performance-r (ubuntu-24.04, source)",
            "run_attempt": 2,
            "status": "completed",
            "started_at": "2026-10-14T00:00:00Z",
            "completed_at": "2026-10-14T00:01:00Z",
        }
        self.assertTrue(index.artifact_matches_attempt(artifact, [job], item, 2))
        self.assertTrue(
            index.artifact_matches_attempt(
                artifact, [dict(job, conclusion="failure")], item, 2
            )
        )
        cases = [
            [],
            [job, dict(job)],
            [dict(job, run_attempt=1)],
            [dict(job, status="in_progress")],
            [dict(job, name="unrelated")],
            [dict(job, completed_at=None)],
            [dict(job, started_at="malformed")],
            [dict(job, started_at="2026-10-14T00:02:00Z")],
        ]
        for jobs in cases:
            with self.subTest(jobs=jobs):
                self.assertFalse(
                    index.artifact_matches_attempt(artifact, jobs, item, 2)
                )
        for value in (None, 0, "malformed", "2026-10-14T00:00:10"):
            with self.subTest(created_at=value):
                self.assertFalse(
                    index.artifact_matches_attempt(
                        dict(artifact, created_at=value), [job], item, 2
                    )
                )

    def test_api_metadata_discards_digests_without_rewriting_other_observations(self):
        original = [
            {
                "id": 1,
                "digest": "irrelevant",
                "workflow_run": {"head_sha": "a" * 40},
                "expired": True,
                "failure": "original failure",
            }
        ]
        retained = index.without_digests(original)
        self.assertNotIn("digest", retained[0])
        self.assertEqual(retained[0]["failure"], "original failure")
        self.assertTrue(retained[0]["expired"])
        self.assertIn("digest", original[0])

    def test_source_bundle_replays_candidate_and_baseline_with_no_checkout(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            checkout = root / "checkout"
            checkout.mkdir()

            def git(*arguments):
                return subprocess.check_output(
                    ["git", *arguments], cwd=checkout, text=True
                ).strip()

            git("init", "-q")
            git("config", "user.name", "Fixture")
            git("config", "user.email", "fixture@example.invalid")
            (checkout / "truth").write_bytes(b"previous")
            git("add", "truth")
            git("commit", "-qm", "baseline")
            baseline = git("rev-parse", "HEAD")
            git("tag", "yamaa-py-0.3.0")
            (checkout / "truth").write_bytes(b"candidate")
            git("commit", "-qam", "candidate")
            revision = git("rev-parse", "HEAD")
            args = SimpleNamespace(
                input=root / "input",
                checkout=checkout,
                repository="elong0527/yamaa",
                revision=revision,
                run_id=123,
                attempt=1,
                baseline_ref="yamaa-py-0.3.0",
                native_run_id="",
                native_attempt="",
            )
            with mock.patch.object(
                index,
                "api",
                side_effect=[
                    selected_run(revision),
                    [{"artifacts": []}],
                    [{"jobs": []}],
                ],
            ):
                index.index(args)
            clone = root / "offline.git"
            subprocess.run(
                [
                    "git",
                    "clone",
                    "--bare",
                    str(args.input / "source.bundle"),
                    str(clone),
                ],
                check=True,
                capture_output=True,
            )
            for commit, expected in ((baseline, b"previous"), (revision, b"candidate")):
                self.assertEqual(
                    subprocess.check_output(
                        ["git", "show", f"{commit}:truth"], cwd=clone
                    ),
                    expected,
                )
            with (
                mock.patch.object(
                    index,
                    "api",
                    side_effect=[
                        selected_run(revision),
                        [{"artifacts": []}],
                        [{"jobs": []}],
                    ],
                ),
                self.assertRaisesRegex(ValueError, "replace an original"),
            ):
                index.index(args)


class UnpublishedRetention(unittest.TestCase):
    def test_segmented_r_artifact_keeps_its_exact_variant_producing_job(self):
        name = "performance-r-windows-latest-source-producer-3"
        self.assertEqual(
            retention.producer_name(name),
            "performance-r (windows-latest, source, producer-3)",
        )
        artifact = {"name": name, "created_at": "2026-10-14T01:00:00Z"}
        job = {
            "name": retention.producer_name(name),
            "started_at": "2026-10-14T00:00:00Z",
            "completed_at": "2026-10-14T01:01:00Z",
            "run_attempt": 2,
        }
        self.assertEqual(retention.original_attempt(artifact, [job]), 2)
        self.assertIsNone(
            retention.original_attempt(
                artifact,
                [dict(job, name="performance-r (windows-latest, source, producer-1)")],
            )
        )
        self.assertIsNone(
            retention.producer_name("performance-r-windows-latest-source-producer-999")
        )

    def test_truncated_download_is_refused_with_original_partial_bytes_preserved(self):
        with tempfile.TemporaryDirectory() as temporary:
            original = Path(temporary) / "original.zip"
            original.write_bytes(b"truncated")
            with self.assertRaisesRegex(ValueError, "truncated"):
                retention.verify_original_download(original, {"size_in_bytes": 100})
            self.assertEqual(original.read_bytes(), b"truncated")
            retention.verify_original_download(
                original, {"size_in_bytes": len(b"truncated")}
            )

    def test_streaming_adapter_uses_original_id_and_removes_only_its_temporary_copy(
        self,
    ):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "manifest.json").write_text('{"repository":"elong0527/yamaa"}')
            entry = {"artifact_id": 42, "archive": "tuple.zip"}

            def retrieve(repository, endpoint, destination, binary=False):
                self.assertEqual(
                    (repository, endpoint),
                    ("elong0527/yamaa", "actions/artifacts/42/zip"),
                )
                destination.write_bytes(b"original ZIP bytes")

            with mock.patch.object(replay, "download", side_effect=retrieve):
                with replay.original_archive(entry, root) as artifact_root:
                    self.assertEqual(
                        (artifact_root / "tuple.zip").read_bytes(),
                        b"original ZIP bytes",
                    )
                self.assertFalse(artifact_root.exists())
            self.assertTrue((root / "manifest.json").exists())
            with mock.patch.object(replay, "download") as download:
                with replay.original_archive(
                    dict(entry, artifact_id=None), root
                ) as artifact_root:
                    self.assertEqual(list(artifact_root.iterdir()), [])
                download.assert_not_called()

    def test_large_zip_segments_reconstruct_the_exact_original_bytes(self):
        with tempfile.TemporaryDirectory() as temporary:
            original = Path(temporary) / "original.zip"
            content = bytes(range(255))
            original.write_bytes(content)
            parts = list(retention.archive_segments(original, maximum=80))
            self.assertEqual([offset for _, offset, _ in parts], [0, 80, 160, 240])
            self.assertTrue(all(count == 4 for _, _, count in parts))
            self.assertEqual(
                b"".join(path.read_bytes() for path, _, _ in parts), content
            )
            self.assertEqual(original.read_bytes(), content)

    def test_old_artifacts_keep_their_actual_producing_attempt(self):
        artifact = {
            "name": "performance-python-ubuntu-24.04-3.12-wheel",
            "created_at": "2026-10-14T01:00:00Z",
        }
        job = {
            "name": "performance-python (ubuntu-24.04, 3.12, wheel)",
            "started_at": "2026-10-14T00:00:00Z",
            "completed_at": "2026-10-14T01:01:00Z",
            "run_attempt": 1,
        }
        newer = dict(
            job,
            started_at="2026-10-14T02:00:00Z",
            completed_at="2026-10-14T03:00:00Z",
            run_attempt=2,
        )
        self.assertEqual(retention.original_attempt(artifact, [job, newer]), 1)
        self.assertIsNone(retention.original_attempt(artifact, [newer]))
        self.assertIsNone(retention.original_attempt(artifact, [job, job]))

    def test_other_events_refs_and_sources_have_no_write_access(self):
        trusted = {
            "GITHUB_EVENT_NAME": "push",
            "GITHUB_REF": "refs/heads/qualification/issue-1998",
            "GITHUB_REPOSITORY": "elong0527/yamaa",
            "GITHUB_SHA": "a" * 40,
            "GITHUB_RUN_ID": "123",
            "GITHUB_RUN_ATTEMPT": "1",
        }
        args = SimpleNamespace(
            repository="elong0527/yamaa",
            revision="a" * 40,
            run_id=123,
            attempt=1,
            output=Path("unused"),
        )
        for field, value in (
            ("GITHUB_EVENT_NAME", "pull_request"),
            ("GITHUB_REF", "refs/heads/main"),
            ("GITHUB_SHA", "b" * 40),
            ("GITHUB_RUN_ATTEMPT", "2"),
        ):
            environment = dict(trusted, **{field: value})
            with (
                mock.patch.dict(os.environ, environment, clear=True),
                mock.patch.object(retention, "api") as api,
                self.assertRaisesRegex(ValueError, "original trusted"),
            ):
                retention.retain(args)
            api.assert_not_called()

    def test_published_or_different_draft_is_never_admitted(self):
        original = {
            "tag_name": "evidence",
            "target_commitish": "a" * 40,
            "draft": True,
            "published_at": None,
            "assets": [],
        }
        retention.admit_release(original, "evidence", "a" * 40)
        for field, value in (
            ("draft", False),
            ("published_at", "2026-10-14"),
            ("target_commitish", "b" * 40),
        ):
            release = dict(original, **{field: value})
            with self.assertRaisesRegex(ValueError, "unpublished exact-source"):
                retention.admit_release(release, "evidence", "a" * 40)

    def test_existing_different_bytes_are_rejected_without_an_upload(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            original = root / "raw.zip"
            original.write_bytes(b"original")
            release = {
                "id": 5,
                "tag_name": "evidence",
                "target_commitish": "a" * 40,
                "draft": True,
                "published_at": None,
                "assets": [
                    {"id": 6, "name": "raw.zip", "state": "uploaded", "url": "asset"}
                ],
            }

            def retrieve(repository, endpoint, destination, binary=False):
                destination.write_bytes(b"different")

            with (
                mock.patch.object(retention, "download", side_effect=retrieve),
                mock.patch.object(retention.subprocess, "run") as mutation,
                self.assertRaisesRegex(ValueError, "never overwrite"),
            ):
                retention.upload(
                    "elong0527/yamaa", release, "evidence", "a" * 40, original, root
                )
            mutation.assert_not_called()
            self.assertEqual(original.read_bytes(), b"original")


if __name__ == "__main__":
    unittest.main()
