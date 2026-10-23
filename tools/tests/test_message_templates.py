"""Keep the engine's tables of the condition registry synchronized with it:
core's message table and the facade's catalogue table (#3700)."""

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

    def test_generated_catalogue_table_matches_the_registry(self):
        self.assertEqual(
            generator.CATALOGUE.read_text(encoding="ascii"),
            generator.render_catalogue(),
        )

    def test_the_catalogue_table_has_every_condition_with_its_history(self):
        registry = yaml.safe_load((ROOT / "yaml/conditions.yaml").read_text("utf-8"))
        self.assertEqual(
            (generator.registered(), generator.retired()),
            (
                [
                    (
                        condition,
                        registration["phases"],
                        registration.get("renamed_from"),
                    )
                    for condition, registration in sorted(
                        registry["conditions"].items()
                    )
                ],
                [
                    (condition, entry["retired_in"])
                    for condition, entry in sorted(registry["retired"].items())
                ],
            ),
        )

    def test_literals_escape_quotes_and_backslashes(self):
        self.assertEqual(generator.literal('a "b" \\ c'), '"a \\"b\\" \\\\ c"')


if __name__ == "__main__":
    unittest.main()
