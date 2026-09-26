#!/usr/bin/env python3
"""Regression checks for lost contracts, aliases, and duplicate authorities."""

import shutil
import tempfile
import unittest
from pathlib import Path

from check_rule_rewrite import check, load_migration, resolve_requirement
from generate_rule_reference import generated

REPO = Path(__file__).resolve().parents[3]


class RuleRewriteTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        shutil.copytree(REPO / "yaml", self.root / "yaml")
        shutil.copytree(REPO / "rules", self.root / "rules")
        (self.root / "benchmarks").symlink_to(
            REPO / "benchmarks", target_is_directory=True
        )
        self.rules = self.root / "rules"

    def replace(self, path, before, after):
        body = path.read_text(encoding="ascii")
        self.assertIn(before, body)
        path.write_text(body.replace(before, after), encoding="ascii")

    def test_complete_cutover_and_legacy_aliases_resolve(self):
        errors, coverage = check(self.root)
        self.assertEqual(errors, [])
        self.assertEqual(sum(total for _, total in coverage.values()), 1071)
        self.assertTrue(all(mapped == total for mapped, total in coverage.values()))
        migration = load_migration(self.root)
        self.assertEqual(resolve_requirement("R011-35", migration), ["REQ-0005"])
        self.assertEqual(resolve_requirement("REQ-0005", migration), ["REQ-0005"])
        self.assertEqual(resolve_requirement("R001-42", migration), ["REQ-0033"])
        self.assertEqual(resolve_requirement("REQ-0073", migration), ["REQ-0033"])
        self.assertEqual(resolve_requirement("R002-28", migration), ["REQ-0080"])
        self.assertEqual(resolve_requirement("REQ-0104", migration), ["REQ-0080"])
        self.assertEqual(resolve_requirement("REQ-0096", migration), ["REQ-0098"])
        self.assertEqual(resolve_requirement("REQ-0332", migration), [])
        self.assertGreater(len(resolve_requirement("R007-9", migration)), 1)
        self.assertEqual(resolve_requirement("R999-1", migration), [])
        self.assertTrue(resolve_requirement("R001-12a", migration))

    def test_file_move_preserves_requirement_ids(self):
        source = self.rules / "values/numbers.md"
        source.rename(source.with_name("representation.md"))
        for base in (self.root / "yaml", self.root / "rules"):
            for path in base.rglob("*"):
                if not path.is_file():
                    continue
                text = path.read_text()
                text = text.replace("values/numbers", "values/representation")
                text = text.replace("(numbers.md", "(representation.md")
                path.write_text(text)
        errors, _ = check(self.root)
        self.assertEqual(errors, [])
        self.assertEqual(
            resolve_requirement("R011-24", load_migration(self.root)), ["REQ-0017"]
        )

    def test_missing_source_record_fails(self):
        path = self.rules / "migration.yaml"
        text = path.read_text()
        path.write_text(
            "\n".join(
                line for line in text.splitlines() if not line.startswith("  R011-6:")
            )
            + "\n"
        )
        errors, _ = check(self.root)
        self.assertTrue(any("retain all 1071" in error for error in errors))
        self.assertTrue(any("unknown provenance" in error for error in errors))

    def test_deleted_target_and_dangling_reference_fail(self):
        self.replace(self.rules / "values/numbers.md", "**REQ-0021.**", "**REQ-9999.**")
        errors, _ = check(self.root)
        self.assertIn("unresolved requirement reference: REQ-0021", errors)
        self.assertIn("migration target missing or in wrong file: REQ-0021", errors)

    def test_duplicate_definitions_across_files_fail(self):
        self.replace(self.rules / "values/text.md", "**REQ-0022.**", "**REQ-0001.**")
        errors, _ = check(self.root)
        self.assertIn("duplicate requirement: REQ-0001", errors)

    def test_retired_requirement_cannot_be_redefined(self):
        path = self.rules / "execution/lifecycle.md"
        path.write_text(path.read_text() + "\n**REQ-0073.** Restored.\n")
        errors, _ = check(self.root)
        self.assertIn("retired requirement still defined: REQ-0073", errors)

    def test_retired_replacement_must_exist(self):
        self.replace(
            self.rules / "migration.yaml",
            '"replacement": ["REQ-0033"]',
            '"replacement": ["REQ-9999"]',
        )
        errors, _ = check(self.root)
        self.assertIn("invalid retired requirement replacement: REQ-0073", errors)

    def test_redirected_legacy_alias_must_match_retired_replacement(self):
        self.replace(
            self.rules / "migration.yaml",
            'R002-28: {"file": "R002-source-binding.md", "targets": ["REQ-0080"]}',
            'R002-28: {"file": "R002-source-binding.md", "targets": ["REQ-0081"]}',
        )
        errors, _ = check(self.root)
        self.assertIn("invalid reverse legacy alias: REQ-0104 -> R002-28", errors)
    def test_retired_requirement_stub_fails(self):
        self.replace(
            self.rules / "values/text.md",
            "**REQ-0022.**",
            "**REQ-0022.** Retired.",
        )
        errors, _ = check(self.root)
        self.assertIn(
            "values/text.md: retired requirement must be deleted: REQ-0022",
            errors,
        )

    def test_duplicate_yaml_keys_fail(self):
        self.replace(
            self.rules / "migration.yaml", "version: 2", "version: 2\nversion: 2"
        )
        errors, _ = check(self.root)
        self.assertTrue(any("duplicate migration key" in error for error in errors))

    def test_schema_prose_cannot_become_a_second_contract(self):
        path = self.root / "yaml/schema_shared.yaml"
        text = path.read_text()
        path.write_text(
            text.replace(
                "description: See ", "description: Silently change behavior. See ", 1
            )
        )
        errors, _ = check(self.root)
        self.assertTrue(
            any(
                "schema description has no canonical owner" in error for error in errors
            )
        )

    def test_index_and_status_are_required(self):
        self.replace(
            self.rules / "values/text.md", "status: normative", "status: proposed"
        )
        self.replace(
            self.rules / "README.md", "(values/text.md)", "(values/missing.md)"
        )
        errors, _ = check(self.root)
        self.assertIn(
            "values/text.md: expected id, title, and normative status", errors
        )
        self.assertIn("values/text.md: absent from normative rule index", errors)

    def test_schema_comment_cannot_become_a_second_contract(self):
        path = self.root / "yaml/schema_shared.yaml"
        path.write_text(path.read_text() + "\n# Missing values become zero.\n")
        errors, _ = check(self.root)
        self.assertIn(
            "schema comment has no canonical owner: schema_shared.yaml", errors
        )

    def test_generated_references_match_sources(self):
        for path, expected in generated(self.root).items():
            self.assertEqual(path.read_text(), expected)
        index = (self.rules / "reference/requirements.md").read_text()
        self.assertNotIn("| [REQ-0073]", index)
        self.assertIn("R001-42", next(
            line for line in index.splitlines() if line.startswith("| [REQ-0033]")
        ))


if __name__ == "__main__":
    unittest.main()
