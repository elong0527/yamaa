"""Generated reference pages render rule cards, requirements and schema fields."""

import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

from tools.paths import ROOT
from tools.rules.check import check
from tools.tests.rules_fixture import write, write_repository
from tools.validation.generate_rule_reference import cell, generated

SCHEMA = """\
version: "1.0"

# See LAYER-1.

literal_value:
    type: [str, int]
    description: See REQ-0001.

path_base:
    type: str
    values: [declaring, entry]
    description: See REQ-0003.

project_path:
    type: str
    min_length: 1
    description: A local path, composed under LAYER-2 | kept whole.
"""


class RuleReferenceTests(unittest.TestCase):
    def setUp(self):
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        self.root = write_repository(Path(directory.name))
        self.schema = write(self.root, "yaml/schema_shared.yaml", SCHEMA)

    def page(self, name):
        return generated(self.root)[self.root / "rules/reference" / name]

    def run_generator(self, *arguments):
        return subprocess.run(
            [
                sys.executable,
                "-m",
                "tools.validation.generate_rule_reference",
                "--root",
                str(self.root),
                *arguments,
            ],
            capture_output=True,
            text=True,
            check=False,
        )

    def test_requirement_index_lists_cards_requirements_and_replaced_ids(self):
        index = self.page("requirements.md")
        self.assertIn(
            "| [LAYER-1](../inheritance.md#layer-1) | Layers compose depth first "
            "| `inheritance.md` |",
            index,
        )
        self.assertIn(
            "| [REQ-0004](../values/types.md#req-0004) | `values/types.md` |", index
        )
        self.assertIn("| REQ-0003 | [LAYER-1](../inheritance.md#layer-1) |", index)
        self.assertLess(
            index.index("## Rule cards"), index.index("## Requirements not yet")
        )
        self.assertLess(
            index.index("## Requirements not yet"), index.index("## Replaced")
        )

    def test_summary_lists_principles_and_each_card_title_and_statement(self):
        summary = self.page("rules-summary.md")
        self.assertTrue(summary.startswith("# Rules at a glance\n"))
        self.assertIn(
            "**P1. Layers are ordered.** Layers compose in one order. Enforced by\n"
            "[LAYER-1](../inheritance.md#layer-1) and "
            "[REQ-0001](../values/types.md#req-0001).",
            summary,
        )
        self.assertIn("## Inheritance\n\nResolve inherited layers.\n", summary)
        self.assertIn(
            "### [LAYER-1](../inheritance.md#layer-1). Layers compose depth first\n\n"
            "Each layer's parents are visited left to right, depth first, and then the\n"
            "layer itself.\n",
            summary,
        )
        # Statement links resolve from the generated page, not the card file.
        self.assertIn("[REQ-0001](../values/types.md#req-0001).", summary)
        # Detail, conditions and evidence stay in the rule file.
        self.assertNotIn("A cycle reports its path", summary)
        self.assertNotIn("Fails:", summary)
        self.assertIn(
            "## Not yet converted\n\nThese contracts keep their `REQ-NNNN` requirements"
            " until they become\ncards.\n\n- [Types](../values/types.md): Define column types.",
            summary,
        )

    def test_schema_fields_link_cards_replaced_ids_and_free_text(self):
        fields = self.page("schema-fields.md")
        self.assertIn(
            '| `literal_value` | `["str", "int"]` | `false` | Absent | -- '
            "| [REQ-0001](../values/types.md#req-0001) |",
            fields,
        )
        # A replaced ID links to the card that now holds its meaning.
        self.assertIn("| [LAYER-1](../inheritance.md#layer-1) |", fields)
        self.assertIn(
            "| A local path, composed under [LAYER-2](../inheritance.md#layer-2) "
            "\\| kept whole. |",
            fields,
        )

    def test_invalid_descriptions_fail_the_generator_and_the_check(self):
        source = "schema_shared.yaml:literal_value.description"
        for value, error in (
            ("null", f"schema description must be a string: {source}"),
            ("17", f"schema description must be a string: {source}"),
            (
                "See REQ-9999.",
                f"schema description cites REQ-9999, which names no rule: {source}",
            ),
        ):
            with self.subTest(value=value):
                self.schema.write_text(
                    SCHEMA.replace(
                        "description: See REQ-0001.", f"description: {value}"
                    )
                )
                with self.assertRaisesRegex(ValueError, error):
                    generated(self.root)
                self.assertIn(error, check(self.root).errors)
                result = self.run_generator("--check")
                self.assertEqual(result.returncode, 1)
                self.assertIn(f"ERROR: {error}", result.stdout.splitlines())
                self.assertEqual(result.stderr, "")

    def test_description_without_descriptor_type_is_also_checked(self):
        self.schema.write_text(SCHEMA + "\nnavigation:\n    description: null\n")
        error = "schema description must be a string: schema_shared.yaml:navigation.description"
        with self.assertRaisesRegex(ValueError, error):
            generated(self.root)
        self.assertIn(error, check(self.root).errors)

    def test_schema_sources_share_strict_yaml_loader(self):
        for replacement, message in (
            ("description: &owner See REQ-0001.", "YAML anchors are not allowed"),
            ("description: !!str See REQ-0001.", "explicit YAML tags are not allowed"),
            (
                "description: See REQ-0001.\n    <<: {}",
                "YAML merge keys are not allowed",
            ),
        ):
            with self.subTest(feature=message):
                self.schema.write_text(
                    SCHEMA.replace("description: See REQ-0001.", replacement)
                )
                errors = check(self.root).errors
                self.assertTrue(
                    any(
                        "schema_shared.yaml: invalid schema source:" in error
                        and message in error
                        for error in errors
                    ),
                    errors,
                )
                with self.assertRaisesRegex(ValueError, message):
                    generated(self.root)

    def test_reference_cells_escape_backtick_runs(self):
        # An embedded backtick must not end the code span early: the fence
        # grows past the longest backtick run in the rendered value.
        self.assertEqual(cell("plain"), '`"plain"`')
        self.assertEqual(cell("a`b"), '``"a`b"``')
        self.assertEqual(cell("a``b"), '```"a``b"```')

    def test_materialized_pages_must_match_their_sources(self):
        # Pages are generated at docs build time, not committed; an absent
        # page is current, a materialized one must match.
        self.assertEqual(self.run_generator("--check").returncode, 0)
        result = self.run_generator()
        self.assertEqual(result.returncode, 0, result.stdout)
        for path, text in generated(self.root).items():
            self.assertEqual(path.read_text(encoding="ascii"), text)
        self.assertEqual(self.run_generator("--check").returncode, 0)
        # Generated pages are not rule files and add no rules.
        self.assertEqual(check(self.root).errors, [])
        summary = self.root / "rules/reference/rules-summary.md"
        summary.write_bytes(b"# Rules\n\ncaf\xc3\xa9\n")
        result = self.run_generator("--check")
        self.assertEqual(result.returncode, 1)
        self.assertIn(
            "ERROR: regenerate rules/reference/rules-summary.md",
            result.stdout.splitlines(),
        )
        self.assertEqual(result.stderr, "")

    def test_moving_a_file_keeps_its_ids(self):
        source = self.root / "rules/values/types.md"
        target = source.with_name("kinds.md")
        source.rename(target)
        target.write_text(
            target.read_text().replace("id: values/types", "id: values/kinds")
        )
        for path in (self.root / "rules/README.md", self.root / "rules/inheritance.md"):
            path.write_text(
                path.read_text().replace("values/types.md", "values/kinds.md")
            )
        self.assertEqual(check(self.root).errors, [])
        self.assertIn(
            "[REQ-0001](../values/kinds.md#req-0001)", self.page("requirements.md")
        )

    def test_repository_pages_render(self):
        pages = generated(ROOT)
        self.assertEqual(
            sorted(path.name for path in pages),
            ["requirements.md", "rules-summary.md", "schema-fields.md"],
        )
        summary = pages[ROOT / "rules/reference/rules-summary.md"]
        self.assertIn("### [LAYER-14](../inheritance.md#layer-14). ", summary)
        self.assertIn("**P10. Artifacts are reproducible.**", summary)


if __name__ == "__main__":
    unittest.main()
