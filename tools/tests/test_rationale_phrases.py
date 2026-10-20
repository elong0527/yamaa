"""Tests for the rationale-phrase check of tools.rules.check.

Rules state expected behavior without rationale phrases (because, in order to,
the reason is, this ensures, so that, to ensure).
"""

import tempfile
import unittest
from pathlib import Path

from tools.rules import check as rules_check


class RationalePhraseTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.rules = Path(temporary.name) / "rules"
        (self.rules / "values").mkdir(parents=True)
        self._saved_allowlist = rules_check.RATIONALE_ALLOWLIST
        rules_check.RATIONALE_ALLOWLIST = {}

    def tearDown(self):
        rules_check.RATIONALE_ALLOWLIST = self._saved_allowlist

    def write(self, label, body):
        path = self.rules / label
        path.write_text(body, encoding="ascii")
        return path

    def errors(self):
        return rules_check.rationale_errors(self.rules.parent)

    def test_each_banned_phrase_fails(self):
        for phrase in (
            "because",
            "in order to",
            "the reason is",
            "this ensures",
            "so that",
            "to ensure",
        ):
            with self.subTest(phrase=phrase):
                self.write(
                    "values/one.md",
                    f"# One\n\nA column fails {phrase} the value is missing.\n",
                )
                found = self.errors()
                self.assertTrue(
                    any(phrase in error for error in found),
                    f"no violation reported for '{phrase}': {found}",
                )
                (self.rules / "values/one.md").unlink()

    def test_clean_prose_passes(self):
        self.write(
            "values/one.md",
            "# One\n\nA column fails when the value is missing. "
            "The run records the failure.\n",
        )
        self.assertEqual(self.errors(), [])

    def test_every_prose_section_is_checked(self):
        self.write(
            "values/one.md",
            "# One\n\n"
            "## Requirements\n\n"
            "A column fails because the value is missing.\n\n"
            "## Rationale\n\n"
            "A column fails because a missing value names no cell.\n\n"
            "## Conformance examples\n\n"
            "A column fails so that the failure is recorded.\n",
        )
        found = self.errors()
        self.assertEqual(len(found), 3)
        self.assertTrue(all("values/one.md:5" in e for e in found[:1]))
        self.assertIn("values/one.md:9", found[1])
        self.assertIn("values/one.md:13", found[2])

    def test_fenced_code_is_excluded(self):
        self.write(
            "values/one.md",
            "# One\n\n```yaml\nreason: because the schema says so\n```\n",
        )
        self.assertEqual(self.errors(), [])

    def test_prose_before_midline_fence_counts(self):
        self.write(
            "values/one.md",
            "# One\n\nA pattern fails because it is empty. ```text\nexample\n```\n",
        )
        self.assertTrue(any("because" in error for error in self.errors()))

    def test_inline_code_is_excluded(self):
        self.write(
            "values/one.md",
            "# One\n\nThe field `because` names the reason column.\n",
        )
        self.assertEqual(self.errors(), [])

    def test_allowlist_suppresses_listed_pattern(self):
        body = "# One\n\nA column fails because the value is missing.\n"
        self.write("values/one.md", body)
        self.assertTrue(self.errors())
        rules_check.RATIONALE_ALLOWLIST = {
            "values/one.md": [(r"because the value is missing", "unavoidable quote")]
        }
        self.assertEqual(self.errors(), [])

    def test_allowlist_pattern_must_match(self):
        self.write(
            "values/one.md",
            "# One\n\nA column fails because the row is missing.\n",
        )
        rules_check.RATIONALE_ALLOWLIST = {
            "values/one.md": [(r"because the value is missing", "unavoidable quote")]
        }
        self.assertTrue(self.errors())

    def test_case_insensitive(self):
        self.write(
            "values/one.md",
            "# One\n\nBecause the value is missing, the column fails.\n",
        )
        self.assertTrue(any("Because" in error for error in self.errors()))

    def test_frontmatter_is_excluded(self):
        self.write(
            "values/one.md",
            "---\ntitle: Because things fail\n---\n\nA column fails.\n",
        )
        self.assertEqual(self.errors(), [])

    def test_line_numbers_count_front_matter(self):
        self.write(
            "values/one.md",
            "---\ntitle: One\n---\n\nA column fails so that it is seen.\n",
        )
        self.assertEqual(len(self.errors()), 1)
        self.assertIn("values/one.md:5:", self.errors()[0])

    def test_card_files_are_checked(self):
        self.write(
            "inheritance.md",
            '# Inheritance\n\n<a id="layer-1"></a>\n\n'
            "### LAYER-1. Layers compose in one order\n\n"
            "Layers compose depth first because order matters.\n\n"
            "Fails: `inheritance_cycle`.\n",
        )
        self.assertEqual(
            self.errors(),
            [
                "inheritance.md:7: rationale phrase 'because': Layers compose depth "
                "first because order matters."
            ],
        )


if __name__ == "__main__":
    unittest.main()
