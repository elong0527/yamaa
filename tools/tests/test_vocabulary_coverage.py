"""Authored vocabulary coverage retains independent failure contracts."""

import tempfile
import unittest
from pathlib import Path
from unittest import mock

from tools.validation import check_vocabulary_coverage as OWNER_VOCABULARY

FORMULA_ERROR = (
    "phase: validation\ncondition: prohibited_construct\n"
    "spec_paths:\n  - {path}\nrule: FORMULA-27\n"
)


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
                "diff",
                ["negative-date-diff-endpoints", "negative-date-diff-units"],
            ),
            [],
        )

    def test_a_spec_path_resolves_keyed_members_by_identity(self):
        document = {
            "columns": [
                {"name": "AGE", "derivation": {"case": [{"then": 1}]}},
                {"name": "SEX", "derivation": "DM.SEX"},
            ],
            "intermediates": [{"id": "LAST", "absent": 0}],
        }
        resolve = OWNER_VOCABULARY.resolve
        self.assertEqual(resolve(document, "columns.SEX.derivation"), "DM.SEX")
        self.assertEqual(resolve(document, "columns[1].derivation"), "DM.SEX")
        self.assertEqual(resolve(document, "columns.AGE.derivation.case[0].then"), 1)
        self.assertEqual(resolve(document, "intermediates.LAST.absent"), 0)
        self.assertIsNone(resolve({"then": None}, "then"))
        for missing in ("columns.AGE.label", "columns.BMI", "columns[2]", "base.x"):
            self.assertIs(resolve(document, missing), OWNER_VOCABULARY.UNRESOLVED)

    def test_a_1_x_negative_names_an_operation_its_grammar_text_holds(self):
        """1.0 writes `compute` and `literal` as grammar text (D18), so a 1.0
        negative covers them by what its error path holds."""
        with tempfile.TemporaryDirectory() as directory:
            case = Path(directory) / "negative-formula"
            (case / "expected").mkdir(parents=True)
            spec = case / "spec.yaml"
            error = case / "expected/error.yaml"
            error.write_text(FORMULA_ERROR.format(path="columns.HIFL.derivation"))
            with mock.patch.object(OWNER_VOCABULARY, "EXAMPLES", Path(directory)):
                check = OWNER_VOCABULARY.check
                for derivation, covered in (
                    ('"AVAL > ANRHI"', ["compute"]),
                    ("\"'H'\"", ["literal"]),
                    ("1", ["literal"]),
                    ("null", ["literal"]),
                    ("AVAL", []),
                ):
                    spec.write_text(
                        'yamaa: "1.0"\ntype: spec\ncolumns:\n'
                        f"  - {{name: HIFL, derivation: {derivation}}}\n"
                    )
                    for operation in ("compute", "literal"):
                        with self.subTest(derivation=derivation, operation=operation):
                            self.assertEqual(
                                check(operation, ["negative-formula"]) == [],
                                operation in covered,
                            )
                # A 0.5 negative names the operation itself.
                spec.write_text(
                    'schema_version: "1.0"\ncolumns:\n'
                    '  - {name: HIFL, derivation: "AVAL > ANRHI"}\n'
                )
                self.assertNotEqual(check("compute", ["negative-formula"]), [])
                error.write_text(
                    FORMULA_ERROR.format(path="columns.HIFL.derivation.compute.expr")
                )
                self.assertEqual(check("compute", ["negative-formula"]), [])

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


if __name__ == "__main__":
    unittest.main()
