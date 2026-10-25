"""Diagnostic guard mutations preserve vocabulary and registry-owned literals."""

import re
import tempfile
import unittest
from pathlib import Path

from tools.paths import ROOT
from tools.tests.support import write
from tools.validation import check_diagnostics as guard


class DiagnosticGuardTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)
        # The registry cites this card.
        self.write(
            "rules/values.md",
            '# Values\n\n<a id="value-1"></a>\n\n### VALUE-1. Values are valid\n\n'
            "A value is valid.\n\nFails: `invalid_value`.\n",
        )
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
            "rust/crates/yamaa-core/src/diagnostic/mod.rs",
            'conditions! {\n    Invalid => ("validation", "invalid_value", Some("VALUE-1")),\n'
            '    Rejected => ("engine", "engine_rejected", None),\n}\n',
        )

    def write(self, relative, text):
        return write(self.root, relative, text)

    def errors(self):
        return guard.violations(self.root)

    def assertReported(self, fragment):
        """Some violation names `fragment`; a failure shows every violation."""
        self.assertIn(fragment, "\n".join(self.errors()))

    def test_registered_core_mapping_passes(self):
        self.assertEqual(self.errors(), [])

    def test_rule_table_catches_original_missing_content_change_without_a_core_cause(
        self,
    ):
        self.write(
            "rules/resources.md",
            "Held bytes cannot change.\n\n## Error conditions\n\n| Condition | Rejects |\n| --- | --- |\n| `resource_path_content_changed` | content changed after validation |\n",
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
        # Registration alone now suffices: a condition with no core cause is
        # derived from the registry difference, with no disposition file.
        self.assertEqual(self.errors(), [])

    def test_registered_non_core_condition_needs_no_disposition_file(self):
        path = self.root / "yaml/conditions.yaml"
        path.write_text(path.read_text() + "  newly_unused: {phases: [validation]}\n")
        self.assertEqual(self.errors(), [])

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
        path = self.root / "rust/crates/yamaa-core/src/diagnostic/mod.rs"
        path.write_text(path.read_text().replace('Some("VALUE-1")', "None"))
        self.assertReported("null rule is not normatively permitted")

    def test_implementation_condition_must_retain_null(self):
        path = self.root / "rust/crates/yamaa-core/src/diagnostic/mod.rs"
        path.write_text(path.read_text().replace("None", 'Some("VALUE-1")'))
        self.assertReported("must retain a null rule")

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
                self.assertReported(expected)
        path.unlink()
        self.assertReported("unavailable normative null policy")

    def test_unknown_requirement_is_rejected_in_every_crate(self):
        for crate in ("yamaa-core", *sorted(guard.HOST_CRATES)):
            with self.subTest(crate=crate):
                path = self.write(
                    f"rust/crates/{crate}/src/unknown.rs",
                    'const REQUIREMENT: &str = "VALUE-999";\n',
                )
                self.assertReported("undefined rule VALUE-999")
                path.unlink()

    def test_new_registered_condition_and_known_requirement_fail_in_each_host(self):
        for crate in sorted(guard.HOST_CRATES):
            with self.subTest(crate=crate):
                self.write(
                    f"rust/crates/{crate}/src/nested/errors.rs",
                    'const MAPPING: (&str, &str) = ("invalid_value", "VALUE-1");\n',
                )
        errors = self.errors()
        self.assertEqual(len(errors), 3)
        self.assertTrue(
            all("new diagnostic literal site outside core" in e for e in errors)
        )

    def test_a_lint_finding_spelled_as_a_condition_stays_in_the_lint_module(self):
        # The lint's closed list names a finding that shares a registered
        # condition's spelling, as `redundant_long_form` does.
        self.write(
            str(guard.LINT_SOURCE),
            'pub const FINDINGS: [&str; 2] = [\n    "field_order",\n    "invalid_value",\n];\n',
        )
        self.write(
            "rust/crates/yamaa-adapters/src/lint/checks.rs",
            'const FINDING: &str = "invalid_value";\n',
        )
        self.assertEqual(self.errors(), [])
        # Outside the lint module the spelling is a diagnostic literal, and a
        # condition the list does not name stays refused within it.
        self.write(
            "rust/crates/yamaa-adapters/src/report.rs",
            'const CONDITION: &str = "invalid_value";\n',
        )
        self.write(
            "rust/crates/yamaa-adapters/src/lint/fix.rs",
            'const CONDITION: &str = "engine_rejected";\n',
        )
        self.assertEqual(
            self.errors(),
            [
                "rust/crates/yamaa-adapters/src/lint/fix.rs:1: new diagnostic literal "
                "site outside core: engine_rejected",
                "rust/crates/yamaa-adapters/src/report.rs:1: new diagnostic literal "
                "site outside core: invalid_value",
            ],
        )

    def test_escaped_and_raw_json_diagnostics_cannot_create_new_host_sites(self):
        sources = [
            r'const ERROR: &str = "{\"condition\":\"invalid_value\",\"rule\":\"VALUE-1\"}";',
            r'const ERROR: &str = "{\"payload\":\"{\\\"condition\\\":\\\"invalid_value\\\",\\\"rule\\\":\\\"VALUE-1\\\"}\"}";',
            r'const ERROR: &str = r###"{"condition":"invalid_value","rule":"VALUE-1"}"###;',
            r'const ERROR: &str = "{\"condition\":\"invalid\\u005Fvalue\",\"rule\":\"VALUE-1\"}";',
            r'const ERROR: &str = r#"{"condition":"invalid\u005Fvalue","rule":"VALUE-1"}"#;',
            r'let error = format!("{{\"condition\":\"invalid_value\",\"rule\":\"VALUE-1\",\"path\":\"{path}\"}}");',
            r'const CONDITION: &str = "invalid\u{005F}value";',
            r'const REQUIREMENT: &str = "VALUE-\x31";',
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

    def test_escaped_json_unknown_requirement_is_still_undefined(self):
        self.write(
            "rust/crates/yamaa-core/src/json_error.rs",
            r'const ERROR: &str = "{\"condition\":\"invalid_value\",\"rule\":\"VALUE-999\"}";'
            + "\n",
        )
        self.assertEqual(len(self.errors()), 2)
        self.assertReported("undefined rule VALUE-999")
        self.assertReported("outside canonical registry")

    def test_comments_quoted_prose_and_character_literals_are_irrelevant(self):
        self.write(
            "rust/crates/yamaa-adapters/src/prose.rs",
            r"""// "VALUE-999" and "invalid_value"
/* outer "VALUE-999" /* nested "invalid_value" */ */
const MESSAGE: &str = "a message containing \"invalid_value\" and \"VALUE-999\"";
let quote = '"';
let escaped_quote = '\"';
let text: &'static str = "ordinary text \u00e9\U0001f642";
""",
        )
        self.assertEqual(self.errors(), [])

    def test_multiline_raw_json_diagnostic_is_a_new_site(self):
        self.write(
            "rust/crates/yamaa-adapters/src/errors.rs",
            'const ERROR: &str = r#"{\n"condition":"invalid_value",\n"rule":"VALUE-1"\n}"#;\n',
        )
        self.assertEqual(len(self.errors()), 1)
        self.assertIn("errors.rs:1: new diagnostic", self.errors()[0])

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

    def test_block_registrations_pass_and_malformed_or_repeated_names_fail_closed(
        self,
    ):
        # The guard reads the registry as YAML, so a block registration with
        # its message is the same registration as the flow form.
        self.write(
            "yaml/conditions.yaml",
            'conditions:\n  invalid_value:\n    phases: [validation]\n    message: "a value is invalid"\n'
            "  engine_rejected: {phases: [engine]}\n",
        )
        self.assertEqual(self.errors(), [])
        self.write(
            "yaml/conditions.yaml",
            'conditions:\n  invalid_value: {phases: [validation]}\n  new_value:\n    message: "no phases"\n',
        )
        self.assertReported("unrecognized registration: new_value")
        self.write(
            "yaml/conditions.yaml",
            "conditions:\n  invalid_value: {phases: [validation]}\n  invalid_value: {phases: [validation]}\n",
        )
        self.assertReported("duplicate key")
        self.write("yaml/conditions.yaml", "conditions: []\n")
        self.assertReported("no condition registrations")

    def test_generated_message_table_is_exempt_from_ownership_but_not_normative_ids(
        self,
    ):
        path = self.write(
            str(guard.MESSAGE_TABLE),
            'const TEMPLATES: &[(&str, &str)] = &[("invalid_value", "a value is invalid")];\n',
        )
        self.assertEqual(self.errors(), [])
        path.write_text('const REQ: &str = "VALUE-999";\n')
        self.assertEqual(
            self.errors(),
            [f"{guard.MESSAGE_TABLE.as_posix()}:1: undefined rule VALUE-999"],
        )
        path.unlink()
        self.write(
            "rust/crates/yamaa-core/src/diagnostic/other.rs",
            'const NAME: &str = "invalid_value";\n',
        )
        self.assertReported("new diagnostic literal site")

    def test_unparseable_or_missing_registry_and_vocabulary_fail_closed(self):
        path = self.root / "rust/crates/yamaa-core/src/diagnostic/mod.rs"
        path.write_text("conditions! {\n    malformed => (),\n}\n")
        self.assertReported("unrecognized cause")
        path.unlink()
        self.write("rust/crates/yamaa-core/src/marker.rs", "// engine source\n")
        self.assertReported("unavailable core registry")
        (self.root / "yaml/conditions.yaml").unlink()
        self.assertReported("unavailable vocabulary")

    def test_new_core_and_engine_sites_are_rejected_in_every_literal_form(self):
        sources = [
            'const CONDITION: &str = "invalid_value";',
            'const REQUIREMENT: &str = "VALUE-1";',
            r'const ERROR: &str = "{\"condition\":\"invalid_value\",\"rule\":\"VALUE-1\"}";',
            r'const ERROR: &str = r#"{"condition":"invalid_value","rule":"VALUE-1"}"#;',
            'const ERROR: &str = r#"{\n"condition":"invalid_value",\n"rule":"VALUE-1"\n}"#;',
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

    def test_core_comments_prose_and_generated_filename_are_not_registry_exemptions(
        self,
    ):
        relative = "rust/crates/yamaa-core/src/prose.rs"
        self.write(
            relative,
            '// "VALUE-999"\n/* "invalid_value" */\nconst PROSE: &str = "about \\"invalid_value\\"";\n',
        )
        self.assertEqual(self.errors(), [])
        self.write(
            "rust/crates/yamaa-core/src/diagnostic.rs",
            'const REQ: &str = "VALUE-1";\n',
        )
        self.write(
            "rust/crates/yamaa-core/src/shipped_schema_data.rs",
            'const CONDITION: &str = "invalid_value";\n',
        )
        self.assertEqual(len(self.errors()), 2)
        self.assertTrue(all("outside canonical registry" in e for e in self.errors()))

    def test_an_issue_row_built_with_an_unregistered_condition_is_refused(self):
        # The facade's cancellation row before #3144: a condition and a phase
        # that no registry names, which a scan for registered names misses.
        self.write(
            "rust/crates/yamaa/src/internal.rs",
            "pub(crate) fn cancelled_issue() -> yamaa_adapters::issue_rows::Issue {\n"
            "    yamaa_adapters::issue_rows::Issue {\n"
            '        phase: "check".into(),\n'
            '        condition: "cancelled".into(),\n'
            "        rule: None,\n"
            "        spec_paths: Vec::new(),\n"
            "        context: yamaa_adapters::json!({}),\n"
            "        source_span: None,\n"
            "        operand_route: None,\n"
            "    }\n"
            "}\n",
        )
        self.assertEqual(
            self.errors(),
            [
                "rust/crates/yamaa/src/internal.rs:4: unregistered condition literal cancelled"
            ],
        )

    def test_every_condition_literal_form_must_be_registered_in_every_crate(self):
        # Each source with the line of its condition literal.
        sources = [
            (
                'let issue = Issue { phase: "engine".into(), condition: "cancelled".into() };',
                1,
            ),
            (
                'let issue = Issue {\n    condition:\n        String::from("cancelled"),\n};',
                3,
            ),
            ('let value = json!({"phase": "engine", "condition": "cancelled"});', 1),
            ('let condition = "cancelled";', 1),
            ('const CONDITION: &str = "cancelled";', 1),
            ('row.condition = "cancelled".to_owned();', 1),
            (r'const ROW: &str = r#"{"phase":"engine","condition":"cancelled"}"#;', 1),
            (r'const ROW: &str = "{\"condition\":\"cancelled\"}";', 1),
            (
                r'const ROW: &str = "{\"payload\":\"{\\\"condition\\\":\\\"cancelled\\\"}\"}";',
                1,
            ),
            (
                r'let row = format!("{{\"condition\":\"cancelled\",\"path\":\"{path}\"}}");',
                1,
            ),
        ]
        for crate in ("yamaa", "yamaa-core", *sorted(guard.HOST_CRATES)):
            for source, line in sources:
                with self.subTest(crate=crate, source=source):
                    path = self.write(f"rust/crates/{crate}/src/rows.rs", source + "\n")
                    self.assertEqual(
                        self.errors(),
                        [
                            f"rust/crates/{crate}/src/rows.rs:{line}: "
                            "unregistered condition literal cancelled"
                        ],
                    )
                    path.unlink()

    def test_test_items_comments_and_condition_reads_are_not_condition_sites(self):
        self.write(
            "rust/crates/yamaa/src/diagnostic.rs",
            "pub fn issue(definition: Definition, failure: &Value) -> Issue {\n"
            "    let reasons = [\n"
            "        // the reported condition:\n"
            '        "not a condition",\n'
            "    ];\n"
            '    let known = failure["condition"] == "cancelled";\n'
            "    Issue {\n"
            "        condition: definition.condition.into(),\n"
            '        rule: text(failure, "condition"),\n'
            "    }\n"
            "}\n"
            "#[cfg(test)]\n"
            "mod tests {\n"
            "    fn issue() -> Issue {\n"
            '        Issue { phase: "input".into(), condition: "conversion_probe".into() }\n'
            "    }\n"
            "    const BRACE: char = '}';\n"
            '    const TEMPLATE: &str = r#"{"condition":"__CONDITION__"}"#;\n'
            "}\n"
            '#[cfg(all(test, feature = "yaml"))]\n'
            "#[allow(dead_code)]\n"
            'fn fixture() { let condition = "finding"; }\n',
        )
        self.assertEqual(self.errors(), [])
        for gate in ("#[cfg(not(test))]", '#[cfg(feature = "test-utils")]'):
            with self.subTest(gate=gate):
                self.write(
                    "rust/crates/yamaa/src/diagnostic.rs",
                    f'{gate}\nmod live {{\n    const CONDITION: &str = "cancelled";\n}}\n',
                )
                self.assertEqual(
                    self.errors(),
                    [
                        "rust/crates/yamaa/src/diagnostic.rs:3: "
                        "unregistered condition literal cancelled"
                    ],
                )

    def test_json_text_is_decoded_only_in_codecs(self):
        source = (
            "pub fn read(text: &str) -> Value {\n"
            "    crate::json_wire::decode(text).unwrap()\n"
            "}\n"
            "use crate::json_wire::{decode_slice, encode};\n"
            "// json_wire::decode in a comment is not a call\n"
            'const NOTE: &str = "json_wire::decode";\n'
            "#[cfg(test)]\n"
            "mod tests {\n"
            '    fn fixture() -> Value { crate::json_wire::decode("{}").unwrap() }\n'
            "}\n"
            '#[cfg(all(test, feature = "yaml"))]\n'
            'fn probe() { yamaa_adapters::json_wire::decode_array("[]").unwrap(); }\n'
        )
        self.write("rust/crates/yamaa-adapters/src/rows.rs", source)
        self.write("rust/crates/yamaa/src/api.rs", source)
        self.assertEqual(
            self.errors(),
            [
                "rust/crates/yamaa/src/api.rs:2: JSON text decoded outside a codec",
                "rust/crates/yamaa/src/api.rs:4: JSON text decoded outside a codec",
                "rust/crates/yamaa-adapters/src/rows.rs:2: JSON text decoded outside a codec",
                "rust/crates/yamaa-adapters/src/rows.rs:4: JSON text decoded outside a codec",
            ],
        )
        for codec in sorted(guard.JSON_CODECS):
            with self.subTest(codec=codec):
                self.write(str(codec), source)
        (self.root / "rust/crates/yamaa-adapters/src/rows.rs").unlink()
        (self.root / "rust/crates/yamaa/src/api.rs").unlink()
        self.assertEqual(self.errors(), [])

    def test_json_reader_imports_and_aliases_cannot_bypass_the_codec_boundary(self):
        sources = [
            "crate :: json_wire :: decode_slice(bytes)",
            "crate::r#json_wire::r#decode(text)",
            "use crate::r#json_wire as r#wire;\nr#wire::r#decode(text)",
            "use crate::json_wire as wire;\nwire::decode(text)",
            "use crate::{json_wire::{self as wire}};\nwire::decode_array(text)",
            "use yamaa_adapters as adapters;\nuse adapters::json_wire as wire;\nwire::decode(text)",
            "use crate::json_wire::*;\ndecode(text)",
            "use crate::json_wire as wire;\nuse wire::*;\ndecode(text)",
            "use crate::{json_wire::{decode as read}};\nread(text)",
            "pub use crate::json_wire::{decode_slice as read, encode};",
            "serde_json :: from_str::<serde_json::Value>(text)",
            "serde_json::from_slice::<crate::json_wire::WireValueOwned>(bytes)",
            "use serde_json as json;\njson::from_reader(reader)",
            "use serde_json::{from_str as read};\nread::<Value>(text)",
            "use serde_json as json;\nuse json::{from_slice as read};\nread(bytes)",
            "use serde_json::*;\nfrom_str::<Value>(text)",
            "use serde_json::de::*;\nDeserializer::from_str(text)",
            "serde_json::de::Deserializer::from_slice(bytes)",
            "use serde_json::Deserializer as Reader;\nReader::from_str(text)",
            "use crate::json_wire::WireValueOwned as Owned;\nOwned::deserialize(reader)",
            "use serde_json::Value as Tree;\nTree::deserialize(reader)",
            "use serde_json::value::Value as Tree;\n<Tree as serde::Deserialize<'_>>::deserialize(reader)",
            "use serde_json::value::*;\nValue::deserialize(reader)",
            "use crate::json_wire as wire;\nfn read() { wire::decode(text); }\nmod other { use unrelated as wire; }",
        ]
        for source in sources:
            with self.subTest(source=source):
                path = self.write("rust/crates/yamaa-adapters/src/rows.rs", source)
                self.assertTrue(self.errors())
                self.assertTrue(
                    all(
                        "JSON text decoded outside a codec" in error
                        for error in self.errors()
                    )
                )
                path.unlink()

    def test_explicit_json_from_str_targets_cannot_bypass_the_codec_boundary(self):
        sources = [
            "fn read() { text.parse::<serde_json::Value>(); }",
            "fn read() { text . parse :: < ::serde_json::value::Value, > (); }",
            "use serde_json::Value as Tree;\nfn read() { text.parse::<Tree>(); }",
            "use serde_json as json;\nfn read() { text.parse::<json::value::Value>(); }",
            "use serde_json::Value as r#Tree;\nfn read() { text.r#parse::<r#Tree>(); }",
            "extern crate serde_json as json;\nfn read() { text.parse::<json::Value>(); }",
            "fn read() { <serde_json::Value as std::str::FromStr>::from_str(text); }",
            "fn read() { <serde_json::Value as std::str::FromStr>::r#from_str(text); }",
            "fn read() { <::serde_json::Value as ::core::str::FromStr>::from_str(text); }",
            "use serde_json::Value as Tree; use std::str::FromStr as Read;\n"
            "fn read() { <Tree as Read>::from_str(text); }",
            "fn read() { str::parse::<serde_json::Value>(text); }",
            "fn read() { std::primitive::str::parse::<serde_json::Value>(text); }",
            "fn read() { ::core::primitive::str::parse::<serde_json::Value>(text); }",
            "use std::primitive::str as Text; use serde_json::Value as Tree;\n"
            "fn read() { Text::parse::<Tree>(text); }",
        ]
        for source in sources:
            with self.subTest(source=source):
                self.assertEqual(guard.json_decodes(source), [len(source.splitlines())])

    def test_parse_routes_respect_target_scope_and_leave_custom_parsers_allowed(self):
        source = (
            "use serde_json::Value as Tree;\n"
            "fn read() { text.parse::<Tree>(); }\n"
            "fn custom() { use other::Value as Tree; text.parse::<Tree>(); }\n"
            "mod child { fn custom() { text.parse::<Tree>(); } }\n"
        )
        self.assertEqual(guard.json_decodes(source), [2])
        for source in (
            "fn read() { parse::<serde_json::Value>(text); }",
            "fn read() { custom::parse::<serde_json::Value>(text); }",
            "fn read() { text.parse::<String>(); text.parse::<u64>(); }",
            "use other::Value as Tree; fn read() { str::parse::<Tree>(text); }",
            "use other::Text as str; fn read() { str::parse::<serde_json::Value>(text); }",
            "use other::FromStr as Read;\n"
            "fn read() { <serde_json::Value as Read>::from_str(text); }",
            "#[cfg(test)] mod tests { fn fixture() { text.parse::<serde_json::Value>(); } }",
            'const NOTE: &str = "text.parse::<serde_json::Value>()"; // text.parse::<serde_json::Value>()',
        ):
            with self.subTest(source=source):
                self.assertEqual(guard.json_decodes(source), [])

    def test_serde_field_deserializers_are_code_but_ordinary_strings_are_not(self):
        sources = [
            '#[serde(deserialize_with = "crate::json_wire::deserialize_value")]\nvalue: Value,',
            'use crate::json_wire as wire;\n#[serde(deserialize_with = "wire::deserialize_value")]\nvalue: Value,',
            'use serde_json as json;\n#[serde(deserialize_with = "json::Value::deserialize")]\nvalue: Value,',
            '#[serde(deserialize_with = r#"crate :: json_wire :: deserialize_value"#)]\nvalue: Value,',
            '#[serde(with = "crate::json_wire")]\nvalue: Value,',
            'use crate::json_wire as wire;\n#[serde(with = "wire")]\nvalue: Value,',
            'use crate::json_wire as wire;\n#[serde(with = r#"wire"#)]\nvalue: Value,',
        ]
        for source in sources:
            with self.subTest(source=source):
                path = self.write("rust/crates/yamaa-adapters/src/rows.rs", source)
                self.assertEqual(
                    self.errors(),
                    [
                        f"rust/crates/yamaa-adapters/src/rows.rs:{len(source.splitlines()) - 1}: JSON text decoded outside a codec"
                    ],
                )
                path.unlink()

    def test_json_aliases_do_not_leak_across_siblings_or_module_boundaries(self):
        sources = [
            "fn encode() { use serde_json as json; json::to_string(value); }\n"
            "fn read() { use other as json; json::from_str(text); }",
            "fn read() { use other as json; json::from_str(text); }\n"
            "fn encode() { use serde_json as json; json::to_string(value); }",
            "mod encode { use serde_json as json; fn write() { json::to_string(value); } }\n"
            "mod read { use other as json; fn run() { json::from_str(text); } }",
            "use serde_json as json;\n"
            "fn read() { use other as json; json::from_str(text); }",
            "use serde_json as json;\n"
            "mod child { fn read() { json::from_str(text); } }",
            "fn outer() { use serde_json as json;\n"
            "mod child { fn read() { json::from_str(text); } } }",
        ]
        for source in sources:
            with self.subTest(source=source):
                self.assertEqual(guard.json_decodes(source), [])

    def test_json_aliases_follow_nested_blocks_and_whole_block_item_scope(self):
        source = (
            "use serde_json as json;\n"
            "fn run() {\n"
            "    { use other as json; json::from_str(text); }\n"
            "    json::from_str(text);\n"
            "}\n"
            "fn outer() {\n"
            "    fn inner() { json::from_str(text); }\n"
            "    use serde_json as json;\n"
            "}\n"
            "fn shadow() {\n"
            "    json::from_str(text);\n"
            "    use other as json;\n"
            "}\n"
        )
        self.assertEqual(guard.json_decodes(source), [4, 7])

    def test_alias_targets_resolve_at_their_declaration_not_their_caller(self):
        for owner, shadow, expected in (
            ("serde_json", "other", [4]),
            ("other", "serde_json", []),
        ):
            source = (
                f"use {owner} as json;\n"
                "use json as saved;\n"
                f"fn read() {{ use {shadow} as json;\n"
                "    saved::from_str(text);\n"
                "}\n"
            )
            with self.subTest(owner=owner):
                self.assertEqual(guard.json_decodes(source), expected)

    def test_local_serde_json_names_do_not_acquire_external_crate_identity(self):
        for declarations, owner in (
            ("use custom::serde_json;", "serde_json"),
            ("use custom::serde_json as json; use json as saved;", "saved"),
            ("use crate::serde_json;", "serde_json"),
            ("mod serde_json {}", "serde_json"),
            ("mod serde_json;", "serde_json"),
            ("mod r#serde_json; use r#serde_json as json;", "json"),
            (
                "mod custom { pub mod serde_json; } use custom::serde_json;",
                "serde_json",
            ),
        ):
            for expression in (
                f"{owner}::from_str(text)",
                f"{owner}::Deserializer::from_slice(bytes)",
                f"text.parse::<{owner}::Value>()",
                f"<{owner}::Value as serde::Deserialize<'_>>::deserialize(reader)",
            ):
                with self.subTest(declarations=declarations, expression=expression):
                    self.assertEqual(
                        guard.json_decodes(
                            f"{declarations}\nfn read() {{ {expression}; }}"
                        ),
                        [],
                    )
        self.assertEqual(
            guard.json_decodes(
                "use custom::serde_json; struct Record {\n"
                '    #[serde(deserialize_with = "serde_json::Value::deserialize")] value: Value\n'
                "}"
            ),
            [],
        )

    def test_local_value_globs_do_not_imply_external_json_values(self):
        for declarations in (
            "use custom::serde_json::value::*;",
            "use custom::serde_json as json; use json::value::*;",
            "use crate::serde_json::value::*;",
            "mod serde_json; use serde_json::value::*;",
            "mod serde_json { pub mod value; } use serde_json::value::*;",
        ):
            with self.subTest(declarations=declarations):
                self.assertEqual(
                    guard.json_decodes(
                        declarations + "\nfn read() { Value::deserialize(reader); "
                        "text.parse::<Value>(); <Value as serde::Deserialize<'_>>::deserialize(reader); }"
                    ),
                    [],
                )

    def test_external_json_identity_survives_local_names_and_explicit_reexports(self):
        for source in (
            "mod serde_json;\nfn read() { ::serde_json::from_str(text); }",
            "mod serde_json { pub use ::serde_json::Value; }\n"
            "fn read() { serde_json::Value::deserialize(reader); }",
            "mod custom { pub use ::serde_json as serde_json; }\n"
            "fn read() { custom::serde_json::from_str(text); }",
            "mod custom { pub extern crate serde_json; }\n"
            "fn read() { custom::serde_json::from_str(text); }",
            "extern crate serde_json;\n"
            "fn read() { crate::serde_json::from_str(text); }",
        ):
            with self.subTest(source=source):
                self.assertEqual(guard.json_decodes(source), [2])

    def test_simple_type_aliases_preserve_explicit_json_reader_targets(self):
        for declaration in (
            "type Tree = serde_json::Value;",
            "type Tree = ::serde_json::value::Value;",
            "type r#Tree = r#serde_json :: r#Value;",
            "use serde_json::Value as Value; type Tree = Value;",
            "use serde_json::value::*; type Tree = Value;",
            "type Tree = Saved; type Saved = serde_json::Value;",
            "mod model { pub type Value = serde_json::Value; } type Tree = model::Value;",
        ):
            for expression in (
                "Tree::from_str(text)",
                "Tree::deserialize(reader)",
                "text.parse::<Tree>()",
                "text.r#parse::<r#Tree>()",
                "str::parse::<Tree>(text)",
                "<Tree as std::str::FromStr>::from_str(text)",
                "<Tree as serde::Deserialize<'_>>::deserialize(reader)",
            ):
                with self.subTest(declaration=declaration, expression=expression):
                    self.assertEqual(
                        guard.json_decodes(
                            f"{declaration}\nfn read() {{ {expression}; }}"
                        ),
                        [2],
                    )
        self.assertEqual(
            guard.json_decodes(
                "type Tree = serde_json::Value; struct Record {\n"
                '    #[serde(deserialize_with = "Tree::deserialize")] value: Tree\n'
                "}"
            ),
            [2],
        )

    def test_type_aliases_use_declaration_scope_and_whole_block_visibility(self):
        for owner, shadow, expected in (
            ("serde_json", "custom", [4]),
            ("custom", "serde_json", []),
        ):
            source = (
                f"use {owner} as json;\n"
                "type Tree = json::Value; type Saved = Tree;\n"
                f"fn read() {{ use {shadow} as json; type Tree = other::Value;\n"
                "    text.parse::<Saved>();\n"
                "}\n"
            )
            with self.subTest(owner=owner):
                self.assertEqual(guard.json_decodes(source), expected)
        source = (
            "type Tree = serde_json::Value;\n"
            "fn run() {\n"
            "    { type Tree = other::Value; Tree::from_str(text); }\n"
            "    text.parse::<Tree>();\n"
            "}\n"
            "fn before() { text.parse::<Tree>(); type Tree = other::Value; }\n"
            "fn outer() {\n"
            "    fn inner() { text.parse::<Saved>(); }\n"
            "    type Saved = Tree;\n"
            "}\n"
            "mod child { fn read() { text.parse::<Tree>(); } }\n"
            "mod sibling { type Tree = other::Value; fn read() { Tree::from_str(text); } }\n"
            "mod imported { use super::*; fn read() { text.parse::<Tree>(); } }\n"
        )
        self.assertEqual(guard.json_decodes(source), [4, 8, 13])

    def test_type_aliases_allow_custom_types_projection_and_cycles(self):
        for declaration in (
            "type Tree = custom::Value;",
            "type Tree = custom::serde_json::Value;",
            "mod serde_json; type Tree = serde_json::Value;",
            "mod serde_json {} type Tree = crate::serde_json::Value;",
            "type Tree = Saved; type Saved = Tree;",
        ):
            with self.subTest(declaration=declaration):
                self.assertEqual(
                    guard.json_decodes(
                        declaration + "\nfn read() { Tree::from_str(text); "
                        "text.parse::<Tree>(); Tree::deserialize(reader); }"
                    ),
                    [],
                )
        self.assertEqual(
            guard.json_decodes(
                "type Tree = serde_json::Value;\n"
                "fn build() { serde_json::from_value::<Record>(Tree::Null); "
                "serde_json::to_value(Tree::Null); }\n"
                "#[cfg(test)] fn fixture() { text.parse::<Tree>(); }"
            ),
            [],
        )
        for declaration in (
            "type Tree<T> = T;",
            "type Tree = custom::Wrapper<serde_json::Value>;",
            "type Tree = &'static serde_json::Value;",
        ):
            with self.subTest(declaration=declaration):
                self.assertEqual(
                    guard.json_decodes(
                        "type Tree = serde_json::Value;\n"
                        f"fn read() {{ {declaration} text.parse::<Tree>(); }}"
                    ),
                    [],
                )

    def test_associated_types_are_not_lexical_aliases_but_method_local_types_are(self):
        for header in (
            "impl Custom for Record",
            "pub trait Custom",
            "#[allow(dead_code)] unsafe impl Custom for Record",
        ):
            with self.subTest(header=header):
                self.assertEqual(
                    guard.json_decodes(
                        f"{header} {{ type Tree = serde_json::Value;\n"
                        "fn read() { text.parse::<Tree>(); }\n"
                        "fn local() { type Tree = serde_json::Value; text.parse::<Tree>(); } }"
                    ),
                    [3],
                )
        for header in ("impl Custom for Record", "trait Custom"):
            with self.subTest(header=header):
                self.assertEqual(
                    guard.json_decodes(
                        "type Tree = serde_json::Value;\n"
                        f"{header} {{ type Tree = other::Value;\n"
                        "fn read() { text.parse::<Tree>(); } }"
                    ),
                    [3],
                )

    def test_return_position_impl_trait_does_not_hide_function_local_type_aliases(self):
        for header in (
            "fn read() -> impl Display",
            "pub fn read() -> impl Display",
            "fn read() -> Result<impl Display, Error>",
        ):
            with self.subTest(header=header):
                self.assertEqual(
                    guard.json_decodes(
                        f"{header} {{ type Tree = serde_json::Value;\n"
                        "text.parse::<Tree>(); result }"
                    ),
                    [2],
                )

    def test_module_qualified_aliases_and_parent_globs_keep_their_owners(self):
        sources = [
            "use serde_json as json;\nmod child { fn read() { super::json::from_str(text); } }",
            "use serde_json as json;\nmod child { use super::json as saved; fn read() { saved::from_str(text); } }",
            "use serde_json as json;\nmod child { use super::*; fn read() { json::from_str(text); } }",
            "use serde_json as json;\nmod child { fn read() { crate::json::from_str(text); } }",
            "mod child { use serde_json as json;\nfn read() { self::json::from_str(text); } }",
            "use serde_json as json;\nfn outer() { use other as json; mod child { fn read() { super::json::from_str(text); } } }",
            "mod helpers { pub use serde_json as json; }\nuse helpers::*; fn read() { json::from_str(text); }",
            "mod helpers { pub use serde_json as json; }\nuse helpers as saved; fn read() { saved::json::from_str(text); }",
            "use serde_json as json;\nmod child { mod grandchild { fn read() { super::super::json::from_str(text); } } }",
        ]
        for source in sources:
            with self.subTest(source=source):
                self.assertEqual(guard.json_decodes(source), [2])

    def test_absolute_json_paths_bypass_local_aliases_and_use_tree_braces(self):
        sources = [
            "use other as serde_json;\nfn read() { ::serde_json::from_str(text); }",
            "use other as serde_json; use ::serde_json as json;\nfn read() { json::from_str(text); }",
            "use ::serde_json::{self as json};\nfn read() { json::from_str(text); }",
            "use crate::{json_wire::{self as wire}};\nfn read() { wire::decode(text); }",
            "use ::r#serde_json::{self as r#json};\nfn read() { r#json::from_str(text); }",
        ]
        for source in sources:
            with self.subTest(source=source):
                self.assertEqual(guard.json_decodes(source), [2])

    def test_extern_crate_aliases_keep_json_reader_identity(self):
        sources = [
            "extern crate serde_json as json;\nfn read() { json::from_str(text); }",
            "extern crate serde_json;\nfn read() { serde_json::from_slice(bytes); }",
            "pub extern crate serde_json as json;\nfn read() { ::json::from_reader(reader); }",
            "extern crate r#serde_json as r#json;\nfn read() { r#json::from_str(text); }",
            "extern crate r#serde_json as r#json;\nfn read() { ::r#json::from_str(text); }",
            "fn read() { extern crate serde_json as json;\njson::from_str(text); }",
            "fn read() {\njson::from_str(text); extern crate serde_json as json; }",
            "mod reader { extern crate serde_json as json;\nfn read() { self::json::from_str(text); } }",
            "extern crate yamaa_adapters as adapters;\nfn read() { adapters::json_wire::decode(text); }",
        ]
        for source in sources:
            with self.subTest(source=source):
                self.assertEqual(guard.json_decodes(source), [2])

    def test_only_root_extern_aliases_enter_the_extern_prelude(self):
        sources = [
            "extern crate serde_json as json;\nmod child { fn read() { json::from_str(text); } }",
            "extern crate serde_json as json;\nmod child { fn read() { ::json::from_str(text); } }",
            "mod parent { extern crate serde_json as json;\nmod child { fn read() { super::json::from_str(text); } } }",
            "fn outer() { extern crate serde_json as json;\nfn inner() { json::from_str(text); } }",
        ]
        for source in sources:
            with self.subTest(source=source):
                self.assertEqual(guard.json_decodes(source), [2])
        for source in (
            "mod parent { extern crate serde_json as json;\nmod child { fn read() { json::from_str(text); } } }",
            "fn outer() { extern crate serde_json as json;\nmod child { fn read() { json::from_str(text); } } }",
            "mod first { extern crate serde_json as json; }\nmod second { fn read() { json::from_str(text); } }",
            "extern crate serde_json as json;\nmod child { use other as json; fn read() { json::from_str(text); } }",
            "extern crate serde_json as json;\nmod child { fn read() { self::json::from_str(text); } }",
        ):
            with self.subTest(source=source):
                self.assertEqual(guard.json_decodes(source), [])

    def test_nonroot_files_do_not_promote_extern_aliases_to_the_extern_prelude(self):
        source = (
            "extern crate serde_json as json;\n"
            "fn read() { json::from_str(text); }\n"
            "mod child { fn read() { json::from_str(text); } }\n"
        )
        self.assertEqual(guard.json_decodes(source, crate_root=False), [2])
        self.assertEqual(guard.json_decodes(source, crate_root=True), [2, 3])
        for name, lines in (
            ("reader.rs", [2]),
            ("lib.rs", [2, 3]),
            ("main.rs", [2, 3]),
        ):
            with self.subTest(name=name):
                relative = f"rust/crates/yamaa-adapters/src/{name}"
                path = self.write(relative, source)
                self.assertEqual(
                    self.errors(),
                    [
                        f"{relative}:{line}: JSON text decoded outside a codec"
                        for line in lines
                    ],
                )
                path.unlink()

    def test_nonroot_extern_self_cannot_retarget_crate_paths_to_file_local_aliases(
        self,
    ):
        source = (
            "use serde_json as json; extern crate self as root;\n"
            "fn read() { root::json::from_str(text); }\n"
            "mod nested { extern crate self as local;\n"
            "fn read() { local::json::from_str(text); } }\n"
        )
        self.assertEqual(guard.json_decodes(source, crate_root=False), [])
        self.assertEqual(guard.json_decodes(source, crate_root=True), [2, 4])

    def test_extern_crate_targets_are_anchored_before_local_shadowing(self):
        sources = [
            "use other as serde_json; extern crate serde_json as json;\nfn read() { json::from_str(text); }",
            "extern crate serde_json as json; use json as saved;\nfn read() { use other as json; saved::from_str(text); }",
            "extern crate serde_json as json;\nmod child { use other as json; fn read() { ::json::from_str(text); } }",
            "extern crate other as serde_json; extern crate serde_json as json;\nfn read() { json::from_str(text); }",
        ]
        for source in sources:
            with self.subTest(source=source):
                self.assertEqual(guard.json_decodes(source), [2])
        for source in (
            "extern crate other as serde_json; fn read() { serde_json::from_str(text); ::serde_json::from_str(text); }",
            "use serde_json as other; extern crate other as json; fn read() { json::from_str(text); }",
            "extern crate other as json; use json as saved; fn read() { use serde_json as json; saved::from_str(text); }",
            "extern crate serde_json as json; fn write() { json::to_string(value); json::from_value(value); }",
            "extern crate serde_json as _; fn read() { json::from_str(text); }",
            "#[cfg(test)]\nextern crate serde_json as json; fn read() { json::from_str(text); }",
        ):
            with self.subTest(source=source):
                self.assertEqual(guard.json_decodes(source), [])

    def test_extern_crate_self_names_the_crate_root_without_external_reader_identity(
        self,
    ):
        for source in (
            "extern crate self as serde_json; fn read() { serde_json::from_str(text); ::serde_json::from_str(text); }",
            "extern crate self as local; mod child { fn read() { local::from_str(text); ::local::from_str(text); } }",
            "mod nested { extern crate self as serde_json; fn read() { serde_json::from_str(text); } }",
        ):
            with self.subTest(source=source):
                self.assertEqual(guard.json_decodes(source), [])
        source = (
            "use serde_json as json; extern crate self as local;\n"
            "mod child { fn read() { local::json::from_str(text); } }\n"
            "mod nested { extern crate self as root; use other as json;\n"
            "fn read() { root::json::from_str(text); } }"
        )
        self.assertEqual(guard.json_decodes(source), [2, 4])

    def test_extern_aliases_cover_serde_readers_value_globs_and_field_hooks(self):
        for route in (
            "fn read() { json::Deserializer::from_str(text); }",
            "use json::value::*; fn read() { Value::deserialize(reader); }",
            "use json::Value as Tree; fn read() { <Tree as serde::Deserialize<'_>>::deserialize(reader); }",
            'struct Record { #[serde(deserialize_with = "json::Value::deserialize")] value: Value }',
            "use json::de::*;",
        ):
            with self.subTest(route=route):
                self.assertEqual(
                    guard.json_decodes("extern crate serde_json as json;\n" + route),
                    [2],
                )
        source = (
            "extern crate yamaa_adapters as adapters; use adapters::json_wire as wire;\n"
            'struct Record { #[serde(with = "wire")] value: Value }'
        )
        self.assertEqual(guard.json_decodes(source), [2])
        self.assertEqual(
            guard.json_decodes(
                "extern crate serde_json as json; use json::ser::*; use json::value::*;\n"
                "fn build() { to_value(Value::Null); from_value(Value::Null); }"
            ),
            [],
        )

    def test_conditional_alias_alternatives_are_unioned_only_in_the_same_scope(self):
        source = (
            '#[cfg(feature = "reader")]\nuse serde_json as json;\n'
            '#[cfg(not(feature = "reader"))]\nuse other as json;\n'
            "fn read() { json::from_str(text); }\n"
            "mod sibling { use other as json; fn read() { json::from_str(text); } }\n"
        )
        self.assertEqual(guard.json_decodes(source), [5])

    def test_serde_globs_distinguish_reader_modules_from_serializers_and_maps(self):
        for owner in ("ser", "map", "error", "value"):
            for source in (
                f"use serde_json::{owner}::*;",
                f"use serde_json::{owner} as module;\nuse module::*;",
                f"use serde_json as json;\nuse json::{{{owner}::*}};",
            ):
                with self.subTest(source=source):
                    self.assertEqual(guard.json_decodes(source), [])
        source = "use serde_json::de as module;\nuse module::*;"
        self.assertEqual(guard.json_decodes(source), [2])

    def test_value_glob_allows_trees_and_projection_but_tracks_deserialization(self):
        source = (
            "use serde_json::value::*;\n"
            "fn build() { let value = Value::Null; to_value(value); from_value(value); }\n"
        )
        self.assertEqual(guard.json_decodes(source), [])
        for imports in (
            "use serde_json::value::*;",
            "use serde_json as json; use json::value::*;",
            "use ::serde_json::value as values; use values::*;",
        ):
            for expression in (
                "Value::deserialize(reader)",
                "<Value as serde::Deserialize<'_>>::deserialize(reader)",
            ):
                with self.subTest(imports=imports, expression=expression):
                    self.assertEqual(
                        guard.json_decodes(f"{imports}\nfn read() {{ {expression}; }}"),
                        [2],
                    )
        self.assertEqual(
            guard.json_decodes(
                "use serde_json::value::*;\n"
                "fn read() { use other::Value; Value::deserialize(reader); }"
            ),
            [],
        )
        for imports in (
            "use crate::serde_json::value::*;",
            "use crate::serde_json as json; use json::value::*;",
        ):
            source = f"extern crate serde_json;\n{imports}\n"
            with self.subTest(imports=imports):
                self.assertEqual(
                    guard.json_decodes(
                        source + "fn build() { to_value(Value::Null); }"
                    ),
                    [],
                )
                self.assertEqual(
                    guard.json_decodes(
                        source + "fn read() { Value::deserialize(reader); }"
                    ),
                    [3],
                )

    def test_serde_deserialize_attributes_resolve_aliases_in_their_own_scope(self):
        source = (
            "use serde_json as json; use json as saved;\n"
            "mod unrelated { use other as json; struct Record {\n"
            '    #[serde(deserialize_with = "json::Value::deserialize")] value: Value,\n'
            "} }\n"
            "mod reader { use super::saved as json; struct Record {\n"
            '    #[serde(deserialize_with = "json::Value::deserialize")] value: Value,\n'
            "} }\n"
            "fn nested() { use other as json; struct Record {\n"
            '    #[serde(deserialize_with = "saved::Value::deserialize")] value: Value,\n'
            "} }\n"
        )
        self.assertEqual(guard.json_decodes(source), [6, 9])

    def test_serde_with_resolves_the_module_deserializer_in_attribute_scope(self):
        source = (
            "use crate::json_wire as wire;\n"
            "mod unrelated { use other as wire; struct Record {\n"
            '    #[serde(with = "wire")] value: Value,\n'
            "} }\n"
            "mod reader { use super::wire as owned; struct Record {\n"
            '    #[serde(with = "owned")] value: Value,\n'
            "} }\n"
        )
        self.assertEqual(guard.json_decodes(source), [6])

    def test_value_glob_resolves_self_super_and_aliases_inside_inline_modules(self):
        for modules, imports, closing in (
            (
                "mod reader { extern crate serde_json;\n",
                "use self::serde_json::value::*;\n",
                "}",
            ),
            (
                "mod reader { extern crate serde_json;\n",
                "use self::serde_json as json; use json::value::*;\n",
                "}",
            ),
            (
                "mod reader { extern crate serde_json;\nmod nested {\n",
                "use super::serde_json::value::*;\n",
                "}}",
            ),
            (
                "mod reader { extern crate serde_json;\nmod nested {\n",
                "use super::serde_json as json; use json::value::*;\n",
                "}}",
            ),
        ):
            source = modules + imports
            with self.subTest(imports=imports):
                self.assertEqual(
                    guard.json_decodes(
                        source
                        + "fn build() { let value = Value::Null; to_value(value); from_value(value); }"
                        + closing
                    ),
                    [],
                )
                for expression in (
                    "Value::deserialize(reader)",
                    "<Value as serde::Deserialize<'_>>::deserialize(reader)",
                ):
                    with self.subTest(expression=expression):
                        self.assertEqual(
                            guard.json_decodes(
                                source + f"fn read() {{ {expression}; }}" + closing
                            ),
                            [source.count("\n") + 1],
                        )

    def test_json_serialization_projection_and_test_fixtures_remain_allowed(self):
        source = (
            "use crate::json_wire as wire;\n"
            "use serde_json as json;\n"
            "use serde_json::Value as Tree;\n"
            "fn encode(value: Tree) {\n"
            "    wire::encode(&wire::project(value));\n"
            "    json::to_string(&value);\n"
            "    json::to_value(&value);\n"
            "    json::from_value::<Record>(value);\n"
            "}\n"
            '#[serde(serialize_with = "wire::serialize_value")]\nvalue: Value,\n'
            "// serde_json::from_str(text)\n"
            'const NOTE: &str = r#"deserialize_with = "wire::deserialize_value""#;\n'
            "#[cfg(test)]\nmod tests {\n"
            "    use crate::json_wire::*;\n"
            "    use serde_json::Deserializer;\n"
            '    #[serde(deserialize_with = "wire::deserialize_value")]\nvalue: Value,\n'
            '    fn fixture() { decode("{}"); }\n'
            "}\n"
        )
        self.write("rust/crates/yamaa-adapters/src/rows.rs", source)
        self.assertEqual(self.errors(), [])

    def test_json_decodes_in_separate_cfg_test_modules_are_fixtures(self):
        self.write(
            "rust/crates/yamaa-adapters/src/lib.rs",
            '#[cfg(test)]\nmod tests;\nmod reader;\n#[cfg(all(test, feature = "yaml"))]\nmod probes { mod nested; }\n',
        )
        self.write(
            "rust/crates/yamaa-adapters/src/reader.rs",
            "#[cfg(test)]\nmod tests;\n",
        )
        for relative in ("tests.rs", "reader/tests.rs", "probes/nested.rs"):
            self.write(
                f"rust/crates/yamaa-adapters/src/{relative}",
                'fn fixture() { crate::json_wire::decode("{}"); }\n',
            )
        self.write("rust/crates/yamaa-adapters/src/tests.rs", "mod helpers;\n")
        self.write(
            "rust/crates/yamaa-adapters/src/tests/helpers.rs",
            'fn fixture() { serde_json::from_str::<serde_json::Value>("{}"); }\n',
        )
        self.assertEqual(self.errors(), [])

    def test_test_filename_or_production_loader_does_not_exempt_json_decoding(self):
        self.write(
            "rust/crates/yamaa-adapters/src/tests.rs",
            'fn fixture() { crate::json_wire::decode("{}"); }\n',
        )
        for loader in (
            "",
            "mod tests;\n",
            "#[cfg(not(test))]\nmod tests;\n",
            '#[cfg(feature = "test-utils")]\nmod tests;\n',
            '#[cfg(test)]\nmod tests;\n#[path = "tests.rs"]\nmod exposed;\n',
            '#[cfg(test)]\nmod tests;\n#[path = r#"tests.rs"#]\nmod exposed;\n',
            '#[cfg(test)]\nmod tests;\n#[cfg_attr(feature = "live", path = "tests.rs")]\nmod exposed;\n',
            '#[cfg(test)]\nmod tests;\ninclude!("tests.rs");\n',
            '#[cfg(test)]\nmod tests;\ninclude!(concat!("test", "s.rs"));\n',
        ):
            with self.subTest(loader=loader):
                self.write("rust/crates/yamaa-adapters/src/lib.rs", loader)
                self.assertEqual(
                    self.errors(),
                    [
                        "rust/crates/yamaa-adapters/src/tests.rs:1: JSON text decoded outside a codec"
                    ],
                )

    def test_explicit_test_module_paths_and_test_includes_are_exempt(self):
        self.write(
            "rust/crates/yamaa-adapters/src/lib.rs",
            '#[cfg(test)]\n#[path = r#"fixtures.rs"#]\nmod tests;\n'
            '#[cfg(test)]\nmod extra { include!("fixtures.rs"); }\n',
        )
        self.write(
            "rust/crates/yamaa-adapters/src/fixtures.rs",
            'fn fixture() { crate::json_wire::decode("{}"); }\n',
        )
        self.assertEqual(self.errors(), [])

    def test_inline_path_in_non_mod_file_tracks_its_production_loader(self):
        self.write(
            "rust/crates/yamaa-adapters/src/lib.rs",
            'mod reader;\n#[cfg(test)]\n#[path = "reader/nested/fixtures.rs"]\nmod tests;\n',
        )
        reader = self.write(
            "rust/crates/yamaa-adapters/src/reader.rs",
            '#[cfg(test)]\n#[path = "fixtures.rs"]\nmod fixtures;\n'
            'mod nested {\n    #[path = "fixtures.rs"]\n    mod production;\n}\n',
        )
        for relative in ("fixtures.rs", "reader/nested/fixtures.rs"):
            self.write(
                f"rust/crates/yamaa-adapters/src/{relative}",
                'fn fixture() { crate::json_wire::decode("{}"); }\n',
            )
        self.assertEqual(
            self.errors(),
            [
                "rust/crates/yamaa-adapters/src/reader/nested/fixtures.rs:1: JSON text decoded outside a codec"
            ],
        )
        reader.write_text(
            reader.read_text().replace("mod nested", "#[cfg(test)]\nmod nested")
        )
        self.assertEqual(self.errors(), [])

    def test_a_raw_identifier_module_loads_its_plain_named_file(self):
        crate = "rust/crates/yamaa-adapters/src"
        for relative in ("reader.rs", "nested/reader.rs"):
            self.write(
                f"{crate}/{relative}",
                'fn fixture() { crate::json_wire::decode("{}"); }\n',
            )
        loader = (
            '#[cfg(test)]\n#[path = "reader.rs"]\nmod tests;\n'
            '#[cfg(test)]\n#[path = "nested/reader.rs"]\nmod nested_tests;\n'
        )
        self.write(f"{crate}/lib.rs", loader)
        self.assertEqual(self.errors(), [])
        for live, relative in (
            ("mod r#reader;\n", "reader.rs"),
            ("mod r#nested {\n    mod r#reader;\n}\n", "nested/reader.rs"),
        ):
            with self.subTest(live=live):
                self.write(f"{crate}/lib.rs", loader + live)
                self.assertEqual(
                    self.errors(),
                    [f"{crate}/{relative}:1: JSON text decoded outside a codec"],
                )

    def test_a_bracket_in_an_attribute_string_hides_no_path_attribute(self):
        crate = "rust/crates/yamaa-adapters/src"
        self.write(
            f"{crate}/fixtures.rs",
            'fn fixture() { crate::json_wire::decode("{}"); }\n',
        )
        loader = '#[cfg(test)]\n#[path = "fixtures.rs"]\nmod tests;\n'
        self.write(f"{crate}/lib.rs", loader)
        self.assertEqual(self.errors(), [])
        self.write(
            f"{crate}/lib.rs",
            loader + '#[path = "fixtures.rs"]\n#[doc = "see [fixtures]"]\nmod live;\n',
        )
        self.assertEqual(
            self.errors(),
            [f"{crate}/fixtures.rs:1: JSON text decoded outside a codec"],
        )

    def test_a_path_on_an_inline_module_names_its_children_directory(self):
        crate = "rust/crates/yamaa-adapters/src"
        self.write(
            f"{crate}/fixtures/data.rs",
            'fn fixture() { crate::json_wire::decode("{}"); }\n',
        )
        loader = '#[cfg(test)]\n#[path = "fixtures/data.rs"]\nmod tests;\n'
        self.write(f"{crate}/lib.rs", loader)
        self.assertEqual(self.errors(), [])
        live = '#[path = "fixtures"]\nmod live {\n    mod data;\n}\n'
        for root, reader in (
            (live, None),
            (
                live.replace(
                    '#[path = "fixtures"]', '#[cfg_attr(unix, path = "fixtures")]'
                ),
                None,
            ),
            (live.replace("mod data;", '#[path = "data.rs"]\n    mod renamed;'), None),
            # In a file that is not a mod-rs file, the attribute stays
            # relative to the file's own directory.
            ("mod reader;\n", live),
        ):
            with self.subTest(root=root, reader=reader):
                self.write(f"{crate}/lib.rs", loader + root)
                if reader is not None:
                    self.write(f"{crate}/reader.rs", reader)
                self.assertEqual(
                    self.errors(),
                    [f"{crate}/fixtures/data.rs:1: JSON text decoded outside a codec"],
                )

    def test_a_file_a_path_attribute_loads_declares_its_children_beside_it(self):
        crate = "rust/crates/yamaa-adapters/src"
        self.write(
            f"{crate}/child.rs", 'fn fixture() { crate::json_wire::decode("{}"); }\n'
        )
        self.write(f"{crate}/other.rs", "mod child;\n")
        loader = "#[cfg(test)]\nmod child;\n"
        # Loaded by its own name, other.rs would hold its child in other/.
        self.write(f"{crate}/lib.rs", loader + "mod other;\n")
        self.assertEqual(self.errors(), [])
        self.write(f"{crate}/lib.rs", loader + '#[path = "other.rs"]\nmod renamed;\n')
        self.assertEqual(
            self.errors(),
            [f"{crate}/child.rs:1: JSON text decoded outside a codec"],
        )

    def test_json_decoders_reached_by_alias_glob_or_serde_route_are_refused(self):
        self.write(
            "rust/crates/yamaa-adapters/src/rows.rs",
            "use crate::json_wire as wire;\n"
            "use crate::json_wire::{self as codec, encode};\n"
            "use crate::json_wire::decode as parse;\n"
            "fn read(t: &str) { wire::decode(t); codec::decode_slice(t); }\n"
            "fn owned(t: &str) { serde_json::from_str::<json_wire::WireValueOwned>(t); }\n"
            "#[derive(Deserialize, Serialize)]\n"
            "struct Held {\n"
            '    #[serde(serialize_with = "crate::json_wire::serialize_value",\n'
            '        deserialize_with = "crate::json_wire::deserialize_value")]\n'
            "    value: Value,\n"
            '    #[serde(deserialize_with = "wire::deserialize_value")]\n'
            "    other: Value,\n"
            "}\n"
            "fn write(v: &Value) -> String { wire::encode(v) + &codec::encode(v) }\n",
        )
        self.write(
            "rust/crates/yamaa/src/glob.rs",
            "use yamaa_adapters::json_wire::*;\n"
            "fn write(v: &Value) -> String { encode(v) }\n"
            "fn read(t: &str) -> Value { decode(t).unwrap() }\n",
        )
        self.write(
            "rust/crates/yamaa/src/encode_only.rs",
            "use yamaa_adapters::json_wire::*;\n"
            "fn write(v: &Value) -> String { encode(v) }\n",
        )
        self.assertEqual(
            self.errors(),
            [
                # The glob already exposes readers, even in encode-only code.
                "rust/crates/yamaa/src/encode_only.rs:1: JSON text decoded outside a codec",
                "rust/crates/yamaa/src/glob.rs:1: JSON text decoded outside a codec",
                # A decoder the glob brings in is a decoder named bare.
                "rust/crates/yamaa/src/glob.rs:3: JSON text decoded outside a codec",
                "rust/crates/yamaa-adapters/src/rows.rs:3: JSON text decoded outside a codec",
                "rust/crates/yamaa-adapters/src/rows.rs:4: JSON text decoded outside a codec",
                "rust/crates/yamaa-adapters/src/rows.rs:5: JSON text decoded outside a codec",
                "rust/crates/yamaa-adapters/src/rows.rs:9: JSON text decoded outside a codec",
                "rust/crates/yamaa-adapters/src/rows.rs:11: JSON text decoded outside a codec",
            ],
        )

    def test_json_wire_renames_are_followed_into_other_sources(self):
        def hook(attribute):
            return (
                "#[derive(Deserialize)]\nstruct Held {\n"
                f"    #[serde({attribute})]\n    value: Value,\n}}\n"
            )

        rows = "yamaa-adapters/src/rows.rs"
        for root, sources, sites in (
            # A public rename at the crate root, named from a module file.
            (
                "pub use json_wire as wire;\n",
                {rows: "fn read(t: &str) { crate::wire::decode(t); }\n"},
                [f"{rows}:1"],
            ),
            (
                "pub(crate) use crate::json_wire::{self as codec};\n",
                {rows: "use crate::codec::decode_slice as read;\n"},
                [f"{rows}:1"],
            ),
            # A private rename is visible to every child module file.
            (
                "use json_wire as wire;\n",
                {rows: "fn read(t: &str) { super::wire::decode(t); }\n"},
                [f"{rows}:1"],
            ),
            (
                "use json_wire as wire;\n",
                {rows: "use super::*;\nfn read(t: &str) { wire::decode_array(t); }\n"},
                [f"{rows}:2"],
            ),
            (
                "use json_wire as wire;\n",
                {
                    rows: "use crate::wire::*;\n"
                    + hook('deserialize_with = "deserialize_value"')
                },
                [f"{rows}:1", f"{rows}:4"],
            ),
            (
                "use json_wire as wire;\n",
                {rows: hook('deserialize_with = "crate::wire::deserialize_value"')},
                [f"{rows}:3"],
            ),
            (
                "use json_wire as wire;\n",
                {rows: hook('with = "super::wire"')},
                [f"{rows}:3"],
            ),
            # A rename of a rename, and a rename inside an inline module.
            (
                "pub use json_wire as wire;\nmod helpers;\n",
                {
                    "yamaa-adapters/src/helpers.rs": "pub use crate::wire as codec;\n",
                    rows: "fn read(t: &str) { crate::helpers::codec::decode(t); }\n",
                },
                [f"{rows}:1"],
            ),
            (
                "mod helpers {\n    pub use crate::json_wire as codec;\n}\n",
                {rows: "fn read(t: &str) { crate::helpers::codec::decode(t); }\n"},
                [f"{rows}:1"],
            ),
            # Another crate names a public rename through the crate.
            (
                "pub use json_wire as wire;\n",
                {
                    "yamaa/src/api.rs": (
                        "fn read(t: &str) { yamaa_adapters::wire::decode(t); }\n"
                    )
                },
                ["yamaa/src/api.rs:1"],
            ),
        ):
            with self.subTest(root=root, sources=sources):
                self.write(
                    "rust/crates/yamaa-adapters/src/lib.rs",
                    "pub mod json_wire;\nmod rows;\n" + root,
                )
                written = [
                    self.write(f"rust/crates/{relative}", text)
                    for relative, text in sources.items()
                ]
                self.assertEqual(
                    self.errors(),
                    [
                        f"rust/crates/{site}: JSON text decoded outside a codec"
                        for site in sites
                    ],
                )
                for path in written:
                    path.unlink()

    def test_a_json_wire_rename_in_a_function_or_test_stays_local(self):
        crate = "rust/crates/yamaa-adapters/src"
        # The module's own `wire` is not the wire module, and encoding through
        # a rename reads no JSON text.
        self.write(
            f"{crate}/rows.rs",
            "mod wire {\n    pub fn decode(_: &str) {}\n}\n"
            "fn read(t: &str) { wire::decode(t); }\n"
            "fn write(v: &Value) -> String { crate::codec::encode(v) }\n",
        )
        for root, tests in (
            (
                "fn encode(v: &Value) -> String {\n"
                "    use crate::json_wire as wire;\n    wire::encode(v)\n}\n",
                None,
            ),
            (
                "#[cfg(test)]\nmod tests {\n    pub use crate::json_wire as wire;\n}\n",
                None,
            ),
            ("#[cfg(test)]\nmod tests;\n", "pub use crate::json_wire as wire;\n"),
            ("pub use json_wire as codec;\n", None),
        ):
            with self.subTest(root=root):
                self.write(f"{crate}/lib.rs", "pub mod json_wire;\nmod rows;\n" + root)
                if tests is not None:
                    self.write(f"{crate}/tests.rs", tests)
                self.assertEqual(self.errors(), [])
                (self.root / crate / "tests.rs").unlink(missing_ok=True)

    def test_a_json_wire_glob_makes_its_decoders_bare_names(self):
        source = (
            "use crate::json_wire::*;\n"
            "#[derive(Deserialize)]\n"
            "struct Held {\n"
            '    #[serde(deserialize_with = "deserialize_value")]\n'
            "    value: Value,\n"
            "}\n"
            "fn read(t: &str) -> Value { decode(t).unwrap() }\n"
            "fn owned(r: Reader) { WireValueOwned::deserialize(r); }\n"
            "fn write(v: &Value) -> String { encode(v) }\n"
            "mod child { use super::*; fn read(t: &str) { decode_slice(t); } }\n"
            "fn shadowed(t: &str) { use other::decode; decode(t); }\n"
            "mod unrelated { fn read(t: &str) { decode(t); } }\n"
        )
        self.assertEqual(guard.json_decodes(source), [1, 4, 7, 8, 10])

    def test_json_wire_routes_name_only_its_public_decoders(self):
        for name in ("decoder", "decoded_len", "decode_admitted", "decode_value"):
            for source, sites in (
                (f"fn read() {{ crate::json_wire::{name}(text); }}", []),
                (f"use crate::json_wire::{{{name}, encode}};", []),
                (f"use crate::json_wire::*;\nfn read() {{ {name}(text); }}", [1]),
            ):
                with self.subTest(source=source):
                    self.assertEqual(guard.json_decodes(source), sites)
        for name in (
            "decode",
            "decode_slice",
            "decode_array",
            "deserialize_value",
            "WireValueOwned",
        ):
            for source, sites in (
                (f"fn read() {{ crate::json_wire::{name}(text); }}", [1]),
                (f"use crate::json_wire::*;\nfn read() {{ {name}(text); }}", [1, 2]),
            ):
                with self.subTest(source=source):
                    self.assertEqual(guard.json_decodes(source), sites)

    def test_json_wire_readers_are_the_wire_modules_public_readers(self):
        # A new decoder that other modules can name must join the guard's list.
        source = (ROOT / "rust/crates/yamaa-adapters/src/json_wire.rs").read_text(
            encoding="utf-8"
        )
        public = set(
            re.findall(
                r"(?m)^pub(?:\([^)]*\))?\s+(?:fn|struct|enum|type)\s+(\w+)", source
            )
        )
        readers = {
            name for name in public if name.startswith(("decode", "deserialize"))
        } | set(
            re.findall(
                r"\bimpl\s*(?:<[^>]*>)?\s*(?:serde\s*::\s*)?Deserialize\s*<[^>]*>"
                r"\s*for\s+(\w+)",
                source,
            )
        )
        self.assertEqual(readers, guard.JSON_WIRE_READERS)
        self.assertLessEqual(guard.JSON_WIRE_READERS, public)

    def test_a_module_file_declared_under_a_test_gate_is_test_code(self):
        decode = 'fn fixture() -> Value { crate::json_wire::decode("{}").unwrap() }\n'
        crate = "rust/crates/yamaa-adapters/src"
        self.write(
            f"{crate}/rows.rs",
            "#[cfg(test)]\nmod tests;\nmod helpers;\n#[cfg(not(test))]\nmod live;\n",
        )
        self.write(f"{crate}/rows/tests.rs", decode + "mod nested;\n")
        self.write(f"{crate}/rows/tests/nested.rs", decode)
        self.write(f"{crate}/rows/helpers.rs", decode)
        self.write(f"{crate}/rows/live.rs", decode)
        self.write(
            f"{crate}/reader/mod.rs",
            '#[cfg(all(test, feature = "yaml"))]\n'
            '#[path = "fixtures.rs"]\n'
            "pub(crate) mod probe;\n",
        )
        self.write(f"{crate}/reader/fixtures.rs", decode)
        self.assertEqual(
            self.errors(),
            [
                f"{crate}/rows/helpers.rs:1: JSON text decoded outside a codec",
                f"{crate}/rows/live.rs:1: JSON text decoded outside a codec",
            ],
        )

    def test_external_test_module_gating_is_independent_of_attribute_order(self):
        crate = "rust/crates/yamaa-adapters/src"
        self.write(
            f"{crate}/fixtures.rs",
            'fn fixture() { crate::json_wire::decode("{}"); }\n',
        )
        for attributes in (
            '#[path = "fixtures.rs"]\n#[cfg(test)]',
            '#[cfg(test)]\n#[path = "fixtures.rs"]',
            "#[allow(dead_code)]\n#[cfg(test)]",
            "#[cfg(test)]\n#[allow(dead_code)]",
            '#[path = "fixtures.rs"]\n#[allow(dead_code)]\n#[cfg(all(test, feature = "yaml"))]',
            '#[cfg(all(test, feature = "yaml"))]\n#[allow(dead_code)]\n#[path = "fixtures.rs"]',
        ):
            with self.subTest(attributes=attributes):
                loader = self.write(
                    f"{crate}/lib.rs", attributes + "\npub(crate) mod fixtures;\n"
                )
                self.assertEqual(self.errors(), [])
                loader.write_text(
                    loader.read_text() + '#[path = "fixtures.rs"]\nmod live;\n'
                )
                self.assertEqual(
                    self.errors(),
                    [f"{crate}/fixtures.rs:1: JSON text decoded outside a codec"],
                )

    def test_unregistered_condition_fixture_requires_every_loader_to_be_test_only(self):
        crate = "rust/crates/yamaa-adapters/src"
        self.write(
            f"{crate}/fixtures.rs",
            'fn fixture() { let row = Issue { condition: "not_registered".into() }; }\n',
        )
        loader = self.write(
            f"{crate}/lib.rs",
            '#[cfg(test)]\n#[path = "fixtures.rs"]\nmod fixtures;\n',
        )
        self.assertEqual(self.errors(), [])
        loader.write_text(loader.read_text() + '#[path = "fixtures.rs"]\nmod live;\n')
        self.assertEqual(
            self.errors(),
            [f"{crate}/fixtures.rs:1: unregistered condition literal not_registered"],
        )

    def test_registered_condition_literal_is_reported_once_as_a_new_site(self):
        self.write(
            "rust/crates/yamaa-adapters/src/rows.rs",
            'let issue = Issue { condition: "invalid_value".into() };\n',
        )
        self.assertEqual(
            self.errors(),
            [
                "rust/crates/yamaa-adapters/src/rows.rs:1: new diagnostic literal "
                "site outside core: invalid_value"
            ],
        )

    def test_generated_tables_and_private_unit_tests_hold_no_condition_sites(self):
        self.write(
            str(guard.CATALOGUE_TABLE),
            'const DATA: &str = r#"{"condition":"cancelled"}"#;\n',
        )
        self.publication_unit_module()
        self.write(
            str(guard.PRIVATE_TEST_MODULE),
            '#[test]\nfn probe() { let row = Issue { condition: "probe".into() }; }\n',
        )
        self.assertEqual(self.errors(), [])

    def test_a_card_id_in_prose_is_not_a_definition(self):
        path = self.root / "rules/values.md"
        path.write_text(path.read_text() + "\nA retired VALUE-2 is historical.\n")
        self.write(
            "rust/crates/yamaa-core/src/retired.rs", 'const RULE: &str = "VALUE-2";\n'
        )
        self.assertReported("undefined rule VALUE-2")

    def publication_unit_module(self):
        self.write(
            "rust/crates/yamaa-application/src/file_define/mod.rs",
            "#[cfg(test)]\nmod publication_failure_tests;\n",
        )
        self.write(
            str(guard.PRIVATE_TEST_MODULE),
            '#[test]\nfn independent_expectation() { assert_eq!("invalid_value", "invalid_value"); }\n',
        )

    def test_private_cfg_unit_assertions_do_not_gain_production_literal_credit(self):
        self.publication_unit_module()
        self.assertEqual(self.errors(), [])

    def test_unit_source_loaded_without_test_cfg_remains_production(self):
        self.publication_unit_module()
        self.write(
            "rust/crates/yamaa-application/src/file_define/mod.rs",
            "mod publication_failure_tests;\n",
        )
        self.assertReported("new diagnostic literal site")

    def test_unit_source_reused_by_production_alias_remains_production(self):
        self.publication_unit_module()
        unit = self.root / guard.PRIVATE_TEST_MODULE
        unit.write_text(
            unit.read_text()
            + 'fn decoded() -> Value { crate::json_wire::decode("{}").unwrap() }\n'
            + 'fn row() -> Issue { Issue { condition: "not_registered".into() } }\n'
        )
        # Under its test gate alone, the module holds fixtures.
        self.assertEqual(self.errors(), [])
        relative = guard.PRIVATE_TEST_MODULE.as_posix()
        for alias in (
            '#[path = "file_define/publication_failure_tests.rs"]\nmod exposed;\n',
            # An alias that never spells the file's name.
            '#[path = "file_define"]\nmod live {\n    mod publication_failure_tests;\n}\n',
        ):
            with self.subTest(alias=alias):
                self.write("rust/crates/yamaa-application/src/lib.rs", alias)
                self.assertEqual(
                    self.errors(),
                    [
                        f"{relative}:2: new diagnostic literal site outside core: "
                        "invalid_value, invalid_value",
                        f"{relative}:3: JSON text decoded outside a codec",
                        f"{relative}:4: unregistered condition literal not_registered",
                    ],
                )

    def test_private_unit_assertions_still_require_defined_requirements(self):
        self.publication_unit_module()
        self.write(
            str(guard.PRIVATE_TEST_MODULE),
            '#[test]\nfn independent_expectation() { assert_eq!("VALUE-999", "VALUE-999"); }\n',
        )
        self.assertReported("undefined rule VALUE-999")

    def card_file(self, fails="Fails: `invalid_value`.\n"):
        self.write(
            "rules/inheritance.md",
            '# Inheritance\n\n<a id="layer-1"></a>\n\n'
            "### LAYER-1. Layers compose in one order\n\n"
            f"Layers compose depth first.\n\n{fails}",
        )

    def test_registry_accepts_the_id_of_any_card(self):
        self.card_file()
        path = self.root / "rust/crates/yamaa-core/src/diagnostic/mod.rs"
        path.write_text(path.read_text().replace('Some("VALUE-1")', 'Some("LAYER-1")'))
        self.assertEqual(self.errors(), [])

    def test_card_cause_reports_a_condition_its_fails_line_names(self):
        path = self.root / "rust/crates/yamaa-core/src/diagnostic/mod.rs"
        path.write_text(path.read_text().replace('Some("VALUE-1")', 'Some("LAYER-1")'))
        for fails in ("", "Fails: `engine_rejected`.\n"):
            with self.subTest(fails=fails):
                self.card_file(fails)
                self.assertEqual(
                    self.errors(),
                    [
                        "core diagnostic registry:Invalid: LAYER-1 does not name "
                        "invalid_value in its Fails line"
                    ],
                )
        self.card_file("Fails: `engine_rejected`, `invalid_value`.\n")
        self.assertEqual(self.errors(), [])

    def test_a_retired_requirement_id_is_no_citation(self):
        path = self.root / "rust/crates/yamaa-core/src/diagnostic/mod.rs"
        path.write_text(path.read_text().replace('Some("VALUE-1")', 'Some("REQ-0001")'))
        self.assertEqual(
            self.errors(),
            [
                "core diagnostic registry: unrecognized cause: Invalid => "
                '("validation", "invalid_value", Some("REQ-0001")),'
            ],
        )

    def test_undefined_citation_is_reported_once(self):
        path = self.root / "rust/crates/yamaa-core/src/diagnostic/mod.rs"
        path.write_text(path.read_text().replace('Some("VALUE-1")', 'Some("VALUE-9")'))
        self.assertEqual(
            self.errors(),
            ["rust/crates/yamaa-core/src/diagnostic/mod.rs:2: undefined rule VALUE-9"],
        )

    def test_undefined_card_id_and_unknown_prefix_are_rejected(self):
        self.card_file()
        self.write(
            "rust/crates/yamaa-core/src/unknown.rs", 'const RULE: &str = "LAYER-99";\n'
        )
        self.assertReported("undefined rule LAYER-99")
        (self.root / "rust/crates/yamaa-core/src/unknown.rs").unlink()
        path = self.root / "rust/crates/yamaa-core/src/diagnostic/mod.rs"
        path.write_text(path.read_text().replace('Some("VALUE-1")', 'Some("RULE-1")'))
        self.assertReported("unrecognized cause")

    def test_missing_rules_and_sources_do_not_pass(self):
        (self.root / "rules/values.md").unlink()
        self.assertEqual(self.errors(), ["no rule definitions found"])
        self.write(
            "rules/values.md",
            '<a id="value-1"></a>\n\n### VALUE-1. Values are valid\n\nA value is valid.\n',
        )
        (self.root / "rust/crates/yamaa-core/src/diagnostic/mod.rs").unlink()
        self.assertEqual(self.errors(), ["no Rust crate sources found"])


if __name__ == "__main__":
    unittest.main()
