"""Exact wide numeric context transport, independent of global integer limits."""

import unittest

from tools.validation import _native_json
from tools.validation.repository_schema import native_diagnostics


class TestNativeJsonFacts(unittest.TestCase):
    def test_wide_signed_integer_diagnostic_context_round_trip(self):
        digits = "7" * 4335
        expected = 7 * (10**4335 - 1) // 9
        for sign, value in [("", expected), ("-", -expected)]:
            encoded = '{"observed":' + sign + digits + ',"present":true}'
            facts = _native_json.loads(encoded)
            self.assertIs(type(facts["observed"]), int)
            self.assertEqual(facts["observed"], value)
            self.assertIs(facts["present"], True)
            self.assertEqual(_native_json.dumps(facts, separators=(",", ":")), encoded)
            # Presentation consumes the actual complete tuple; it does not
            # reinterpret integer kind or replace the native diagnostic cause.
            [finding] = native_diagnostics(
                [("validation", "invalid_field_type", None, ["X"], encoded)], "authored"
            )
            self.assertEqual(finding.path, "authored.X")
            self.assertEqual(finding.condition, "invalid_field_type")
            self.assertEqual(finding.context, facts)
            self.assertIn(encoded, str(finding))


if __name__ == "__main__":
    unittest.main()
