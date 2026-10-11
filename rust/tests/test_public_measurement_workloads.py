"""Independent workload arithmetic/order/serialization and admission controls."""

import csv
import importlib.util
import io
import json
import math
import tempfile
import unittest
from pathlib import Path

PATH = Path(__file__).resolve().parents[1] / "tools/prepare_public_workloads.py"
SPEC = importlib.util.spec_from_file_location("measurement_workloads", PATH)
workloads = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(workloads)


class Workloads(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name) / "workloads"
        self.manifest = workloads.prepare(self.root, (20,))

    def case(self, name):
        (record,) = [c for c in self.manifest["cases"] if c["name"] == name]
        return record, self.root / name

    def expected(self, name):
        record, directory = self.case(name)
        return json.loads((directory / record["expected_output"]).read_text())

    def test_stored_sum_is_not_compensated_sum_and_missing_group_stays_missing(self):
        rows = self.expected("aggregate-20-10")["rows"]
        self.assertEqual(rows[0][1], {"type": "missing", "value": None})
        self.assertEqual(rows[1][1], workloads.typed(0.0, "float"))
        self.assertEqual(math.fsum([2**53, 1, -(2**53), 1, -1, 0.25, -0.25, 3, -3]), 1)

    def test_lookup_ties_select_first_authored_donor_and_unmatched_keys_are_missing(
        self,
    ):
        expected = self.expected("lookup-20-4")["rows"]
        self.assertEqual(expected[0][1], {"type": "missing", "value": None})
        self.assertEqual(expected[1][1], {"type": "str", "value": "é,1:0"})
        record, directory = self.case("lookup-20-4")
        donors = list(
            csv.DictReader(io.StringIO((directory / "donor.csv").read_text()))
        )
        self.assertEqual(
            [r["TEXT"] for r in donors[:4]], [f"é,1:{i}" for i in range(4)]
        )
        self.assertIn('"é,1:0"', (directory / "expected/out.csv").read_text())
        self.assertEqual(record["dimensions"], {"donors_per_match": 4})

    def test_window_dense_ties_and_missing_offset_differ_from_last_present(self):
        rows = self.expected("window-20-10")["rows"]
        self.assertEqual(rows[1][3], workloads.typed(1, "int"))
        self.assertEqual(rows[2][3], workloads.typed(1, "int"))
        self.assertEqual(rows[0][3], workloads.typed(5, "int"))
        self.assertEqual(rows[4][4], {"type": "missing", "value": None})
        self.assertEqual(rows[4][5], workloads.typed(7.0, "float"))
        self.assertEqual(rows[4][6], workloads.typed(2.0, "float"))

    def test_shared_producer_truth_pins_serialized_rounding_before_consumer_sum(self):
        record, directory = self.case("producer-20-3")
        self.assertEqual(
            record["expected_nodes"],
            [
                "producer-0.yaml",
                "producer-1.yaml",
                "producer-2.yaml",
                "left.yaml",
                "right.yaml",
                "spec.yaml",
            ],
        )
        self.assertEqual(record["expected_producer_executions"], 5)
        self.assertEqual(
            (directory / "expected/producer-0.csv").read_bytes().splitlines()[1:5],
            [b"K000000,0.13", b"K000001,0.38", b"K000002,-0.13", b"K000003,-0.38"],
        )
        self.assertEqual(
            (directory / "expected/out.csv").read_bytes().splitlines()[1],
            b"K000000,0.26",
        )
        self.assertEqual(
            self.expected("producer-20-3")["columns"],
            [["ID", "str"], ["VALUE", "float"]],
        )

    def test_expected_callback_count_never_becomes_an_observed_metric(self):
        record, _ = self.case("callback-20-1")
        self.assertEqual(
            record["expected_callbacks"],
            {
                "lock": 1,
                "binding": 1,
                "test_invocations": 2,
                "derivation_invocations": 17,
                "missing_short_circuits": 3,
            },
        )
        self.assertIsNone(self.manifest["unavailable"]["actual_callback_counts"])

    def test_every_retained_member_has_exact_size_and_no_digest_identity(self):
        for record in self.manifest["cases"]:
            for member in record["files"]:
                self.assertEqual(
                    (self.root / record["name"] / member["path"]).stat().st_size,
                    member["bytes"],
                )
        self.assertNotIn("digest", json.dumps(self.manifest))
        self.assertNotIn("checksum", json.dumps(self.manifest))
        record, directory = self.case("callback-20-1")
        self.assertEqual(
            (directory / "python/uv.lock").read_bytes(),
            (workloads.REPOSITORY / "python/uv.lock").read_bytes(),
        )
        self.assertEqual(
            (directory / "r/renv.lock").read_bytes(),
            (
                workloads.REPOSITORY / "benchmarks/schema-functions/r/renv.lock"
            ).read_bytes(),
        )

    def test_existing_evidence_and_duplicate_or_invalid_requests_refuse(self):
        with self.assertRaises(FileExistsError):
            workloads.prepare(self.root, (20,))
        for sizes in ((0,), (-1,), (True,), (20, 20), ()):
            with self.assertRaises(ValueError):
                workloads.prepare(self.root / "invalid", sizes)
        for families in (("unknown",), ("window", "window"), ()):
            with self.assertRaises(ValueError):
                workloads.prepare(self.root / "invalid", (20,), families)
        self.assertFalse((self.root / "invalid").exists())


if __name__ == "__main__":
    unittest.main()
