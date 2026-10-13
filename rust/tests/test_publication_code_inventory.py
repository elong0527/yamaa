"""A newly introduced real failure cannot evade prepublication report admission."""
import importlib.util
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location(
    "publication_codes", ROOT / "rust/tools/check_publication_report_codes.py")
codes = importlib.util.module_from_spec(spec)
spec.loader.exec_module(codes)


class PublicationCodes(unittest.TestCase):
    def test_actual_authorities_are_exhaustive(self):
        self.assertEqual(codes.check(), set())

    def test_new_real_limit_or_original_core_failure_requires_admitted_storage(self):
        sources = [(ROOT / path).read_text() for path in codes.AUTHORITIES]
        report = (ROOT / codes.REPORT).read_text()
        self.assertEqual(codes.missing_codes(
            sources + ['Err(Failure::Limit("producer_publication_shape"))'], report),
            {"producer_publication_shape"})
        self.assertIn(codes.CORE, codes.missing_codes(
            sources, report.replace(codes.CORE, "FuturePublicationCode")))
