"""Audit retained numerical decisions without generating or loosening goldens."""

import json
import os
import sys
import tempfile
import unittest
from collections import Counter
from pathlib import Path
from types import ModuleType
from unittest.mock import patch

TOOLS = Path(__file__).parents[1] / "tools"
sys.path.insert(0, str(TOOLS))
import assess_math_decision
from assess_math import difference
from assess_math_decision import native_request
from math_certificates import certify
from math_corpus import expected_inputs
from math_decision_corpus import cases


class TemporaryWorkspaceCleanup(unittest.TestCase):
    """Temporary directories can be removed on hosts that lock the current cwd."""

    def checked_directory(self):
        previous = Path.cwd()
        directory = tempfile.TemporaryDirectory(prefix="yamaa-math-cleanup-test-")
        owner = self

        class CheckedDirectory:
            def __enter__(self):
                return directory.__enter__()

            def __exit__(self, *exception):
                owner.assertEqual(Path.cwd(), previous)
                return directory.__exit__(*exception)

        return CheckedDirectory()

    def test_success_restores_directory_before_cleanup(self):
        directory = self.checked_directory()
        with patch.object(
            assess_math_decision.tempfile, "TemporaryDirectory", return_value=directory
        ):
            with assess_math_decision.temporary_workspace("unused-") as temporary:
                os.chdir(temporary)
                Path("evidence.txt").write_text("retained", encoding="ascii")
            self.assertFalse(Path(temporary).exists())

    def test_both_witnesses_restore_directory_before_exception_cleanup(self):
        failure = RuntimeError("original witness failure")
        fake = ModuleType("yamaa")
        fake._native = ModuleType("yamaa._native")
        reference = ModuleType("yamaa._reference_domain")
        reference.yamaa_domain = lambda *args, **kwargs: None
        modules = {
            "yamaa": fake,
            "yamaa._reference_domain": reference,
            "yaml": ModuleType("yaml"),
        }

        def refuse(*args, **kwargs):
            raise failure

        def copy_fixture(source, target):
            target.mkdir()

        fake.domain = refuse
        for witness, args in (
            (assess_math_decision.downstream, [("test", "3ff0000000000000")]),
            (assess_math_decision.fixture_evidence, None),
        ):
            with self.subTest(witness=witness.__name__):
                directory = self.checked_directory()
                with (
                    patch.dict(sys.modules, modules),
                    patch.object(assess_math_decision.shutil, "copytree", copy_fixture),
                    patch.object(
                        assess_math_decision.tempfile,
                        "TemporaryDirectory",
                        return_value=directory,
                    ),
                    self.assertRaises(RuntimeError) as caught,
                ):
                    witness() if args is None else witness(args)
                self.assertIs(caught.exception, failure)


class MathDecisionEvidence(unittest.TestCase):
    """Retained raw inputs, classes and exact observables are independently pinned."""

    @classmethod
    def setUpClass(cls):
        cls.report = json.loads(
            (TOOLS.parent / "qualification/math-decision-v1.json").read_text()
        )

    def test_input_corpus_is_bounded_and_contains_independent_named_witnesses(self):
        declarations = cases()
        self.assertEqual(len(declarations), 58)
        self.assertEqual(len({case["id"] for case in declarations}), 58)
        actual = {case["id"]: case["inputs"] for case in declarations}
        self.assertEqual(actual["exp-one"], ["3ff0000000000000"])
        self.assertEqual(
            actual["power-integer-admitted"],
            ["40689ed31ed9c3bf", "401b2641709adeb1"],
        )
        self.assertEqual(
            actual["power-integer-refused"],
            ["3fcfffffffffffff", "bff0000000000000"],
        )
        self.assertEqual(actual["exp-zero-edge-0"], ["c0874910d52d3052"])
        self.assertEqual(
            [row["input"] for row in self.report["boundaries"]], declarations
        )

    def test_integer_promotion_retains_original_input_and_promoted_bits(self):
        case = next(case for case in cases() if case["id"] == "power-promoted-odd")
        request = native_request(case)
        self.assertEqual(request["bindings"][1]["value"], {"int": "9007199254740993"})
        self.assertEqual(case["inputs"][1], "4340000000000000")

    def test_sample_union_retains_every_host_and_exact_original_input(self):
        inputs = expected_inputs()
        sample = self.report["sample"]
        self.assertEqual(sample["observations_per_report"], 30033)
        hosts = {report["identity"] for report in sample["reports"]}
        self.assertEqual(len(hosts), 9)
        union = sample["mismatch_union"]
        self.assertEqual(
            Counter(row["function"] for row in union),
            {"EXP": 1006, "LN": 280, "POWER": 960},
        )
        self.assertEqual(len({(r["case"], r["function"]) for r in union}), 2246)
        for row in union:
            self.assertEqual(tuple(row["inputs"]), inputs[row["case"], row["function"]])
            retained = [
                host for history in row["historical"] for host in history["reports"]
            ]
            self.assertEqual(len(retained), 9)
            self.assertEqual(set(retained), hosts)
            differences = [
                difference(row["candidate"], history["result"])
                for history in row["historical"]
            ]
            self.assertTrue(any(category is not None for category, _ in differences))
            self.assertTrue(
                all(item in ((None, None), ("finite_bits", 1)) for item in differences)
            )

    def test_accuracy_is_independent_and_every_exclusion_is_explicit(self):
        for row in self.report["boundaries"]:
            actual = row["accuracy"]
            candidate = actual["candidate"]
            independent = certify(
                row["input"]["function"], row["input"]["inputs"], candidate
            )
            self.assertEqual(independent, actual)
            poisoned = certify(
                row["input"]["function"], row["input"]["inputs"], "missing"
            )
            self.assertEqual(poisoned["nearest_binary64"], actual["nearest_binary64"])
            self.assertEqual(poisoned.get("proof"), actual.get("proof"))
            if actual["status"] == "excluded":
                self.assertIsNone(actual["nearest_binary64"])
                self.assertEqual(actual["comparison"], "not-assessed")
                self.assertTrue(actual["reason"])
        matches = Counter()
        for row in self.report["sample"]["mismatch_union"]:
            accuracy = row["accuracy"]
            self.assertEqual(
                accuracy, certify(row["function"], row["inputs"], row["candidate"])
            )
            self.assertEqual(accuracy["status"], "certified")
            self.assertEqual(accuracy["candidate"], row["candidate"])
            if accuracy["comparison"] == "matches":
                matches[row["function"]] += 1
        self.assertEqual(dict(matches), {"EXP": 23, "POWER": 7})
        self.assertEqual(
            Counter(
                (row["accuracy"]["status"], row["accuracy"]["comparison"])
                for row in self.report["boundaries"]
            ),
            {
                ("certified", "matches"): 31,
                ("certified", "differs"): 2,
                ("language", "matches"): 15,
                ("language", "not-assessed"): 6,
                ("excluded", "not-assessed"): 4,
            },
        )

    def test_domain_refusals_retain_complete_independent_diagnostic_truth(self):
        expected = {
            "ln-positive-zero": ("LN", None),
            "ln-negative-zero": ("LN", None),
            "ln-negative": ("LN", None),
            "power-positive-zero-negative": (
                "POWER",
                ("0000000000000000", "bff0000000000000"),
            ),
            "power-negative-zero-negative": (
                "POWER",
                ("8000000000000000", "bff0000000000000"),
            ),
            "power-negative-fraction": (
                "POWER",
                ("c000000000000000", "3fe0000000000000"),
            ),
        }
        for row in self.report["boundaries"]:
            if row["input"]["id"] not in expected:
                continue
            function, operands = expected[row["input"]["id"]]
            context = {"expr": {"str": "LN(A)" if function == "LN" else "POWER(A, B)"}}
            if operands is not None:
                context.update(
                    base={"float": operands[0]}, exponent={"float": operands[1]}
                )
            diagnostic = {
                "phase": "derivation",
                "condition": "ln_of_nonpositive"
                if function == "LN"
                else "invalid_power",
                "requirement": "REQ-0432" if function == "LN" else "REQ-0433",
                "spec_paths": ["columns.VALUE.derivation.value.compute"],
                "context": context,
            }
            self.assertEqual(
                row["historical"]["outcome"],
                {"status": "failure", "diagnostic": diagnostic},
            )
            self.assertEqual(
                row["native"]["outcome"],
                {
                    "status": "failure",
                    "diagnostic": {
                        **diagnostic,
                        "source_span": {
                            "start": "0",
                            "end": "5" if function == "LN" else "11",
                        },
                        "operand_route": [],
                    },
                },
            )
            self.assertEqual(row["historical"]["handler_counts"], [])
            self.assertEqual(row["native"]["handler_counts"], [])
            self.assertEqual(
                row["native"]["resolutions"], ["A"] if function == "LN" else ["A", "B"]
            )

    def test_exp_one_retains_predicate_verification_rounding_and_complete_csv(self):
        historical, candidate = self.report["exp_one_downstream"]
        self.assertEqual(historical["input_bits"], "4005bf0a8b145769")
        self.assertEqual(candidate["input_bits"], "4005bf0a8b14576a")
        self.assertEqual(historical["rows"][0]["ABOVE"], {"int": "0"})
        self.assertEqual(candidate["rows"][0]["ABOVE"], {"int": "1"})
        self.assertEqual(candidate["rows"][0]["ROUNDED"], {"float": "4005bf0a8b14576b"})
        self.assertTrue(historical["verification_succeeded"])
        self.assertEqual(historical["verification_issues"], [])
        self.assertFalse(candidate["verification_succeeded"])
        self.assertEqual(
            candidate["verification_issues"],
            [
                {
                    "phase": "verification",
                    "condition": "assert_failed",
                    "requirement": "REQ-0384",
                    "spec_paths": ["verifications[0].assert"],
                    "context": '{"failure_count":1,"keys":[{"ID":1}],"verification_id":"historical-exp-one-limit"}',
                }
            ],
        )
        self.assertEqual(
            bytes.fromhex(historical["canonical_csv_hex"]),
            b"ID,VALUE,ABOVE,ROUNDED\n1,2.718281828459045,0,2.718281828459045\n",
        )
        self.assertEqual(
            bytes.fromhex(candidate["canonical_csv_hex"]),
            b"ID,VALUE,ABOVE,ROUNDED\n1,2.7182818284590455,1,2.718281828459046\n",
        )

    def test_both_integer_handler_directions_remain_observable(self):
        admitted, refused = self.report["integer_handlers"]
        self.assertEqual(
            admitted["native"]["outcome"]["value"], {"int": "3739524560436070"}
        )
        self.assertEqual(admitted["historical"]["outcome"]["value"], {"int": "9"})
        self.assertEqual(refused["native"]["outcome"]["value"], {"int": "9"})
        self.assertEqual(refused["historical"]["outcome"]["value"], {"int": "4"})
        for row, native_count, reference_count in (
            (admitted, "0", "1"),
            (refused, "1", "0"),
        ):
            for name, count in (
                ("native", native_count),
                ("historical", reference_count),
            ):
                self.assertEqual(
                    row[name]["handler_counts"],
                    [
                        {
                            "spec_path": "columns.VALUE.derivation.unconvertible",
                            "handler": "unconvertible",
                            "count": count,
                        }
                    ],
                )

    def test_affected_artifacts_match_original_truth_and_blocker_is_not_a_pass(self):
        fixtures = self.report["affected_fixtures"]
        self.assertEqual(
            {row["name"] for row in fixtures},
            {
                "adam-adeg-derived-intervals",
                "schema-numeric-functions",
                "negative-log-zero",
                "adam-adsl-bmi",
                "adam-advs-percentiles",
            },
        )
        blocked = next(
            row for row in fixtures if row["name"] == "adam-adeg-derived-intervals"
        )
        self.assertEqual(blocked["native_artifacts"], [])
        self.assertEqual(
            blocked["qualification"], "frontend-blocked; numerical isolation only"
        )
        self.assertEqual(
            blocked["native_issues"],
            [
                {
                    "phase": "planning",
                    "condition": "unsupported_operation",
                    "requirement": None,
                    "spec_paths": ["rows[1].derivations.QTVAL.aggregate.filter"],
                    "context": '{"operation":"filter"}',
                }
            ],
        )
        self.assertEqual(
            blocked["follow_up"],
            "#1751 shared filtered aggregate, then #1742 full fixture qualification",
        )
        left, right = blocked["policy_isolation"]
        self.assertEqual(left["calls"], right["calls"])
        self.assertEqual(len(left["calls"]), 8)
        for row in fixtures:
            artifacts = row["native_artifacts"]
            for isolation in row.get("policy_isolation", []):
                artifacts = artifacts + isolation["artifacts"]
            for artifact in artifacts:
                original = (
                    TOOLS.parents[1]
                    / "benchmarks"
                    / row["name"]
                    / "expected"
                    / artifact["name"]
                )
                self.assertEqual(
                    bytes.fromhex(artifact["expected_hex"]), original.read_bytes()
                )
                self.assertEqual(artifact["actual_hex"], artifact["expected_hex"])


if __name__ == "__main__":
    unittest.main()
