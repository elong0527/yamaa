"""Every rule check reports its failure and passes a valid repository."""

import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

from tools.paths import ROOT
from tools.rules.check import check
from tools.tests.rules_fixture import write, write_repository


class RuleCheckTests(unittest.TestCase):
    def setUp(self):
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        self.root = write_repository(Path(directory.name))

    def write(self, relative, text):
        return write(self.root, relative, text)

    def replace(self, relative, before, after):
        path = self.root / relative
        text = path.read_text(encoding="utf-8")
        self.assertIn(before, text)
        path.write_text(text.replace(before, after, 1), encoding="utf-8")

    def errors(self):
        return check(self.root).errors

    def assert_error(self, expected):
        errors = self.errors()
        self.assertIn(expected, "\n".join(errors))

    def test_valid_repository_passes(self):
        self.assertEqual(self.errors(), [])

    def test_command_line_reports_errors(self):
        command = [sys.executable, "-m", "tools.rules.check", "--root", str(self.root)]
        result = subprocess.run(command, capture_output=True, text=True, check=False)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn(
            "PASS: 4 cards name every registered condition on a Fails line;",
            result.stdout,
        )
        self.replace("rules/inheritance.md", '<a id="layer-2">', '<a id="layer-9">')
        result = subprocess.run(command, capture_output=True, text=True, check=False)
        self.assertEqual(result.returncode, 1)
        self.assertIn(
            "ERROR: rules/inheritance.md:22: LAYER-2: anchor 'layer-9' must be 'layer-2'",
            result.stdout.splitlines(),
        )

    # -- the index ------------------------------------------------------------

    def test_index_needs_its_rule_files_table(self):
        self.replace("rules/README.md", "## Rule files", "## Files")
        self.assertEqual(
            self.errors(),
            ["rules/README.md: no Rule files table (| File | Prefix | Covers |)"],
        )

    def test_every_rule_file_is_listed_once_and_exists(self):
        self.write("rules/values/text.md", "# Text\n\n## Requirements\n")
        self.replace(
            "rules/README.md",
            "| [Types](values/types.md) | TYPE | Define column types. |",
            "| [Types](values/types.md) | TYPE | Define column types. |\n"
            "| [Types](values/types.md) | TYPE | Again. |\n"
            "| [Gone](values/gone.md) | GONE | Missing. |\n"
            "| Bad row |",
        )
        errors = self.errors()
        for expected in (
            "rules/README.md:14: values/types.md is listed twice",
            "rules/README.md:15: values/gone.md is not a rule file",
            "rules/README.md:16: expected | [Title](file.md) | PREFIX | Covers |",
            "rules/values/text.md: absent from the Rule files table in rules/README.md",
        ):
            with self.subTest(expected=expected):
                self.assertIn(expected, errors)

    def test_prefixes_are_capitals_and_unique(self):
        self.write(
            "rules/other.md",
            '# Other\n\n<a id="layer-3"></a>\n\n### LAYER-3. Other rule\n\nA statement.\n',
        )
        self.replace(
            "rules/README.md",
            "| [Inheritance](inheritance.md) | LAYER | Resolve inherited layers. |",
            "| [Inheritance](inheritance.md) | LAYER | Resolve inherited layers. |\n"
            "| [Other](other.md) | Layer | Other. |",
        )
        errors = self.errors()
        self.assertIn(
            "rules/README.md:13: prefix 'Layer' must be capital letters", errors
        )
        # A malformed listed prefix is reported once, not against each card.
        self.assertFalse(any("must use the prefix" in error for error in errors))
        self.replace("rules/README.md", "| Layer |", "| TYPE |")
        errors = self.errors()
        self.assertIn(
            "rules/README.md:14: prefix TYPE is already used by other.md", errors
        )
        self.assertIn(
            "rules/other.md:5: LAYER-3: cards in other.md must use the prefix TYPE",
            errors,
        )

    def test_listing_uses_the_file_title(self):
        self.replace(
            "rules/README.md", "[Types](values/types.md)", "[Kinds](values/types.md)"
        )
        self.assertEqual(
            self.errors(),
            [
                "rules/README.md:13: values/types.md is listed as 'Kinds' but titled 'Types'"
            ],
        )

    def test_principles_are_numbered_and_cite_rules(self):
        self.replace("rules/README.md", "**P1.", "**P2.")
        self.assert_error("rules/README.md:5: P2 should be P1")
        self.replace(
            "rules/README.md",
            "Enforced by\n[LAYER-1](inheritance.md#layer-1) and [TYPE-1](values/types.md#type-1).",
            "Nothing enforces it.",
        )
        self.assert_error("rules/README.md:5: P2 cites no rule")
        self.replace(
            "rules/README.md", "**P2. Layers are ordered.**", "Layers are ordered."
        )
        self.assert_error("rules/README.md: no principles")

    # -- rule files -----------------------------------------------------------

    def test_a_rule_file_holds_cards(self):
        self.write("rules/values/empty.md", "# Empty\n\nNothing.\n")
        self.replace(
            "rules/README.md",
            "| [Types]",
            "| [Empty](values/empty.md) | EMPTY | Nothing. |\n| [Types]",
        )
        self.assertEqual(self.errors(), ["rules/values/empty.md: defines no cards"])

    def test_card_files_have_no_front_matter_or_other_rule_headings(self):
        self.replace(
            "rules/inheritance.md",
            "# Inheritance\n",
            "---\nid: x\n---\n# Inheritance\n",
        )
        self.replace(
            "rules/inheritance.md", "## Layers\n", "## Layers\n\n### Overview\n"
        )
        errors = self.errors()
        self.assertIn(
            "rules/inheritance.md: a card file must not have front matter", errors
        )
        self.assert_error(
            "rules/inheritance.md:10: '### Overview' is not a card heading ('### PREFIX-N. Title')"
        )

    # -- cards ----------------------------------------------------------------

    def test_card_anchor_number_prefix_heading_and_title(self):
        self.replace("rules/inheritance.md", '<a id="layer-1"></a>\n\n', "")
        self.replace(
            "rules/inheritance.md",
            '<a id="layer-2"></a>\n\n### LAYER-2. A null clears an inherited field',
            '<a id="layer-02"></a>\n\n### LAYER-02 A null clears an inherited field.',
        )
        errors = self.errors()
        for expected in (
            'rules/inheritance.md:7: LAYER-1: needs <a id="layer-1"></a> above its heading',
            "rules/inheritance.md:20: LAYER-02: the number must be at least 1, without leading zeros",
            "rules/inheritance.md:20: LAYER-02: the heading must read '### LAYER-02. Title'",
            "rules/inheritance.md:20: LAYER-02: the title must not end with a period",
        ):
            with self.subTest(expected=expected):
                self.assertIn(expected, errors)
        self.replace(
            "rules/inheritance.md",
            "### LAYER-1. Layers compose depth first",
            "### TYPE-1. Untitled",
        )
        self.assert_error(
            "rules/inheritance.md:7: TYPE-1: cards in inheritance.md must use the prefix LAYER"
        )
        self.replace("rules/inheritance.md", "### TYPE-1. Untitled", "### TYPE-1.")
        self.assert_error("rules/inheritance.md:7: TYPE-1: has no title")

    def test_card_ids_are_unique_across_files(self):
        self.write(
            "rules/more.md",
            '# More\n\n<a id="layer-1"></a>\n\n### LAYER-1. Again\n\nA statement.\n',
        )
        self.replace(
            "rules/README.md",
            "| [Types]",
            "| [More](more.md) | MORE | More. |\n| [Types]",
        )
        self.assert_error(
            "duplicate card LAYER-1: rules/inheritance.md:9, rules/more.md:5"
        )

    def test_statement_is_present_and_short(self):
        self.replace(
            "rules/inheritance.md",
            "A null written over an inherited field clears it; see\n[TYPE-1](values/types.md#type-1).",
            " ".join(["word"] * 91),
        )
        self.assert_error(
            "rules/inheritance.md:24: LAYER-2: the statement has 91 words; the limit is 90"
        )
        self.replace(
            "rules/inheritance.md", " ".join(["word"] * 91), "| table | only |"
        )
        self.assert_error(
            "rules/inheritance.md:22: LAYER-2: has no statement paragraph after its heading"
        )

    def test_metadata_is_one_ordered_paragraph(self):
        self.replace(
            "rules/inheritance.md",
            "Fails: `inheritance_cycle`.\nEvidence:",
            "Evidence: `schema-inheritance`.\nFails: `inheritance_cycle`.\nEvidence:",
        )
        errors = self.errors()
        self.assertIn("rules/inheritance.md:14: LAYER-1: repeats Evidence", errors)
        self.assertIn(
            "rules/inheritance.md:14: LAYER-1: metadata must be ordered Fails, Evidence",
            errors,
        )

    def test_metadata_outside_its_paragraph_is_reported(self):
        self.replace(
            "rules/inheritance.md", "layer itself.\n", "layer itself.\nFails: `x`.\n"
        )
        self.assert_error(
            "rules/inheritance.md:13: LAYER-1: Fails and Evidence must form one "
            "paragraph directly after the statement"
        )
        self.replace(
            "rules/inheritance.md", "layer itself.\nFails: `x`.\n", "layer itself.\n"
        )
        self.replace(
            "rules/inheritance.md",
            "A cycle reports its path.",
            "A cycle.\n\nEvidence: `schema-inheritance`.",
        )
        self.assert_error("rules/inheritance.md:20: LAYER-1: Fails and Evidence")

    def test_fails_names_registered_conditions(self):
        self.replace(
            "rules/inheritance.md",
            "Fails: `inheritance_cycle`.",
            "Fails: `missing_one`, cycle",
        )
        errors = self.errors()
        self.assertIn(
            "rules/inheritance.md:14: LAYER-1: the Fails line must end with a period",
            errors,
        )
        self.assertIn(
            "rules/inheritance.md:14: LAYER-1: missing_one is not registered in yaml/conditions.yaml",
            errors,
        )
        self.assertIn(
            "rules/inheritance.md:14: LAYER-1: Fails item 'cycle' is not a `condition`",
            errors,
        )

    def test_every_registered_condition_is_named_on_a_fails_line(self):
        self.replace(
            "yaml/conditions.yaml",
            "  conversion_failed: {phases: [convert]}\n",
            "  conversion_failed: {phases: [convert]}\n"
            "  orphan_condition: {phases: [validation]}\n",
        )
        uncited = (
            "yaml/conditions.yaml:5: orphan_condition is registered, but no "
            "card's Fails line names it"
        )
        self.assertEqual(self.errors(), [uncited])
        # A statement or detail that mentions the condition does not cite it.
        self.replace(
            "rules/values/types.md",
            "A column has one type.\n",
            "A column has one type, or fails with `orphan_condition`.\n",
        )
        self.replace(
            "rules/inheritance.md",
            "A cycle reports its path.",
            "A cycle reports its path, never `orphan_condition`.",
        )
        self.assertEqual(self.errors(), [uncited])
        self.replace(
            "rules/values/types.md",
            "Fails: `conversion_failed`.",
            "Fails: `conversion_failed`, `orphan_condition`.",
        )
        self.assertEqual(self.errors(), [])

    def test_an_unreadable_registry_cites_nothing(self):
        # A registry without a conditions mapping is reported once, against the
        # cards that name conditions, and leaves no condition uncited.
        for text in ('version: "1.0"\n', "conditions: [a, b]\n", "- a\n", ""):
            with self.subTest(text=text):
                self.write("yaml/conditions.yaml", text)
                self.assertEqual(
                    self.errors(),
                    ["yaml/conditions.yaml: cannot read the registered conditions"],
                )

    def test_evidence_items_exist(self):
        cases = {
            "`missing-benchmark`": "missing-benchmark is not a benchmark under benchmarks/",
            "`rules/missing.md`": "rules/missing.md does not exist",
            "`rules/values/types.md::absent_name`": "'absent_name' does not occur in rules/values/types.md",
            "`../outside.md`": "../outside.md must be a repository-relative path",
            "`schema-inheritance::name`": "schema-inheritance::name: a benchmark has no '::' name",
            "`benchmarks/schema-inheritance::name`": (
                "benchmarks/schema-inheritance is not a file that can hold 'name'"
            ),
            "plain": "Evidence item 'plain' is not a `benchmark` or `path`",
        }
        original = "Evidence: `schema-inheritance`,\n`rules/values/types.md::column`."
        for item, expected in cases.items():
            with self.subTest(item=item):
                self.replace("rules/inheritance.md", original, f"Evidence: {item}.")
                self.assert_error(f"rules/inheritance.md:15: LAYER-1: {expected}")
                self.replace("rules/inheritance.md", f"Evidence: {item}.", original)
        self.assertEqual(self.errors(), [])

    def test_verification_checks_match_the_registries(self):
        self.write(
            "rules/verification.md",
            "# Verification\n\n## Checks\n\n"
            '<a id="check-1"></a>\n\n### CHECK-1. Keys are distinct\n\n'
            "`unique` requires distinct keys.\n\n"
            '<a id="check-2"></a>\n\n### CHECK-2. One value is present\n\n'
            "`exactly_one` requires one value.\n",
        )
        self.replace(
            "rules/README.md",
            "| [Types]",
            "| [Verification](verification.md) | CHECK | Checks. |\n| [Types]",
        )
        self.write(
            "yaml/schema_verification.yaml",
            'version: "1.0"\ncolumn_verifications:\n    unique:\n'
            "        - id: {type: str, required: false}\n    row_count:\n"
            "        - id: {type: str, required: false}\n",
        )
        errors = self.errors()
        self.assertIn(
            "verification.md: CHECK-2 defines `exactly_one`, which no "
            "verification registry declares",
            errors,
        )
        self.assertIn(
            "schema_verification.yaml: `row_count` is registered but no "
            "verification.md rule defines it",
            errors,
        )

    # -- citations ------------------------------------------------------------

    def test_citations_resolve_repository_wide(self):
        self.write("yaml/schema_notes.yaml", "# See LAYER-99.\nnotes: {}\n")
        self.write("rust/crates/core/src/lib.rs", 'const R: &str = "LAYER-7";\n')
        self.write("rust/crates/core/tests/fixtures/data.json", '{"rule": "TYPE-2"}\n')
        self.write(
            "python/module.py", "# TYPE-1 and LAYER-2 resolve; UTF-8 is no rule.\n"
        )
        self.write("docs/page.md", "# Page\n\nTYPE-2 resolves, LAYER-3 does not.\n")
        self.write("build/lib/stale.py", "# LAYER-99 in build output is ignored.\n")
        self.write("tools/tests/test_rules_check.py", "# LAYER-99 is exempt here.\n")
        errors = self.errors()
        self.assertEqual(
            sorted(error for error in errors if "unresolved rule citation" in error),
            [
                "docs/page.md:3: unresolved rule citation LAYER-3",
                "rust/crates/core/src/lib.rs:1: unresolved rule citation LAYER-7",
                "yaml/schema_notes.yaml:1: unresolved rule citation LAYER-99",
            ],
        )

    def test_retired_requirement_ids_are_refused(self):
        retired = "retired requirement ID {}; cite the card that replaced it"
        self.write("rust/crates/core/src/lib.rs", "// REQ-0001 and LAYER-1.\n")
        self.replace(
            "rules/inheritance.md",
            "A cycle reports its path.",
            "A cycle reports its path.\nReplaces: REQ-0002.",
        )
        # Records of their time keep their IDs, and an input is data.
        for kept in (
            "docs/releases/0.5.0.md",
            "benchmarks/schema-case/input/dm.csv",
            "tools/tests/test_rules_check.py",
        ):
            self.write(kept, "REQ-0003 stays.\n")
        self.assertEqual(
            self.errors(),
            [
                "rules/inheritance.md:19: " + retired.format("REQ-0002"),
                "rust/crates/core/src/lib.rs:1: " + retired.format("REQ-0001"),
            ],
        )

    def test_negative_benchmarks_expect_a_card_id(self):
        path = "benchmarks/negative-case/expected/error.yaml"
        for rule, expected in (
            ("LAYER-2", None),
            ("~", None),
            ("REQ-0002", "rule must be a card ID"),
            ("R001-1", "rule must be a card ID"),
            ("LAYER-9", "unresolved rule citation LAYER-9"),
        ):
            with self.subTest(rule=rule):
                self.write(path, f"phase: validation\nrule: {rule}\n")
                errors = self.errors()
                if expected is None:
                    self.assertEqual(errors, [])
                else:
                    self.assertIn(expected, "\n".join(errors))

    # -- links ----------------------------------------------------------------

    def test_links_name_existing_files_and_anchors(self):
        self.replace(
            "rules/inheritance.md",
            "A cycle reports its path.",
            "A cycle reports its [path](values/missing.md), [types](values/types.md#nowhere)"
            " and [here](#absent).",
        )
        errors = self.errors()
        for expected in (
            "rules/inheritance.md:18: link target values/missing.md does not exist",
            "rules/inheritance.md:18: rules/values/types.md has no anchor #nowhere",
            "rules/inheritance.md:18: rules/inheritance.md has no anchor #absent",
        ):
            with self.subTest(expected=expected):
                self.assertIn(expected, errors)

    def test_links_into_rules_from_other_markdown(self):
        self.write(
            "yaml/README.md", "# YAML\n\n[Rules](../rules/README.md#specification)\n"
        )
        self.write("README.md", "# Home\n\n[Rules](rules/README.md#rule-files)\n")
        self.assertEqual(
            self.errors(),
            ["yaml/README.md:3: rules/README.md has no anchor #specification"],
        )

    def test_docs_links_follow_canonical_and_generated_pages(self):
        self.write("README.md", "# Home\n")
        self.write(
            "docs/articles/page.md",
            "# Page\n\n[Rules](rules.md#rule-files) [Index](../reference/rule-index.md#x)"
            " [Dashboards](../benchmark/index.md) [Home](../index.md)"
            " [Web](https://example.org/x.md#y)\n",
        )
        self.write(
            "docs/articles/benchmark.md", "# Template\n\n[$source](missing.html)\n"
        )
        self.assertEqual(self.errors(), [])
        self.replace("docs/articles/page.md", "rules.md#rule-files", "rules.md#blocks")
        self.assertEqual(
            self.errors(),
            ["docs/articles/page.md:3: rules/README.md has no anchor #blocks"],
        )

    def test_a_link_whose_text_is_a_rule_id_targets_that_rule(self):
        self.replace(
            "rules/inheritance.md",
            "[LAYER-2](#layer-2)",
            "[LAYER-2](#layer-1), [TYPE-2](values/types.md)",
        )
        self.assertEqual(
            self.errors(),
            [
                "rules/inheritance.md:18: [LAYER-2] links to #layer-1, not inheritance.md#layer-2",
                "rules/inheritance.md:18: [TYPE-2] links to values/types.md, not values/types.md#type-2",
            ],
        )

    # -- schemas --------------------------------------------------------------

    def test_schema_descriptions_cite_rules_that_resolve(self):
        schema = "yaml/schema_shared.yaml"
        self.write(
            schema,
            'version: "1.0"\n\n# See LAYER-1.\n\nliteral_value:\n    type: str\n'
            "    description: See TYPE-1.\n",
        )
        self.assertEqual(self.errors(), [])
        cases = {
            "See LAYER-1.": None,
            "A free description of the field, governed by LAYER-2.": None,
            "See RULE-1.": "schema description cites RULE-1, which names no rule",
            "Governed by LAYER-99.": "schema description cites LAYER-99, which names no rule",
            "null": "schema description must be a string: schema_shared.yaml:literal_value.description",
            "''": "schema description is empty",
        }
        for value, expected in cases.items():
            with self.subTest(value=value):
                self.replace(
                    schema, "description: See TYPE-1.", f"description: {value}"
                )
                errors = self.errors()
                if expected is None:
                    self.assertEqual(errors, [])
                else:
                    self.assertIn(expected, "\n".join(errors))
                self.replace(
                    schema, f"description: {value}", "description: See TYPE-1."
                )

    def test_schema_comments_point_at_a_rule(self):
        schema = "yaml/schema_shared.yaml"
        self.write(schema, 'version: "1.0"\n\n# See LAYER-9.\n')
        self.assertIn(
            "schema comment has no canonical owner: schema_shared.yaml", self.errors()
        )
        self.write(schema, 'version: "1.0"\n\n# Missing values become zero.\n')
        self.assertIn(
            "schema comment has no canonical owner: schema_shared.yaml", self.errors()
        )
        self.write(schema, 'version: "1.0"\nversion: "1.0"\n')
        self.assert_error("schema_shared.yaml: invalid schema source")

    # -- the repository -------------------------------------------------------

    def test_the_repository_passes(self):
        self.assertEqual(check(ROOT).errors, [])


if __name__ == "__main__":
    unittest.main()
