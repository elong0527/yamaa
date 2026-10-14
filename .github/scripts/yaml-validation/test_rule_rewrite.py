#!/usr/bin/env python3
"""Regression checks for current contracts and duplicate authorities."""

import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

from check_rule_rewrite import (
    check,
    load_schema,
    requirement_owners,
    schema_descriptions,
)
from generate_rule_reference import generated

REPO = Path(__file__).resolve().parents[3]
CHECKER = Path(__file__).resolve().with_name("check_rule_rewrite.py")


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

    def assert_rejected(self, error):
        """Both the returned errors and the command-line contract report it."""
        errors, _ = check(self.root)
        self.assertIn(error, errors)
        result = subprocess.run(
            [sys.executable, str(CHECKER), "--root", str(self.root)],
            capture_output=True,
            text=True,
            check=False,
        )
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        self.assertIn(f"ERROR: {error}", result.stdout.splitlines())

    def navigation_errors(self):
        """Keep independent current owners; generation never rewrites this fixture."""
        fixture = Path(__file__).with_name("fixtures") / "schema-navigation.tsv"
        expected = dict(
            line.split("\t")
            for line in fixture.read_text(encoding="ascii").splitlines()
        )
        actual = {
            f"{path.name}:{field}": value
            for path in sorted((self.root / "yaml").glob("schema*.yaml"))
            for field, value in schema_descriptions(load_schema(path))
        }
        return [
            f"schema navigation owner changed: {source}: expected {expected.get(source)!r}, got {actual.get(source)!r}"
            for source in sorted(expected.keys() | actual.keys())
            if expected.get(source) != actual.get(source)
        ]

    def test_every_schema_owner_matches_independent_current_expectations(self):
        self.assertEqual(self.navigation_errors(), [])

    def test_wrong_existing_requirement_owner_is_rejected(self):
        self.replace(
            self.root / "yaml/schema_shared.yaml",
            "description: See REQ-1149.",
            "description: See REQ-0005.",
        )
        self.assertIn(
            "schema navigation owner changed: schema_shared.yaml:literal_value.description: expected 'See REQ-1149.', got 'See REQ-0005.'",
            self.navigation_errors(),
        )

    def test_current_contracts_resolve_without_history_inventory(self):
        errors, owners = check(self.root)
        self.assertEqual(errors, [])
        self.assertEqual(owners, requirement_owners(self.root))
        self.assertEqual(owners["REQ-0005"], "values/types.md")
        self.assertEqual(owners["REQ-1269"], "specification/binding.md")
        self.assertFalse((self.rules / "migration.yaml").exists())

    def test_contract_defining_an_unregistered_verification_fails(self):
        # REQ-1153 and REQ-1155 once defined checks no registry declared.
        self.replace(
            self.rules / "execution/verification.md",
            "**REQ-0382.** `all_or_none` requires",
            "**REQ-0382.** `exactly_one` requires",
        )
        self.assert_rejected(
            "execution/verification.md: REQ-0382 defines `exactly_one`, "
            "which no verification registry declares"
        )

    def test_schema_descriptor_keys_are_not_verification_fields(self):
        # `type` is how the schema describes a field, not a check or a field.
        self.replace(
            self.rules / "execution/verification.md",
            "**REQ-0382.** `all_or_none` requires",
            "**REQ-0382.** `type` requires",
        )
        self.assert_rejected(
            "execution/verification.md: REQ-0382 defines `type`, "
            "which no verification registry declares"
        )

    def test_registered_verification_without_a_defining_requirement_fails(self):
        self.replace(
            self.root / "yaml/schema_verification.yaml",
            "    row_count: # See REQ-1152.\n",
            "    exactly_one:\n"
            "        - id: {type: verification_id, required: false}\n"
            "    row_count: # See REQ-1152.\n",
        )
        self.assert_rejected(
            "schema_verification.yaml: `exactly_one` is registered but no "
            "execution/verification.md requirement defines it"
        )

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
            requirement_owners(self.root)["REQ-0017"], "values/representation.md"
        )
        index = generated(self.root)[self.rules / "reference/requirements.md"]
        self.assertIn("[REQ-0017](../values/representation.md#req-0017)", index)

    def test_deleted_target_and_dangling_reference_fail(self):
        self.replace(self.rules / "values/numbers.md", "**REQ-0021.**", "**REQ-9999.**")
        errors, _ = check(self.root)
        self.assertIn("unresolved requirement reference: REQ-0021", errors)

    def test_duplicate_definitions_across_files_fail(self):
        self.replace(self.rules / "values/text.md", "**REQ-0022.**", "**REQ-0001.**")
        errors, _ = check(self.root)
        self.assertIn("duplicate requirement: REQ-0001", errors)

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
            self.root / "yaml/schema_shared.yaml",
            'version: "1.0"',
            'version: "1.0"\nversion: "1.0"',
        )
        errors, _ = check(self.root)
        self.assertTrue(any("found duplicate key" in error for error in errors))

    def test_present_non_string_description_fails_checker_and_generator(self):
        path = self.root / "yaml/schema_shared.yaml"
        original = path.read_text()
        source = "schema_shared.yaml:literal_value.description"
        for malformed in ("null", "true", "17", "[]", "{}"):
            with self.subTest(value=malformed):
                path.write_text(
                    original.replace(
                        "description: See REQ-1149.", f"description: {malformed}"
                    )
                )
                error = f"schema description must be a string: {source}"
                self.assert_rejected(error)
                with self.assertRaisesRegex(ValueError, error):
                    generated(self.root)
                result = subprocess.run(
                    [
                        sys.executable,
                        str(CHECKER.with_name("generate_rule_reference.py")),
                        "--root",
                        str(self.root),
                        "--check",
                    ],
                    capture_output=True,
                    text=True,
                    check=False,
                )
                self.assertEqual(result.returncode, 1)
                self.assertIn(f"ERROR: {error}", result.stdout.splitlines())
                self.assertEqual(result.stderr, "")

    def test_description_without_descriptor_type_is_also_checked(self):
        path = self.root / "yaml/schema_shared.yaml"
        path.write_text(path.read_text() + "\nnavigation:\n    description: null\n")
        error = "schema description must be a string: schema_shared.yaml:navigation.description"
        self.assert_rejected(error)
        with self.assertRaisesRegex(ValueError, error):
            generated(self.root)

    def test_schema_sources_share_strict_yaml_loader(self):
        path = self.root / "yaml/schema_shared.yaml"
        original = path.read_text()
        cases = (
            ("description: &owner See REQ-1149.", "YAML anchors are not allowed"),
            ("description: *owner", "YAML aliases are not allowed"),
            ("description: !!str See REQ-1149.", "explicit YAML tags are not allowed"),
            (
                "description: See REQ-1149.\n    <<: {}",
                "YAML merge keys are not allowed",
            ),
        )
        for replacement, message in cases:
            with self.subTest(feature=message):
                path.write_text(
                    original.replace("description: See REQ-1149.", replacement)
                )
                errors, _ = check(self.root)
                self.assertTrue(
                    any(
                        "schema_shared.yaml: invalid schema source:" in error
                        and message in error
                        for error in errors
                    ),
                    errors,
                )
                with self.assertRaisesRegex(ValueError, message):
                    generated(self.root)

    def test_malformed_verification_schema_does_not_hide_independent_errors(self):
        self.replace(
            self.root / "yaml/schema_verification.yaml",
            'version: "1.0"',
            'version: !!str "1.0"',
        )
        self.replace(
            self.rules / "values/text.md", "status: normative", "status: proposed"
        )
        errors, _ = check(self.root)
        self.assertIn(
            "values/text.md: expected id, title, and normative status", errors
        )
        self.assertTrue(
            any(
                "schema_verification.yaml: invalid schema source:" in error
                and "explicit YAML tags are not allowed" in error
                for error in errors
            ),
            errors,
        )
        result = subprocess.run(
            [sys.executable, str(CHECKER), "--root", str(self.root)],
            capture_output=True,
            text=True,
            check=False,
        )
        self.assertEqual(result.returncode, 1)
        self.assertEqual(result.stderr, "")
        with self.assertRaisesRegex(
            ValueError, "schema_verification.yaml: invalid schema source"
        ):
            generated(self.root)

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
        self.assertNotIn("Historical citations", index)

    def test_schema_navigation_tracks_current_requirement_without_manual_map(self):
        path = self.rules / "values/numbers.md"
        path.write_text(path.read_text() + "\n**REQ-9999.** New current requirement.\n")
        self.replace(
            self.root / "yaml/schema_shared.yaml",
            "description: See REQ-1149.",
            "description: See REQ-9999.",
        )
        errors, _ = check(self.root)
        self.assertEqual(errors, [])
        fields = generated(self.root)[self.rules / "reference/schema-fields.md"]
        self.assertIn("[REQ-9999](../values/numbers.md#req-9999)", fields)

    def test_schema_navigation_to_an_undefined_requirement_fails(self):
        self.replace(
            self.root / "yaml/schema_shared.yaml",
            "description: See REQ-1149.",
            "description: See REQ-9999.",
        )
        errors, _ = check(self.root)
        self.assertIn(
            "schema description has no canonical owner: schema_shared.yaml:literal_value.description",
            errors,
        )
        with self.assertRaisesRegex(ValueError, "unknown requirement: REQ-9999"):
            generated(self.root)


if __name__ == "__main__":
    unittest.main()
