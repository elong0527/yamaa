#!/usr/bin/env python3
"""Regression checks for migration loss and file-independent identity."""

import shutil
import tempfile
import unittest
from pathlib import Path

from check_rule_rewrite import check

REPO = Path(__file__).resolve().parents[3]


class RuleRewriteTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        for name in ("rules", "rules-next"):
            shutil.copytree(REPO / "yaml" / name, self.root / "yaml" / name)
        self.drafts = self.root / "yaml" / "rules-next"

    def replace(self, path, before, after):
        body = path.read_text(encoding="ascii")
        self.assertIn(before, body)
        path.write_text(body.replace(before, after), encoding="ascii")

    def test_file_move_preserves_identity(self):
        source = self.drafts / "values" / "numbers.md"
        destination = self.drafts / "another-block" / "representation.md"
        destination.parent.mkdir()
        source.rename(destination)
        errors, coverage = check(self.root)
        self.assertEqual(errors, [])
        self.assertEqual(coverage["R011"], (35, 35))

    def test_losing_complete_source_mapping_fails(self):
        self.replace(self.drafts / "migration.yaml", "sources: [R011-6]", "sources: [R011-5]")
        errors, _ = check(self.root)
        self.assertIn("complete source has unmapped requirement: R011-6", errors)

    def test_deleted_target_and_dangling_reference_fail(self):
        self.replace(self.drafts / "values" / "numbers.md", "**REQ-0021.**", "**REQ-0099.**")
        errors, _ = check(self.root)
        self.assertIn("unknown migration target: REQ-0021", errors)
        self.assertIn("unresolved draft reference: REQ-0021", errors)
        self.assertIn("draft requirement has no legacy provenance: REQ-0099", errors)

    def test_duplicate_definitions_across_files_fail(self):
        self.replace(self.drafts / "values" / "text.md", "**REQ-0022.**", "**REQ-0001.**")
        errors, _ = check(self.root)
        self.assertIn("duplicate draft requirement: REQ-0001", errors)

    def test_source_typo_and_duplicate_target_fail(self):
        path = self.drafts / "migration.yaml"
        self.replace(path, "sources: [R011-6]", "sources: [R999-6]")
        self.replace(path, "target: REQ-0003", "target: REQ-0002")
        errors, _ = check(self.root)
        self.assertIn("unknown legacy requirement: R999-6", errors)
        self.assertIn("duplicate migration target: REQ-0002", errors)

    def test_duplicate_yaml_keys_fail(self):
        self.replace(self.drafts / "migration.yaml", "version: 1", "version: 1\nversion: 1")
        errors, _ = check(self.root)
        self.assertTrue(any("duplicate migration key" in error for error in errors))


if __name__ == "__main__":
    unittest.main()
