"""Installed public producer builds against independently authored study truth."""
import gc
import runpy
import json
import os
import shutil
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

import yamaa_benchmarks as program
from yamaa import DomainError, domain
from yamaa import _locked_functions as host
from yamaa import _native as native

TRUTH = runpy.run_path(str(Path(__file__).with_name("host-fixtures") / "reader.py"))["load"](
    __file__, "producer-graph"
)

ROOT = Path(__file__).parent / "project-original"
RAW = TRUTH["raw_csv"].encode("utf-8")
PRODUCED = TRUTH["produced_csv"].encode("utf-8")
CONSUMED = TRUTH["consumed_csv"].encode("utf-8")
PRODUCER = TRUTH["producer_yaml"]
CONSUMER = TRUTH["consumer_yaml"]
FUNCTION = TRUTH["function_yaml"].replace("HOST_CALLABLE", "yamaa_benchmarks.project_value")


class PublicProducers(unittest.TestCase):
    def setUp(self):
        work = tempfile.TemporaryDirectory()
        self.addCleanup(work.cleanup)
        self.root = Path(work.name).resolve()
        self.write("yamaa-project.yaml", "version: '1.0'\n")
        self.write("producer/layers/base.yaml", PRODUCER)
        self.write("producer/p.yaml", TRUTH["producer_parent_yaml"])
        self.write("consumer/root.yaml", CONSUMER)
        self.write("raw/raw.csv", RAW)
        self.write("env/environment.yaml", "schema_version: '1.0'\nlanguage: python\nlock: uv.lock\nfunctions: {id_float: float.yaml}\n")
        self.write("env/float.yaml", FUNCTION)
        self.write("env/uv.lock", (ROOT / "schema-functions/python/uv.lock").read_bytes())

    def write(self, name, value):
        path = self.root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(value.encode() if isinstance(value, str) else value)

    def build(self):
        return domain(self.root / "consumer/root.yaml", environment=self.root / "env/environment.yaml")

    def entered(self, result):
        return result._native.producer_attempt()

    def observe(self, result):
        text = result._native.observations()
        output = os.environ.get("YAMAA_PUBLIC_REPORT_DIR")
        if output:
            # Preserve the complete actual report and separately snapshot disk
            # targets; held artifact content alone cannot prove publication.
            self._witness_count = getattr(self, "_witness_count", 0) + 1
            directory = Path(output) / "producer-witnesses/python" / self._testMethodName / f"{self._witness_count:02d}"
            directory.mkdir(parents=True, exist_ok=True)
            (directory / "report.json").write_text(text, encoding="utf-8")
            saved = []
            for name in ("generated", "published"):
                for path in sorted((self.root / name).glob("*.csv")):
                    if path.is_file():
                        relative = path.relative_to(self.root)
                        target = directory / "targets" / relative
                        target.parent.mkdir(parents=True, exist_ok=True)
                        target.write_bytes(path.read_bytes())
                        saved.append(relative.as_posix())
            (directory / "witness.json").write_text(json.dumps({
                "scope": "supplemental_producer_publication",
                "truth": "rust/tests/installed_public_producers.py",
                "test": self._testMethodName,
                "report": "report.json",
                "saved_targets": saved,
            }, indent=2) + "\n", encoding="utf-8")
            provenance = os.environ.get("YAMAA_SUPPLEMENTAL_EVIDENCE")
            if provenance:
                (directory / "provenance.json").write_text(provenance, encoding="utf-8")
        return json.loads(text)

    def test_absent_environment_graph_keeps_no_metadata_and_saves_held_artifacts(self):
        from yamaa import check

        self.write("producer/layers/base.yaml", PRODUCER.replace(
            TRUTH["identity_derivation"], "RAW.VALUE"
        ))
        self.write("env/environment.yaml", "[")
        (self.root / "generated").mkdir()
        with (
            patch.object(host, "verify_versions", side_effect=AssertionError("unexpected activation")),
            patch.object(host, "resolve_callable", side_effect=AssertionError("unexpected binding")),
        ):
            result = domain(self.root / "consumer/root.yaml")
            self.assertTrue(result.issues.is_empty(), result.issues)
            self.assertEqual(result.output["VALUE"].to_list(), [2.46, 4.70])
            observed = self.observe(result)
            self.assertEqual(observed["environment"], {"source": None, "lock_source": None})
            self.assertEqual(observed["activation"]["lock"], "not_requested")
            self.assertFalse(any(name.endswith("environment.yaml") for name, _ in self.entered(result).retained_sources()))
            (self.root / "raw/raw.csv").unlink()
            self.assertTrue(check(self.root / "consumer/root.yaml").issues.is_empty())
            for name in tuple(TRUTH["retained_specification_files"]):
                (self.root / name).unlink()
            for _ in range(2):
                self.assertTrue(result.save())
                self.assertEqual((self.root / "generated/producer.csv").read_bytes(), PRODUCED)
                self.assertEqual((self.root / "generated/root.csv").read_bytes(), CONSUMED)

    def test_original_aliases_fresh_union_before_data_and_retained_public_views(self):
        self.write("raw/raw.csv", b"ID,VALUE\n1,99.99\n")
        events = []
        verify, resolve, original = host.verify_versions, host.resolve_callable, program.project_value

        def versions(lock, calls):
            events.append(("lock", tuple(calls)))
            return verify(lock, calls)

        def binding(call, params):
            events.append(("bind", call))
            return resolve(call, params)

        def value(x):
            events.append(("invoke", x))
            if x == 0.0:
                self.write("raw/raw.csv", RAW)
            return original(x)

        with patch.object(host, "verify_versions", versions), patch.object(host, "resolve_callable", binding), patch.object(program, "project_value", value):
            results = [self.build(), self.build()]
        self.assertEqual(events, [("lock", ("yamaa_benchmarks.project_value",)), ("bind", "yamaa_benchmarks.project_value"), ("invoke", 7.25), ("invoke", 0.0), ("invoke", 1.234), ("invoke", 2.345)] * 2)
        self.assertFalse((self.root / "generated").exists())
        shutil.rmtree(self.root)
        gc.collect()
        for result in results:
            self.assertEqual(result.output.to_dicts(), [{"ID": 1, "VALUE": 2.46}, {"ID": 2, "VALUE": 4.70}])
            self.assertTrue(result.issues.is_empty())
            self.assertIsNone(result.verification_log)
            self.assertIsNone(result.warning_log)
            held = self.entered(result)
            self.assertTrue(held.accepted())
            self.assertEqual(held.entered_nodes(), [1, 0])
            self.assertEqual(held.artifact(1), PRODUCED)
            self.assertEqual(held.artifact(0), CONSUMED)
            self.assertIn(RAW, dict(held.retained_sources()).values())
            observed = self.observe(result)
            self.assertEqual(observed["outcome"], "success")
            self.assertEqual(len(observed["source_reads"]), 1)
            self.assertEqual([item["content"].encode() for item in observed["artifacts"]], [PRODUCED, CONSUMED])
            self.assertEqual(observed["activation"]["lock"], "verified")
            self.assertEqual([item["case"] for item in observed["activation"]["tests"]], ["normal", "boundary", "missing"])
            self.assertFalse(observed["activation"]["tests"][-1]["invoked"])
            self.assertFalse(result.save())
            after = self.observe(result)
            self.assertEqual(after.pop("publication")["entered"], [])
            self.assertEqual(after, observed)
            self.assertEqual(result.output["VALUE"].to_list(), [2.46, 4.70])

    def test_root_output_keeps_unrounded_values_and_projection_order(self):
        text = CONSUMER.replace("output: {path:", TRUTH["root_unrounded_column"])
        text = text.replace("columns: [ID, VALUE], decimals: 2", TRUTH["root_unrounded_projection"])
        self.write("consumer/root.yaml", text)
        result = self.build()
        self.assertTrue(result.issues.is_empty())
        self.assertEqual(result.output.columns, TRUTH["root_unrounded_columns"])
        self.assertEqual(result.output["ROOT_UNROUNDED"].to_list(), [2.464, 4.704])
        self.assertEqual(self.entered(result).artifact(0), TRUTH["root_unrounded_csv"].encode("utf-8"))
        self.assertEqual(self.entered(result).artifact(1), PRODUCED)
        changed = result.output.with_columns(result.output["VALUE"] * 10)
        self.assertEqual(changed["VALUE"].to_list(), [24.6, 47.0])
        self.assertEqual(result.output["VALUE"].to_list(), [2.46, 4.70])

    def test_inherited_diamond_runs_shared_leaf_once_without_prospective_disk_inputs(self):
        bridge = TRUTH["retained_bridge_yaml"]
        self.write("bridge/left.yaml", bridge)
        self.write("bridge/right.yaml", bridge.replace("schema: ../producer/p.yaml", "schema: ../producer/./p.yaml").replace("generated/left.csv", "generated/right.csv"))
        inherited = CONSUMER.replace(", no_match: null", "").replace("../generated/producer.csv, schema: ../producer/p.yaml", TRUTH["nested_left_reference"]).replace("../generated/producer.csv, schema: ../producer/./p.yaml", TRUTH["nested_right_reference"]).replace("path: ../generated/root.csv", "path: ../../generated/root.csv")
        self.write("consumer/layers/base.yaml", inherited)
        self.write("consumer/root.yaml", TRUTH["root_parent_yaml"])
        calls = []
        original = program.project_value
        def value(x):
            calls.append(x)
            return original(x)
        with patch.object(program, "project_value", value):
            result = self.build()
        self.assertEqual(calls, [7.25, 0.0, 1.234, 2.345])
        self.assertTrue(result.issues.is_empty())
        self.assertEqual(result.output["VALUE"].to_list(), [2.46, 4.70])
        self.assertEqual(self.entered(result).entered_nodes(), [3, 1, 2, 0])
        self.assertEqual(len(self.observe(result)["nodes"]), 4)
        self.assertFalse((self.root / "generated").exists())
        (self.root / "generated").mkdir()
        before = self.observe(result)
        for directory in tuple(TRUTH["owned_bridge_directories"]):
            shutil.rmtree(self.root / directory)
        gc.collect()
        for _ in range(2):
            self.assertTrue(result.save())
            self.assertEqual((self.root / "generated/producer.csv").read_bytes(), PRODUCED)
            self.assertEqual((self.root / "generated/left.csv").read_bytes(), PRODUCED)
            self.assertEqual((self.root / "generated/right.csv").read_bytes(), PRODUCED)
            self.assertEqual((self.root / "generated/root.csv").read_bytes(), CONSUMED)
            after = self.observe(result)
            self.assertEqual(after.pop("publication"), {"status": "success", "entered": [{"node": node, "published": True} for node in (3, 1, 2, 0)], "error": None})
            self.assertEqual(after, before)
            self.assertEqual(calls, [7.25, 0.0, 1.234, 2.345])
            self.assertTrue(result.issues.is_empty())
        shutil.rmtree(self.root)
        sources = dict(self.entered(result).retained_sources())
        self.assertEqual(len(sources), 10)
        self.assertFalse(any(name.endswith("/yamaa-project.yaml") for name in sources))
        self.assertEqual(result.output["VALUE"].to_list(), [2.46, 4.70])

    def test_save_retains_rounded_alias_bytes_and_unrounded_root_after_originals_drop(self):
        text = CONSUMER.replace("output: {path:", TRUTH["root_unrounded_column"])
        text = text.replace("columns: [ID, VALUE], decimals: 2", TRUTH["root_unrounded_projection"])
        self.write("consumer/root.yaml", text)
        (self.root / "generated").mkdir()
        result = self.build()
        before = self.observe(result)
        self.assertEqual(list((self.root / "generated").iterdir()), [])
        self.write("raw/raw.csv", b"ID,VALUE\n1,99.99\n")
        for directory in tuple(TRUTH["owned_directories"]):
            shutil.rmtree(self.root / directory)
        gc.collect()
        with patch.object(host, "verify_versions", side_effect=AssertionError("Save repeated activation")), patch.object(host, "resolve_callable", side_effect=AssertionError("Save rebound a callable")), patch.object(program, "project_value", side_effect=AssertionError("Save replayed a callback")):
            for _ in range(2):
                self.assertTrue(result.save())
                self.assertEqual((self.root / "generated/producer.csv").read_bytes(), PRODUCED)
                self.assertEqual((self.root / "generated/root.csv").read_bytes(), TRUTH["root_unrounded_csv"].encode("utf-8"))
                self.assertEqual(result.output["ROOT_UNROUNDED"].to_list(), [2.464, 4.704])
                after = self.observe(result)
                self.assertEqual(after.pop("publication")["entered"], [{"node": 1, "published": True}, {"node": 0, "published": True}])
                self.assertEqual(after, before)

    def test_whole_set_preflight_refuses_invalid_late_target_without_producer_write(self):
        (self.root / "generated/root.csv").mkdir(parents=True)
        result = self.build()
        self.assertIsNotNone(result.output)
        self.assertFalse(result.save())
        self.assertFalse((self.root / "generated/producer.csv").exists())
        self.assertEqual(self.observe(result)["publication"]["entered"], [])
        self.assertTrue(self.entered(result).accepted())
        self.assertEqual(result.output["VALUE"].to_list(), [2.46, 4.70])
        (self.root / "generated/root.csv").rmdir()
        self.assertTrue(result.save())
        self.assertTrue(result.issues.is_empty())

    def test_actual_producer_or_consumer_save_failure_preserves_entered_prefix_and_prior_targets(self):
        self.write("consumer/root.yaml", CONSUMER.replace("../generated/root.csv", "../published/root.csv"))
        for failed in ("producer", "consumer"):
            self.write("generated/producer.csv", b"previous producer bytes")
            self.write("published/root.csv", b"previous consumer bytes")
            result = self.build()
            before = self.observe(result)
            target = self.root / ("generated/producer.csv" if failed == "producer" else "published/root.csv")
            readonly = target if os.name == "nt" else target.parent
            readonly.chmod(0o400 if os.name == "nt" else 0o500)
            try:
                if os.name != "nt":
                    probe = readonly / ".write-permission-probe"
                    try:
                        probe.write_bytes(b"permission probe")
                    except PermissionError:
                        pass
                    else:
                        probe.unlink()
                        self.skipTest("directory permissions cannot deny writes on this host")
                self.assertFalse(result.save())
                entered = [{"node": 1, "published": False}] if failed == "producer" else [{"node": 1, "published": True}, {"node": 0, "published": False}]
                self.assertEqual(self.observe(result)["publication"]["entered"], entered)
                self.assertEqual(target.read_bytes(), b"previous producer bytes" if failed == "producer" else b"previous consumer bytes")
                self.assertEqual((self.root / "generated/producer.csv").read_bytes(), b"previous producer bytes" if failed == "producer" else PRODUCED)
                self.assertEqual(result.output["VALUE"].to_list(), [2.46, 4.70])
                self.assertTrue(self.entered(result).accepted())
                self.assertIn("REQ-0764", result.issues["requirement"].to_list())
                self.assertEqual(list(target.parent.glob("*.stage")), [])
            finally:
                readonly.chmod(0o600 if os.name == "nt" else 0o700)
            self.assertTrue(result.save())
            self.assertTrue(result.issues.is_empty())
            after = self.observe(result)
            after.pop("publication")
            self.assertEqual(after, before)

    @unittest.skipUnless(os.name == "nt", "Windows uses ordinal case-insensitive targets")
    def test_absent_windows_case_alias_targets_refuse_every_write(self):
        self.write("consumer/root.yaml", CONSUMER.replace("../generated/root.csv", "../generated/PRODUCER.csv"))
        (self.root / "generated").mkdir()
        result = self.build()
        self.assertIsNotNone(result.output)
        self.assertFalse(result.save())
        self.assertEqual(list((self.root / "generated").iterdir()), [])
        self.assertEqual(self.observe(result)["publication"]["entered"], [])
        self.assertTrue(self.entered(result).accepted())

    @unittest.skipUnless(sys.platform == "darwin", "macOS native parent case policy")
    def test_macos_absent_case_aliases_and_unproven_unicode_targets_preflight(self):
        (self.root / "generated").mkdir()
        self.write("generated/case-probe", b"native case witness")
        sensitive = not (self.root / "generated/CASE-PROBE").exists()
        (self.root / "generated/case-probe").unlink()
        for name in ("PRODUCER.csv", r"\u00e9.csv"):
            self.write("consumer/root.yaml", CONSUMER.replace("../generated/root.csv", '"../generated/' + name + '"'))
            result = self.build()
            self.assertIsNotNone(result.output)
            self.assertTrue(self.entered(result).accepted())
            if name == "PRODUCER.csv" and sensitive:
                self.assertTrue(result.save())
                self.assertEqual((self.root / "generated/producer.csv").read_bytes(), PRODUCED)
                self.assertEqual((self.root / "generated/PRODUCER.csv").read_bytes(), CONSUMED)
                for path in (self.root / "generated").iterdir():
                    path.unlink()
            else:
                self.assertFalse(result.save())
                self.assertEqual(self.observe(result)["publication"]["entered"], [])
                self.assertEqual(list((self.root / "generated").iterdir()), [])

    def test_consumer_only_bad_binding_precedes_all_study_and_runtime_effects(self):
        bad = FUNCTION.replace("params: [{name: x", "params: [{name: z").replace("args: {x:", "args: {z:").replace("short-circuit-missing:x", "short-circuit-missing:z")
        self.write("env/bad.yaml", bad)
        self.write("env/environment.yaml", "schema_version: '1.0'\nlanguage: python\nlock: uv.lock\nfunctions: {id_float: float.yaml, bad: bad.yaml}\n")
        self.write("consumer/root.yaml", CONSUMER.replace(TRUTH["sum_derivation"], TRUTH["bad_derivation"]))
        shutil.rmtree(self.root / "raw")
        calls = []
        original = program.project_value
        def value(x):
            calls.append(x)
            return original(x)
        with patch.object(program, "project_value", value):
            result = self.build()
        self.assertEqual(calls, [])
        self.assertIsNone(result.output)
        self.assertEqual(self.entered(result).entered_nodes(), [])
        observed = self.observe(result)
        self.assertEqual(observed["source_reads"], [])
        self.assertEqual(observed["activation"]["tests"], [])
        self.assertEqual(result.issues["condition"].to_list(), ["project_environment_invalid"])
        self.assertEqual(result.issues["requirement"].to_list(), ["REQ-0695"])

    def test_original_host_failure_and_owned_actual_prefix_survive_file_deletion(self):
        original = RuntimeError("original public producer failure")
        real = program.project_value
        def value(x):
            if x == 1.234:
                raise original
            return real(x)
        with patch.object(program, "project_value", value):
            result = self.build()
        shutil.rmtree(self.root)
        gc.collect()
        self.assertIsNone(result.output)
        self.assertEqual(self.entered(result).entered_nodes(), [1])
        self.assertEqual(result.issues["condition"].to_list(), ["function_call_failed"])
        self.assertIs(result._native.project_failures()[0][1], original)
        self.assertIs(self.entered(result).host_failures()[0][1], original)
        observed = self.observe(result)
        self.assertEqual(observed["artifacts"], [])
        self.assertEqual(len(observed["nodes"]), 1)
        self.assertEqual(len(observed["source_reads"]), 1)
        with self.assertRaisesRegex(DomainError, "cannot save a failed build"):
            result.save()

    def test_public_interrupt_propagates_original_and_native_owner_retains_its_prefix(self):
        original = KeyboardInterrupt("original public graph interrupt")
        real = program.project_value
        def value(x):
            if x == 1.234:
                raise original
            return real(x)
        with patch.object(program, "project_value", value):
            held = native._domain_file(str(self.root / "consumer/root.yaml"), str(self.root / "env/environment.yaml"))
            with self.assertRaises(KeyboardInterrupt) as raised:
                self.build()
        self.assertIs(raised.exception, original)
        traceback = original.__traceback__
        while traceback is not None:
            if traceback.tb_frame.f_code.co_name == "domain":
                self.assertNotIn("native", traceback.tb_frame.f_locals)
            traceback = traceback.tb_next
        shutil.rmtree(self.root)
        gc.collect()
        self.assertIs(held.project_failures()[0][1], original)
        self.assertEqual(held.producer_attempt().entered_nodes(), [1])
        with self.assertRaises(KeyboardInterrupt) as retained:
            held.propagate_interrupt()
        self.assertIs(retained.exception, original)
        self.assertIsNone(held.output())

    def test_opaque_oversized_exception_rendering_preserves_original_and_refuses_public_output(self):
        original = RuntimeError("x" * 65537)
        real = program.project_value
        def value(x):
            if x == 1.234:
                raise original
            return real(x)
        with patch.object(program, "project_value", value):
            result = self.build()
        self.assertIsNone(result.output)
        self.assertIs(result._native.project_failures()[0][1], original)
        self.assertEqual(self.entered(result).entered_nodes(), [1])
        self.assertEqual(json.loads(result.issues["context"][0])["code"], "report_limit")

    def test_failed_consumer_verification_has_no_output_from_accepted_producer(self):
        self.write("consumer/root.yaml", CONSUMER + TRUTH["fatal_row_count_yaml"])
        result = self.build()
        self.assertIsNone(result.output)
        held = self.entered(result)
        self.assertFalse(held.accepted())
        self.assertEqual(held.entered_nodes(), [1, 0])
        self.assertEqual(held.artifact(1), PRODUCED)
        self.assertIsNone(held.artifact(0))
        self.assertEqual(result.issues["condition"].to_list(), ["row_count_failed"])
        self.assertFalse((self.root / "generated").exists())
        shutil.rmtree(self.root)
        self.assertIsNone(result.output)
        with self.assertRaisesRegex(DomainError, "cannot save a failed build"):
            result.save()

    def test_actual_ingestion_failure_stops_before_dependents(self):
        self.write("raw/raw.csv", b"ID,VALUE\n1,bad\n")
        result = self.build()
        self.assertIsNone(result.output)
        self.assertEqual(self.entered(result).entered_nodes(), [1])
        self.assertIn(b"ID,VALUE\n1,bad\n", dict(self.entered(result).retained_sources()).values())
        self.assertEqual(self.observe(result)["artifacts"], [])
        self.assertGreater(result.issues.height, 0)

    def test_public_copy_byte_quota_preserves_complete_accepted_graph_without_output_or_save(self):
        # Text stays below the execution allowance, while IPC offsets and
        # numeric columns take the public copy beyond its 8 MiB byte cap.
        payload = "x" * 2048
        extra = f'  - {{name: PAYLOAD, type: str, label: Payload, derivation: {{literal: "{payload}"}}}}\n'
        self.write("consumer/root.yaml", CONSUMER.replace("output: {path:", extra + "output: {path:").replace("columns: [ID, VALUE], decimals: 2", TRUTH["payload_projection"]))
        self.write("raw/raw.csv", b"ID,VALUE\n" + "".join(f"{index},1.234\n" for index in range(1, 4096)).encode())
        result = self.build()
        self.assertTrue(self.entered(result).accepted())
        self.assertEqual(self.entered(result).entered_nodes(), [1, 0])
        self.assertIsNone(result.output)
        self.assertEqual(result._native.producer_projection_failure(), "table output exceeds byte limit")
        self.assertEqual(json.loads(result.issues["context"][0])["code"], "output_projection")
        self.assertEqual(self.observe(result)["outcome"], "success")
        self.assertEqual(len(self.observe(result)["artifacts"]), 2)
        self.assertFalse((self.root / "generated").exists())
        shutil.rmtree(self.root)
        with self.assertRaisesRegex(DomainError, "cannot save a failed build"):
            result.save()
        self.assertEqual(result._native.producer_projection_failure(), "table output exceeds byte limit")
        self.assertTrue(self.entered(result).accepted())

    def test_public_copy_quota_preserves_complete_accepted_graph_without_output_or_save(self):
        self.write("producer/layers/base.yaml", PRODUCER.replace("ID: int", "ID: str").replace("name: ID, type: int", "name: ID, type: str"))
        extra = "".join(f"  - {{name: EXTRA{index}, type: str, label: Extra value, derivation: {{literal: null}}}}\n" for index in range(59))
        names = ", ".join(["ID", "VALUE"] + [f"EXTRA{index}" for index in range(59)])
        self.write("consumer/root.yaml", CONSUMER.replace("name: ID, type: int", "name: ID, type: str").replace("output: {path:", extra + "output: {path:").replace("columns: [ID, VALUE], decimals: 2", f"columns: [{names}], decimals: 2"))
        # 23,000 rows with 63 owned columns visit 4,347,000 cells across the
        # three public-copy passes, exceeding the 4,194,304-cell allowance.
        self.write("raw/raw.csv", b"ID,VALUE\n" + "".join(f"{index},1.234\n" for index in range(1, 23001)).encode())
        result = self.build()
        self.assertTrue(self.entered(result).accepted())
        self.assertEqual(self.entered(result).entered_nodes(), [1, 0])
        self.assertIsNone(result.output)
        self.assertEqual(result._native.producer_projection_failure(), "table output exceeds byte limit")
        self.assertEqual(json.loads(result.issues["context"][0])["code"], "output_projection")
        self.assertEqual(self.observe(result)["outcome"], "success")
        self.assertEqual(len(self.observe(result)["artifacts"]), 2)
        self.assertFalse((self.root / "generated").exists())
        shutil.rmtree(self.root)
        with self.assertRaisesRegex(DomainError, "cannot save a failed build"):
            result.save()
        self.assertEqual(result._native.producer_projection_failure(), "table output exceeds byte limit")
        self.assertTrue(self.entered(result).accepted())

    def test_invalid_original_producer_metadata_keeps_rejection_and_never_falls_back(self):
        self.write("consumer/root.yaml", CONSUMER.replace("schema: ../producer/p.yaml", TRUTH["incompatible_reference"]))
        with patch.object(host, "verify_versions") as verify, patch.object(program, "project_value") as invoked:
            result = self.build()
        verify.assert_not_called()
        invoked.assert_not_called()
        self.assertIsNone(self.entered(result))
        self.assertIn("REQ-0523", result.issues["requirement"].to_list())
        before = dict(result._native.preparation_sources())
        shutil.rmtree(self.root)
        self.assertEqual(dict(result._native.preparation_sources()), before)
        self.assertIn(PRODUCER.encode(), before.values())

    def test_early_environment_and_schema_null_failures_retain_original_bytes(self):
        self.write("env/environment.yaml", b"schema_version: [\n")
        with patch.object(host, "verify_versions") as verify:
            result = self.build()
        verify.assert_not_called()
        self.assertIsNone(result.output)
        self.assertIsNone(self.entered(result))
        self.assertIn(b"schema_version: [\n", dict(result._native.preparation_sources()).values())
        self.write("env/environment.yaml", "schema_version: '1.0'\nlanguage: python\nlock: uv.lock\nfunctions: {id_float: float.yaml}\n")
        self.write("consumer/root.yaml", CONSUMER.replace("schema: ../producer/p.yaml", "schema: null"))
        rejected = self.build()
        self.assertIsNone(self.entered(rejected))
        self.assertIn("REQ-0287", rejected.issues["requirement"].to_list())
        shutil.rmtree(self.root)
        self.assertIn(b"schema_version: [\n", dict(result._native.preparation_sources()).values())


if __name__ == "__main__":
    unittest.main()
