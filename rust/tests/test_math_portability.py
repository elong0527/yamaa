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
