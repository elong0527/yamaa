"""Guard exact mismatch reporting so characterization cannot silently claim parity."""

import importlib.util
import unittest
from pathlib import Path

spec = importlib.util.spec_from_file_location(
    "assess_math", Path(__file__).parents[1] / "tools/assess_math.py"
)
assessment = importlib.util.module_from_spec(spec)
spec.loader.exec_module(assessment)


class MathAssessmentTests(unittest.TestCase):
    """Exercise mismatches, missingness and invalid probe records independently."""

    def test_exact_sample_still_does_not_qualify_function(self):
        """A matching limited sample cannot enable an unsupported function."""
        report = assessment.assess(
            ["exact\tLN\t3ff0000000000000\t0000000000000000\t0000000000000000"]
        )
        self.assertEqual(report["qualification"], "not-qualified")
        self.assertEqual(
            report["default_functions_remain_unsupported"], ["EXP", "LN", "POWER"]
        )
        self.assertEqual(report["counts"]["LN"]["exact"], 1)

    def test_one_ulp_is_retained_as_a_blocker(self):
        """A deliberately wrong neighbor is evidence, never an accepted tolerance."""
        report = assessment.assess(
            ["neighbor\tEXP\t0000000000000000\t0000000000000000\t3ff0000000000001"]
        )
        self.assertEqual(report["qualification"], "blocked-by-mismatches")
        self.assertEqual(report["counts"]["EXP"]["mismatches"], 1)
        self.assertEqual(report["mismatches"][0]["ulp_distance"], 1)
        self.assertEqual(report["mismatches"][0]["reference"], "3ff0000000000000")

    def test_missingness_and_zero_sign_are_not_ulp_tolerances(self):
        """Retain categorical failures that finite bit-distance summaries cannot express."""
        self.assertEqual(
            assessment.difference("missing", "3ff0000000000000"), ("missingness", None)
        )
        self.assertEqual(
            assessment.difference("8000000000000000", "0000000000000000"),
            ("zero_sign", None),
        )
        self.assertEqual(
            assessment.difference("bff0000000000000", "3ff0000000000000"),
            ("sign", None),
        )
        self.assertEqual(assessment.normalized_bits(float("nan")), "missing")
        self.assertEqual(assessment.reference("EXP", 710.0, 0.0), "missing")

    def test_rejects_duplicate_or_unknown_records(self):
        """Duplicate IDs and unrecognized functions must fail instead of inflating coverage."""
        row = "same\tLN\t3ff0000000000000\t0000000000000000\t0000000000000000"
        with self.assertRaises(ValueError):
            assessment.assess([row, row])
        with self.assertRaises(ValueError):
            assessment.assess([row.replace("LN", "BAD")])

    def test_rejects_unqualified_input_domains(self):
        """Domain diagnostics are outside this numerical assessment, not silently missing."""
        for name, left, right in [
            ("LN", 0.0, 0.0),
            ("POWER", -1.0, 0.5),
            ("EXP", float("inf"), 0.0),
        ]:
            with self.subTest(name=name), self.assertRaises(ValueError):
                assessment.reference(name, left, right)


if __name__ == "__main__":
    unittest.main()
