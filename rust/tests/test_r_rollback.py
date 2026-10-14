"""A matching package version alone cannot qualify a rollback artifact."""

import importlib.util
import io
import json
import tarfile
import tempfile
import unittest
from unittest import mock
from pathlib import Path

spec = importlib.util.spec_from_file_location(
    "prepare_r_rollback",
    Path(__file__).resolve().parents[1] / "tools/prepare_r_rollback.py",
)
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


class RetainedSourceIdentity(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.directory = Path(temporary.name)
        self.source = self.directory / "download"
        self.proof = self.directory / "proof"
        self.source.mkdir()
        self.proof.mkdir()
        self.name = "native-r-ubuntu-24.04"
        self.artifact_id = module.ARTIFACTS[self.name]
        self.run = {
            "id": module.PRIOR_RUN,
            "head_sha": module.PRIOR_SOURCE,
            "run_attempt": 1,
            "status": "completed",
            "conclusion": "success",
            "html_url": "https://github.com/elong0527/yamaa/actions/runs/38394553441",
        }
        self.artifact = {
            "id": self.artifact_id,
            "name": self.name,
            "expired": False,
            "expires_at": "2027-01-11T12:03:42Z",
            "workflow_run": {"id": module.PRIOR_RUN, "head_sha": module.PRIOR_SOURCE},
        }
        nested = self.source / self.name
        nested.mkdir()
        self.archive = nested / "yamaa_0.3.0.tar.gz"
        with tarfile.open(self.archive, "w:gz") as archive:
            for name, data in {
                "yamaa/DESCRIPTION": b"Package: yamaa\nVersion: 0.3.0\n",
                "yamaa/src/rust/Cargo.lock": b"locked source",
                "yamaa/src/rust/Cargo.toml": b"workspace",
                "yamaa/src/rust/rust-toolchain.toml": b"toolchain",
            }.items():
                member = tarfile.TarInfo(name)
                member.size = len(data)
                archive.addfile(member, io.BytesIO(data))

    def prepare(self):
        (self.proof / "prior-run.json").write_text(json.dumps(self.run))
        (self.proof / "prior-artifact.json").write_text(json.dumps(self.artifact))
        module.prepare(self.source, self.proof, self.name, self.artifact_id)

    def test_retains_exact_bytes_and_never_claims_a_published_release(self):
        self.prepare()
        self.assertEqual(
            (self.source / "yamaa_0.3.0.tar.gz").read_bytes(), self.archive.read_bytes()
        )
        receipt = json.loads((self.proof / "artifact-receipt.json").read_text())
        self.assertEqual(receipt["source_revision"], module.PRIOR_SOURCE)
        self.assertEqual(receipt["rehearsal"], "pending")
        self.assertIn("not a published release", receipt["status"])

    def test_correct_version_from_another_source_is_refused_before_install(self):
        self.artifact["workflow_run"]["head_sha"] = "other-source"
        with self.assertRaisesRegex(ValueError, "identity does not match"):
            self.prepare()
        self.assertFalse((self.proof / "artifact-receipt.json").exists())

    def test_windows_description_is_parsed_without_changing_source_bytes(self):
        with tarfile.open(self.archive, "r:gz") as source:
            members = [(member, source.extractfile(member).read()) for member in source]
        with tarfile.open(self.archive, "w:gz") as destination:
            for member, data in members:
                if member.name == "yamaa/DESCRIPTION":
                    data = data.replace(b"\n", b"\r\n")
                    member.size = len(data)
                destination.addfile(member, io.BytesIO(data))
        unchanged = self.archive.read_bytes()
        self.prepare()
        self.assertEqual((self.source / "yamaa_0.3.0.tar.gz").read_bytes(), unchanged)

    def test_expired_or_failed_qualification_does_not_become_rollback_evidence(self):
        for expired, conclusion in ((True, "success"), (False, "failure")):
            with self.subTest(expired=expired, conclusion=conclusion):
                self.artifact["expired"] = expired
                self.run["conclusion"] = conclusion
                with self.assertRaisesRegex(ValueError, "identity does not match"):
                    self.prepare()
                self.assertFalse((self.proof / "artifact-receipt.json").exists())

    def test_durable_draft_asset_never_depends_on_expiring_actions_artifact(self):
        release = {
            "id": module.RETAINED_RELEASE, "tag_name": module.RETAINED_TAG,
            "target_commitish": module.PRIOR_SOURCE, "draft": True,
            "assets": [{"id": module.RETAINED_ASSET, "name": "yamaa_0.3.0.tar.gz",
                        "size": self.archive.stat().st_size, "state": "uploaded",
                        "url": "https://api.github.com/repos/elong0527/yamaa/releases/assets/637944247"}],
        }
        destination, proof = self.directory / "durable-source", self.directory / "durable-proof"
        with mock.patch.object(module, "RETAINED_BYTES", self.archive.stat().st_size), mock.patch.object(
            module, "github", side_effect=[release, self.archive.read_bytes()]
        ) as api:
            module.retained(destination, proof)
        self.assertEqual(api.call_args_list, [mock.call("releases/411979803"),
                                              mock.call("releases/assets/637944247", binary=True)])
        self.assertEqual((destination / "yamaa_0.3.0.tar.gz").read_bytes(), self.archive.read_bytes())
        receipt = json.loads((proof / "artifact-receipt.json").read_text())
        self.assertTrue(receipt["draft"])
        self.assertNotIn("artifact_expires_at", receipt)
        self.assertEqual(receipt["rehearsal"], "pending")

    def test_published_wrong_target_or_replaced_durable_asset_is_refused(self):
        release = {
            "id": module.RETAINED_RELEASE, "tag_name": module.RETAINED_TAG,
            "target_commitish": module.PRIOR_SOURCE, "draft": True,
            "assets": [{"id": module.RETAINED_ASSET, "name": "yamaa_0.3.0.tar.gz",
                        "size": module.RETAINED_BYTES, "state": "uploaded"}],
        }
        for key, value in (("draft", False), ("target_commitish", "other-source")):
            with self.subTest(key=key), mock.patch.object(module, "github", return_value={**release, key: value}):
                with self.assertRaisesRegex(ValueError, "release/source identity"):
                    module.retained(self.directory / "refused-source", self.directory / "refused-proof")
        release["assets"][0]["id"] += 1
        with mock.patch.object(module, "github", return_value=release):
            with self.assertRaisesRegex(ValueError, "asset identity"):
                module.retained(self.directory / "refused-source", self.directory / "refused-proof")


if __name__ == "__main__":
    unittest.main()
