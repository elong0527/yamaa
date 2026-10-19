"""The migration helper moves citations to cards and reports a conversion."""

import contextlib
import io
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

from tools.rules.migrate import (
    citation_summary,
    legacy_report,
    main,
    migrate_citations,
    migrate_links,
    statement_report,
)
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


CITATION_INDEX = """\
# Rules

## Rule files

| File | Prefix | Covers |
| --- | --- | --- |
| [Inheritance](inheritance.md) | LAYER | Layers. |
| [Types](values/types.md) | REQ | Types. |
"""

CITATION_CARDS = """\
# Inheritance

## Layers

<a id="layer-1"></a>

### LAYER-1. Layers compose depth first

Layers compose depth first; a null clears an inherited field.

Replaces: REQ-0010,
REQ-0011.

Detail cites REQ-0012.

<a id="layer-2"></a>

### LAYER-2. Parents are local files

A parent is a local file.

Replaces: REQ-0012.
"""

CITATION_CONTRACT = """\
---
id: values/types
title: Types
status: normative
---

# Types

## Requirements

<a id="req-0001"></a>

**REQ-0001.** A column has one type.
"""

PARQUET = b"PAR1\x00\x15\x08REQ-0012\x00\x00PAR1"

CITING = {
    "rules/README.md": CITATION_INDEX,
    "rules/inheritance.md": CITATION_CARDS,
    "rules/values/types.md": CITATION_CONTRACT,
    "rust/src/lib.rs": (
        "//! Layers compose depth first (REQ-0010); a column has one type (REQ-0001).\n"
        'const RULE: &str = "REQ-0011";\n'
    ),
    "benchmarks/negative-cycle/expected/error.yaml": (
        "phase: validation\ncondition: inheritance_cycle\nrequirement: REQ-0011\n"
    ),
    "benchmarks/schema-checks/expected/checks.csv": (
        "RULE,status\r\nREQ-0012,held\r\nLAYER-1,held\r\n"
    ),
    "benchmarks/schema-checks/input/dm.csv": "USUBJID,NOTE\nS-1,REQ-0010\n",
    "docs/page.md": "Layers follow REQ-0010, as LAYER-2 says.\n",
    "docs/releases/0.4.0.md": "REQ-0010 becomes LAYER-1.\n",
    "tools/unknown.py": 'UNKNOWN = "REQ-9999"\n',
    "tools/plain.py": "PLAIN = 1\n",
}


class CitationTests(unittest.TestCase):
    def setUp(self):
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        self.root = Path(directory.name)
        for relative, text in CITING.items():
            path = self.root / relative
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(text.encode("utf-8"))
        parquet = self.root / "benchmarks/schema-checks/expected/checks.parquet"
        parquet.write_bytes(PARQUET)

    def read(self, relative):
        return (self.root / relative).read_bytes().decode("utf-8")

    def snapshot(self):
        return {
            path.relative_to(self.root).as_posix(): path.read_bytes()
            for path in sorted(self.root.rglob("*"))
            if path.is_file()
        }

    def test_citations_move_to_the_replacing_card(self):
        migration = migrate_citations(self.root)
        self.assertEqual(migration.refused, [])
        self.assertEqual(
            sorted(migration.rewrites),
            [
                (
                    "benchmarks/negative-cycle/expected/error.yaml",
                    3,
                    "REQ-0011",
                    "LAYER-1",
                ),
                (
                    "benchmarks/schema-checks/expected/checks.csv",
                    2,
                    "REQ-0012",
                    "LAYER-2",
                ),
                ("docs/page.md", 1, "REQ-0010", "LAYER-1"),
                ("rules/inheritance.md", 14, "REQ-0012", "LAYER-2"),
                ("rust/src/lib.rs", 1, "REQ-0010", "LAYER-1"),
                ("rust/src/lib.rs", 2, "REQ-0011", "LAYER-1"),
            ],
        )
        self.assertEqual(
            self.read("rust/src/lib.rs"),
            "//! Layers compose depth first (LAYER-1); a column has one type (REQ-0001).\n"
            'const RULE: &str = "LAYER-1";\n',
        )
        self.assertEqual(
            self.read("benchmarks/negative-cycle/expected/error.yaml"),
            "phase: validation\ncondition: inheritance_cycle\nrequirement: LAYER-1\n",
        )
        # Line endings stay as written.
        self.assertEqual(
            self.read("benchmarks/schema-checks/expected/checks.csv"),
            "RULE,status\r\nLAYER-2,held\r\nLAYER-1,held\r\n",
        )
        # The cards keep their Replaces lines; their detail moves.
        self.assertEqual(
            self.read("rules/inheritance.md"),
            CITATION_CARDS.replace("Detail cites REQ-0012.", "Detail cites LAYER-2."),
        )
        self.assertEqual(migration.replaces, 3)
        # A legacy requirement still defined and a sentinel stay.
        self.assertEqual(
            sorted(migration.kept),
            [
                ("rules/values/types.md", 13, "REQ-0001"),
                ("rust/src/lib.rs", 1, "REQ-0001"),
                ("tools/unknown.py", 1, "REQ-9999"),
            ],
        )
        self.assertEqual(self.read("rules/values/types.md"), CITATION_CONTRACT)
        self.assertEqual(self.read("tools/unknown.py"), 'UNKNOWN = "REQ-9999"\n')

    def test_binary_files_are_listed_and_not_edited(self):
        migration = migrate_citations(self.root)
        self.assertEqual(
            migration.binary,
            {
                "benchmarks/schema-checks/expected/checks.parquet": [
                    ("REQ-0012", 1, "LAYER-2")
                ]
            },
        )
        parquet = self.root / "benchmarks/schema-checks/expected/checks.parquet"
        self.assertEqual(parquet.read_bytes(), PARQUET)

    def test_inputs_release_notes_and_rule_fixtures_stay_as_written(self):
        write(self.root, "tools/tests/test_rules_model.py", 'ID = "REQ-0010"\n')
        write(self.root, "docs/design/interface-1.0/rules.md", "REQ-0011 moves.\n")
        migration = migrate_citations(self.root)
        self.assertEqual(
            migration.left,
            {
                "benchmarks/schema-checks/input/dm.csv": "benchmark inputs are data",
                "docs/design/interface-1.0/rules.md": (
                    "the design quotes the IDs it measured"
                ),
                "docs/releases/0.4.0.md": (
                    "release notes name the requirements of their release"
                ),
                "tools/tests/test_rules_model.py": "builds a rule set of its own",
            },
        )
        self.assertEqual(
            self.read("benchmarks/schema-checks/input/dm.csv"),
            CITING["benchmarks/schema-checks/input/dm.csv"],
        )
        self.assertEqual(
            self.read("docs/releases/0.4.0.md"), "REQ-0010 becomes LAYER-1.\n"
        )
        self.assertEqual(
            self.read("tools/tests/test_rules_model.py"), 'ID = "REQ-0010"\n'
        )

    def test_a_second_run_changes_nothing(self):
        migrate_citations(self.root)
        written = self.snapshot()
        again = migrate_citations(self.root)
        self.assertEqual((again.rewrites, again.texts, again.refused), ([], {}, []))
        self.assertEqual(self.snapshot(), written)

    def test_a_dry_run_reports_without_writing(self):
        before = self.snapshot()
        migration = migrate_citations(self.root, dry_run=True)
        self.assertEqual(len(migration.rewrites), 6)
        self.assertEqual(self.snapshot(), before)

    def test_an_id_that_names_no_rule_is_refused_and_nothing_is_written(self):
        write(self.root, "docs/other.md", "See REQ-0500 and REQ-0010.\n")
        before = self.snapshot()
        migration = migrate_citations(self.root)
        self.assertEqual(
            migration.refused, [("docs/other.md", 1, "REQ-0500", "names no rule")]
        )
        self.assertEqual(len(migration.rewrites), 7)
        self.assertEqual(self.snapshot(), before)

    def test_ranges_and_abbreviated_lists_are_refused(self):
        write(
            self.root,
            "docs/spans.md",
            "Covers REQ-0010 to REQ-0012, REQ-0010/0012 and REQ-0011-0012.\n"
            "Also REQ-0010 through\nREQ-0012 and REQ-0040-0041.\n"
            "Not spans: REQ-0010-style, REQ-0011/REQ-0012.\n",
        )
        before = self.snapshot()
        migration = migrate_citations(self.root)
        naming = "names IDs it does not write out; rewrite it by hand, naming "
        self.assertEqual(
            migration.refused,
            [
                (
                    "docs/spans.md",
                    1,
                    "REQ-0010 to REQ-0012",
                    naming + "LAYER-1, LAYER-2",
                ),
                ("docs/spans.md", 1, "REQ-0010/0012", naming + "LAYER-1, LAYER-2"),
                ("docs/spans.md", 1, "REQ-0011-0012", naming + "LAYER-1, LAYER-2"),
                (
                    "docs/spans.md",
                    2,
                    "REQ-0010 through REQ-0012",
                    naming + "LAYER-1, LAYER-2",
                ),
                ("docs/spans.md", 3, "REQ-0040-0041", "names no rule"),
            ],
        )
        self.assertIn(("docs/spans.md", 4, "REQ-0010", "LAYER-1"), migration.rewrites)
        self.assertIn(("docs/spans.md", 4, "REQ-0012", "LAYER-2"), migration.rewrites)
        self.assertEqual(self.snapshot(), before)

    def test_symlinks_are_not_followed(self):
        try:
            os.symlink(self.root / "docs/page.md", self.root / "docs/link.md")
        except OSError as exc:
            self.skipTest(f"cannot create a symbolic link: {exc}")
        migration = migrate_citations(self.root)
        self.assertNotIn("docs/link.md", migration.found)
        self.assertEqual(
            self.read("docs/link.md"), "Layers follow LAYER-1, as LAYER-2 says.\n"
        )

    def test_summary_accounts_by_extension(self):
        summary = citation_summary(migrate_citations(self.root, dry_run=True), True)
        self.assertEqual(
            summary.splitlines(),
            [
                "Would rewrite 6 REQ IDs in 5 files, of 15 found in 10 files.",
                "",
                "| Extension | Files | REQ IDs | Rewritten |",
                "| --- | ---: | ---: | ---: |",
                "| `.md` | 4 | 7 | 2 |",
                "| `.rs` | 1 | 3 | 2 |",
                "| `.csv` | 2 | 2 | 1 |",
                "| `.parquet` | 1 | 1 | 0 |",
                "| `.py` | 1 | 1 | 0 |",
                "| `.yaml` | 1 | 1 | 1 |",
                "| Total | 10 | 15 | 6 |",
                "",
                "Expected files: 2, holding 2 values: 1 `expected/error.yaml` file, and:",
                "- `benchmarks/schema-checks/expected/checks.csv`: 1 value",
                "",
                "Kept: 3 REQ IDs on the cards' Replaces lines, 1 sentinel ID, 2 IDs of "
                "legacy requirements still defined.",
                "",
                "Left as written: 2 files.",
                "- `benchmarks/schema-checks/input/dm.csv` (1): benchmark inputs are data",
                "- `docs/releases/0.4.0.md` (1): release notes name the requirements of "
                "their release",
                "",
                "Binary files to edit by hand: 1.",
                "- `benchmarks/schema-checks/expected/checks.parquet`: REQ-0012 "
                "(LAYER-2), 1 time",
            ],
        )

    def test_command_line(self):
        output = io.StringIO()
        with contextlib.redirect_stdout(output):
            status = main(["--root", str(self.root), "citations", "--dry-run"])
        self.assertEqual(status, 0)
        self.assertIn("rust/src/lib.rs:2: REQ-0011 -> LAYER-1\n", output.getvalue())
        self.assertIn("Would rewrite 6 REQ IDs in 5 files", output.getvalue())
        self.assertIn("REQ-0011", self.read("rust/src/lib.rs"))
        with contextlib.redirect_stdout(io.StringIO()):
            self.assertEqual(main(["--root", str(self.root), "citations"]), 0)
        self.assertNotIn("REQ-0011", self.read("rust/src/lib.rs"))
        write(self.root, "docs/other.md", "See REQ-0500.\n")
        output = io.StringIO()
        with contextlib.redirect_stdout(output):
            status = main(["--root", str(self.root), "citations"])
        self.assertEqual(status, 1)
        self.assertIn(
            "Refused, so nothing is written: 1.\n"
            "- `docs/other.md:1`: `REQ-0500` names no rule\n",
            output.getvalue(),
        )


if __name__ == "__main__":
    unittest.main()
