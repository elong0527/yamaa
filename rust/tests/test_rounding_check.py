"""Ensure the Rust rounding gate cannot pass altered, missing or relabeled outputs."""

import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parents[1] / "tools"))
from check_rounding import verify


class RoundingCheckTests(unittest.TestCase):
    """Use the independently known inclusive endpoint result under both policies."""

    def test_exact_results_and_identity_are_required(self):
        """Bit, zero-sign, input, policy, case and missingness drift must all fail."""
        samples = [("tie", 0.5, 0)]
        rows = [
            f"tie\t{policy}\t3fe0000000000000\t0\t3ff0000000000000"
            for policy in ("ReferenceSubset", "PortableLibmV1")
        ]
        self.assertEqual(verify(samples, rows), 2)
        for index, replacement in (
            (0, "other"),
            (1, "other"),
            (2, "3fe0000000000001"),
            (3, "1"),
            (4, "missing"),
            (4, "3ff0000000000001"),
            (4, "8000000000000000"),
        ):
            changed = rows.copy()
            fields = changed[0].split("\t")
            fields[index] = replacement
            changed[0] = "\t".join(fields)
            with self.assertRaises(AssertionError):
                verify(samples, changed)
        for changed in ([], rows[:1], rows + [rows[0]], rows[::-1]):
            with self.assertRaises(AssertionError):
                verify(samples, changed)
        with self.assertRaises(AssertionError):
            verify([], [])
        zero_rows = [
            f"zero\t{policy}\t0000000000000000\t0\t0000000000000000"
            for policy in ("ReferenceSubset", "PortableLibmV1")
        ]
        self.assertEqual(verify([("zero", 0.0, 0)], zero_rows), 2)
        zero_rows[0] = zero_rows[0][:-16] + "8000000000000000"
        with self.assertRaises(AssertionError):
            verify([("zero", 0.0, 0)], zero_rows)
