"""Approved citation changes preserve all independently captured schema facts."""
import collections
import copy
import csv
import importlib.util
import json
import unittest
from pathlib import Path

TOOLS = Path(__file__).parents[1] / "tools"
spec = importlib.util.spec_from_file_location("private_schema_requirements", TOOLS / "private_schema_requirements.py")
policy = importlib.util.module_from_spec(spec)
spec.loader.exec_module(policy)
FIXTURES = TOOLS.parent / "crates/yamaa-adapters/tests/fixtures"


class SchemaCitations(unittest.TestCase):
    def differences(self, before, after):
        if isinstance(before, dict):
            self.assertEqual(before.keys(), after.keys())
            for key in before:
                if key == "requirement" and before[key] != after[key]:
                    self.assertIsNone(before[key])
                    self.assertIn(after[key], {"REQ-0281", "REQ-0285", "REQ-0286", "REQ-0287", "REQ-0657"})
                    yield after[key]
                else:
                    yield from self.differences(before[key], after[key])
        elif isinstance(before, list):
            self.assertEqual(len(before), len(after))
            for a, b in zip(before, after, strict=True):
                yield from self.differences(a, b)
        else:
            self.assertEqual(before, after)

    def test_captured_inventory_changes_only_44_approved_requirement_fields(self):
        expected_counts = {
            "schema_model.tsv": {"REQ-0287": 33, "REQ-0285": 4, "REQ-0286": 4},
            "schema_transport.tsv": {"REQ-0281": 1},
            "schema_layer_admission.tsv": {"REQ-0285": 1},
            "schema_composition.tsv": {}, "schema_windows.tsv": {},
            "schema_inheritance_dependencies.tsv": {}, "inheritance_preparation.tsv": {"REQ-0285": 1},
        }
        for name, counts in expected_counts.items():
            original_bytes = (FIXTURES / name).read_bytes()
            changes = collections.Counter()
            with (FIXTURES / name).open(encoding="utf-8", newline="") as stream:
                for row in csv.DictReader(stream, delimiter="\t"):
                    historical = json.loads(row["expected"])
                    preserved = copy.deepcopy(historical)
                    expected = policy.reconciled_schema_requirements(historical)
                    self.assertEqual(historical, preserved)
                    changes.update(self.differences(historical, expected))
            self.assertEqual(dict(changes), counts, name)
            self.assertEqual((FIXTURES / name).read_bytes(), original_bytes)

    def test_resolved_output_owner_is_explicit_and_only_requirement_changes(self):
        historical = {"phase": "validation", "condition": "missing_required_field", "requirement": None,
                      "path": "output", "context": {"field": "output", "class": "root_class"}}
        self.assertEqual(policy.reconciled_schema_requirements(historical),
                         {**historical, "requirement": "REQ-0285"})
        self.assertEqual(policy.reconciled_schema_requirements(historical, resolved_output=True),
                         {**historical, "requirement": "REQ-0657"})
        non_output = {**historical, "path": "columns.X.type"}
        self.assertEqual(policy.reconciled_schema_requirements(non_output, resolved_output=True),
                         {**non_output, "requirement": "REQ-0285"})

    def test_diagnostic_shaped_source_context_and_unrelated_payloads_are_unchanged(self):
        nested = {"condition": "missing_required_field", "requirement": None,
                  "path": "spoof", "context": {"reason": "retained source data"}}
        historical = {"outcome": {"diagnostics": [{
            "condition": "unknown_field", "requirement": None,
            "path": "extra", "context": {"actual": nested}}]},
            "schema": {"diagnostics": [nested]}, "payload": nested}
        expected = copy.deepcopy(historical)
        expected["outcome"]["diagnostics"][0]["requirement"] = "REQ-0285"
        self.assertEqual(policy.reconciled_schema_requirements(historical), expected)
        self.assertIsNone(historical["outcome"]["diagnostics"][0]["requirement"])

    def test_existing_citations_and_implementation_nulls_are_unchanged(self):
        historical = [{"condition": "unknown_field", "requirement": "REQ-0443"},
                      {"condition": "unsupported_operation", "requirement": None},
                      {"condition": "engine_rejected", "requirement": None},
                      {"condition": "invalid_field_type", "requirement": "REQ-0287"}]
        self.assertEqual(policy.reconciled_schema_requirements(historical), historical)


if __name__ == "__main__":
    unittest.main()
