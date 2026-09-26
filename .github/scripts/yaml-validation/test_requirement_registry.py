#!/usr/bin/env python3
"""Tests for check_requirement_registry.py.

Every active requirement ID is defined exactly once as a bold dotted marker
``**REQ-0001.**``. Retired IDs occur only in migration.yaml; other citations
in rules/ must resolve to active definitions.
"""

import tempfile
import unittest
from pathlib import Path

import check_requirement_registry


class RequirementRegistryTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.rules = self.root / "rules"
        (self.rules / "values").mkdir(parents=True)
        (self.rules / "operations").mkdir(parents=True)

    def write(self, label, body):
        path = self.rules / label
        path.write_text(body, encoding="ascii")
        return path

    def errors(self):
        return check_requirement_registry.check(self.root)

    def test_valid_registry_passes(self):
        self.write(
            "values/types.md",
            "# Types\n\n**REQ-0001.** A column has one type.\n",
        )
        self.write(
            "operations/compute.md",
            "# Compute\n\nSee REQ-0001 for the type.\n",
        )
        (self.rules / "migration.yaml").write_text(
            "version: 2\nprose:\n  - R001-1: [REQ-0001]\n", encoding="ascii"
        )
        self.assertEqual(self.errors(), [])

    def test_duplicate_definition_across_files_fails(self):
        self.write("values/a.md", "**REQ-0001.** First.\n")
        self.write("operations/b.md", "**REQ-0001.** Second.\n")
        found = self.errors()
        self.assertTrue(
            any(
                "duplicate requirement definition: REQ-0001" in error
                and "operations/b.md" in error
                and "values/a.md" in error
                for error in found
            ),
            found,
        )

    def test_duplicate_definition_within_one_file_fails(self):
        self.write(
            "values/a.md",
            "**REQ-0001.** First.\n\n**REQ-0001.** Second.\n",
        )
        found = self.errors()
        self.assertTrue(
            any(
                "duplicate requirement definition: REQ-0001" in error and "2x" in error
                for error in found
            ),
            found,
        )

    def test_unresolved_citation_fails(self):
        self.write(
            "operations/b.md",
            "# Compute\n\nSee REQ-0099 for the type.\n",
        )
        found = self.errors()
        self.assertTrue(
            any(
                "unresolved requirement citation: REQ-0099" in error
                and "operations/b.md" in error
                for error in found
            ),
            found,
        )

    def test_unresolved_migration_citation_fails(self):
        self.write("values/a.md", "**REQ-0001.** First.\n")
        (self.rules / "migration.yaml").write_text(
            "version: 2\nprose:\n  - R001-1: [REQ-0099]\n", encoding="ascii"
        )
        found = self.errors()
        self.assertTrue(
            any(
                "unresolved requirement citation: REQ-0099" in error
                and "migration.yaml" in error
                for error in found
            ),
            found,
        )

    def test_retired_id_is_allowed_only_in_migration(self):
        self.write("values/a.md", "**REQ-0001.** First.\n")
        (self.rules / "migration.yaml").write_text(
            "requirements:\n"
            "  REQ-0099: {retired: true, replacement: [REQ-0001]}\n",
            encoding="ascii",
        )
        self.assertEqual(self.errors(), [])
        self.write("operations/b.md", "See REQ-0099.\n")
        self.assertTrue(
            any("unresolved requirement citation: REQ-0099" in error for error in self.errors())
        )

    def test_definition_in_fenced_code_does_not_count(self):
        self.write(
            "values/a.md",
            "# Types\n\n```yaml\n**REQ-0099.** Not a definition.\n```\n",
        )
        self.write(
            "operations/b.md",
            "# Compute\n\nSee REQ-0099 for the type.\n",
        )
        found = self.errors()
        self.assertTrue(
            any(
                "unresolved requirement citation: REQ-0099" in error for error in found
            ),
            found,
        )

    def test_citation_in_inline_code_does_not_count(self):
        self.write(
            "operations/b.md",
            "# Compute\n\nThe marker `REQ-0099` is literal text.\n",
        )
        self.assertEqual(self.errors(), [])

    def test_missing_migration_yaml_is_tolerated(self):
        self.write("values/a.md", "**REQ-0001.** First.\n")
        self.assertEqual(self.errors(), [])


if __name__ == "__main__":
    unittest.main()
