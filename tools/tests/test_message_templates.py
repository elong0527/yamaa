"""Keep the engine's tables of the condition registry synchronized with it:
core's message table and the facade's catalogue table (#3700)."""

import copy
import unittest

import yaml
from tools.packaging import generate_message_templates as generator
from tools.paths import ROOT
from tools.validation.repository_manifests import registry_errors


class MessageTemplateTests(unittest.TestCase):
    def test_context_variants_keep_their_priority_and_reject_ambiguous_declarations(
        self,
    ):
        # The registry-to-core conditional template table is the internal seam.
        document = {
            "version": "1.0",
            "conditions": {
                "example": {
                    "phases": ["engine"],
                    "message": "the operation failed",
                    "message_variants": [
                        {
                            "when": "host_error",
                            "message": "the observer raised {host_error}",
                        },
                        {
                            "when": "host_message",
                            "message": "the observer said {host_message}",
                        },
                    ],
                }
            },
        }
        self.assertEqual(registry_errors(document), [])
        self.assertIn(
            '    ("example", "host_error", "the observer raised {host_error}"),\n'
            '    ("example", "host_message", "the observer said {host_message}"),',
            generator.render(document),
        )
        for variants, expected in (
            ({}, "expected a list"),
            (
                [
                    {
                        "when": "host_error",
                        "message": "the observer failed",
                        "extra": True,
                    }
                ],
                "expected when and message",
            ),
            (
                [{"when": "host_error", "message": "the observer failed"}] * 2,
                "expected a unique context key",
            ),
        ):
            with self.subTest(variants=variants):
                changed = copy.deepcopy(document)
                changed["conditions"]["example"]["message_variants"] = variants
                errors = registry_errors(changed)
                self.assertEqual(len(errors), 1)
                self.assertIn(expected, errors[0])

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
