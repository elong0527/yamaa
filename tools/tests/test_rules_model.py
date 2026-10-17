"""The shared rules model reads cards, legacy requirements and citations once."""

import subprocess
import tempfile
import unittest
from pathlib import Path

from tools.rules import model
from tools.tests.rules_fixture import write, write_repository


class MarkdownTests(unittest.TestCase):
    def test_prose_blanks_front_matter_fences_comments_and_code(self):
        text = (
            "---\ntitle: Because\n---\n"
            "Prose `code` here.\n"
            "```yaml\nfenced: REQ-0099\n```\n"
            "<!-- hidden -->Visible.\n"
            "**REQ-0160.** ```text\ngrammar := rule\n```\n"
            "After the block.\n"
        )
        lines = model.prose_lines(text)
        self.assertEqual(len(lines), len(text.splitlines()))
        self.assertEqual(lines[:3], ["", "", ""])
        self.assertEqual(lines[3], "Prose        here.")
        self.assertEqual(lines[4:7], ["", "", ""])
        self.assertEqual(lines[7], " " * len("<!-- hidden -->") + "Visible.")
        # A fence opening after prose keeps that prose.
        self.assertEqual(lines[8], "**REQ-0160.** ")
        self.assertEqual(lines[9:11], ["", ""])
        self.assertEqual(lines[11], "After the block.")
        self.assertEqual(model.prose_lines(text, code=True)[3], "Prose `code` here.")

    def test_unterminated_comment_and_longer_fences(self):
        lines = model.prose_lines(
            "<!-- open\nstill hidden\n--> shown\n````\n```\n````\nend\n"
        )
        self.assertEqual(lines[0].strip(), "")
        self.assertEqual(lines[1].strip(), "")
        self.assertEqual(lines[2].strip(), "shown")
        self.assertEqual(lines[3:6], ["", "", ""])
        self.assertEqual(lines[6], "end")

    def test_links_and_reference_definitions_outside_code(self):
        text = (
            '[One](a.md#x "Title") and ![image](i.svg) and `[code](c.md)`\n'
            "[Two](<b c.md>) [Web](https://example.org/)\n"
            "[ref]: values/types.md?plain=1#requirements\n"
        )
        links = model.markdown_links(text)
        self.assertEqual(
            [(link.line, link.text, link.target) for link in links],
            [
                (1, "One", "a.md#x"),
                (2, "Two", "b c.md"),
                (2, "Web", "https://example.org/"),
                (3, "ref", "values/types.md?plain=1#requirements"),
            ],
        )
        one, two, web, ref = links
        self.assertEqual((one.path, one.fragment, one.external), ("a.md", "x", False))
        self.assertEqual(
            text.splitlines()[0][one.target_start : one.target_end], "a.md#x"
        )
        self.assertEqual(
            text.splitlines()[1][two.target_start : two.target_end], "<b c.md>"
        )
        self.assertTrue(web.external)
        self.assertEqual((ref.path, ref.fragment), ("values/types.md", "requirements"))

    def test_edit_lines_applies_edits_right_to_left(self):
        text = "[A](a.md) and [B](b.md)\nkeep\n"
        links = model.markdown_links(text)
        edits = [
            (link.line, link.target_start, link.target_end, "z.md") for link in links
        ]
        self.assertEqual(
            model.edit_lines(text, edits), "[A](z.md) and [B](z.md)\nkeep\n"
        )

    def test_anchors_cover_explicit_ids_and_both_heading_styles(self):
        text = (
            '<a id="layer-3"></a>\n\n'
            "### LAYER-3. Layers contribute `depth` first\n\n"
            "## Rule files\n\n## Rule files\n\n"
            "## A -- B\n\n## Custom {#chosen}\n\n"
            '```markdown\n<a id="fenced"></a>\n## Fenced\n```\n'
        )
        found = model.anchors(text)
        for anchor in (
            "layer-3",
            "layer-3-layers-contribute-depth-first",
            "rule-files",
            "rule-files_1",
            "rule-files-1",
            "a-b",
            "a----b",
            "chosen",
        ):
            self.assertIn(anchor, found)
        self.assertNotIn("fenced", found)

    def test_words_count_link_text_once(self):
        self.assertEqual(model.words("See [LAYER-2](#layer-2) for `null` clears."), 5)


class RuleFileTests(unittest.TestCase):
    def setUp(self):
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        self.root = write_repository(Path(directory.name))
        self.rules = model.load(self.root)

    def test_load_discovers_rule_files_only(self):
        self.assertEqual(
            sorted(self.rules.files), ["inheritance.md", "values/types.md"]
        )
        for page in model.GENERATED_PAGES:
            write(self.root / "rules", page, "# Generated\n\n**REQ-0099.** No.\n")
        self.assertEqual(
            sorted(model.load(self.root).files), ["inheritance.md", "values/types.md"]
        )

    def test_card_fields(self):
        card = self.rules.cards["LAYER-1"]
        self.assertEqual(
            (card.prefix, card.number, card.file, card.anchor, card.topic),
            ("LAYER", "1", "inheritance.md", "layer-1", "Layers"),
        )
        self.assertEqual(card.title, "Layers compose depth first")
        self.assertTrue(card.dotted)
        self.assertTrue(card.statement.startswith("Each layer's parents"))
        self.assertEqual(card.statement_line, 11)
        self.assertEqual(card.fails, ("inheritance_cycle",))
        self.assertEqual(
            card.evidence, ("schema-inheritance", "rules/values/types.md::column")
        )
        self.assertEqual(card.replaces, ("REQ-0002", "REQ-0003"))
        replaces = card.entry("Replaces")
        self.assertEqual((replaces.line, replaces.end), (16, 17))
        self.assertTrue(card.detail.startswith("A cycle reports its path."))
        self.assertEqual(card.stray_metadata, ())
        second = self.rules.cards["LAYER-2"]
        self.assertEqual(second.metadata, ())
        self.assertEqual(second.detail, "")
        self.assertIn("REQ-0001", second.statement)

    def test_legacy_fields(self):
        first, second = self.rules.files["values/types.md"].requirements
        self.assertEqual(
            (first.id, first.anchor, first.section, first.text),
            ("REQ-0001", "req-0001", "Requirements", "A column has one type."),
        )
        self.assertEqual((second.id, second.section), ("REQ-0004", "Error conditions"))
        self.assertEqual(self.rules.files["values/types.md"].kind, "legacy")
        self.assertEqual(self.rules.files["inheritance.md"].kind, "cards")

    def test_misplaced_metadata_stray_headings_and_kinds(self):
        rule_file = model.parse_file(
            "a.md",
            "# A\n\n### A-1. Title\nFails: `x`.\n\n"
            "### A-2 Undotted\n\nStatement.\nReplaces: REQ-0001.\n\n"
            "Detail.\n\nEvidence: `b`.\n\n### Notes\n\n**REQ-0002.** Legacy.\n",
        )
        first, second = rule_file.cards
        self.assertIsNone(first.statement)
        self.assertEqual(first.fails, ("x",))
        self.assertFalse(second.dotted)
        self.assertEqual(second.title, "Undotted")
        self.assertEqual(second.stray_metadata, (9, 13))
        self.assertEqual([item.text for item in rule_file.stray_headings], ["Notes"])
        self.assertEqual(rule_file.kind, "mixed")
        self.assertEqual(model.parse_file("b.md", "# B\n\nProse.\n").kind, "empty")

    def test_index_listing_and_principles(self):
        self.assertEqual(
            [(row.path, row.title, row.prefix) for row in self.rules.listing],
            [
                ("inheritance.md", "Inheritance", "LAYER"),
                ("values/types.md", "Types", "REQ"),
            ],
        )
        self.assertEqual(self.rules.listing_problems, [])
        (principle,) = self.rules.principles
        self.assertEqual(
            (principle.number, principle.title), (1, "Layers are ordered.")
        )
        self.assertIn("[LAYER-1](inheritance.md#layer-1)", principle.text)
        rows, problems = model.parse_listing(
            "## Rule files\n\n| File | Prefix | Covers |\n| --- | --- | --- |\n"
            "| inheritance.md | LAYER | Layers. |\n"
        )
        self.assertEqual(rows, [])
        self.assertEqual(problems[0][0], 5)
        self.assertEqual(model.parse_listing("# Rules\n"), (None, []))

    def test_resolution_and_citations(self):
        rules = self.rules
        self.assertEqual(rules.prefixes, frozenset({"LAYER"}))
        self.assertIs(rules.resolve("REQ-0002"), rules.cards["LAYER-1"])
        self.assertEqual(rules.location("REQ-0003"), ("inheritance.md", "layer-1"))
        self.assertEqual(rules.location("REQ-0001"), ("values/types.md", "req-0001"))
        self.assertIsNone(rules.resolve("REQ-0099"))
        self.assertEqual(
            rules.identifiers(),
            {"LAYER-1", "LAYER-2", "REQ-0001", "REQ-0002", "REQ-0003", "REQ-0004"},
        )
        text = "UTF-8, ISO-8601, LAYER-2, MULTILAYER-3, REQ-0001 and REQ-999999.\n"
        self.assertEqual(
            [identifier for _, _, identifier in rules.citations(text)],
            ["LAYER-2", "REQ-0001", "REQ-999999"],
        )
        self.assertIsNotNone(rules.identifier.fullmatch("LAYER-12"))
        self.assertIsNone(rules.identifier.fullmatch("UTF-8"))

    def test_load_ref_reads_a_revision(self):
        def git(*arguments):
            subprocess.run(
                ["git", "-C", str(self.root), *arguments],
                check=True,
                capture_output=True,
            )

        git("init", "-q")
        git("add", "-A")
        git(
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@example.org",
            "commit",
            "-qm",
            "base",
        )
        (self.root / "rules/inheritance.md").unlink()
        rules = model.load_ref(self.root, "HEAD")
        self.assertIn("LAYER-1", rules.cards)
        self.assertEqual(rules.listing[0].prefix, "LAYER")
        self.assertNotIn("LAYER-1", model.load(self.root).cards)
        with self.assertRaises(model.RuleSourceError):
            model.load_ref(self.root, "no-such-revision")

    def test_authored_files_skip_build_output_and_generated_pages(self):
        for relative in (
            "build/lib/tools/x.md",
            "site/index.md",
            "python/.venv/lib/x.md",
            "rust/target/doc/x.md",
            "docs/benchmark/index.md",
            "yamaa.egg-info/x.md",
            "rules/reference/requirements.md",
            "docs/page.md",
        ):
            write(self.root, relative, "# Page\n")
        found = {
            path.relative_to(self.root).as_posix()
            for path in model.authored_files(self.root, {".md"})
        }
        self.assertIn("docs/page.md", found)
        self.assertIn("rules/README.md", found)
        for skipped in (
            "build/lib/tools/x.md",
            "site/index.md",
            "python/.venv/lib/x.md",
            "rust/target/doc/x.md",
            "docs/benchmark/index.md",
            "yamaa.egg-info/x.md",
            "rules/reference/requirements.md",
        ):
            self.assertNotIn(skipped, found)


if __name__ == "__main__":
    unittest.main()
