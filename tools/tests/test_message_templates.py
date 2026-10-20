"""Keep core's message table synchronized with the condition registry."""

import unittest

import yaml
from tools.packaging import generate_message_templates as generator
from tools.paths import ROOT


class MessageTemplateTests(unittest.TestCase):
    def test_generated_table_matches_the_registry(self):
        self.assertEqual(
            generator.TARGET.read_text(encoding="ascii"), generator.render()
        )

    def test_every_registered_condition_has_one_row_in_order(self):
        registry = yaml.safe_load((ROOT / "yaml/conditions.yaml").read_text("utf-8"))
        rows = generator.templates()
        self.assertEqual(
            [condition for condition, _ in rows], sorted(registry["conditions"])
        )
        self.assertEqual(
            dict(rows),
            {
                condition: registration["message"]
                for condition, registration in registry["conditions"].items()
            },
        )

    def test_literals_escape_quotes_and_backslashes(self):
        self.assertEqual(generator.literal('a "b" \\ c'), '"a \\"b\\" \\\\ c"')


if __name__ == "__main__":
    unittest.main()
