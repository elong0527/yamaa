"""Generated reference pages render rule cards and schema fields."""

import re
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
    description: See TYPE-1.

path_base:
    type: str
    values: [declaring, entry]
    description: See LAYER-1.

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

    def test_rule_index_lists_every_card(self):
        index = self.page("rule-index.md")
        self.assertIn(
            "| Rule | Title | File |\n| --- | --- | --- |\n"
            "| [LAYER-1](../inheritance.md#layer-1) | Layers compose depth first "
            "| `inheritance.md` |\n",
            index,
        )
        self.assertIn(
            "| [TYPE-2](../values/types.md#type-2) | A failed conversion fails "
            "| `values/types.md` |",
            index,
        )
        self.assertEqual(index.count("\n## "), 1)

    def test_summary_lists_principles_and_each_card_title(self):
        summary = self.page("rules-summary.md")
        self.assertTrue(summary.startswith("# Rules at a glance\n"))
        self.assertIn(
            "**P1. Layers are ordered.** Layers compose in one order. Enforced by\n"
            "[LAYER-1](../inheritance.md#layer-1) and "
            "[TYPE-1](../values/types.md#type-1).",
            summary,
        )
        self.assertIn("## Inheritance\n\nResolve inherited layers.\n", summary)
        self.assertIn(
            "- [LAYER-1](../inheritance.md#layer-1). Layers compose depth first\n",
            summary,
        )
        # Statements, detail, conditions and evidence stay in the rule file.
        self.assertNotIn("Each layer's parents are visited left to right", summary)
        self.assertNotIn("A cycle reports its path", summary)
        self.assertNotIn("Fails:", summary)
        self.assertIn(
            "## Types\n\nDefine column types.\n\n### Columns\n\n"
            "- [TYPE-1](../values/types.md#type-1). A column has one type\n",
            summary,
        )

    def test_schema_fields_link_cards_and_free_text(self):
        fields = self.page("schema-fields.md")
        # A union of str and int takes the kind its members share, value.
        self.assertIn(
            '| `literal_value` | `["str", "int"]` | `value` | `false` | Absent | -- '
            "| [TYPE-1](../values/types.md#type-1) |",
            fields,
        )
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
                "See TYPE-9.",
                f"schema description cites TYPE-9, which names no rule: {source}",
            ),
        ):
            with self.subTest(value=value):
                self.schema.write_text(
                    SCHEMA.replace("description: See TYPE-1.", f"description: {value}")
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
            ("description: &owner See TYPE-1.", "YAML anchors are not allowed"),
            ("description: !!str See TYPE-1.", "explicit YAML tags are not allowed"),
            (
                "description: See TYPE-1.\n    <<: {}",
                "YAML merge keys are not allowed",
            ),
        ):
            with self.subTest(feature=message):
                self.schema.write_text(
                    SCHEMA.replace("description: See TYPE-1.", replacement)
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
        for path in (self.root / "rules/README.md", self.root / "rules/inheritance.md"):
            path.write_text(
                path.read_text().replace("values/types.md", "values/kinds.md")
            )
        self.assertEqual(check(self.root).errors, [])
        self.assertIn("[TYPE-1](../values/kinds.md#type-1)", self.page("rule-index.md"))

    def test_repository_pages_render(self):
        pages = generated(ROOT)
        self.assertEqual(
            sorted(path.name for path in pages),
            ["rule-index.md", "rules-summary.md", "schema-fields.md"],
        )
        summary = pages[ROOT / "rules/reference/rules-summary.md"]
        self.assertIn("- [LAYER-14](../inheritance.md#layer-14). ", summary)
        self.assertIn("**P10. Artifacts are reproducible.**", summary)

    def test_the_fields_page_has_a_section_and_a_kind_for_every_schema_file(self):
        fields = generated(ROOT)[ROOT / "rules/reference/schema-fields.md"]
        self.assertEqual(
            re.findall(r"^## (\S+)$", fields, re.M),
            sorted(path.name for path in (ROOT / "yaml").glob("schema*.yaml")),
        )
        self.assertIn("| Field or value type | Type | Kind | Required |", fields)
        self.assertIn(
            '| `root_class.clear` | `"list[spec_path]"` | `value` | `false` |', fields
        )
        self.assertIn(
            '| `expressions.mapping.source` | `"filtered_source"` | `name` |', fields
        )
        # A registry entry's payload is no field and has no kind.
        self.assertIn(
            '| `expressions.flag` | `["predicate", "flag_class"]` | -- |', fields
        )

    def test_a_class_field_description_must_cite_rules_that_exist(self):
        write(
            self.root,
            "yaml/schema_spec.yaml",
            'version: "1.0"\n\nroot_class:\n'
            "    - yamaa:\n        type: str\n        description: See TYPE-9.\n",
        )
        source = "schema_spec.yaml:root_class.yamaa.description"
        error = f"schema description cites TYPE-9, which names no rule: {source}"
        with self.assertRaisesRegex(ValueError, error):
            generated(self.root)
        self.assertIn(error, check(self.root).errors)


if __name__ == "__main__":
    unittest.main()
