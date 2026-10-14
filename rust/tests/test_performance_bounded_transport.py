"""Refusal controls for immutable reader metadata and bounded byte transport."""

import copy
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
            release = {"id": 789, "assets": []}
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

    def changed_original_asset_refuses(self, mode):
        """Synthetic transport fixture; raw/numeric admission is separate."""
        import aggregate_public_performance as independent

        for replacement in (None, 999):
            with self.subTest(mode=mode, replacement=replacement), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                prepared = root / "prepared"
                (prepared / "input").mkdir(parents=True)
                (prepared / "catalog").mkdir()
                (prepared / "input/completion-run.json").write_text("{}")
                (prepared / "catalog/performance-replay-catalog.json").write_text('{"units":[]}')
                (prepared / "preparation.json").write_text('{"accepted":false}')
                identity = {"repository": "elong0527/yamaa", "source_revision": "a" * 40,
                            "run_id": 123, "run_attempt": 1}
                reader = {"run_id": 456, "run_attempt": 2, "revision": "b" * 40}
                record = {"original": identity, "reader": reader if mode == "budgets" else None,
                          "release_label": "fixture-source-label", "current_revision": "c" * 40,
                          "current_run_id": 789, "current_run_attempt": 3, "mode": mode}
                storage_name = transport.reader_prefix(reader) + "-metadata-storage.json"
                storage = {"protocol": "public-performance-reader-metadata-storage/1",
                           "original": identity, "reader": reader,
                           "archive_name": transport.reader_prefix(reader) + "-metadata.zip", "assets": []}
                storage_bytes = json.dumps(storage).encode()
                initial = {"id": 111, "tag_name": record["release_label"],
                           "target_commitish": identity["source_revision"], "draft": True,
                           "published_at": None,
                           "assets": [{"id": 11, "name": "original-source.zip", "size": 3, "state": "uploaded"},
                                      {"id": 12, "name": storage_name, "size": len(storage_bytes), "state": "uploaded"}]}
                server = copy.deepcopy(initial)
                output = root / "output"
                budget = root / "budget.json"
                budget.write_text("{}")
                args = types.SimpleNamespace(input=prepared, output=output, budgets=budget)

                def api(endpoint, server=server):
                    return copy.deepcopy(server) if "/releases/" in endpoint else {}

                def retained(*arguments, server=server, replacement=replacement):
                    if replacement is None:
                        server["assets"] = [asset for asset in server["assets"] if asset["id"] != 11]
                    else:
                        server["assets"][0]["id"] = replacement
                    return copy.deepcopy(server), []

                def reader_snapshot(destination, *_args):
                    for name, value in (("reader-run.json", {}), ("reader-jobs.json", []),
                                        ("reader-artifacts.json", [])):
                        (destination / name).write_text(json.dumps(value))

                def metadata(_archive, destination, identity=identity, reader=reader):
                    (destination / "reader-retention.json").write_text(json.dumps({"original": identity, "reader": reader}))
                    (destination / "aggregate-input.json").write_text("{}")

                def audit(arguments, **_kwargs):
                    destination = Path(arguments[arguments.index("--output") + 1])
                    destination.mkdir()
                    (destination / "performance-audit.json").write_text(json.dumps({"accepted": mode == "budgets"}))
                    (destination / "performance-audit.md").write_text("synthetic transport fixture")
                    return 0

                with (patch.object(transport, "prepared", return_value=(record, copy.deepcopy(initial), {})),
                      patch.object(transport, "api", side_effect=api),
                      patch.object(transport, "retain_file", side_effect=retained),
                      patch.object(transport, "upload", return_value=(copy.deepcopy(server), {})),
                      patch.object(transport, "snapshot_reader", side_effect=reader_snapshot),
                      patch.object(transport, "download", side_effect=lambda _r, _e, path, _fixture_bytes=storage_bytes, **_kw: path.write_bytes(_fixture_bytes)),
                      patch.object(transport, "reassemble", side_effect=lambda _r, _repo, _release, path: path.write_bytes(b"fixture")),
                      patch.object(transport, "safe_metadata", side_effect=metadata),
                      patch.object(transport.subprocess, "check_output", return_value=b""),
                      patch.object(independent, "validate_reader_run"),
                      patch.object(independent, "main", side_effect=audit)):
                    if mode == "refusal":
                        output.mkdir()
                        (output / "replay-refusal.json").write_text('{"accepted":false}')
                        action = transport.retain_refusal
                    else:
                        action = transport.aggregate
                    with self.assertRaisesRegex(ValueError, "original unpublished evidence asset identities changed"):
                        action(args)
                self.assertEqual(initial["assets"][0]["id"], 11)

    def test_collection_retention_refuses_original_asset_deletion_and_replacement_during_upload(self):
        self.changed_original_asset_refuses("units")

    def test_numeric_retention_refuses_original_asset_deletion_and_replacement_during_upload(self):
        self.changed_original_asset_refuses("budgets")

    def test_refusal_retention_refuses_original_asset_deletion_and_replacement_during_upload(self):
        self.changed_original_asset_refuses("refusal")
