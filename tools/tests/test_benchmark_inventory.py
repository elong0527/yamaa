"""Independent inventory corruption controls against authored metadata."""

import unittest

from tools.validation import check_execution_manifest as EXECUTION_CHECK


class TestDerivedBenchmarkInventory(unittest.TestCase):
    def payload(self):
        names = {"active": None, "excluded": "reviewed exclusion"}
        return {
            "protocol": EXECUTION_CHECK.PROTOCOL,
            "examples": [
                {
                    "name": name,
                    "entries": {"python": [{}], "r": [{}]},
                    "excluded": names[name],
                }
                for name in names
            ],
        }, names

    def test_derived_cases_include_both_hosts_and_explicit_exclusions(self):
        payload, names = self.payload()
        self.assertEqual(EXECUTION_CHECK.check_inventory(payload, names), [])

    def test_missing_duplicate_and_unexpected_cases_are_rejected(self):
        payload, names = self.payload()
        payload["examples"][0]["name"] = "unexpected"
        payload["examples"].append(payload["examples"][0])
        errors = EXECUTION_CHECK.check_inventory(payload, names)
        self.assertIn("missing case: active", errors)
        self.assertIn("unexpected case: unexpected", errors)
        self.assertIn("duplicate case: unexpected", errors)

    def test_missing_host_and_changed_exclusion_are_rejected(self):
        payload, names = self.payload()
        del payload["examples"][0]["entries"]["r"]
        payload["examples"][1]["excluded"] = "unreviewed reason"
        errors = EXECUTION_CHECK.check_inventory(payload, names)
        self.assertIn("missing public host entry: active", errors)
        self.assertIn("exclusion reason differs: excluded", errors)

    def test_unauthored_cases_cannot_be_approved_by_an_exclusion(self):
        payload, names = self.payload()
        self.assertIn(
            "unexpected case: excluded",
            EXECUTION_CHECK.check_inventory(payload, {"active": None}),
        )

    def test_unknown_protocol_is_rejected(self):
        self.assertEqual(
            EXECUTION_CHECK.check_inventory({}, {}),
            ["unknown benchmark inventory protocol"],
        )


if __name__ == "__main__":
    unittest.main()
