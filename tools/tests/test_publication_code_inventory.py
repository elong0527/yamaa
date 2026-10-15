"""A newly introduced real failure cannot evade prepublication report admission."""

import unittest

from tools.paths import ROOT
from tools.validation import check_publication_report_codes as codes


class PublicationCodes(unittest.TestCase):
    def test_actual_authorities_are_exhaustive(self):
        self.assertEqual(codes.check(), set())

    def test_new_real_limit_or_original_core_failure_requires_admitted_storage(self):
        sources = [(ROOT / path).read_text() for path in codes.AUTHORITIES]
        report = (ROOT / codes.REPORT).read_text()
        self.assertEqual(
            codes.missing_codes(
                sources + ['Err(Failure::Limit("producer_publication_shape"))'], report
            ),
            {"producer_publication_shape"},
        )
        self.assertIn(
            codes.CORE,
            codes.missing_codes(
                sources, report.replace(codes.CORE, "FuturePublicationCode")
            ),
        )
