#!/usr/bin/env python3
"""Tests for schema release policy checks."""

import importlib.util
import unittest
from pathlib import Path
from unittest import mock

SCRIPT = Path(__file__).with_name("check_schema_release.py")
SPEC = importlib.util.spec_from_file_location("check_schema_release", SCRIPT)
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


class SchemaReleaseTest(unittest.TestCase):
    def test_repository_release_metadata_is_consistent(self):
        self.assertEqual([], MODULE.validate())

    def test_unchanged_version_rejects_sensitive_diff(self):
        with mock.patch.object(MODULE, "schema_version_at", side_effect=["1.0.0-rc.1", "1.0.0-rc.1"]), \
             mock.patch.object(MODULE, "changed_paths", return_value=["yaml/rules/R001-execution-model.md"]):
            errors = MODULE.validate("base")
        self.assertTrue(any("without a bundle version" in error for error in errors))

    def test_version_change_allows_sensitive_diff(self):
        with mock.patch.object(MODULE, "schema_version_at", side_effect=["1.0.0-rc.1", "1.0.0-rc.0"]), \
             mock.patch.object(MODULE, "changed_paths", return_value=["yaml/schema.yaml"]):
            self.assertEqual([], MODULE.validate("base"))


if __name__ == "__main__":
    unittest.main()
