"""Independent known answers and reporting regressions for the rounding assessment."""

import importlib.util
import math
import sys
import unittest
from pathlib import Path

spec = importlib.util.spec_from_file_location(
    "assess_rounding", Path(__file__).parents[1] / "tools/assess_rounding.py"
)
rounding = importlib.util.module_from_spec(spec)
spec.loader.exec_module(rounding)


class RoundingAssessmentTests(unittest.TestCase):
    """Check the exact rule without treating reference observations as expected truth."""

    def test_known_decimal_results_and_positive_zero(self):
        """Use hand-specified ties, ordinary neighbors and range outcomes."""
        for value, digits, expected in (
            (1.25, 1, 1.3),
            (-1.25, 1, -1.3),
            (2.5, 0, 3.0),
            (1.24, 1, 1.2),
            (145.0, -1, 150.0),
            (144.0, -1, 140.0),
            (-0.0, 2, 0.0),
            (-0.04, 1, 0.0),
            (5e-324, 324, 5e-324),
            (5e-324, 323, 0.0),
            (sys.float_info.max, 0, sys.float_info.max),
        ):
            with self.subTest(value=value, digits=digits):
                self.assertEqual(
                    rounding.rational_round(value, digits), rounding.bits(expected)
                )

    def test_near_tie_interval_is_inclusive_and_sign_symmetric(self):
        """The exact representable lower endpoint rounds away, its predecessor does not."""
        threshold = 0.5 - 2.0**-26
        for sign in (1, -1):
            for value, expected in (
                (math.nextafter(threshold, 0.0), 0.0),
                (threshold, sign * 1.0),
                (math.nextafter(threshold, 1.0), sign * 1.0),
            ):
                self.assertEqual(
                    rounding.rational_round(sign * value, 0), rounding.bits(expected)
                )
        self.assertEqual(
            rounding.rational_round(50.0 - 100.0 / (1 << 26), -2), rounding.bits(100.0)
        )

    def test_extreme_digits_and_result_overflow(self):
        """Unbounded decimal powers are unnecessary for the full i64 digit range."""
        for value in (
            0.0,
            -0.0,
            5e-324,
            -5e-324,
            sys.float_info.max,
            -sys.float_info.max,
        ):
            self.assertEqual(
                rounding.rational_round(value, -(1 << 63)), rounding.bits(0.0)
            )
            self.assertEqual(
                rounding.rational_round(value, (1 << 63) - 1),
                rounding.bits(value or 0.0),
            )
        self.assertEqual(rounding.rational_round(sys.float_info.max, -308), "missing")
        self.assertEqual(
            rounding.rational_round(sys.float_info.max, -309), rounding.bits(0.0)
        )

    def test_report_retains_wrong_bits_and_escaped_exceptions(self):
        """A wrong result and an exception both block qualification; neither becomes truth."""
        report = rounding.assess([("wrong", 0.5, 0)], lambda value, digits: 0.0)
        self.assertEqual(report["counts"], {"samples": 1, "exact": 0, "mismatches": 1})
        self.assertEqual(report["qualification"], "blocked-by-mismatches")
        self.assertEqual(report["mismatches"][0]["rational"], rounding.bits(1.0))

        def overflowing(value, digits):
            """Represent the reference helper escaping with a rounding overflow."""
            raise OverflowError("not an expected result")

        report = rounding.assess([("overflow", sys.float_info.max, -308)], overflowing)
        self.assertEqual(
            report["mismatches"][0]["reference"], "exception:OverflowError"
        )
        self.assertTrue(report["rust_rounding_remains_unsupported"])
        exact = rounding.assess([("exact", 1.25, 1)], lambda value, digits: 1.3)
        self.assertEqual(exact["qualification"], "not-qualified")

    def test_corpus_identity_and_invalid_assessments(self):
        """Ensure the specified corpus is nonempty, stable and unambiguously named."""
        cases = rounding.cases()
        self.assertEqual(len(cases), 2936)
        self.assertEqual(len({case for case, _, _ in cases}), 2936)
        with self.assertRaises(ValueError):
            rounding.assess([], lambda value, digits: value)
        with self.assertRaises(ValueError):
            rounding.assess([("x", 1.0, 0), ("x", 1.0, 0)], lambda value, digits: value)
        for value, digits in ((math.inf, 1), (math.nan, 1), (1.0, 1.0), (1.0, True)):
            with self.assertRaises(ValueError):
                rounding.rational_round(value, digits)
