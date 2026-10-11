"""Independent original-input identity and M1 corruption controls.

Synthetic native metadata below exercises the comparator only. It is never
installed-owner or qualification evidence.
"""

import ast
import copy
import importlib.util
import json
import tempfile
import unittest
from pathlib import Path

from yamaa.adapters.conformance import ExampleReport, compare_reports

ROOT = Path(__file__).resolve().parents[2]
FIXTURES = ROOT / "rust/qualification/producer-cohort"
spec = importlib.util.spec_from_file_location("qualify_producer_cohort", ROOT / "rust/tools/qualify_producer_cohort.py")
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


class OriginalProducerCohort(unittest.TestCase):
    def test_authored_inputs_are_exact_original_test_literals(self):
        tree = ast.parse((ROOT / "python/tests/planning/test_workflow.py").read_text())
        function = next(node for node in tree.body if isinstance(node, ast.FunctionDef) and node.name == "test_shared_producer_executes_once_and_shares_generated_snapshot")
        found = {}
        for node in ast.walk(function):
            if isinstance(node, ast.Call) and isinstance(node.func, ast.Attribute) and node.func.attr == "write_text" and isinstance(node.func.value, ast.BinOp) and isinstance(node.func.value.right, ast.Constant) and isinstance(node.args[0], ast.Constant):
                found[node.func.value.right.value] = node.args[0].value.encode("ascii")
        self.assertEqual(set(found), {"seed.csv", "producer.yaml", "spec.yaml"})
        for name, payload in found.items():
            self.assertEqual((FIXTURES / module.NAME / name).read_bytes(), payload)
        self.assertEqual((FIXTURES / module.NAME / "expected/out.csv").read_bytes(), b"ID\n01\n")

    def setUp(self):
        self.reference = ExampleReport.model_validate_json((FIXTURES / module.NAME / "reference.json").read_text())
        self.case = "/unit-unqualified/original"
        self.raw = self.reference.model_dump(mode="json")
        self.raw.update(backend="rust", example="domain", entry="spec.yaml", environment={}, activation={"lock": "not_requested", "bindings": [], "tests": []})
        for group in ("nodes", "tables", "verifications", "callbacks"):
            for observation in self.raw[group]:
                observation["specification"] = self.case + "/" + observation["specification"]
        for source in self.raw["source_reads"]:
            source.update(specification=self.case + "/producer.yaml", base_directory=self.case)
        for artifact in self.raw["artifacts"]:
            artifact["specification"] = self.case + "/" + ("producer.yaml" if artifact["name"] == "produced" else "spec.yaml")

    def test_projection_preserves_every_portable_ledger(self):
        projected = module.project_report(self.raw, self.case)
        self.assertTrue(compare_reports(self.reference, projected).passed)
        self.assertEqual(len(projected.tables), 5)
        self.assertEqual([table.name for table in projected.tables if table.stage == "source"], ["SEED", "A", "B"])

    def test_canonical_windows_identities_are_compared_without_host_filesystem_access(self):
        case = "C:/Users/runner/AppData/Local/Temp/original"
        raw = json.loads(json.dumps(self.raw).replace(self.case, case))
        self.assertTrue(compare_reports(self.reference, module.project_report(raw, case)).passed)
        raw["nodes"][0]["specification"] = "C:/external/producer.yaml"
        with self.assertRaises(ValueError):
            module.project_report(raw, case)

    def test_native_staging_contains_only_original_authored_producer_inputs(self):
        specification = importlib.util.spec_from_file_location("stage_project_inputs", ROOT / "rust/tools/stage_project_inputs.py")
        staging = importlib.util.module_from_spec(specification)
        specification.loader.exec_module(staging)
        with tempfile.TemporaryDirectory() as temporary:
            destination = Path(temporary) / "project-original"
            staging.stage(destination)
            producer = destination / module.NAME
            self.assertEqual({item.name for item in producer.iterdir()}, {"seed.csv", "producer.yaml", "spec.yaml"})
            for item in producer.iterdir():
                self.assertEqual(item.read_bytes(), (FIXTURES / module.NAME / item.name).read_bytes())

    def test_generated_aliases_cannot_be_hidden_from_a_corrupted_read_ledger(self):
        for _ in range(2):
            self.raw["source_reads"].append({
                "specification": self.case + "/spec.yaml", "base_directory": self.case,
                "path": "produced.parquet", "outcome": "captured", "condition": None, "snapshots_created": 0,
            })
        verdict = compare_reports(self.reference, module.project_report(self.raw, self.case))
        self.assertFalse(verdict.passed)
        self.assertEqual(verdict.findings[0].kind, "report.source_reads.count")
        self.assertEqual((verdict.findings[0].expected, verdict.findings[0].actual), (1, 3))

    def test_complete_protocol_rejects_missing_changed_or_reordered_observations(self):
        for group in ("nodes", "tables", "artifacts", "source_reads"):
            raw = copy.deepcopy(self.raw)
            raw[group].pop()
            self.assertFalse(compare_reports(self.reference, module.project_report(raw, self.case)).passed)
        self.raw["nodes"].reverse()
        self.assertFalse(compare_reports(self.reference, module.project_report(self.raw, self.case)).passed)
        self.raw["nodes"].reverse()
        self.raw["tables"][2]["rows"][0][0]["value"] = "02"
        self.assertFalse(compare_reports(self.reference, module.project_report(self.raw, self.case)).passed)
        self.raw.pop("source_reads")
        with self.assertRaises(KeyError):
            module.project_report(self.raw, self.case)

    def test_unknown_metadata_and_external_artifact_owner_are_rejected(self):
        self.raw["invented_ledger"] = []
        with self.assertRaisesRegex(ValueError, "unknown native"):
            module.project_report(self.raw, self.case)
        self.raw.pop("invented_ledger")
        self.raw["artifacts"][0]["specification"] = self.case + "/spec.yaml"
        with self.assertRaisesRegex(ValueError, "actual node"):
            module.project_report(self.raw, self.case)

    def test_saved_byte_corruption_is_rejected_beside_complete_report(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            owner = directory / "public-reports/producer-cohort/python" / module.NAME
            owner.mkdir(parents=True)
            record = {"evidence": {"source_revision": "component-only", "run": "unit:unqualified", "artifact_reference": "unit:unqualified"}, "installation": {}}
            (owner / "provenance.json").write_text(json.dumps({"scope": "supplemental_suites", **record}))
            (owner / "case-directory.txt").write_text(self.case)
            (owner / "before.json").write_text(json.dumps(self.raw))
            saved = owner / "after-1"
            saved.mkdir()
            after = copy.deepcopy(self.raw)
            after["publication"] = {"status": "success", "error": None, "entered": [{"node": 1, "published": True}, {"node": 0, "published": True}]}
            (saved / "report.json").write_text(json.dumps(after))
            (saved / "produced.parquet").write_bytes((FIXTURES / module.NAME / "expected/produced.parquet").read_bytes())
            (saved / "out.csv").write_bytes(b"ID\n02\n")
            with self.assertRaisesRegex(ValueError, "CSV bytes"):
                module.retain_owner(directory, "python", record, self.reference, directory / "output")


if __name__ == "__main__":
    unittest.main()
