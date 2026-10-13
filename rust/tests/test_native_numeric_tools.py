"""Retained native qualification cannot depend on the retiring reference families."""

import builtins
import importlib
import json
import sys
import unittest
from pathlib import Path
from unittest.mock import patch

TOOLS = Path(__file__).parents[1] / "tools"
sys.path.insert(0, str(TOOLS))


class NativeToolConsumers(unittest.TestCase):
    """Check independence, exact input identity and truthful coverage disposition."""

    def test_all_active_tools_import_with_reference_families_denied(self):
        original = builtins.__import__
        retired = (
            "yamaa.expressions",
            "yamaa.runtime",
            "yamaa.planning",
            "yamaa.specification",
            "yamaa.regex",
            "yamaa._reference",
        )

        def guarded(name, *args, **kwargs):
            if name.startswith(retired):
                raise AssertionError(f"retiring reference import: {name}")
            return original(name, *args, **kwargs)

        with patch("builtins.__import__", side_effect=guarded):
            for name in (
                "check_numeric_grammar",
                "check_aggregate_grammar",
                "check_numeric_syntax",
                "check_predicate_syntax",
                "assess_regex_reference",
                "assess_rounding",
                "assess_math_decision",
            ):
                with self.subTest(tool=name):
                    importlib.reload(importlib.import_module(name))
            decision = importlib.import_module("assess_math_decision")
            from math_decision_corpus import cases

            archive = json.loads(
                (TOOLS.parent / "qualification/math-decision-v1.json").read_text()
            )
            for case, row in zip(cases(), archive["boundaries"], strict=True):
                self.assertEqual(decision.historical(case), row["historical"])
            for row in archive["integer_handlers"]:
                self.assertEqual(
                    decision.historical(row["input"], "int", True), row["historical"]
                )
            changed = {**cases()[0], "inputs": ["4000000000000000"]}
            with self.assertRaises(ValueError):
                decision.historical(changed)
            with self.assertRaises(ValueError):
                decision.historical(cases()[0], "int", False)

    def test_former_differential_input_counts_and_independent_controls_remain_visible(
        self,
    ):
        import check_aggregate_grammar as aggregate
        import check_numeric_syntax as numeric
        import check_predicate_syntax as predicate
        import native_syntax_checks as syntax

        self.assertEqual(len(numeric.cases()), 136)
        self.assertEqual(len(numeric.cases()) + len(numeric.numeric_sources()), 2444)
        self.assertEqual(len(aggregate.cases()), 166)
        self.assertEqual(len(aggregate.cases()) + len(aggregate.samples()), 3180)
        self.assertEqual(len(predicate.cases()), 72)
        self.assertEqual(len(predicate.cases()) + len(predicate.samples()), 1088)
        for family, count in (("numeric", 8), ("aggregate", 6), ("predicate", 11)):
            self.assertEqual(len(syntax.controls(family)), count)
        with self.assertRaises(ValueError):
            syntax.replay("reference", [])

    def test_compiled_rounding_reports_every_policy_and_bit_difference(self):
        from assess_rounding import compiled_assessment

        sample = [("tie", 0.5, 0)]
        lines = [
            "tie\tReferenceSubset\t3fe0000000000000\t0\t3ff0000000000000",
            "tie\tPortableLibmV1\t3fe0000000000000\t0\t0000000000000000",
        ]
        report = compiled_assessment(sample, lines)
        self.assertEqual(
            report["counts"],
            {"samples": 1, "observations": 2, "exact": 1, "mismatches": 1},
        )
        self.assertEqual(report["qualification"], "blocked-by-mismatches")
        self.assertEqual(report["mismatches"][0]["policy"], "PortableLibmV1")
        self.assertEqual(report["mismatches"][0]["rational"], "3ff0000000000000")
        self.assertEqual(report["mismatches"][0]["candidate"], "0000000000000000")
        for poisoned in (
            lines[:1],
            lines[::-1],
            [lines[0].replace("3fe", "3ff"), lines[1]],
            [lines[0].replace("\t0\t", "\t1\t"), lines[1]],
        ):
            with self.subTest(poisoned=poisoned), self.assertRaises(ValueError):
                compiled_assessment(sample, poisoned)

    def test_regex_assessment_keeps_exact_capture_mismatches(self):
        import assess_regex_reference as regex

        with (
            patch.object(
                regex, "replay", side_effect=lambda rows: [row[3] for row in rows]
            ),
            patch.object(regex, "check", return_value=None),
        ):
            report = regex.assess()
        self.assertEqual(len(report["cases"]), 55)
        self.assertEqual(report["mismatches"], 0)
        self.assertEqual(report["qualification"], "not-qualified")
        with patch.object(
            regex,
            "replay",
            side_effect=lambda rows: [{"accepted": False} for _ in rows],
        ):
            poisoned = regex.assess()
        self.assertGreater(poisoned["mismatches"], 0)
        self.assertTrue(
            all(row["candidate"] == {"accepted": False} for row in poisoned["cases"])
        )


if __name__ == "__main__":
    unittest.main()
