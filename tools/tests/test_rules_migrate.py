"""The migration helper moves citations to cards and reports a conversion."""

import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

from tools.rules.migrate import legacy_report, migrate_links, statement_report
from tools.rules.model import load, load_ref
from tools.tests.rules_fixture import write

INDEX = """\
# Rules

## Rule files

| File | Prefix | Covers |
| --- | --- | --- |
| [Specification composition](specification/composition.md) | REQ | Layers. |
| [Execution lifecycle](execution/lifecycle.md) | REQ | Order. |
| [Types](values/types.md) | REQ | Types. |
"""

COMPOSITION = """\
---
id: specification/composition
title: Specification composition
status: normative
---

# Specification composition

## Requirements

<a id="req-0010"></a>

**REQ-0010.** Layers compose depth first.

<a id="req-0011"></a>

**REQ-0011.** A null clears an inherited field.

<a id="req-0012"></a>

**REQ-0012.** Parents are local files.
"""

LIFECYCLE = """\
---
id: execution/lifecycle
title: Execution lifecycle
status: normative
---

# Execution lifecycle

## Requirements

<a id="req-0020"></a>

**REQ-0020.** [Specification composition](../specification/composition.md)
composes first. A null clears under
[REQ-0011](../specification/composition.md#req-0011), see also REQ-0010 and
`REQ-0010`. Types follow [REQ-0001](../values/types.md).
"""

TYPES = """\
---
id: values/types
title: Types
status: normative
---

# Types

## Requirements

<a id="req-0001"></a>

**REQ-0001.** A column has one type.

<a id="req-0004"></a>

**REQ-0004.** A failed conversion fails.
"""

CARDS = """\
# Inheritance

## Layers

<a id="layer-1"></a>

### LAYER-1. Layers compose depth first

Layers compose depth first; a null clears an inherited field.

Replaces: REQ-0010,
REQ-0011.

Detail cites REQ-0011 again.

<a id="layer-2"></a>

### LAYER-2. Parents are local files

A parent is a local file.

Replaces: REQ-0099.
"""

NUMBERS = """\
---
id: values/numbers
title: Numbers
status: normative
---

# Numbers

## Requirements

<a id="req-0004"></a>

**REQ-0004.** A failed conversion fails.
"""


class MigrationTests(unittest.TestCase):
    def setUp(self):
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        self.root = Path(directory.name)
        for relative, text in {
            "rules/README.md": INDEX,
            "rules/specification/composition.md": COMPOSITION,
            "rules/execution/lifecycle.md": LIFECYCLE,
            "rules/values/types.md": TYPES,
            "docs/page.md": (
                "# Page\n\n[REQ-0011](../rules/specification/composition.md#req-0011)"
                " and REQ-0010, pinned at [REQ-0010](https://github.com/elong0527/"
                "yamaa/blob/yamaa-v0.4.0/rules/specification/composition.md#req-0010).\n"
            ),
            "rust/README.md": (
                "# Rust\n\nLayers follow the\n"
                "[composition contract](../rules/specification/composition.md).\n"
            ),
        }.items():
            write(self.root, relative, text)
        self.git("init", "-q")
        self.git("add", "-A")
        self.git(
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@example.org",
            "commit",
            "-qm",
            "base",
        )

    def git(self, *arguments):
        subprocess.run(
            ["git", "-C", str(self.root), *arguments], check=True, capture_output=True
        )

    def convert(self):
        """Convert composition into cards and move REQ-0004, as a PR would."""
        (self.root / "rules/specification/composition.md").unlink()
        write(
            self.root,
            "rules/README.md",
            INDEX.replace(
                "| [Specification composition](specification/composition.md) | REQ |",
                "| [Inheritance](inheritance.md) | LAYER |",
            )
            + "| [Numbers](values/numbers.md) | REQ | Numbers. |\n",
        )
        write(self.root, "rules/inheritance.md", CARDS)
        write(self.root, "rules/values/numbers.md", NUMBERS)
        types = self.root / "rules/values/types.md"
        types.write_text(TYPES.split('<a id="req-0004">')[0].rstrip() + "\n")

    def read(self, relative):
        return (self.root / relative).read_text(encoding="utf-8")

    def test_links_follow_replaced_requirements_and_moved_files(self):
        self.convert()
        migration = migrate_links(self.root, load_ref(self.root, "HEAD"))
        self.assertEqual(migration.unresolved, [])
        lifecycle = self.read("rules/execution/lifecycle.md")
        self.assertIn(
            "**REQ-0020.** [Inheritance](../inheritance.md)\ncomposes first.", lifecycle
        )
        self.assertIn(
            "[LAYER-1](../inheritance.md#layer-1), see also LAYER-1 and\n`REQ-0010`.",
            lifecycle,
        )
        self.assertIn(
            "Types follow [REQ-0001](../values/types.md#req-0001).", lifecycle
        )
        # A card's own Replaces line keeps the IDs it absorbs; its detail moves.
        cards = self.read("rules/inheritance.md")
        self.assertIn("Replaces: REQ-0010,\nREQ-0011.", cards)
        self.assertIn("Detail cites LAYER-1 again.", cards)
        # Outside rules/, links move and bare IDs keep resolving through Replaces.
        page = self.read("docs/page.md")
        self.assertIn("[LAYER-1](../rules/inheritance.md#layer-1) and REQ-0010", page)
        self.assertIn("yamaa-v0.4.0/rules/specification/composition.md#req-0010", page)
        self.assertIn(
            "[composition contract](../rules/inheritance.md).",
            self.read("rust/README.md"),
        )
        self.assertEqual(len(migration.log), 7)
        # A second run finds nothing left to move.
        self.assertEqual(migrate_links(self.root, load_ref(self.root, "HEAD")).log, [])

    def test_links_without_a_base_report_deleted_files(self):
        self.convert()
        migration = migrate_links(self.root, None, dry_run=True)
        self.assertEqual(
            migration.unresolved,
            [
                "rules/execution/lifecycle.md:13: ../specification/composition.md: "
                "pass --base with a revision that has it",
                "rust/README.md:4: ../rules/specification/composition.md: "
                "pass --base with a revision that has it",
            ],
        )
        self.assertIn(
            "rules/execution/lifecycle.md:15: [REQ-0011](../specification/composition.md"
            "#req-0011) -> [LAYER-1](../inheritance.md#layer-1)",
            migration.log,
        )
        # A dry run writes nothing.
        self.assertIn(
            "[REQ-0011](../specification/composition.md#req-0011)",
            self.read("rules/execution/lifecycle.md"),
        )

    def test_report_accounts_for_every_legacy_requirement(self):
        self.convert()
        report = legacy_report(load_ref(self.root, "HEAD"), load(self.root), "base")
        self.assertEqual(
            report.splitlines(),
            [
                "Legacy requirements defined at base: 6; 2 replaced by cards, 1 moved, "
                "1 no longer defined, 2 unchanged.",
                "",
                "| Requirement | Was in | Now |",
                "| --- | --- | --- |",
                "| REQ-0004 | `values/types.md` | moved to `values/numbers.md` |",
                "| REQ-0010 | `specification/composition.md` | LAYER-1 (`inheritance.md`) |",
                "| REQ-0011 | `specification/composition.md` | LAYER-1 (`inheritance.md`) |",
                "| REQ-0012 | `specification/composition.md` | no longer defined |",
                "",
                "Replaced IDs that base does not define: REQ-0099 (LAYER-2).",
            ],
        )

    def test_statements_list_added_changed_and_removed_language(self):
        self.convert()
        self.assertTrue(
            statement_report(
                load_ref(self.root, "HEAD"), load(self.root), "base"
            ).startswith(
                "Card language relative to base: 2 added, 0 changed, 0 removed.\n\n## Added\n\n"
                "### LAYER-1. Layers compose depth first\n\n`rules/inheritance.md`\n\n"
                "Layers compose depth first; a null clears an inherited field.\n"
            )
        )
        self.git("add", "-A")
        self.git(
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@example.org",
            "commit",
            "-qm",
            "cards",
        )
        cards = self.root / "rules/inheritance.md"
        text = cards.read_text().replace(
            "a null clears an inherited field.", "a null\nclears an inherited field."
        )
        cards.write_text(text.split('<a id="layer-2">')[0].rstrip() + "\n")
        report = statement_report(load_ref(self.root, "HEAD"), load(self.root), "cards")
        # Rewrapping is not a language change; removing a card is.
        self.assertEqual(
            report,
            "Card language relative to cards: 0 added, 0 changed, 1 removed.\n\n"
            "## Removed\n\n### LAYER-2. Parents are local files\n\n"
            "`rules/inheritance.md`\n\nA parent is a local file.\n",
        )
        cards.write_text(
            text.replace(
                "Layers compose depth first\n", "Layers compose breadth first\n", 1
            ).replace("; a null", ", and a null")
        )
        report = statement_report(load_ref(self.root, "HEAD"), load(self.root), "cards")
        self.assertIn("1 changed", report)
        self.assertIn(
            "```diff\n- Layers compose depth first\n+ Layers compose breadth first\n\n"
            "- Layers compose depth first; a null clears an inherited field.\n"
            "+ Layers compose depth first, and a null clears an inherited field.\n```",
            report,
        )

    def test_command_line(self):
        self.convert()
        base = [sys.executable, "-m", "tools.rules.migrate", "--root", str(self.root)]
        result = subprocess.run(
            [*base, "report", "--base", "HEAD"],
            capture_output=True,
            text=True,
            check=False,
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn(
            "| REQ-0010 | `specification/composition.md` | LAYER-1", result.stdout
        )
        result = subprocess.run(
            [*base, "statements", "--base", "missing-ref"],
            capture_output=True,
            text=True,
            check=False,
        )
        self.assertEqual(result.returncode, 1)
        self.assertIn("ERROR: git", result.stderr)
        result = subprocess.run(
            [*base, "links", "--dry-run"], capture_output=True, text=True, check=False
        )
        self.assertEqual(result.returncode, 1)
        self.assertIn("would rewrite", result.stdout)
        self.assertIn("UNRESOLVED: rust/README.md:4", result.stderr)


if __name__ == "__main__":
    unittest.main()
