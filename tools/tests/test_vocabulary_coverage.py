"""Authored vocabulary coverage retains independent failure contracts."""

import unittest

from tools.validation import check_vocabulary_coverage as OWNER_VOCABULARY


class TestVocabularyCoverage(unittest.TestCase):
    def test_shared_refusals_cover_the_declared_operation(self):
        self.assertEqual(
            OWNER_VOCABULARY.check_shared(
                "date_shift",
                ["date_shift_float_days", "date_shift_upper_overflow"],
            ),
            [],
        )

    def test_successful_shared_case_cannot_cover_a_failure_contract(self):
        errors = OWNER_VOCABULARY.check_shared(
            "date_shift", ["date_literal_days_and_phase_dates"]
        )
        self.assertTrue(any("lacks a refusal contract" in error for error in errors))

    def test_missing_and_unrelated_shared_cases_are_refused(self):
        errors = OWNER_VOCABULARY.check_shared(
            "duration_value", ["absent_case", "date_shift_upper_overflow"]
        )
        self.assertTrue(any("missing absent_case" in error for error in errors))
        self.assertTrue(any("never names duration_value" in error for error in errors))

    def test_original_benchmark_refusal_owner_is_preserved(self):
        self.assertEqual(
            OWNER_VOCABULARY.check(
                "date_diff",
                ["negative-date-diff-endpoints", "negative-date-diff-units"],
            ),
            [],
        )

    def test_metadata_coverage_names_only_registered_operations_and_original_case_owners(
        self,
    ):
        coverage = OWNER_VOCABULARY.declared_coverage()
        self.assertEqual(set(coverage), set(OWNER_VOCABULARY.registered_operations()))
        self.assertEqual(
            coverage["date_diff"]["benchmarks"],
            ["negative-date-diff-endpoints", "negative-date-diff-units"],
        )
        self.assertIn(
            "date_shift_upper_overflow", coverage["date_shift"]["shared_cases"]
        )
        self.assertEqual(OWNER_VOCABULARY.main(), 0)


if __name__ == "__main__":
    unittest.main()
