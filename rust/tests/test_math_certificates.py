"""Test independent certificates at exact binary64 rounding and language boundaries."""

import importlib.util
import json
import struct
import unittest
from fractions import Fraction
from pathlib import Path
from unittest.mock import patch

spec = importlib.util.spec_from_file_location(
    "math_certificates", Path(__file__).parents[1] / "tools/math_certificates.py"
)
certificates = importlib.util.module_from_spec(spec)
spec.loader.exec_module(certificates)


def bits(value):
    """Encode fixture inputs exactly; fixture expected bits are written independently."""
    return struct.pack(">d", value).hex()


class MathCertificateTests(unittest.TestCase):
    """Require numerical claims to follow independent enclosures, including wrong candidates."""

    def test_known_transcendentals_use_independent_correctly_rounded_operations(self):
        """Use documented Decimal operations rather than the candidate or platform math."""
        for function, inputs, expected, method in (
            ("EXP", [bits(1.0)], "4005bf0a8b145769", "decimal-enclosure"),
            ("LN", [bits(2.0)], "3fe62e42fefa39ef", "decimal-enclosure"),
            (
                "POWER",
                [bits(2.0), bits(0.5)],
                "3ff6a09e667f3bcd",
                "python-decimal-power-enclosure",
            ),
        ):
            with self.subTest(function=function):
                report = certificates.certify(function, inputs, expected)
                self.assertEqual(report["status"], "certified")
                self.assertEqual(report["nearest_binary64"], expected)
                self.assertEqual(report["comparison"], "matches")
                self.assertEqual(report["method"], method)
                self.assertEqual(report["proof"]["precision_decimal_digits"], 160)
                json.dumps(report)

    def test_candidate_changes_only_the_comparison(self):
        """A wrong adjacent candidate must not select its own rounding cell or truth."""
        correct = certificates.certify("EXP", [bits(1.0)], "4005bf0a8b145769")
        wrong = certificates.certify("EXP", [bits(1.0)], "4005bf0a8b145768")
        missing = certificates.certify("EXP", [bits(1.0)], "missing")
        for report in (wrong, missing):
            self.assertEqual(report["comparison"], "differs")
            self.assertEqual(report["nearest_binary64"], correct["nearest_binary64"])
            self.assertEqual(report["proof"], correct["proof"])

    def test_binary64_input_is_not_a_short_decimal_approximation(self):
        """Retain the full exact promoted 0.1 input before high-precision evaluation."""
        report = certificates.certify("EXP", [bits(0.1)], "missing")
        self.assertEqual(
            report["proof"]["input_decimals"],
            ["0.1000000000000000055511151231257827021181583404541015625"],
        )

    def test_power_uses_pure_python_decimal_not_c_decimal_power(self):
        """Changing the C POWER implementation cannot influence the independent certificate."""
        with patch.object(
            certificates.decimal,
            "Context",
            side_effect=AssertionError("C context used"),
        ):
            report = certificates.certify(
                "POWER", [bits(2.0), bits(0.5)], "3ff6a09e667f3bcd"
            )
        self.assertEqual(report["proof"]["decimal_implementation"], "_pydecimal")

    def test_rational_power_retains_exact_underflow_ties_and_sign(self):
        """Integral powers decide zero/subnormal halfway cases with exact fractions."""
        for base, exponent, expected in (
            (2.0, -1075.0, "0000000000000000"),
            (-2.0, -1075.0, "8000000000000000"),
            (2.0, -1074.0, "0000000000000001"),
            (-2.0, -1073.0, "8000000000000002"),
            (2.0, 1024.0, "missing"),
            (-2.0, 1025.0, "missing"),
        ):
            with self.subTest(base=base, exponent=exponent):
                report = certificates.certify(
                    "POWER", [bits(base), bits(exponent)], expected
                )
                self.assertEqual(report["status"], "certified")
                self.assertEqual(report["method"], "exact-rational-power")
                self.assertEqual(report["nearest_binary64"], expected)
                self.assertEqual(report["comparison"], "matches")

    def test_subnormal_and_normal_midpoints_use_ties_to_even(self):
        """Check both parities and the normal/subnormal boundary without Decimal output."""
        q = certificates.MIN_SUBNORMAL
        for exact, expected in (
            (q / 2, 0),
            (q * Fraction(3, 2), 2),
            (q * Fraction(5, 2), 2),
            (Fraction(1) + Fraction(1, 1 << 53), 0x3FF0000000000000),
            (Fraction(1) + Fraction(3, 1 << 53), 0x3FF0000000000002),
            (q * ((1 << 52) - Fraction(1, 2)), 0x0010000000000000),
        ):
            with self.subTest(exact=exact):
                self.assertEqual(certificates._nearest_bits(exact), expected)
                self.assertTrue(certificates._inside_cell(exact, exact, expected))
                self.assertEqual(
                    certificates._nearest_bits(-exact), certificates.SIGN | expected
                )

    def test_overflow_halfway_tie_and_maximum_finite_cell(self):
        """The overflow tie belongs to infinity; the odd maximum finite cell excludes it."""
        threshold = certificates.OVERFLOW_THRESHOLD
        self.assertEqual(
            certificates._nearest_bits(threshold), certificates.POSITIVE_INFINITY
        )
        self.assertEqual(
            certificates._nearest_bits(threshold - 1), certificates.MAX_FINITE
        )
        self.assertTrue(
            certificates._inside_cell(
                threshold, threshold + 1, certificates.POSITIVE_INFINITY
            )
        )
        self.assertFalse(
            certificates._inside_cell(threshold - 1, threshold, certificates.MAX_FINITE)
        )
        self.assertEqual(
            certificates._nearest_bits(-threshold),
            certificates.SIGN | certificates.POSITIVE_INFINITY,
        )

    def test_enclosures_crossing_cells_are_explicitly_excluded(self):
        """An insufficiently precise independent bound cannot become a numerical pass."""
        midpoint = Fraction(1) + Fraction(1, 1 << 53)
        report = certificates._certified(
            {
                "candidate": "3ff0000000000000",
                "nearest_binary64": None,
                "comparison": "not-assessed",
            },
            midpoint - Fraction(1, 1 << 160),
            midpoint + Fraction(1, 1 << 160),
            "fixture",
            {},
        )
        self.assertEqual(report["status"], "excluded")
        self.assertIsNone(report["nearest_binary64"])
        self.assertEqual(report["comparison"], "not-assessed")

    def test_decimal_exp_boundaries_cover_finite_zero_and_missing(self):
        """A Decimal enclosure must decide underflow and overflow, not just ordinary results."""
        for argument, expected in (
            (710.0, "missing"),
            (-746.0, "0000000000000000"),
            (-744.0, "0000000000000002"),
        ):
            with self.subTest(argument=argument):
                report = certificates.certify(
                    "EXP", [bits(argument), bits(0.0)], expected
                )
                self.assertEqual(report["status"], "certified")
                self.assertEqual(report["nearest_binary64"], expected)
                self.assertEqual(report["comparison"], "matches")

    def test_language_missing_domain_and_identities_are_not_accuracy_claims(self):
        """Preserve semantic distinctions, including missing before POWER domain checks."""
        for function, inputs, expected in (
            ("EXP", [bits(-0.0)], bits(1.0)),
            ("LN", [bits(1.0)], bits(0.0)),
            ("LN", [bits(-0.0)], "REQ-0432"),
            ("POWER", [bits(0.0), bits(-1.0)], "REQ-0433"),
            ("POWER", [bits(-2.0), bits(0.5)], "REQ-0433"),
            ("POWER", ["missing", bits(-1.0)], "missing"),
            ("POWER", [bits(0.0), bits(0.0)], bits(1.0)),
            ("POWER", [bits(-0.0), bits(3.0)], bits(-0.0)),
            ("POWER", [bits(-0.0), bits(0.5)], bits(0.0)),
            ("POWER", [bits(-0.0), bits(1.0)], bits(-0.0)),
            ("POWER", [bits(-1.0), bits(3.0)], bits(-1.0)),
            ("POWER", [bits(1.0), "7fefffffffffffff"], bits(1.0)),
        ):
            with self.subTest(function=function, inputs=inputs):
                candidate = (
                    expected
                    if expected in ("missing",) or len(expected) == 16
                    else "missing"
                )
                report = certificates.certify(function, inputs, candidate)
                self.assertEqual(report["status"], "language")
                self.assertEqual(report["language_expected"], expected)
                self.assertIsNone(report["nearest_binary64"])
                self.assertNotIn("proof", report)

    def test_large_operations_are_excluded_before_decimal_work(self):
        """Bounded work produces honest exclusions rather than optimistic overflow claims."""
        with patch.object(
            certificates,
            "_decimal_certificate",
            side_effect=AssertionError("Decimal work entered"),
        ):
            for function, inputs in (
                ("EXP", ["7fefffffffffffff"]),
                ("POWER", [bits(2.0), bits(2049.0)]),
                ("POWER", ["0000000000000001", bits(2048.0)]),
                ("POWER", ["7fefffffffffffff", bits(10.5)]),
            ):
                with self.subTest(function=function, inputs=inputs):
                    report = certificates.certify(function, inputs, "missing")
                    self.assertEqual(report["status"], "excluded")
                    self.assertIsNone(report["nearest_binary64"])
                    self.assertEqual(report["comparison"], "not-assessed")

    def test_malformed_evidence_is_rejected(self):
        """Reject nonfinite, noncanonical and ambiguous operand records explicitly."""
        for function, inputs, result in (
            ("SQRT", [bits(2.0)], bits(1.0)),
            ("POWER", [bits(2.0)], bits(1.0)),
            ("EXP", [bits(2.0), bits(1.0)], bits(1.0)),
            ("LN", ["3FF0000000000000"], bits(0.0)),
            ("LN", ["7ff0000000000000"], "missing"),
            ("LN", [bits(1.0)], "7ff8000000000000"),
            ("LN", ["1"], bits(0.0)),
        ):
            with (
                self.subTest(function=function, inputs=inputs),
                self.assertRaises(ValueError),
            ):
                certificates.certify(function, inputs, result)


if __name__ == "__main__":
    unittest.main()
