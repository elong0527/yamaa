"""Installed dataset/1 execution over independent Arrow and observation truth."""

import csv
import gc
import json
import unittest
from pathlib import Path

import yamaa_native

ROOT = Path(__file__).parent / "datasets"


class InstalledDatasets(unittest.TestCase):
    """Exercise copied native buffers and failed outcomes outside the checkout."""

    def test_shared_dataset_cases(self):
        """Both installed archive forms must replay all independent native outcomes."""
        self.assertIn("site-packages", str(Path(yamaa_native.__file__).resolve()))
        cases = json.loads((ROOT / "expected.json").read_text())
        with (ROOT / "expected.tsv").open() as stream:
            tabular = list(csv.DictReader(stream, delimiter="\t"))
        self.assertEqual(len(cases), len(tabular))
        for case, tab in zip(cases, tabular, strict=True):
            with self.subTest(case=case["case"]):
                self.assertEqual(json.loads(tab["request"]), case["request"])
                self.assertEqual(json.loads(tab["expected"]), case["expected"])
                self.assertEqual(
                    json.loads(tab["snapshot"]) if tab["snapshot"] else None,
                    case["snapshot"],
                )
                source = (ROOT / case["input"]).read_bytes()
                table, outcome = yamaa_native.execute_dataset(
                    json.dumps(case["request"]), source
                )
                del source
                gc.collect()
                self.assertEqual(json.loads(outcome), case["expected"])
                if case["snapshot"] is None:
                    self.assertIsNone(table)
                else:
                    self.assertIs(type(table), bytes)
                    self.assertEqual(
                        json.loads(yamaa_native.table_snapshot(table)), case["snapshot"]
                    )
                    self.assertEqual(yamaa_native.table_round_trip(table), table)
        self.assertFalse(yamaa_native.engine_info()["execution_supported"])

    def test_admission_precedes_ipc_and_recovers(self):
        """Invalid typed requests cannot decode bad IPC or poison subsequent execution."""
        request = json.loads((ROOT / "adlb-plan.json").read_text())
        request["columns"][0]["expression"] = {"source": 999}
        with self.assertRaisesRegex(ValueError, "invalid bound dataset plan"):
            yamaa_native.execute_dataset(json.dumps(request), b"invalid IPC")
        with self.assertRaisesRegex(ValueError, "resource limit"):
            yamaa_native.execute_dataset(" " * 1048577, b"")
        with self.assertRaises(TypeError):
            yamaa_native.execute_dataset({}, b"")
        request = (ROOT / "adlb-plan.json").read_text()
        table, outcome = yamaa_native.execute_dataset(
            request, (ROOT / "adlb.arrow").read_bytes()
        )
        self.assertIs(type(table), bytes)
        self.assertEqual(json.loads(outcome)["outcome"]["status"], "success")


if __name__ == "__main__":
    unittest.main()
