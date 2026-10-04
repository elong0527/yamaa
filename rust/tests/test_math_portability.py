"""Prove the portability check rejects changes hidden by numeric equality or coverage."""

import copy
import importlib.util
import sys
import unittest
from pathlib import Path

TOOLS = Path(__file__).parents[1] / "tools"
sys.path.insert(0, str(TOOLS))
spec = importlib.util.spec_from_file_location(
    "check_math_portability", TOOLS / "check_math_portability.py"
)
portability = importlib.util.module_from_spec(spec)
spec.loader.exec_module(portability)


class PortabilityTests(unittest.TestCase):
    """Use independent synthetic observations, not outputs promoted from an engine."""

    def report(self):
        """Supply the exact LN(1) identity as a minimal report."""
        return {
            "schema_version": 2,
            "policy": "PortableLibmV1",
            "candidate_results": [
                {
                    "case": "one",
                    "function": "LN",
                    "inputs": ["3ff0000000000000", "0000000000000000"],
                    "result": "0000000000000000",
                }
            ],
        }

    def test_matches_are_exact(self):
        """Identical observations pass without treating them as independent truth."""
        self.assertEqual(portability.compare([self.report(), self.report()]), 1)

    def test_rejects_bit_missing_and_input_differences(self):
        """Reject even zero-sign differences and a single adjacent bit."""
        for value in ("8000000000000000", "0000000000000001", "missing"):
            report = self.report()
            report["candidate_results"][0]["result"] = value
            with self.assertRaises(AssertionError):
                portability.compare([self.report(), report])
        report = self.report()
        report["candidate_results"][0]["inputs"][0] = "3ff0000000000001"
        with self.assertRaises(AssertionError):
            portability.compare([self.report(), report])

    def test_rejects_incomplete_or_duplicate_samples(self):
        """Missing or duplicated records cannot inflate sampled coverage."""
        report = self.report()
        report["candidate_results"] = []
        with self.assertRaises(AssertionError):
            portability.compare([self.report(), report])
        report = self.report()
        report["candidate_results"].append(
            copy.deepcopy(report["candidate_results"][0])
        )
        with self.assertRaises(ValueError):
            portability.observations(report)
        with self.assertRaises(ValueError):
            portability.compare([self.report()])

    def test_rejects_wrong_policy_and_nonfinite_records(self):
        """Only normalized outputs from the declared policy enter exact comparisons."""
        report = self.report()
        report["policy"] = "unknown"
        with self.assertRaises(ValueError):
            portability.observations(report)
        report = self.report()
        report["candidate_results"][0]["result"] = "7ff0000000000000"
        with self.assertRaises(ValueError):
            portability.observations(report)


class CorpusTests(unittest.TestCase):
    """Bind a complete agreeing sample to independent identities and inputs."""

    def test_rejects_common_case_replacement_with_unchanged_counts(self):
        """All hosts omitting the same case must fail despite equal per-function counts."""
        from math_corpus import expected_inputs, validate_corpus

        rows = {key: (value, "missing") for key, value in expected_inputs().items()}
        validate_corpus(rows)
        row = rows.pop(("boundary-0", "EXP"))
        rows["replacement", "EXP"] = row
        with self.assertRaisesRegex(AssertionError, "case identities"):
            validate_corpus(rows)

    def test_rejects_common_input_bit_drift(self):
        """Stable case IDs cannot hide a producer changing an input's zero sign."""
        from math_corpus import expected_inputs, validate_corpus

        expected = expected_inputs()
        self.assertEqual(len(expected), 30033)
        self.assertEqual(
            expected["boundary-0", "EXP"], ("8000000000000000", "0000000000000000")
        )
        self.assertEqual(
            expected["boundary-3", "LN"], ("0000000000000001", "0000000000000000")
        )
        rows = {key: (value, "missing") for key, value in expected.items()}
        rows["boundary-0", "EXP"] = (
            ("0000000000000000", "0000000000000000"),
            "missing",
        )
        with self.assertRaisesRegex(AssertionError, "inputs differ"):
            validate_corpus(rows)
