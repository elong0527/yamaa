"""Diagnostic guard mutations preserve vocabulary and the host-literal ratchet."""

import importlib.util
import json
import subprocess
import tempfile
import unittest
from pathlib import Path

spec = importlib.util.spec_from_file_location(
    "check_diagnostics", Path(__file__).parents[1] / "tools/check_diagnostics.py"
)
guard = importlib.util.module_from_spec(spec)
spec.loader.exec_module(guard)


class DiagnosticGuardTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)
        self.write("rules/values.md", "**REQ-0001.** A normative requirement.\n")
        self.write(
            str(guard.NULL_POLICY),
            "Explicit implementation exception.\n"
            + guard.NULL_START
            + "\n- `engine_rejected` (`engine`)\n"
            + guard.NULL_END
            + "\n",
        )
        self.write(
            "yaml/conditions.yaml",
            'version: "1.0"\nconditions:\n  invalid_value: {phases: [validation]}\n'
            "  engine_rejected: {phases: [engine]}\n",
        )
        self.write(
            "rust/crates/yamaa-core/src/diagnostic.rs",
            'conditions! {\n    Invalid => ("validation", "invalid_value", Some("REQ-0001")),\n'
            '    Rejected => ("engine", "engine_rejected", None),\n}\n',
        )
        self.seed = {}
        self.active = {}
        self.save_sites()

    def write(self, relative, text):
        path = self.root / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text, encoding="utf-8")
        return path

    def save_sites(self):
        self.write(str(guard.SEED), json.dumps(self.seed))
        self.write(str(guard.ALLOWLIST), json.dumps(self.active))

    def allow(self, relative, source, count=1):
        identifier = relative + ":1"
        self.seed[identifier] = {
            "path": relative,
            "source": source.strip(),
            "occurrences": count,
        }
        self.active[identifier] = count
        self.save_sites()
        return identifier

    def errors(self):
        return guard.violations(self.root)

    def commit_baseline(self):
        for arguments in (
            ["init", "--quiet"],
            ["add", "."],
            [
                "-c",
                "user.email=diagnostic-test@example.invalid",
                "-c",
                "user.name=Diagnostic test",
                "commit",
                "--quiet",
                "-m",
                "baseline",
            ],
        ):
            subprocess.run(
                ["git", *arguments], cwd=self.root, check=True, capture_output=True
            )

    def test_baseline_rejects_reactivation_after_retirement(self):
        relative = "rust/crates/yamaa-adapters/src/errors.rs"
        source = 'const REQUIREMENT: &str = "REQ-0001";'
        identifier = self.allow(relative, source, count=2)
        self.active[identifier] = 1
        self.save_sites()
        self.write(relative, source + "\n")
        self.commit_baseline()
        self.assertEqual(guard.violations(self.root, "HEAD"), [])
        self.active[identifier] = 2
        self.save_sites()
        self.write(relative, source + "\n" + source + "\n")
        errors = guard.violations(self.root, "HEAD")
        self.assertEqual(len(errors), 1)
        self.assertIn("literal allowlist grew from HEAD", errors[0])

    def test_baseline_seed_comparison_ignores_json_layout_but_rejects_edits(self):
        self.commit_baseline()
        (self.root / guard.SEED).write_text("{\n}\n")
        self.assertEqual(guard.violations(self.root, "HEAD"), [])
        self.allow("rust/crates/yamaa-adapters/src/other.rs", '"REQ-0001"')
        self.assertTrue(
            any(
                "frozen literal seed changed" in e
                for e in guard.violations(self.root, "HEAD")
            )
        )

    def test_first_introduction_is_explicit_and_missing_baseline_fails(self):
        (self.root / guard.SEED).unlink()
        (self.root / guard.ALLOWLIST).unlink()
        self.write(str(guard.CHECKER), "# Original requirement-only checker\n")
        self.commit_baseline()
        self.save_sites()
        self.assertEqual(guard.violations(self.root, "HEAD"), [])
        self.assertTrue(
            any(
                "unavailable revision" in e
                for e in guard.violations(self.root, "missing-ref")
            )
        )

    def test_existing_baseline_guard_cannot_lose_its_files(self):
        (self.root / guard.SEED).unlink()
        (self.root / guard.ALLOWLIST).unlink()
        self.write(
            str(guard.CHECKER),
            f"# Requires {guard.SEED.name} and {guard.ALLOWLIST.name}\n",
        )
        self.commit_baseline()
        self.save_sites()
        self.assertTrue(
            any(
                "existing guard is missing" in e
                for e in guard.violations(self.root, "HEAD")
            )
        )

    def test_incomplete_baseline_cannot_be_bootstrapped(self):
        (self.root / guard.ALLOWLIST).unlink()
        self.commit_baseline()
        self.save_sites()
        self.assertTrue(
            any(
                "incomplete seed and allowlist" in e
                for e in guard.violations(self.root, "HEAD")
            )
        )

    def test_registered_core_mapping_passes(self):
        self.assertEqual(self.errors(), [])

    def test_new_null_language_cause_is_rejected(self):
        path = self.root / "rust/crates/yamaa-core/src/diagnostic.rs"
        path.write_text(path.read_text().replace('Some("REQ-0001")', "None"))
        self.assertTrue(
            any(
                "null requirement is not normatively permitted" in e
                for e in self.errors()
            )
        )

    def test_implementation_condition_must_retain_null(self):
        path = self.root / "rust/crates/yamaa-core/src/diagnostic.rs"
        path.write_text(path.read_text().replace("None", 'Some("REQ-0001")'))
        self.assertTrue(
            any("must retain a null requirement" in e for e in self.errors())
        )

    def test_null_policy_is_explicit_registered_and_reached(self):
        path = self.root / guard.NULL_POLICY
        original = path.read_text()
        for body, expected in [
            ("", "expected one normative null policy block"),
            (original + original, "expected one normative null policy block"),
            (guard.NULL_END + guard.NULL_START, "reversed normative null policy block"),
            (
                original.replace("- `engine_rejected` (`engine`)", ""),
                "no normative null conditions",
            ),
            (
                original.replace("- `engine_rejected` (`engine`)", "- engine_rejected"),
                "invalid normative null entry",
            ),
            (
                original.replace("(`engine`)", "(`validation`)"),
                "unregistered normative null entry",
            ),
            (
                original.replace("engine_rejected", "unknown_condition"),
                "unregistered normative null entry",
            ),
            (
                original.replace(
                    guard.NULL_END, "- `engine_rejected` (`engine`)\n" + guard.NULL_END
                ),
                "duplicate normative null entry",
            ),
            (
                original.replace(
                    guard.NULL_END,
                    "- `invalid_value` (`validation`)\n" + guard.NULL_END,
                ),
                "unused normative null condition",
            ),
        ]:
            with self.subTest(expected=expected):
                path.write_text(body)
                self.assertTrue(
                    any(expected in e for e in self.errors()), self.errors()
                )
        path.unlink()
        self.assertTrue(
            any("unavailable normative null policy" in e for e in self.errors())
        )

    def test_unknown_requirement_is_rejected_in_every_crate(self):
        for crate in ("yamaa-core", "yamaa-engine", *sorted(guard.HOST_CRATES)):
            with self.subTest(crate=crate):
                path = self.write(
                    f"rust/crates/{crate}/src/unknown.rs",
                    'const REQUIREMENT: &str = "REQ-999999";\n',
                )
                self.assertTrue(
                    any("undefined requirement REQ-999999" in e for e in self.errors())
                )
                path.unlink()

    def test_new_registered_condition_and_known_requirement_fail_in_each_host(self):
        for crate in sorted(guard.HOST_CRATES):
            with self.subTest(crate=crate):
                self.write(
                    f"rust/crates/{crate}/src/nested/errors.rs",
                    'const MAPPING: (&str, &str) = ("invalid_value", "REQ-0001");\n',
                )
        errors = self.errors()
        self.assertEqual(len(errors), 3)
        self.assertTrue(
            all("new diagnostic literal site outside core" in e for e in errors)
        )

    def test_escaped_and_raw_json_diagnostics_cannot_create_new_host_sites(self):
        sources = [
            r'const ERROR: &str = "{\"condition\":\"invalid_value\",\"requirement\":\"REQ-0001\"}";',
            r'const ERROR: &str = "{\"payload\":\"{\\\"condition\\\":\\\"invalid_value\\\",\\\"requirement\\\":\\\"REQ-0001\\\"}\"}";',
            r'const ERROR: &str = r###"{"condition":"invalid_value","requirement":"REQ-0001"}"###;',
            r'const ERROR: &str = "{\"condition\":\"invalid\\u005Fvalue\",\"requirement\":\"REQ-0001\"}";',
            r'const ERROR: &str = r#"{"condition":"invalid\u005Fvalue","requirement":"REQ-0001"}"#;',
            r'let error = format!("{{\"condition\":\"invalid_value\",\"requirement\":\"REQ-0001\",\"path\":\"{path}\"}}");',
            r'const CONDITION: &str = "invalid\u{005F}value";',
            r'const REQUIREMENT: &str = "REQ-\x30\x30\x30\x31";',
        ]
        for crate in sorted(guard.HOST_CRATES):
            for source in sources:
                with self.subTest(crate=crate, source=source):
                    path = self.write(
                        f"rust/crates/{crate}/src/errors.rs", source + "\n"
                    )
                    errors = self.errors()
                    self.assertEqual(len(errors), 1)
                    self.assertIn("new diagnostic literal site outside core", errors[0])
                    path.unlink()

    def test_seeded_escaped_json_is_counted_at_its_actual_source_line(self):
        relative = "rust/crates/yamaa-adapters/src/errors.rs"
        source = r'const ERROR: &str = "{\"condition\":\"invalid_value\",\"requirement\":\"REQ-0001\"}";'
        self.allow(relative, source)
        self.write(relative, "// unrelated insertion\n" + source + "\n")
        self.assertEqual(self.errors(), [])
        self.write(relative, source + "\n" + source + "\n")
        self.assertEqual(len(self.errors()), 1)
        self.assertIn("errors.rs:2: new diagnostic", self.errors()[0])

    def test_escaped_json_unknown_requirement_is_still_undefined(self):
        self.write(
            "rust/crates/yamaa-core/src/json_error.rs",
            r'const ERROR: &str = "{\"condition\":\"invalid_value\",\"requirement\":\"REQ-999999\"}";'
            + "\n",
        )
        self.assertEqual(len(self.errors()), 1)
        self.assertIn("undefined requirement REQ-999999", self.errors()[0])

    def test_comments_quoted_prose_and_character_literals_are_irrelevant(self):
        self.write(
            "rust/crates/yamaa-adapters/src/prose.rs",
            r"""// "REQ-999999" and "invalid_value"
/* outer "REQ-999999" /* nested "invalid_value" */ */
const MESSAGE: &str = "a message containing \"invalid_value\" and \"REQ-999999\"";
let quote = '"';
let escaped_quote = '\"';
let text: &'static str = "ordinary text \u00e9\U0001f642";
""",
        )
        self.assertEqual(self.errors(), [])

    def test_multiline_raw_json_diagnostic_is_a_new_site(self):
        self.write(
            "rust/crates/yamaa-adapters/src/errors.rs",
            'const ERROR: &str = r#"{\n"condition":"invalid_value",\n"requirement":"REQ-0001"\n}"#;\n',
        )
        self.assertEqual(len(self.errors()), 1)
        self.assertIn("errors.rs:1: new diagnostic", self.errors()[0])

    def test_exact_seeded_site_survives_unrelated_line_insertions(self):
        relative = "rust/crates/yamaa-adapters/src/errors.rs"
        source = 'const REQUIREMENT: &str = "REQ-0001";'
        self.allow(relative, source)
        self.write(relative, "// unrelated insertion\n    " + source + "\n")
        self.assertEqual(self.errors(), [])

    def test_copying_or_moving_a_seeded_site_is_not_allowed(self):
        relative = "rust/crates/yamaa-adapters/src/errors.rs"
        source = 'const REQUIREMENT: &str = "REQ-0001";'
        self.allow(relative, source)
        path = self.write(relative, source + "\n")
        self.write("rust/crates/yamaa-adapters/src/other.rs", source + "\n")
        self.assertEqual(len(self.errors()), 1)
        path.unlink()
        self.assertTrue(any("stale literal site" in e for e in self.errors()))
        self.assertTrue(any("other.rs:1: new diagnostic" in e for e in self.errors()))

    def test_duplicate_seeded_line_and_changed_line_fail(self):
        relative = "rust/crates/yamaa-adapters/src/errors.rs"
        source = 'const REQUIREMENT: &str = "REQ-0001";'
        self.allow(relative, source)
        self.write(relative, source + "\n" + source + "\n")
        self.assertEqual(len(self.errors()), 1)
        self.write(relative, source.replace("REQUIREMENT", "OTHER") + "\n")
        self.assertEqual(len(self.errors()), 2)

    def test_removal_must_shrink_active_allowlist(self):
        relative = "rust/crates/yamaa-adapters/src/errors.rs"
        source = 'const REQUIREMENT: &str = "REQ-0001";'
        self.allow(relative, source, count=2)
        self.write(relative, source + "\n")
        self.assertTrue(any("stale literal site" in e for e in self.errors()))
        identifier = next(iter(self.active))
        self.active[identifier] = 1
        self.save_sites()
        self.assertEqual(self.errors(), [])
        self.write(relative, "// diagnostic ownership moved to core\n")
        self.active.clear()
        self.save_sites()
        self.assertEqual(self.errors(), [])

    def test_allowlist_cannot_add_sites_or_grow_counts(self):
        relative = "rust/crates/yamaa-adapters/src/errors.rs"
        source = 'const REQUIREMENT: &str = "REQ-0001";'
        identifier = self.allow(relative, source)
        self.write(relative, source + "\n")
        for addition in ({identifier: 2}, {"new.rs:1": 1}, {identifier: "1"}):
            with self.subTest(addition=addition):
                self.active = addition
                self.save_sites()
                self.assertTrue(
                    any("literal allowlist may only shrink" in e for e in self.errors())
                )

    def test_generated_schema_is_exempt_from_ownership_but_not_normative_ids(self):
        path = self.write(
            "rust/crates/yamaa-adapters/src/shipped_schema_data.rs",
            'const DATA: &str = "invalid_value";\nconst REQ: &str = "REQ-0001";\n',
        )
        self.assertEqual(self.errors(), [])
        path.write_text('const REQ: &str = "REQ-999999";\n')
        self.assertEqual(len(self.errors()), 1)
        self.assertIn("undefined requirement REQ-999999", self.errors()[0])

    def test_missing_condition_and_wrong_phase_fail(self):
        path = self.root / "yaml/conditions.yaml"
        path.write_text(
            "conditions:\n  other_value: {phases: [validation]}\n"
            "  engine_rejected: {phases: [engine]}\n"
        )
        self.assertEqual(
            self.errors(),
            ["core diagnostic registry:Invalid: unregistered condition invalid_value"],
        )
        path.write_text(
            "conditions:\n  invalid_value: {phases: [derivation]}\n"
            "  engine_rejected: {phases: [engine]}\n"
        )
        self.assertEqual(
            self.errors(),
            [
                "core diagnostic registry:Invalid: unregistered phase validation for invalid_value"
            ],
        )

    def test_unrecognized_vocabulary_layout_and_duplicate_names_fail_closed(self):
        self.write(
            "yaml/conditions.yaml",
            "conditions:\n  invalid_value: {phases: [validation]}\n  new_value:\n    phases: [validation]\n",
        )
        self.assertTrue(any("unrecognized registration" in e for e in self.errors()))
        self.write(
            "yaml/conditions.yaml",
            "conditions:\n  invalid_value: {phases: [validation]}\n  invalid_value: {phases: [validation]}\n",
        )
        self.assertTrue(any("duplicate registration" in e for e in self.errors()))

    def test_unparseable_or_missing_registry_and_vocabulary_fail_closed(self):
        path = self.root / "rust/crates/yamaa-core/src/diagnostic.rs"
        path.write_text("conditions! {\n    malformed => (),\n}\n")
        self.assertTrue(any("unrecognized cause" in e for e in self.errors()))
        path.unlink()
        self.write("rust/crates/yamaa-engine/src/marker.rs", "// engine source\n")
        self.assertTrue(any("unavailable core registry" in e for e in self.errors()))
        (self.root / "yaml/conditions.yaml").unlink()
        self.assertTrue(any("unavailable vocabulary" in e for e in self.errors()))

    def test_missing_or_invalid_allowlist_does_not_pass(self):
        path = self.root / guard.ALLOWLIST
        path.unlink()
        self.assertTrue(
            any("invalid diagnostic literal sites" in e for e in self.errors())
        )
        path.write_text("[]")
        self.assertTrue(
            any("invalid diagnostic literal sites" in e for e in self.errors())
        )

    def test_retired_citation_is_not_a_definition(self):
        self.write(
            "rules/values.md",
            "**REQ-0001.** Required.\nRetired REQ-0002 is historical.\n",
        )
        self.write(
            "rust/crates/yamaa-core/src/retired.rs", 'const REQ: &str = "REQ-0002";\n'
        )
        self.assertTrue(
            any("undefined requirement REQ-0002" in e for e in self.errors())
        )

    def test_missing_rules_and_sources_do_not_pass(self):
        (self.root / "rules/values.md").unlink()
        self.assertEqual(self.errors(), ["no normative requirement definitions found"])
        self.write("rules/values.md", "**REQ-0001.** Required.\n")
        (self.root / "rust/crates/yamaa-core/src/diagnostic.rs").unlink()
        self.assertEqual(self.errors(), ["no Rust crate sources found"])


if __name__ == "__main__":
    unittest.main()
