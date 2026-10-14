"""Refusal controls for immutable reader metadata and bounded byte transport."""

import importlib.util
import json
import sys
import tempfile
import types
import unittest
import zipfile
from pathlib import Path
from unittest.mock import patch

TOOLS = Path(__file__).resolve().parents[1] / "tools"
sys.path.insert(0, str(TOOLS))
SPEC = importlib.util.spec_from_file_location("bounded_transport", TOOLS / "orchestrate_public_performance_replay.py")
transport = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(transport)


class BoundedTransport(unittest.TestCase):
    def test_reader_request_keeps_original_and_current_revisions_separate(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "request.json"
            value = {"protocol": "public-performance-replay/2", "repository": "elong0527/yamaa",
                     "source_revision": "a" * 40, "run_id": 123, "run_attempt": 1,
                     "reader": {"run_id": 456, "run_attempt": 2, "revision": "b" * 40}}
            path.write_text(json.dumps(value))
            self.assertEqual(transport.request(path), value)
            value["reader"]["run_attempt"] = True
            path.write_text(json.dumps(value))
            with self.assertRaises(ValueError):
                transport.request(path)

    def test_segment_reassembly_preserves_original_order_and_partial_refusal(self):
        data = {11: b"first", 12: b"second"}
        release = {"assets": [{"id": identifier, "name": str(identifier),
                               "size": len(value), "state": "uploaded"}
                              for identifier, value in data.items()]}
        rows = [{"asset_id": identifier, "name": str(identifier), "bytes": len(value),
                 "bytes_equal": True, "segment_count": 2, "original_zip_bytes": 11,
                 "byte_offset": 0 if identifier == 11 else 5}
                for identifier, value in data.items()]

        def download(_repository, endpoint, destination, binary):
            destination.write_bytes(data[int(endpoint.rsplit("/", 1)[1])])

        with tempfile.TemporaryDirectory() as directory, patch.object(transport, "download", download):
            path = Path(directory) / "archive.zip"
            transport.reassemble(list(reversed(rows)), "elong0527/yamaa", release, path)
            self.assertEqual(path.read_bytes(), b"firstsecond")
            bad = [dict(row) for row in rows]
            bad[1]["byte_offset"] = 4
            path = Path(directory) / "refused.zip"
            with self.assertRaises(ValueError):
                transport.reassemble(bad, "elong0527/yamaa", release, path)
            self.assertEqual(path.read_bytes(), b"first")

    def test_metadata_does_not_extract_arbitrary_files_or_truncated_proof(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for filename in ("../outside", "original/C:/outside", "audit/execute.py", "reader-run.json"):
                archive = root / "input.zip"
                with zipfile.ZipFile(archive, "w") as output:
                    output.writestr(filename, b"{}")
                target = root / str(len(list(root.iterdir())))
                target.mkdir()
                with self.subTest(filename=filename), self.assertRaises(ValueError):
                    transport.safe_metadata(archive, target)
            self.assertFalse((root / "outside").exists())

    def test_metadata_original_nul_name_cannot_be_truncated_to_whitelisted_proof(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            archive = root / "input.zip"
            authored = b"reader-run.jsonXshadow"
            with zipfile.ZipFile(archive, "w") as output:
                output.writestr(authored.decode(), b"{}")
            archive.write_bytes(archive.read_bytes().replace(authored, b"reader-run.json\0shadow"))
            destination = root / "output"
            destination.mkdir()
            with self.assertRaisesRegex(ValueError, "unsafe original reader metadata"):
                transport.safe_metadata(archive, destination)
            self.assertFalse((destination / "reader-run.json").exists())

    def test_refused_reader_metadata_and_actual_partial_bytes_are_retained_without_acceptance(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            prepared = root / "prepared"
            prepared.mkdir()
            (prepared / "preparation.json").write_bytes(b'{"accepted":false}\n')
            output = root / "output"
            output.mkdir()
            (output / "replay-refusal.json").write_bytes(b'{"accepted":false,"reason":"unit failed"}\n')
            (output / "fragment.zip.part-0").write_bytes(b"\0actual truncated download")
            identity = {"repository": "elong0527/yamaa", "source_revision": "a" * 40,
                        "run_id": 123, "run_attempt": 1}
            record = {"original": identity, "reader": None, "release_label": "evidence-label",
                      "current_revision": "b" * 40, "current_run_id": 456, "current_run_attempt": 2}
            release = {"id": 789}
            asset = {"asset_id": 99, "name": "fixture.zip", "bytes_equal": True}

            def retained(repository, actual_release, label, revision, archive, work):
                self.assertEqual((repository, label, revision),
                                 (identity["repository"], record["release_label"], identity["source_revision"]))
                with zipfile.ZipFile(archive) as original:
                    self.assertEqual(original.read("fragment.zip.part-0"), b"\0actual truncated download")
                    self.assertIn(b"unit failed", original.read("replay-refusal.json"))
                return actual_release, [asset]

            with (patch.object(transport, "prepared", return_value=(record, release, {})),
                  patch.object(transport, "api", return_value=release),
                  patch.object(transport, "admit_release"),
                  patch.object(transport, "retain_file", side_effect=retained),
                  patch.object(transport, "upload") as upload,
                  patch.object(transport, "unchanged_source_assets")):
                transport.retain_refusal(types.SimpleNamespace(input=prepared, output=output))
            receipt = json.loads(upload.call_args.args[4].read_text())
            self.assertFalse(receipt["accepted"])
            self.assertEqual(receipt["assets"], [asset])
            self.assertEqual(receipt["reader"], {"revision": "b" * 40, "run_id": 456, "run_attempt": 2})
            self.assertIn("456-attempt-2-revision-", upload.call_args.args[4].name)
