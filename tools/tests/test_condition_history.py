"""The registry carries the conditions 0.6.0 renamed or retired (#3700).

tools/migrate/renames.yaml records each condition that the 1.0 language
renamed or retired, which 0.6.0 ships (#3673 D289). yaml/conditions.yaml
carries them, as `renamed_from` on the condition that took the name and as
a `retired` entry with `retired_in: "0.6.0"`, so the catalogue that `conditions()` returns names
every condition a program written for 0.5.0 matched, and no package ships the
rename table.
"""

import unittest

import yaml
from tools.paths import ROOT


class ConditionHistory(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        table = yaml.safe_load(
            (ROOT / "tools/migrate/renames.yaml").read_text(encoding="utf-8")
        )
        cls.table = table["conditions"]
        cls.registry = yaml.safe_load(
            (ROOT / "yaml/conditions.yaml").read_text(encoding="utf-8")
        )

    def test_the_former_names_are_the_renames_of_the_table(self):
        self.assertEqual(
            {
                condition: registration["renamed_from"]
                for condition, registration in self.registry["conditions"].items()
                if "renamed_from" in registration
            },
            {
                entry["proposed"]: entry["today"]
                for entry in self.table
                if entry["kind"] == "rename"
            },
        )

    def test_the_retired_conditions_are_the_removals_of_the_table(self):
        self.assertEqual(
            {
                condition: entry["retired_in"]
                for condition, entry in self.registry["retired"].items()
            },
            {
                entry["today"]: "0.6.0"
                for entry in self.table
                if entry["kind"] == "removed"
            },
        )


if __name__ == "__main__":
    unittest.main()
