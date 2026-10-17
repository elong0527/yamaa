"""Installed host resource controls using independently authored synthetic data."""

import tempfile
import unittest
from pathlib import Path

import pyarrow as pa

from yamaa import check, domain
from yamaa.resources import (
    MemoryResolver,
    ResourceFailure,
    ResourceResolver,
    SyntheticRemoteResolver,
)

SPEC = b"""schema_version: '1.0'
domain: TEST
keys: [ID]
input: {RAW: raw.csv}
columns:
  - {name: ID, type: int, label: Identifier, derivation: RAW.ID}
output: {path: out.csv, columns: [ID]}
"""


class HostResources(unittest.TestCase):
    def test_nested_entry_and_absolute_output_retain_logical_locations(self):
        spec = SPEC.replace(b"raw.csv", b"../raw.csv").replace(
            b"out.csv", b"/dest/out.csv"
        )
        resolver = MemoryResolver({"study/spec.yaml": spec, "raw.csv": b"ID\n7\n"})
        result = domain("study/spec.yaml", resolver=resolver)
        self.assertTrue(result.issues.is_empty(), result.issues)
        self.assertTrue(result.save())
        self.assertEqual(resolver.outputs, {"memory:/dest/out.csv": b"ID\n7\n"})

    def test_local_host_callback_uses_explicit_references_and_native_snapshots(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "spec.yaml").write_bytes(SPEC)
            (root / "raw.csv").write_bytes(b"ID\n7\n")
            outputs = MemoryResolver()

            def callback(request):
                if request.kind == "output":
                    return outputs._request(request)
                key = (root / request.written).resolve()
                if not key.is_file():
                    raise ResourceFailure("missing")
                from yamaa.resources import ResourceResult

                return ResourceResult(
                    key.as_uri(),
                    key.read_bytes() if request.operation == "capture" else None,
                )

            result = domain("spec.yaml", resolver=ResourceResolver(callback))
            (root / "raw.csv").unlink()
            self.assertEqual(result.output["ID"].to_list(), [7])
            self.assertTrue(result.save())
            self.assertEqual(outputs.outputs["memory:/out.csv"], b"ID\n7\n")

    def test_put_rejects_unreachable_keys_and_bad_references(self):
        # A non-string or empty key stores an object no lookup can reach,
        # and a bad reference is only discovered at capture time as
        # 'rejected'; both must be refused at put time instead.
        for backend in (MemoryResolver, SyntheticRemoteResolver):
            with self.subTest(backend=backend):
                resolver = backend()
                for key in (123, None, b"raw.csv", 1.5):
                    with self.subTest(key=key), self.assertRaises(TypeError):
                        resolver.put(key, b"ID\n7\n")
                    with (
                        self.subTest(key=key, frame=True),
                        self.assertRaises(TypeError),
                    ):
                        resolver.put_frame(key, object())
                for key in ("", "raw\x00.csv"):
                    with self.subTest(key=key), self.assertRaises(ValueError):
                        resolver.put(key, b"ID\n7\n")
                    with (
                        self.subTest(key=key, frame=True),
                        self.assertRaises(ValueError),
                    ):
                        resolver.put_frame(key, object())
                for reference in (123, b"ref", 1.5):
                    with (
                        self.subTest(reference=reference),
                        self.assertRaises(TypeError),
                    ):
                        resolver.put("raw.csv", b"ID\n7\n", reference=reference)
                for reference in ("", "ref\x00"):
                    with (
                        self.subTest(reference=reference),
                        self.assertRaises(ValueError),
                    ):
                        resolver.put("raw.csv", b"ID\n7\n", reference=reference)
                self.assertEqual(resolver.objects, {})
                with self.assertRaises(TypeError):
                    backend({123: b"ID\n7\n"})
                # Valid keys and references still work.
                resolver.put("raw.csv", b"ID\n7\n", reference="memory:/raw.csv#v1")
                self.assertEqual(
                    resolver.objects["raw.csv"].reference, "memory:/raw.csv#v1"
                )

    def test_put_frame_rejects_none_frame(self):
        # A None frame stores a ResourceResult with neither data nor
        # frame; capture then fails 'rejected', misleadingly blaming the
        # specification or session instead of this call. Refuse it at
        # put time. Frames are otherwise opaque host objects.
        for backend in (MemoryResolver, SyntheticRemoteResolver):
            with self.subTest(backend=backend):
                resolver = backend()
                with self.assertRaises(TypeError):
                    resolver.put_frame("raw.csv", None)
                self.assertEqual(resolver.objects, {})
                resolver.put_frame("raw.csv", object())
                self.assertIn("raw.csv", resolver.objects)

    def test_frame_exporter_interrupt_preserves_original_object(self):
        interruption = KeyboardInterrupt("frame exporter interrupted")

        class Frame:
            def __arrow_c_stream__(self):
                raise interruption

        resolver = MemoryResolver({"spec.yaml": SPEC})
        resolver.put_frame("raw.csv", Frame())
        with self.assertRaises(KeyboardInterrupt) as raised:
            domain("spec.yaml", resolver=resolver)
        self.assertIs(raised.exception, interruption)

    def test_memory_and_synthetic_remote_publish_held_bytes_without_reopening_inputs(
        self,
    ):
        for backend in (MemoryResolver, SyntheticRemoteResolver):
            with self.subTest(backend=backend):
                resolver = backend({"spec.yaml": SPEC, "raw.csv": b"ID\n7\n"})
                result = domain("spec.yaml", resolver=resolver)
                self.assertTrue(result.issues.is_empty(), result.issues)
                self.assertEqual(result.output["ID"].to_list(), [7])
                records = result.resource_provenance
                self.assertEqual(
                    [(r.declaring, r.written) for r in records],
                    [("/", "spec.yaml"), ("/spec.yaml", "raw.csv")],
                )
                self.assertTrue(
                    all(r.reference.startswith(resolver.prefix) for r in records)
                )
                resolver.objects.clear()
                self.assertTrue(result.save())
                self.assertEqual(
                    resolver.outputs, {resolver.prefix + "out.csv": b"ID\n7\n"}
                )
                self.assertEqual(resolver.staging, {})

    def test_owned_arrow_frame_uses_native_types_nulls_ordinal_and_retains_buffers(
        self,
    ):
        resolver = MemoryResolver(
            {
                "spec.yaml": SPEC.replace(
                    b"RAW: raw.csv", b"RAW: {path: raw.csv, ordinal: POSITION}"
                )
            }
        )
        frame = pa.table({"ID": pa.array([7, 8], type=pa.int64())})
        resolver.put_frame("raw.csv", frame)
        result = domain("spec.yaml", resolver=resolver)
        del frame
        resolver.objects.clear()
        self.assertTrue(result.issues.is_empty(), result.issues)
        self.assertEqual(result.output["ID"].to_list(), [7, 8])
        self.assertEqual(result.resource_provenance[-1].representation, "frame")
        self.assertTrue(result.save())
        self.assertEqual(resolver.outputs["memory:/out.csv"], b"ID\n7\n8\n")

    def test_missing_is_structured_and_check_does_not_capture_study_data(self):
        backend = MemoryResolver({"spec.yaml": SPEC})
        requests = []

        def callback(request):
            requests.append(request)
            return backend._request(request)

        resolver = ResourceResolver(callback)
        checked = check("spec.yaml", resolver=resolver)
        self.assertTrue(checked.issues.is_empty(), checked.issues)
        self.assertFalse(
            any(r.operation == "capture" and r.kind == "input" for r in requests)
        )
        result = domain("spec.yaml", resolver=resolver)
        self.assertIsNone(result.output)
        self.assertIn("resource_path_missing", result.issues["condition"].to_list())

    def test_changed_metadata_reference_is_refused_and_new_attempt_accepts_new_version(
        self,
    ):
        backend = MemoryResolver({"spec.yaml": SPEC, "raw.csv": b"ID\n7\n"})
        changed = False

        def callback(request):
            nonlocal changed
            if (
                request.operation == "capture"
                and request.reference is not None
                and not changed
            ):
                changed = True
                backend.put("spec.yaml", SPEC)
            return backend._request(request)

        result = domain("spec.yaml", resolver=ResourceResolver(callback))
        self.assertIsNone(result.output)
        self.assertTrue(result.issues.height)
        fresh = domain("spec.yaml", resolver=backend)
        self.assertEqual(fresh.output["ID"].to_list(), [7])

    def test_interrupt_identity_and_cleanup_after_commit_cancellation(self):
        backend = MemoryResolver({"spec.yaml": SPEC, "raw.csv": b"ID\n7\n"})
        interruption = KeyboardInterrupt("synthetic")

        def callback(request):
            if request.operation == "commit":
                raise interruption
            return backend._request(request)

        result = domain("spec.yaml", resolver=ResourceResolver(callback))
        with self.assertRaises(KeyboardInterrupt) as raised:
            result.save()
        self.assertIs(raised.exception, interruption)
        self.assertEqual(backend.outputs, {})
        self.assertEqual(backend.staging, {})

    def test_partial_publication_preserves_order_and_failed_stage_cleanup(self):
        spec = SPEC.replace(
            b"path: out.csv",
            b"path: out.csv, verification_log: checks.parquet, warning_log: warnings.parquet",
        )
        backend = MemoryResolver({"spec.yaml": spec, "raw.csv": b"ID\n7\n"})
        commits = []

        def callback(request):
            if request.operation == "stage" and request.written == "out.csv":
                # Stage is atomic: a failed Stage leaves no provisional output.
                raise ResourceFailure("publication")
            result = backend._request(request)
            if request.operation == "commit":
                commits.append(result.reference)
            return result

        result = domain("spec.yaml", resolver=ResourceResolver(callback))
        self.assertFalse(result.save())
        self.assertEqual(
            commits, ["memory:/checks.parquet", "memory:/warnings.parquet"]
        )
        self.assertNotIn("memory:/out.csv", backend.outputs)
        self.assertEqual(backend.staging, {})

    def test_abort_removes_only_the_staging_token(self):
        from yamaa.resources import ResourceRequest

        backend = MemoryResolver()
        first = backend._request(
            ResourceRequest(
                "stage",
                "output",
                "/spec.yaml",
                "out.csv",
                "memory:/out.csv",
                b"ID\n7\n",
            )
        )
        second = backend._request(
            ResourceRequest(
                "stage",
                "output",
                "/spec.yaml",
                "out.csv",
                "memory:/out.csv",
                b"ID\n8\n",
            )
        )
        self.assertEqual(len(backend.staging), 2)
        # Abort by the target identity must not delete either attempt.
        backend._request(
            ResourceRequest(
                "abort", "output", "/spec.yaml", "out.csv", "memory:/out.csv"
            )
        )
        self.assertEqual(len(backend.staging), 2)
        backend._request(
            ResourceRequest("abort", "output", "/spec.yaml", "out.csv", first.reference)
        )
        self.assertEqual(list(backend.staging), [second.reference])

    def test_inherited_and_producer_resources_use_each_owning_logical_entry(self):
        producer = SPEC.replace(b"raw.csv", b"../raw/data.csv").replace(
            b"out.csv", b"../out/producer.csv"
        )
        consumer = SPEC.replace(
            b"RAW: raw.csv",
            b"RAW: {path: ../out/producer.csv, schema: ../producer/root.yaml}",
        ).replace(b"out.csv", b"../out/consumer.csv")
        backend = SyntheticRemoteResolver(
            {
                "consumer/root.yaml": consumer,
                "producer/root.yaml": producer,
                "raw/data.csv": b"ID\n7\n",
            }
        )
        result = domain("consumer/root.yaml", resolver=backend)
        self.assertTrue(result.issues.is_empty(), result.issues)
        self.assertEqual(result.output["ID"].to_list(), [7])
        self.assertTrue(result.save())
        self.assertEqual(
            backend.outputs,
            {
                "object://synthetic/out/producer.csv": b"ID\n7\n",
                "object://synthetic/out/consumer.csv": b"ID\n7\n",
            },
        )
        backend.put_frame(
            "raw/data.csv", pa.table({"ID": pa.array([7], type=pa.int64())})
        )
        framed = domain("consumer/root.yaml", resolver=backend)
        self.assertTrue(framed.issues.is_empty(), framed.issues)
        self.assertEqual(framed.output["ID"].to_list(), [7])
        self.assertTrue(framed.save())


if __name__ == "__main__":
    unittest.main()
