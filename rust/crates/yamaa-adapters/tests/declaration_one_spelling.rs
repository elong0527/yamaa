#![cfg(feature = "yaml")]
//! One spelling per meaning: lists, shorthand and bare source references
//! through the shipped schema, before any study data is read.
mod declaration {
    mod one_spelling {
        use crate::support::Edit as _;
        use yamaa_adapters::json;
        use yamaa_adapters::{
            shipped_schema, specification_diagnostics::capture_failure,
            specification_source::Source,
        };
        use yamaa_core::json::Value;

        const HEAD: &str = "schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\nbase: DM\noutput: {path: out.csv, columns: [ID, V]}\n";
        const INPUT: &str = "input: {DM: dm.csv, EX: ex.csv}\n";

        /// The ordered diagnostics of a standalone preparation, empty when it succeeds.
        fn diagnostics(text: &str) -> Vec<Value> {
            let schema = shipped_schema::capture().unwrap();
            let Err(error) = schema.prepare_standalone(Source {
                identity: "spec.yaml".into(),
                bytes: text.as_bytes().to_vec(),
            }) else {
                return Vec::new();
            };
            match capture_failure(&error) {
                yamaa_adapters::specification_diagnostics::Rejection::Invalid(findings) => findings
                    .iter()
                    .map(yamaa_adapters::report_storage::diagnostic_value)
                    .collect(),
                other => panic!("expected diagnostics, got {other:?}"),
            }
        }
        fn column(derivation: &str) -> String {
            format!("{HEAD}{INPUT}columns:\n  - {{name: ID, type: str, derivation: DM.ID}}\n  - name: V\n    type: int\n    derivation: {derivation}\n")
        }
        fn restated(path: &str, field: &str, shorthand: Value) -> Value {
            json!({
                "phase": "validation", "condition": "redundant_long_form",
                "rule": "SCHEMA-15", "spec_paths": [path],
                "context": {"field": field, "shorthand": shorthand},
            })
        }

        #[test]
        fn a_long_form_that_only_restates_its_shorthand_is_refused_where_written() {
            for (derivation, path, field, shorthand) in [
                (
                    "{aggregate: {expr: 'COUNT(EX.*)'}}",
                    "columns.V.derivation.aggregate",
                    "expr",
                    json!("COUNT(EX.*)"),
                ),
                (
                    "{source: DM.V}",
                    "columns.V.derivation",
                    "source",
                    json!("DM.V"),
                ),
                (
                    "{source: {variable: DM.V}}",
                    "columns.V.derivation.source",
                    "variable",
                    json!("DM.V"),
                ),
                (
                    "{value: {source: DM.V}}",
                    "columns.V.derivation",
                    "value",
                    json!("DM.V"),
                ),
                (
                    "{value: {literal: 1}}",
                    "columns.V.derivation",
                    "value",
                    json!("{\"literal\": 1}"),
                ),
                (
                    "{flag: {condition: 'DM.V > 1'}}",
                    "columns.V.derivation.flag",
                    "condition",
                    json!("DM.V > 1"),
                ),
                (
                    "{str_template: {template: '{DM.ID}'}}",
                    "columns.V.derivation.str_template",
                    "template",
                    json!("{DM.ID}"),
                ),
                (
                    "{odm: {item: DM.IT.SEX}}",
                    "columns.V.derivation.odm",
                    "item",
                    json!("DM.IT.SEX"),
                ),
                (
                    "{case: [{when: 'TRUE', then: {source: DM.V}}]}",
                    "columns.V.derivation.case[0].then",
                    "source",
                    json!("DM.V"),
                ),
                (
                    "{first_available: {sources: [{variable: DM.V}]}}",
                    "columns.V.derivation.first_available.sources[0]",
                    "variable",
                    json!("DM.V"),
                ),
                (
                    "{row_number: {window: {order_by: [{variable: DM.ID}]}}}",
                    "columns.V.derivation.row_number.window.order_by[0]",
                    "variable",
                    json!("DM.ID"),
                ),
            ] {
                assert_eq!(
                    diagnostics(&column(derivation)),
                    [restated(path, field, shorthand)],
                    "{derivation}"
                );
            }
            let valid = column("DM.V");
            for (text, path, field, shorthand) in [
                (
                    valid.edit("DM: dm.csv", "DM: {path: dm.csv}"),
                    "input.DM",
                    "path",
                    json!("dm.csv"),
                ),
                (
                    format!("{valid}verifications: [{{unique: {{columns: [V]}}}}]\n"),
                    "verifications[0].unique",
                    "columns",
                    json!("[\"V\"]"),
                ),
                (
                    valid.edit(
                        "derivation: DM.V\n",
                        "derivation: DM.V\n    submission: {comment: {text: Collected}}\n",
                    ),
                    "columns.V.submission.comment",
                    "text",
                    json!("Collected"),
                ),
                (
                    format!("{valid}intermediates:\n  - id: P\n    dataset: EX\n    key: [ID]\n    derivations: {{N: {{source: EX.N}}}}\n    no_match: {{literal: null}}\n"),
                    "intermediates.P.derivations.N",
                    "source",
                    json!("EX.N"),
                ),
                (
                    format!("{valid}intermediates:\n  - id: P\n    dataset: EX\n    key: [ID]\n    derivations: {{N: {{type: int, derivation: {{value: {{source: EX.N}}}}}}}}\n    no_match: {{literal: null}}\n"),
                    "intermediates.P.derivations.N.derivation",
                    "value",
                    json!("EX.N"),
                ),
            ] {
                assert_eq!(diagnostics(&text), [restated(path, field, shorthand)], "{text}");
            }
        }

        #[test]
        fn a_long_form_that_adds_a_field_keeps_its_meaning() {
            for derivation in [
                "{aggregate: {expr: 'COUNT(EX.*)', key: [ID]}}",
                "{source: {variable: DM.V, absent: 0}}",
                "{value: {source: DM.V}, unconvertible: 0}",
                // An explicit default and an explicit null are both written.
                "{flag: {condition: 'DM.V > 1', true_value: 'Y'}}",
                "{flag: {condition: 'DM.V > 1', false_value: null}}",
                "{str_template: {template: '{DM.ID}', missing: {literal: null}}}",
                "{row_number: {window: {order_by: [{variable: DM.ID, direction: desc}]}}}",
            ] {
                assert_eq!(
                    diagnostics(&column(derivation)),
                    [] as [Value; 0],
                    "{derivation}"
                );
            }
            let valid = column("DM.V");
            for text in [
                valid.edit("DM: dm.csv", "DM: {path: dm.csv, types: {V: int}}"),
                format!("{valid}verifications: [{{unique: {{columns: [V], id: one-v}}}}]\n"),
            ] {
                assert_eq!(diagnostics(&text), [] as [Value; 0], "{text}");
            }
        }

        #[test]
        fn a_list_field_takes_a_list_even_for_one_value() {
            let valid = column("DM.V");
            for (text, path, expected) in [
                (
                    valid.edit(
                        "derivation: DM.V\n",
                        "derivation: DM.V\n    verifications: {not_missing: {}}\n",
                    ),
                    "columns.V.verifications",
                    "list",
                ),
                (
                    format!("{valid}verifications: {{row_count: {{min: 1}}}}\n"),
                    "verifications",
                    "list",
                ),
                (
                    format!("{valid}intermediates:\n  - {{id: P, dataset: EX, key: ID, no_match: {{literal: null}}}}\n"),
                    "intermediates.P.key",
                    "match_key",
                ),
                (
                    column("{aggregate: {expr: 'SUM(EX.N)', key: ID}}"),
                    "columns.V.derivation.aggregate.key",
                    "match_key",
                ),
                (
                    column("{odm: {item: DM.IT.SEX, event: SCREENING}}"),
                    "columns.V.derivation.odm.event",
                    "list",
                ),
            ] {
                let found = diagnostics(&text);
                assert_eq!(found.len(), 1, "{text}: {found:?}");
                assert_eq!(found[0]["condition"], json!("invalid_field_type"), "{text}");
                assert_eq!(found[0]["spec_paths"], json!([path]), "{text}");
                assert_eq!(found[0]["context"]["expected"], json!(expected), "{text}");
            }
        }

        struct ParentSource(Vec<u8>);
        impl yamaa_adapters::specification_source::InheritancePort for ParentSource {
            type Error = ();
            fn canonicalize(
                &mut self,
                _: &str,
                _: &str,
            ) -> Result<yamaa_core::inheritance::Source, yamaa_core::inheritance::SourceError<()>>
            {
                Ok(yamaa_core::inheritance::Source {
                    identity: "parent.yaml".into(),
                    display_path: "parent.yaml".into(),
                })
            }
            fn capture(
                &mut self,
                _: &yamaa_core::inheritance::Source,
            ) -> Result<Vec<u8>, yamaa_core::inheritance::SourceError<()>> {
                Ok(self.0.clone())
            }
            fn rebase(
                &mut self,
                _: &yamaa_core::inheritance::Source,
                _: &yamaa_core::inheritance::Source,
                written: &str,
            ) -> Result<String, ()> {
                Ok(written.into())
            }
        }
        fn inherited(entry: &str, parent: &str) -> Vec<Value> {
            let Err(error) = shipped_schema::prepare(
                Source {
                    identity: "entry.yaml".into(),
                    bytes: entry.as_bytes().to_vec(),
                },
                "entry.yaml".into(),
                &mut ParentSource(parent.as_bytes().to_vec()),
            ) else {
                return Vec::new();
            };
            match yamaa_adapters::specification_diagnostics::inheritance_failure_ref(&error)
                .unwrap()
            {
                yamaa_adapters::specification_diagnostics::Rejection::Invalid(findings) => findings
                    .iter()
                    .map(yamaa_adapters::report_storage::diagnostic_value)
                    .collect(),
                other => panic!("expected diagnostics, got {other:?}"),
            }
        }

        #[test]
        fn every_layer_is_held_to_the_shorthand_it_writes() {
            let entry = format!("{HEAD}parent: parent.yaml\n");
            let parent = |columns: &str| {
                format!("schema_version: '1.0'\n{INPUT}columns:\n  - {{name: ID, type: str, derivation: DM.ID}}\n{columns}")
            };
            // The parent's canonical expansion is composed and validated again,
            // and only what a layer wrote is held to its shorthand.
            assert_eq!(
                inherited(
                    &entry,
                    &parent("  - {name: V, type: int, derivation: {flag: 'DM.V > 1'}}\n")
                ),
                [] as [Value; 0]
            );
            let found = inherited(
                &entry,
                &parent("  - {name: V, type: int, derivation: {flag: {condition: 'DM.V > 1'}}}\n"),
            );
            assert_eq!(found.len(), 1, "{found:?}");
            assert_eq!(found[0]["condition"], json!("redundant_long_form"));
            assert_eq!(found[0]["spec_paths"], json!(["columns.V.derivation.flag"]));
            assert_eq!(found[0]["context"]["source"], json!("parent.yaml"));
            // A child replacing an inherited dataset path writes the bare path;
            // a mapping holding only that path restates it.
            let child = |input: &str| format!("{entry}input: {input}\n");
            let parent = parent("  - {name: V, type: int, derivation: DM.V}\n")
                .edit("DM: dm.csv", "DM: {path: dm.csv, types: {V: int}}");
            assert_eq!(
                inherited(&child("{DM: other.csv}"), &parent),
                [] as [Value; 0]
            );
            let found = inherited(&child("{DM: {path: other.csv}}"), &parent);
            assert_eq!(found.len(), 1, "{found:?}");
            assert_eq!(found[0]["condition"], json!("redundant_long_form"));
            assert_eq!(found[0]["spec_paths"], json!(["input.DM"]));
            assert_eq!(found[0]["context"]["source"], json!("entry.yaml"));
        }

        #[test]
        fn a_bare_derivation_string_is_only_a_source_reference() {
            // Text spelled like a call is not dispatched to the operation it
            // names; it fails as a source variable name (EXPR-5).
            for derivation in ["to_date(DM.D)", "'upper(DM.ID)'", "'DM.V + 1'"] {
                let found = diagnostics(&column(derivation));
                assert_eq!(found.len(), 1, "{derivation}: {found:?}");
                assert_eq!(
                    found[0]["condition"],
                    json!("pattern_mismatch"),
                    "{derivation}"
                );
                assert_eq!(
                    found[0]["spec_paths"],
                    json!(["columns.V.derivation.source"]),
                    "{derivation}"
                );
            }
        }
    }
}
