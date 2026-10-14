"""Diagnostic guard mutations preserve vocabulary and every source-literal ratchet."""

import importlib.util
import json
import subprocess
import tempfile
import unittest
from pathlib import Path, PureWindowsPath
from unittest.mock import patch

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
        self.core_seed = {}
        self.core_active = {}
        self.save_core_sites()
        self.dispositions = {
            "version": 1,
            "scope": "Static test dispositions, not execution proof.",
            "conditions": {},
        }
        self.save_dispositions()

    def save_dispositions(self):
        self.write(str(guard.DISPOSITIONS), json.dumps(self.dispositions))

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

    def save_core_sites(self):
        self.write(str(guard.CORE_SEED), json.dumps(self.core_seed))
        self.write(str(guard.CORE_ALLOWLIST), json.dumps(self.core_active))

    def allow_core(self, relative, source, count=1):
        identifier = relative + ":1"
        self.core_seed[identifier] = {
            "path": relative,
            "source": source.strip(),
            "occurrences": count,
        }
        self.core_active[identifier] = count
        self.save_core_sites()
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

    def test_baseline_uses_git_paths_and_utf8_under_windows_defaults(self):
        identifier = self.allow(
            "rust/crates/yamaa-adapters/src/errors.rs",
            'const REQUIREMENT: &str = "REQ-0001"; // \u00e9',
        )
        self.write(str(guard.SEED), json.dumps(self.seed, ensure_ascii=False))
        self.commit_baseline()
        with (
            patch.object(guard, "CHECKER", PureWindowsPath(guard.CHECKER.as_posix())),
            patch("subprocess._text_encoding", return_value="cp1252"),
        ):
            seed = PureWindowsPath(guard.SEED.as_posix())
            allowlist = PureWindowsPath(guard.ALLOWLIST.as_posix())
            self.assertEqual(
                guard.baseline_errors(self.root, "HEAD", seed, allowlist), []
            )
            self.active[identifier] = 2
            self.save_sites()
            errors = guard.baseline_errors(self.root, "HEAD", seed, allowlist)
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

    def register_non_core(
        self, condition, kind="historical_policy", phase="validation"
    ):
        path = self.root / "yaml/conditions.yaml"
        path.write_text(path.read_text() + f"  {condition}: {{phases: [{phase}]}}\n")
        self.write(
            "rules/history.md",
            f"**REQ-0002.** Historical policy names condition `{condition}`.\n",
        )
        self.write(
            "rust/tests/history_control.py", "def test_retained_history():\n    pass\n"
        )
        entry = {
            "kind": kind,
            "phases": [phase],
            "reason": "Approved historical behavior is preserved as history while retained semantics have their own current controls.",
            "owners": ["rules/history.md"],
            "controls": ["rust/tests/history_control.py"],
            "producer": None,
            "history": "Explicit synthetic historical fixture; this adds no claim about pre-removal reverse coverage.",
        }
        self.dispositions["conditions"][condition] = entry
        self.save_dispositions()
        return entry

    def test_rule_table_catches_original_missing_content_change_without_a_core_cause(
        self,
    ):
        self.write(
            "rules/resources.md",
            "**REQ-0788.** Held bytes cannot change.\n\n## Error conditions\n\n| Condition | Rejects |\n| --- | --- |\n| `resource_path_content_changed` | content changed after validation |\n",
        )
        self.assertEqual(
            self.errors(),
            [
                "rules/resources.md:7: rule names unregistered condition resource_path_content_changed"
            ],
        )
        path = self.root / "yaml/conditions.yaml"
        path.write_text(
            path.read_text() + "  resource_path_content_changed: {phases: [ingest]}\n"
        )
        self.assertEqual(
            self.errors(),
            [
                "vocabulary:resource_path_content_changed: registered condition has no core cause or explicit disposition"
            ],
        )

    def test_new_unexplained_dormant_name_is_not_a_passing_registration(self):
        path = self.root / "yaml/conditions.yaml"
        path.write_text(path.read_text() + "  newly_unused: {phases: [validation]}\n")
        self.assertEqual(
            self.errors(),
            [
                "vocabulary:newly_unused: registered condition has no core cause or explicit disposition"
            ],
        )

    def test_explicit_historical_condition_passes_without_inventing_core_emission(self):
        self.register_non_core("resource_path_outside_project")
        self.assertEqual(self.errors(), [])

    def test_disposition_cannot_hide_a_new_core_cause_or_removed_name(self):
        self.register_non_core("old_condition")
        path = self.root / "rust/crates/yamaa-core/src/diagnostic.rs"
        path.write_text(
            path.read_text().replace(
                "}\n",
                '    Old => ("validation", "old_condition", Some("REQ-0001")),\n}\n',
            )
        )
        self.assertEqual(
            self.errors(),
            [
                "vocabulary:old_condition: core-owned condition cannot retain a non-core disposition"
            ],
        )
        path = self.root / "yaml/conditions.yaml"
        path.write_text(
            path.read_text().replace("  old_condition: {phases: [validation]}\n", "")
        )
        self.assertTrue(
            any(
                "disposition names an unregistered condition" in error
                for error in self.errors()
            )
        )

    def test_disposition_requires_matching_phase_reason_and_available_owners_controls(
        self,
    ):
        entry = self.register_non_core("old_condition")
        for field, value, expected in [
            ("phases", ["output"], "phases differ"),
            ("phases", ["validation", "validation"], "phases differ"),
            ("reason", "", "missing reason"),
            ("history", "", "missing history"),
            ("owners", ["rules/missing.md"], "unavailable owners path"),
            ("controls", ["rust/tests/missing.py"], "unavailable controls path"),
            ("owners", ["../outside.md"], "unavailable owners path"),
            ("kind", [], "invalid disposition kind"),
        ]:
            previous = entry[field]
            entry[field] = value
            self.save_dispositions()
            self.assertTrue(
                any(expected in error for error in self.errors()),
                (field, self.errors()),
            )
            entry[field] = previous

    def test_duplicate_disposition_keys_and_missing_file_cannot_silently_pass(self):
        path = self.root / guard.DISPOSITIONS
        path.write_text(
            '{"version":1,"scope":"fixture","conditions":{},"conditions":{}}'
        )
        self.assertTrue(
            any("duplicate disposition key" in error for error in self.errors())
        )
        path.unlink()
        self.assertTrue(
            any(
                "unavailable vocabulary dispositions" in error
                for error in self.errors()
            )
        )

    def test_live_validator_requires_actual_refusal_in_its_named_producer(self):
        entry = self.register_non_core("unsupported_regex_engine", kind="validator")
        owner = "rust/tools/validator_fixture.py"
        self.write(
            owner,
            'def require_binding():\n    raise RuntimeError("unsupported_regex_engine")\n',
        )
        entry["owners"].append(owner)
        entry["producer"] = {"path": owner, "symbol": "require_binding"}
        self.save_dispositions()
        self.assertEqual(self.errors(), [])
        self.write(
            owner,
            'def require_binding():\n    raise RuntimeError("cannot validate: unsupported_regex_engine under R022")\n',
        )
        self.assertEqual(self.errors(), [])
        for source in [
            'def require_binding():\n    return "unsupported_regex_engine"\n',
            'CONDITION = "unsupported_regex_engine"\ndef require_binding():\n    raise RuntimeError("opaque")\n',
            'def require_binding():\n    def unused():\n        raise RuntimeError("unsupported_regex_engine")\n    return True\n',
            'def require_binding():\n    raise RuntimeError("unsupported_regex_engine_bogus")\n',
            'def require_binding():\n    raise RuntimeError("bogus_unsupported_regex_engine")\n',
            'def require_binding():\n    raise RuntimeError("unsupported_regex_engine2")\n',
            'def require_binding():\n    raise RuntimeError("éunsupported_regex_engine")\n',
        ]:
            self.write(owner, source)
            self.assertEqual(
                self.errors(),
                [
                    "vocabulary:unsupported_regex_engine: unavailable actual producing owner"
                ],
            )

    def test_conditional_capability_remains_normative_and_does_not_invent_a_producer(
        self,
    ):
        entry = self.register_non_core("unsupported_text_contract", kind="capability")
        self.assertEqual(self.errors(), [])
        entry["producer"] = {"path": "rules/history.md", "symbol": "fictional_runtime"}
        self.save_dispositions()
        self.assertTrue(
            any(
                "only validator dispositions may supply an explicit producer" in error
                for error in self.errors()
            )
        )
        entry["producer"] = None
        self.write(
            "rules/history.md", "**REQ-0002.** No named capability obligation.\n"
        )
        self.save_dispositions()
        self.assertEqual(
            self.errors(),
            [
                "vocabulary:unsupported_text_contract: capability disposition lacks its named normative owner"
            ],
        )

    def test_rule_refusals_keep_wrapped_lists_and_condition_column_identity(self):
        source = "A field `group_by` and operation `str_contains` are not diagnostic declarations.\n\nA failure must fail before evaluation with\n`unsupported_text_contract`, `invalid_value`, or\n`missing_binding`, reporting the position.\n\n| Field | Condition | Phase |\n| --- | --- | --- |\n| `input_path` | `resource_path_content_changed` | ingest |\n"
        self.assertEqual(
            guard.rule_condition_references(source),
            [
                (4, "invalid_value"),
                (4, "unsupported_text_contract"),
                (5, "missing_binding"),
                (9, "resource_path_content_changed"),
            ],
        )

    def test_rule_parser_does_not_turn_examples_comments_fields_or_phases_into_conditions(
        self,
    ):
        source = "<!-- fail with `comment_example`. -->\n\n```yaml\nfail with `fenced_example`\n```\n\n### Error conditions\n`group_by`, `dataset_json`, `validation` and `row_count` are fields/phases.\n\n| Field | Type |\n| --- | --- |\n| `input_path` | `project_path` |\n\nThe condition named `actual_condition` is a declared diagnostic.\n"
        self.assertEqual(
            guard.rule_condition_references(source), [(14, "actual_condition")]
        )

    def test_input_state_table_and_field_compatibility_prose_are_not_diagnostic_declarations(
        self,
    ):
        source = "| Condition | Result |\n| --- | --- |\n| No records after `filter` | missing |\n\nStatic checking rejects a lock format incompatible with `language`.\n"
        self.assertEqual(guard.rule_condition_references(source), [])

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
        self.assertEqual(len(self.errors()), 2)
        self.assertTrue(
            any("undefined requirement REQ-999999" in e for e in self.errors())
        )
        self.assertTrue(any("outside canonical registry" in e for e in self.errors()))

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
            [
                "core diagnostic registry:Invalid: unregistered condition invalid_value",
                "vocabulary:other_value: registered condition has no core cause or explicit disposition",
            ],
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

    def test_new_core_and_engine_sites_are_rejected_in_every_literal_form(self):
        sources = [
            'const CONDITION: &str = "invalid_value";',
            'const REQUIREMENT: &str = "REQ-0001";',
            r'const ERROR: &str = "{\"condition\":\"invalid_value\",\"requirement\":\"REQ-0001\"}";',
            r'const ERROR: &str = r#"{"condition":"invalid_value","requirement":"REQ-0001"}"#;',
            'const ERROR: &str = r#"{\n"condition":"invalid_value",\n"requirement":"REQ-0001"\n}"#;',
        ]
        for crate in sorted(guard.CORE_CRATES):
            for source in sources:
                with self.subTest(crate=crate, source=source):
                    path = self.write(
                        f"rust/crates/{crate}/src/errors.rs", source + "\n"
                    )
                    errors = self.errors()
                    self.assertEqual(len(errors), 1)
                    self.assertIn(
                        "new diagnostic literal site outside canonical registry",
                        errors[0],
                    )
                    path.unlink()

    def test_core_exact_sites_cannot_move_duplicate_or_change(self):
        relative = "rust/crates/yamaa-engine/src/errors.rs"
        source = 'const REQUIREMENT: &str = "REQ-0001";'
        self.allow_core(relative, source)
        path = self.write(relative, "// unrelated insertion\n    " + source + "\n")
        self.assertEqual(self.errors(), [])
        path.write_text(source + "\n" + source + "\n")
        self.assertEqual(len(self.errors()), 1)
        path.write_text(source.replace("REQUIREMENT", "OTHER") + "\n")
        self.assertEqual(len(self.errors()), 2)
        path.unlink()
        self.write("rust/crates/yamaa-core/src/errors.rs", source + "\n")
        self.assertEqual(len(self.errors()), 2)

    def test_core_retirement_requires_active_shrink_and_cannot_reactivate(self):
        relative = "rust/crates/yamaa-core/src/errors.rs"
        source = 'const CONDITION: &str = "invalid_value";'
        identifier = self.allow_core(relative, source, count=2)
        self.write(relative, source + "\n")
        self.assertTrue(any("stale literal site" in e for e in self.errors()))
        self.core_active[identifier] = 1
        self.save_core_sites()
        self.commit_baseline()
        self.assertEqual(guard.violations(self.root, "HEAD"), [])
        self.core_active[identifier] = 2
        self.save_core_sites()
        self.write(relative, source + "\n" + source + "\n")
        self.assertTrue(
            any(
                "literal allowlist grew from HEAD" in e
                for e in guard.violations(self.root, "HEAD")
            )
        )
        self.core_active.clear()
        self.save_core_sites()
        self.write(relative, "// moved to canonical registry\n")
        self.assertEqual(guard.violations(self.root, "HEAD"), [])

    def test_core_seed_is_frozen_and_namespace_paths_cannot_cross(self):
        self.commit_baseline()
        (self.root / guard.CORE_SEED).write_text("{\n}\n")
        self.assertEqual(guard.violations(self.root, "HEAD"), [])
        relative = "rust/crates/yamaa-core/src/errors.rs"
        self.allow_core(relative, '"REQ-0001"')
        self.assertTrue(
            any(
                str(guard.CORE_SEED) + ": frozen literal seed changed" in e
                for e in guard.violations(self.root, "HEAD")
            )
        )
        self.allow("rust/crates/yamaa-core/src/host_seed.rs", '"REQ-0001"')
        self.allow_core("rust/crates/yamaa-adapters/src/core_seed.rs", '"REQ-0001"')
        self.assertEqual(
            sum("invalid frozen literal site" in e for e in self.errors()), 2
        )

    def test_core_initial_introduction_is_separate_from_existing_host_guard(self):
        (self.root / guard.CORE_SEED).unlink()
        (self.root / guard.CORE_ALLOWLIST).unlink()
        self.write(
            str(guard.CHECKER),
            f"# Requires {guard.SEED.name} and {guard.ALLOWLIST.name}\n",
        )
        self.commit_baseline()
        self.save_core_sites()
        self.assertEqual(guard.violations(self.root, "HEAD"), [])

    def test_core_existing_guard_and_partial_seed_cannot_bootstrap(self):
        for partial in (False, True):
            with self.subTest(partial=partial):
                (self.root / guard.CORE_ALLOWLIST).unlink()
                if not partial:
                    (self.root / guard.CORE_SEED).unlink()
                self.write(
                    str(guard.CHECKER),
                    f"# Requires {guard.CORE_SEED.name} and {guard.CORE_ALLOWLIST.name}\n",
                )
                self.commit_baseline()
                self.save_core_sites()
                errors = guard.violations(self.root, "HEAD")
                expected = (
                    "incomplete seed and allowlist"
                    if partial
                    else "existing guard is missing"
                )
                self.assertTrue(any(expected in e for e in errors), errors)

    def test_core_comments_prose_and_generated_filename_are_not_registry_exemptions(
        self,
    ):
        relative = "rust/crates/yamaa-core/src/prose.rs"
        self.write(
            relative,
            '// "REQ-999999"\n/* "invalid_value" */\nconst PROSE: &str = "about \\"invalid_value\\"";\n',
        )
        self.assertEqual(self.errors(), [])
        self.write(
            "rust/crates/yamaa-engine/src/diagnostic.rs",
            'const REQ: &str = "REQ-0001";\n',
        )
        self.write(
            "rust/crates/yamaa-core/src/shipped_schema_data.rs",
            'const CONDITION: &str = "invalid_value";\n',
        )
        self.assertEqual(len(self.errors()), 2)
        self.assertTrue(all("outside canonical registry" in e for e in self.errors()))

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
