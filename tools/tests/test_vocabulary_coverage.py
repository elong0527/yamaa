"""Authored vocabulary coverage retains independent failure contracts."""

import unittest

from tools.validation import check_vocabulary_coverage as OWNER_VOCABULARY


class TestVocabularyCoverage(unittest.TestCase):
    def test_shared_refusals_cover_the_declared_operation(self):
        self.assertEqual(
            OWNER_VOCABULARY.check_shared(
                "shift",
                ["date_shift_float_days", "date_shift_upper_overflow"],
            ),
            [],
        )

    def test_successful_shared_case_cannot_cover_a_failure_contract(self):
        errors = OWNER_VOCABULARY.check_shared(
            "shift", ["date_literal_days_and_phase_dates"]
        )
        self.assertIn("lacks a refusal contract", "\n".join(errors))

    def test_missing_and_unrelated_shared_cases_are_refused(self):
        errors = OWNER_VOCABULARY.check_shared(
            "duration_value", ["absent_case", "date_shift_upper_overflow"]
        )
        self.assertIn("missing absent_case", "\n".join(errors))
        self.assertIn("never names duration_value", "\n".join(errors))

    def test_original_benchmark_refusal_owner_is_preserved(self):
        self.assertEqual(
            OWNER_VOCABULARY.check(
                "diff",
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
            coverage["diff"]["benchmarks"],
            ["negative-date-diff-endpoints", "negative-date-diff-units"],
        )
        self.assertIn("date_shift_upper_overflow", coverage["shift"]["shared_cases"])
        self.assertEqual(OWNER_VOCABULARY.main(), 0)

    def test_the_1_0_registry_names_function_and_neither_compute_nor_literal(self):
        # A formula and a literal are grammar text in 1.0 (#3030 D18), and a
        # project function call is an operation (D8).
        registered = OWNER_VOCABULARY.registered_operations()
        self.assertEqual(
            {name: registered.get(name) for name in ("compute", "literal", "function")},
            {
                "compute": None,
                "literal": None,
                "function": "schema_expression_function.yaml",
            },
        )
        self.assertEqual(
            OWNER_VOCABULARY.declared_coverage()["function"],
            {
                "benchmarks": [
                    "negative-function-contract",
                    "negative-function-not-listed",
                ],
                "shared_cases": [],
            },
        )


if __name__ == "__main__":
    unittest.main()
