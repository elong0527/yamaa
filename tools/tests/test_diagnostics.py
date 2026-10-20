"""Diagnostic guard mutations preserve vocabulary and registry-owned literals."""

import tempfile
import unittest
from pathlib import Path

from tools.validation import check_diagnostics as guard


class DiagnosticGuardTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)
        # The registry cites REQ-0001, which this card replaces.
        self.write(
            "rules/values.md",
            '# Values\n\n<a id="value-1"></a>\n\n### VALUE-1. Values are valid\n\n'
            "A value is valid.\n\nFails: `invalid_value`.\nReplaces: REQ-0001.\n",
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
            'conditions! {\n    Invalid => ("validation", "invalid_value", Some("REQ-0001")),\n'
            '    Rejected => ("engine", "engine_rejected", None),\n}\n',
        )

    def write(self, relative, text):
        path = self.root / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text, encoding="utf-8")
        return path

    def errors(self):
        return guard.violations(self.root)

    def test_registered_core_mapping_passes(self):
        self.assertEqual(self.errors(), [])

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
        path.write_text(path.read_text().replace('Some("REQ-0001")', "None"))
        self.assertTrue(
            any(
                "null requirement is not normatively permitted" in e
                for e in self.errors()
            )
        )

    def test_implementation_condition_must_retain_null(self):
        path = self.root / "rust/crates/yamaa-core/src/diagnostic/mod.rs"
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
        for crate in ("yamaa-core", *sorted(guard.HOST_CRATES)):
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

    def test_generated_schema_is_exempt_from_ownership_but_not_normative_ids(self):
        path = self.write(
            "rust/crates/yamaa-adapters/src/shipped_schema/data.rs",
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
        path = self.root / "rust/crates/yamaa-core/src/diagnostic/mod.rs"
        path.write_text("conditions! {\n    malformed => (),\n}\n")
        self.assertTrue(any("unrecognized cause" in e for e in self.errors()))
        path.unlink()
        self.write("rust/crates/yamaa-core/src/marker.rs", "// engine source\n")
        self.assertTrue(any("unavailable core registry" in e for e in self.errors()))
        (self.root / "yaml/conditions.yaml").unlink()
        self.assertTrue(any("unavailable vocabulary" in e for e in self.errors()))

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
            "rust/crates/yamaa-core/src/diagnostic.rs",
            'const REQ: &str = "REQ-0001";\n',
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
            "        requirement: None,\n"
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
            '        requirement: text(failure, "condition"),\n'
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

    def test_serde_field_deserializers_are_code_but_ordinary_strings_are_not(self):
        sources = [
            '#[serde(deserialize_with = "crate::json_wire::deserialize_value")]\nvalue: Value,',
            'use crate::json_wire as wire;\n#[serde(deserialize_with = "wire::deserialize_value")]\nvalue: Value,',
            'use serde_json as json;\n#[serde(deserialize_with = "json::Value::deserialize")]\nvalue: Value,',
            '#[serde(deserialize_with = r#"crate :: json_wire :: deserialize_value"#)]\nvalue: Value,',
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

    def test_generated_schema_and_private_unit_tests_hold_no_condition_sites(self):
        self.write(
            str(guard.SHIPPED_SCHEMA),
            'const DATA: &str = r#"{"condition":"cancelled"}"#;\n',
        )
        self.publication_unit_module()
        self.write(
            str(guard.PRIVATE_TEST_MODULE),
            '#[test]\nfn probe() { let row = Issue { condition: "probe".into() }; }\n',
        )
        self.assertEqual(self.errors(), [])

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

    def publication_unit_module(self):
        self.write(
            "rust/crates/yamaa-adapters/src/file_define/mod.rs",
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
            "rust/crates/yamaa-adapters/src/file_define/mod.rs",
            "mod publication_failure_tests;\n",
        )
        self.assertTrue(any("new diagnostic literal site" in e for e in self.errors()))

    def test_unit_source_reused_by_production_alias_remains_production(self):
        self.publication_unit_module()
        self.write(
            "rust/crates/yamaa-adapters/src/lib.rs",
            '#[path = "file_define/publication_failure_tests.rs"]\nmod exposed;\n',
        )
        self.assertTrue(any("new diagnostic literal site" in e for e in self.errors()))

    def test_private_unit_assertions_still_require_defined_requirements(self):
        self.publication_unit_module()
        self.write(
            str(guard.PRIVATE_TEST_MODULE),
            '#[test]\nfn independent_expectation() { assert_eq!("REQ-999999", "REQ-999999"); }\n',
        )
        self.assertTrue(
            any("undefined requirement REQ-999999" in e for e in self.errors())
        )

    def card_file(self, fails="Fails: `invalid_value`.\n"):
        self.write(
            "rules/inheritance.md",
            '# Inheritance\n\n<a id="layer-1"></a>\n\n'
            "### LAYER-1. Layers compose in one order\n\n"
            f"Layers compose depth first.\n\n{fails}Replaces: REQ-0002.\n",
        )

    def test_registry_accepts_card_and_replaced_requirement_ids(self):
        self.card_file()
        path = self.root / "rust/crates/yamaa-core/src/diagnostic/mod.rs"
        for requirement in ("LAYER-1", "REQ-0002"):
            with self.subTest(requirement=requirement):
                path.write_text(
                    path.read_text().replace(
                        'Some("REQ-0001")', f'Some("{requirement}")'
                    )
                )
                self.assertEqual(self.errors(), [])
                path.write_text(
                    path.read_text().replace(
                        f'Some("{requirement}")', 'Some("REQ-0001")'
                    )
                )

    def test_card_cause_reports_a_condition_its_fails_line_names(self):
        path = self.root / "rust/crates/yamaa-core/src/diagnostic/mod.rs"
        path.write_text(path.read_text().replace('Some("REQ-0001")', 'Some("LAYER-1")'))
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

    def test_requirement_cause_reports_a_condition_its_replacing_card_names(self):
        path = self.root / "rust/crates/yamaa-core/src/diagnostic/mod.rs"
        path.write_text(
            path.read_text().replace('Some("REQ-0001")', 'Some("REQ-0002")')
        )
        for fails in ("", "Fails: `engine_rejected`.\n"):
            with self.subTest(fails=fails):
                self.card_file(fails)
                self.assertEqual(
                    self.errors(),
                    [
                        "core diagnostic registry:Invalid: REQ-0002 is replaced by "
                        "LAYER-1, which does not name invalid_value in its Fails line"
                    ],
                )
        self.card_file("Fails: `engine_rejected`, `invalid_value`.\n")
        self.assertEqual(self.errors(), [])

    def test_legacy_requirement_cannot_be_cited(self):
        self.write("rules/values.md", "**REQ-0001.** A normative requirement.\n")
        self.assertEqual(
            self.errors(),
            [
                "core diagnostic registry:Invalid: REQ-0001 is a legacy requirement "
                "with no Fails line naming invalid_value"
            ],
        )

    def test_undefined_citation_is_reported_once(self):
        path = self.root / "rust/crates/yamaa-core/src/diagnostic/mod.rs"
        path.write_text(
            path.read_text().replace('Some("REQ-0001")', 'Some("REQ-0009")')
        )
        self.assertEqual(
            self.errors(),
            [
                "rust/crates/yamaa-core/src/diagnostic/mod.rs:2: "
                "undefined requirement REQ-0009"
            ],
        )

    def test_undefined_card_id_and_unknown_prefix_are_rejected(self):
        self.card_file()
        self.write(
            "rust/crates/yamaa-core/src/unknown.rs", 'const RULE: &str = "LAYER-99";\n'
        )
        self.assertTrue(
            any("undefined requirement LAYER-99" in e for e in self.errors())
        )
        (self.root / "rust/crates/yamaa-core/src/unknown.rs").unlink()
        path = self.root / "rust/crates/yamaa-core/src/diagnostic/mod.rs"
        path.write_text(path.read_text().replace('Some("REQ-0001")', 'Some("RULE-1")'))
        self.assertTrue(any("unrecognized cause" in e for e in self.errors()))

    def test_missing_rules_and_sources_do_not_pass(self):
        (self.root / "rules/values.md").unlink()
        self.assertEqual(self.errors(), ["no rule definitions found"])
        self.write("rules/values.md", "**REQ-0001.** Required.\n")
        (self.root / "rust/crates/yamaa-core/src/diagnostic/mod.rs").unlink()
        self.assertEqual(self.errors(), ["no Rust crate sources found"])


if __name__ == "__main__":
    unittest.main()
