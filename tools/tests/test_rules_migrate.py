"""The statements report lists the card language a pull request changes."""

import tempfile
import unittest
from pathlib import Path

from tools.rules import migrate
from tools.rules.migrate import statement_report
from tools.rules.model import load, load_ref
from tools.tests import support
from tools.tests.rules_fixture import write

INDEX = """\
# Rules

## Rule files

| File | Prefix | Covers |
| --- | --- | --- |
| [Inheritance](inheritance.md) | LAYER | Layers. |
"""

CARDS = """\
# Inheritance

## Layers

<a id="layer-1"></a>

### LAYER-1. Layers compose depth first

Layers compose depth first; a null clears an inherited field.

<a id="layer-2"></a>

### LAYER-2. Parents are local files

A parent is a local file.
"""


class StatementTests(unittest.TestCase):
    def setUp(self):
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        self.root = Path(directory.name)
        write(self.root, "rules/README.md", INDEX)
        self.cards = write(self.root, "rules/inheritance.md", CARDS)
        support.init(self.root)
        support.git(self.root, "add", "-A")
        support.git(self.root, "commit", "-qm", "base")

    def report(self):
        return statement_report(load_ref(self.root, "HEAD"), load(self.root), "base")

    def test_rewrapping_is_no_language_change(self):
        unchanged = "Card language relative to base: 0 added, 0 changed, 0 removed.\n"
        self.assertEqual(self.report(), unchanged)
        self.cards.write_text(
            CARDS.replace("a null clears an", "a null\nclears an"), encoding="utf-8"
        )
        self.assertEqual(self.report(), unchanged)

    def test_statements_list_added_changed_and_removed_language(self):
        text = CARDS.split('<a id="layer-2">')[0]
        text = text.replace(
            "Layers compose depth first\n", "Layers compose breadth first\n", 1
        ).replace("; a null", ", and a null")
        text += (
            '<a id="layer-3"></a>\n\n### LAYER-3. A layer names one parent\n\n'
            "A layer names at most one parent.\n"
        )
        self.cards.write_text(text, encoding="utf-8")
        self.assertEqual(
            self.report(),
            "Card language relative to base: 1 added, 1 changed, 1 removed.\n\n"
            "## Added\n\n### LAYER-3. A layer names one parent\n\n"
            "`rules/inheritance.md`\n\nA layer names at most one parent.\n\n"
            "## Changed\n\n### LAYER-1. Layers compose breadth first\n\n"
            "`rules/inheritance.md`\n\n```diff\n- Layers compose depth first\n"
            "+ Layers compose breadth first\n\n"
            "- Layers compose depth first; a null clears an inherited field.\n"
            "+ Layers compose depth first, and a null clears an inherited field.\n"
            "```\n\n## Removed\n\n### LAYER-2. Parents are local files\n\n"
            "`rules/inheritance.md`\n\nA parent is a local file.\n",
        )

    def test_command_line(self):
        root = ("--root", str(self.root))
        status, output, errors = support.run_main(
            migrate.main, *root, "statements", "--base", "HEAD"
        )
        self.assertEqual(status, 0, errors)
        self.assertIn("0 added, 0 changed, 0 removed", output)
        status, _, errors = support.run_main(
            migrate.main, *root, "statements", "--base", "missing-ref"
        )
        self.assertEqual(status, 1)
        self.assertIn("ERROR: git", errors)


if __name__ == "__main__":
    unittest.main()
