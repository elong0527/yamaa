#![cfg(feature = "yaml")]
//! Execution contracts: declarative SELF, declarative source selection,
//! diagnostic sites, empty child condition, inherited aggregate bindings,
//! inherited identifier collision, inherited intermediate verification,
//! inherited lookup keys, integer literal overflow, intermediate dependencies,
//! intermediate row drivers, key expressions, namespace admission, nested
//! windows, original named windows, original verification, predicate
//! suggestions, missing order and baseline operands.

mod execution {
    mod declarative_self {
        //! Independent SELF witnesses preserve phase context, donor order and complete outputs.
        use crate::support::Edit as _;
        use std::fs;
        use yamaa_adapters::file_application;
        use yamaa_core::domain_entry::Request;
        struct Fixture(crate::support::Scratch);
        impl Fixture {
            fn new() -> Self {
                let path = crate::support::Scratch::new("self");
                Self(path)
            }
            fn run(
                &self,
                text: &str,
            ) -> Result<file_application::Domain, Vec<yamaa_core::diagnostic::Diagnostic>>
            {
                let specification = self.0.join("spec.yaml");
                fs::write(&specification, text).unwrap();
                let specification = specification.to_str().unwrap();
                crate::support::study::domain(
                    yamaa_core::project_function::Language::Python,
                    Request {
                        specification,
                        environment: None,
                    },
                    crate::support::study::identity("independent-self", specification),
                )
            }
        }
        use crate::support::study::issues;
        const BODY: &str = r#"yamaa: "1.0"
type: spec
domain: TEST
input:
  SRC: src.csv
  OTHER: other.csv
keys: [ID, SECTION]
output: {path: result.csv, columns: [ID, SECTION, K, SEQ, V, FINAL, FIRST]}
intermediates:
  - id: P
    dataset: SELF
    key: {K: K}
    order_by: [SELF.SEQ]
    keep: last
    absent: 0
  - id: F
    dataset: SELF
    key: [K]
    filter: "SECTION = 'first'"
    order_by: [SEQ]
    keep: last
columns:
  - {name: ID, type: str, derivation: SRC.ID}
  - {name: SECTION, type: str}
  - {name: K, type: str, derivation: SRC.K}
  - {name: SEQ, type: int, derivation: SRC.SEQ}
  - {name: V, type: int, derivation: SRC.V}
  - {name: FINAL, type: int, derivation: P.V}
  - {name: FIRST, type: int, derivation: F.V}
rows:
  - id: collected
    dataset: SRC
    derivations:
      SECTION: "'first'"
  - id: grouped
    dataset: SRC
    group_by: [SRC.K]
    derivations:
      ID: "'g'"
      SECTION: "'second'"
      SEQ: 4
      V: "P.V + 10"
  - id: added
    dataset: OTHER
    derivations:
      ID: OTHER.ID
      SECTION: "'third'"
      K: OTHER.K
      SEQ: 9
      V: "P.V + 100"
"#;
        #[test]
        fn completed_templates_are_borrowed_and_republished_in_construction_order() {
            let fixture = Fixture::new();
            fs::write(fixture.0.join("src.csv"), "ID,K,SEQ,V\na,x,2,5\nb,x,1,9\n").unwrap();
            fs::write(fixture.0.join("other.csv"), "ID,K\nc,x\nd,x\n").unwrap();
            let mut result = fixture.run(BODY).unwrap();
            assert!(result.issues().is_empty(), "{:?}", result.issues());
            assert!(result.save().unwrap());
            assert_eq!(fs::read_to_string(fixture.0.join("result.csv")).unwrap(), "ID,SECTION,K,SEQ,V,FINAL,FIRST\na,first,x,2,5,115,5\nb,first,x,1,9,115,5\ng,second,x,4,15,115,5\nc,third,x,9,115,115,5\nd,third,x,9,115,115,5\n");
            assert_eq!(
                result.observations()["handler_counts"],
                yamaa_adapters::json!([
                    {"spec_path":"rows.grouped.derivations.V.multiple_matches","handler":"multiple_matches","count":1},
                    {"spec_path":"rows.grouped.derivations.V.absent","handler":"absent","count":0},
                    {"spec_path":"rows.added.derivations.V.multiple_matches","handler":"multiple_matches","count":2},
                    {"spec_path":"rows.added.derivations.V.absent","handler":"absent","count":0},
                    {"spec_path":"columns.FINAL.derivation.source.multiple_matches","handler":"multiple_matches","count":5},
                    {"spec_path":"columns.FINAL.derivation.source.absent","handler":"absent","count":0},
                    {"spec_path":"columns.FIRST.derivation.source.multiple_matches","handler":"multiple_matches","count":5},
                    {"spec_path":"columns.FIRST.derivation.source.absent","handler":"absent","count":0},
                ])
            );
            // The two third-template records both see completed value 15, never 115
            // from the first unfinished third-template record. The column sees all five.
        }
        #[test]
        fn first_template_read_preserves_full_phase_context_before_study_access() {
            let fixture = Fixture::new();
            let body = BODY.edit("SECTION: \"'first'\"", "SECTION: P.K");
            assert_eq!(
                issues(fixture.run(&body)),
                vec![yamaa_adapters::json!({
                    "phase":"validation", "condition":"phase_boundary", "rule":"LOOKUP-7",
                    "spec_paths":["rows.collected.derivations.SECTION.source"],
                    "context":{"identifier":"P.K","available_phase":"completed_prior_row_template","required_phase":"row_construction"}
                })]
            );
            assert!(!fixture.0.join("src.csv").exists());
        }
        #[test]
        fn column_only_donor_is_unavailable_to_later_templates() {
            let fixture = Fixture::new();
            fs::write(fixture.0.join("src.csv"), "ID,K,SEQ,V\na,x,2,5\n").unwrap();
            fs::write(fixture.0.join("other.csv"), "ID,K\nc,x\n").unwrap();
            let body = BODY.edit("V: \"P.V + 10\"", "V: \"P.FINAL + 10\"");
            let found = issues(fixture.run(&body));
            assert_eq!(
                found,
                vec![yamaa_adapters::json!({
                    "phase":"validation", "condition":"unknown_field", "rule":"LOOKUP-11",
                    "spec_paths":["rows.grouped.derivations.V"], "context":{"identifier":"P.FINAL"}
                })]
            );
        }
        #[test]
        fn empty_completed_template_uses_literal_absence_in_later_templates() {
            let fixture = Fixture::new();
            fs::write(fixture.0.join("src.csv"), "ID,K,SEQ,V\na,x,2,5\nb,x,1,9\n").unwrap();
            fs::write(fixture.0.join("other.csv"), "ID,K\nc,x\nd,x\n").unwrap();
            let body = BODY.edit(
                "id: collected\n    dataset: SRC",
                "id: collected\n    dataset: SRC\n    filter: \"SRC.ID = 'absent'\"",
            );
            let mut result = fixture.run(&body).unwrap();
            assert!(result.issues().is_empty(), "{:?}", result.issues());
            assert!(result.save().unwrap());
            assert_eq!(fs::read_to_string(fixture.0.join("result.csv")).unwrap(), "ID,SECTION,K,SEQ,V,FINAL,FIRST\ng,second,x,4,10,110,\nc,third,x,9,110,110,\nd,third,x,9,110,110,\n");
            assert_eq!(
                result.observations()["handler_counts"],
                yamaa_adapters::json!([
                    {"spec_path":"rows.grouped.derivations.V.multiple_matches","handler":"multiple_matches","count":0},
                    {"spec_path":"rows.grouped.derivations.V.absent","handler":"absent","count":1},
                    {"spec_path":"rows.added.derivations.V.multiple_matches","handler":"multiple_matches","count":0},
                    {"spec_path":"rows.added.derivations.V.absent","handler":"absent","count":0},
                    {"spec_path":"columns.FINAL.derivation.source.multiple_matches","handler":"multiple_matches","count":3},
                    {"spec_path":"columns.FINAL.derivation.source.absent","handler":"absent","count":0},
                    {"spec_path":"columns.FIRST.derivation.source.multiple_matches","handler":"multiple_matches","count":0},
                    {"spec_path":"columns.FIRST.derivation.source.absent","handler":"absent","count":3},
                ])
            );
        }

        #[test]
        fn reserved_input_collision_and_missing_row_phase_are_refused_before_study_access() {
            let fixture = Fixture::new();
            let collision = BODY.edit("  OTHER: other.csv", "  OTHER: other.csv\n  SELF: self.csv");
            assert_eq!(
                issues(fixture.run(&collision)),
                vec![
                    yamaa_adapters::json!({"phase":"validation", "condition":"duplicate_identifier", "rule":"LOOKUP-3", "spec_paths":["intermediates.P.dataset","input.SELF"], "context":{"identifier":"SELF"}}),
                    yamaa_adapters::json!({"phase":"validation", "condition":"duplicate_identifier", "rule":"LOOKUP-3", "spec_paths":["intermediates.F.dataset","input.SELF"], "context":{"identifier":"SELF"}}),
                ]
            );
            let missing_rows = BODY
                .split("rows:\n")
                .next()
                .unwrap()
                .edit("domain: TEST", "domain: TEST\nbase: SRC")
                .edit(
                    "  - {name: SECTION, type: str}",
                    "  - {name: SECTION, type: str, derivation: \"'first'\"}",
                );
            assert_eq!(
                issues(fixture.run(&missing_rows)),
                vec![
                    yamaa_adapters::json!({"phase":"validation", "condition":"unknown_field", "rule":"LOOKUP-7", "spec_paths":["intermediates.P.dataset"], "context":{"identifier":"SELF","intermediate":"P"}}),
                    yamaa_adapters::json!({"phase":"validation", "condition":"unknown_field", "rule":"LOOKUP-7", "spec_paths":["intermediates.F.dataset"], "context":{"identifier":"SELF","intermediate":"F"}}),
                ]
            );
            assert!(!fixture.0.join("src.csv").exists());
        }

        #[test]
        fn hidden_first_template_alternative_preserves_its_authored_phase_path() {
            let fixture = Fixture::new();
            let body = BODY.edit("SECTION: \"'first'\"", "SECTION: {case: [{when: \"SRC.ID = 'absent'\", then: P.K}, {otherwise: \"'first'\"}]}");
            assert_eq!(
                issues(fixture.run(&body)),
                vec![yamaa_adapters::json!({
                    "phase":"validation", "condition":"phase_boundary", "rule":"LOOKUP-7",
                    "spec_paths":["rows.collected.derivations.SECTION.case[0].then.source"],
                    "context":{"identifier":"P.K","available_phase":"completed_prior_row_template","required_phase":"row_construction"}
                })]
            );
        }

        #[test]
        fn hidden_first_template_first_available_source_preserves_its_authored_phase_path() {
            let fixture = Fixture::new();
            let body = BODY.edit(
                "SECTION: \"'first'\"",
                "SECTION: {first_available: {sources: [P.K], missing: first}}",
            );
            assert_eq!(
                issues(fixture.run(&body)),
                vec![yamaa_adapters::json!({
                    "phase":"validation", "condition":"phase_boundary", "rule":"LOOKUP-7",
                    "spec_paths":["rows.collected.derivations.SECTION.first_available.sources[0]"],
                    "context":{"identifier":"P.K","available_phase":"completed_prior_row_template","required_phase":"row_construction"}
                })]
            );
        }

        #[test]
        fn aggregate_first_template_proof_keeps_full_references_including_record_star() {
            for (replacement, identifier, path) in [
                (
                    "SECTION: {aggregate: 'SUM(P.V)'}",
                    "P.V",
                    "rows.collected.derivations.SECTION.aggregate",
                ),
                (
                    "SECTION: {aggregate: 'COUNT(P.*)'}",
                    "P.*",
                    "rows.collected.derivations.SECTION.aggregate",
                ),
                (
                    "SECTION: {case: [{when: 'TRUE', then: {aggregate: 'COUNT(P.*)'}}]}",
                    "P.*",
                    "rows.collected.derivations.SECTION.case[0].then.aggregate",
                ),
            ] {
                let fixture = Fixture::new();
                let body = BODY.edit("SECTION: \"'first'\"", replacement);
                assert_eq!(
                    issues(fixture.run(&body)),
                    [yamaa_adapters::json!({
                        "phase":"validation", "condition":"phase_boundary", "rule":"LOOKUP-7",
                        "spec_paths":[path], "context":{"identifier":identifier,
                            "available_phase":"completed_prior_row_template", "required_phase":"row_construction"}
                    })]
                );
                assert!(!fixture.0.join("src.csv").exists());
                assert!(!fixture.0.join("other.csv").exists());
            }
        }

        #[test]
        fn donor_qualified_global_aggregate_does_not_require_the_primary_row_driver() {
            let fixture = Fixture::new();
            fs::write(fixture.0.join("src.csv"), "ID\n99\n").unwrap();
            fs::write(fixture.0.join("alt.csv"), "ID\n1\n2\n").unwrap();
            fs::write(fixture.0.join("donor.csv"), "ID,V\n1,4\n1,7\n2,3\n").unwrap();
            let body = r#"yamaa: "1.0"
type: spec
domain: TEST
base: SRC
keys: [ID]
input:
  SRC: {path: src.csv, types: {ID: int}}
  ALT: {path: alt.csv, types: {ID: int}}
  D: {path: donor.csv, key: [ID], types: {ID: int, V: int}}
output: {path: result.csv, columns: [ID, TOTAL]}
rows:
  - id: alternate
    dataset: ALT
    derivations: {ID: ALT.ID}
columns:
  - {name: ID, type: int}
  - {name: TOTAL, type: int, derivation: {aggregate: 'SUM(D.V)'}}
"#;
            let mut result = fixture.run(body).unwrap();
            assert!(result.issues().is_empty(), "{:?}", result.issues());
            assert!(result.save().unwrap());
            assert_eq!(
                fs::read_to_string(fixture.0.join("result.csv")).unwrap(),
                "ID,TOTAL\n1,11\n2,3\n"
            );
            assert_eq!(
                result.observations()["handler_counts"],
                yamaa_adapters::json!([])
            );
        }

        #[test]
        fn mapping_and_cut_read_completed_self_donor_types() {
            for operation in [
                "mapping: {source: P.K, dict: {x: 15}}",
                "cut: {source: P.V, breaks: [10], labels: ['15', '25']}",
            ] {
                let fixture = Fixture::new();
                fs::write(fixture.0.join("src.csv"), "ID,K,SEQ,V\na,x,2,5\nb,x,1,9\n").unwrap();
                fs::write(fixture.0.join("other.csv"), "ID,K\nc,x\nd,x\n").unwrap();
                let body = BODY.edit("V: \"P.V + 10\"", &format!("V: {{{operation}}}"));
                let mut result = fixture.run(&body).unwrap();
                assert!(result.issues().is_empty(), "{:?}", result.issues());
                assert!(result.save().unwrap());
                assert_eq!(
            fs::read_to_string(fixture.0.join("result.csv")).unwrap(),
            "ID,SECTION,K,SEQ,V,FINAL,FIRST\na,first,x,2,5,115,5\nb,first,x,1,9,115,5\ng,second,x,4,15,115,5\nc,third,x,9,115,115,5\nd,third,x,9,115,115,5\n",
            "{operation}",
        );
            }
        }

        #[test]
        fn first_template_mapping_and_cut_self_reads_fail_before_study_access() {
            for operation in [
                "mapping: {source: P.K, dict: {x: first}}",
                "cut: {source: P.V, breaks: [10], labels: [first, later]}",
            ] {
                let fixture = Fixture::new();
                let body = BODY.edit("SECTION: \"'first'\"", &format!("SECTION: {{{operation}}}"));
                let operation_name = operation.split(':').next().unwrap();
                let identifier = if operation_name == "mapping" {
                    "P.K"
                } else {
                    "P.V"
                };
                assert_eq!(
                    issues(fixture.run(&body)),
                    vec![yamaa_adapters::json!({
                        "phase":"validation", "condition":"phase_boundary", "rule":"LOOKUP-7",
                        "spec_paths":[format!("rows.collected.derivations.SECTION.{operation_name}")],
                        "context":{"identifier":identifier,"available_phase":"completed_prior_row_template","required_phase":"row_construction"}
                    })],
                );
                assert!(!fixture.0.join("src.csv").exists());
            }
        }

        /// W's `between` value reads P, and W2's reads W, so reading either
        /// window reads the completed rows P selects from.
        const WINDOWS: &str = r#"yamaa: "1.0"
type: spec
domain: TEST
input:
  SRC: {path: src.csv, types: {DAY: int}}
  WIN: {path: win.csv, types: {LO: int, HI: int}}
keys: [ID, SECTION, DAY]
output: {path: result.csv, columns: [ID, SECTION, DAY, WINDOW]}
intermediates:
  - id: P
    dataset: SELF
    key: [ID]
    order_by: [SELF.DAY]
    keep: last
  - id: W
    dataset: WIN
    key: [ID]
    between: {value: P.DAY, lower: LO, upper: HI}
  - id: W2
    dataset: WIN
    key: [ID]
    between: {value: W.HI, lower: LO, upper: HI}
columns:
  - {name: ID, type: str, derivation: SRC.ID}
  - {name: SECTION, type: str}
  - {name: DAY, type: int, derivation: SRC.DAY}
  - {name: WINDOW, type: str}
rows:
  - id: first
    dataset: SRC
    derivations: {SECTION: "'first'", WINDOW: "'none'"}
  - id: second
    dataset: SRC
    derivations: {SECTION: "'second'", WINDOW: W2.NAME}
"#;
        #[test]
        fn first_template_reads_through_between_values_fail_before_study_access() {
            for (window, paths) in [
                (
                    "W.NAME",
                    vec![
                        "rows.first.derivations.WINDOW.source",
                        "intermediates.W.between.value",
                    ],
                ),
                (
                    "W2.NAME",
                    vec![
                        "rows.first.derivations.WINDOW.source",
                        "intermediates.W2.between.value",
                        "intermediates.W.between.value",
                    ],
                ),
                (
                    "{case: [{when: \"W.NAME = 'late'\", then: \"'late'\"}, {otherwise: \"'none'\"}]}",
                    vec![
                        "rows.first.derivations.WINDOW.case[0].when",
                        "intermediates.W.between.value",
                    ],
                ),
            ] {
                let fixture = Fixture::new();
                let body = WINDOWS.edit("WINDOW: \"'none'\"", &format!("WINDOW: {window}"));
                assert_eq!(
                    issues(fixture.run(&body)),
                    vec![yamaa_adapters::json!({
                        "phase":"validation", "condition":"phase_boundary", "rule":"LOOKUP-7",
                        "spec_paths":paths,
                        "context":{"identifier":"P.DAY","available_phase":"completed_prior_row_template","required_phase":"row_construction"}
                    })],
                    "{window}"
                );
                assert!(!fixture.0.join("src.csv").exists());
            }
        }
        /// The first template's row filter reads for its own rows, before any
        /// row is complete, directly or through `between` values (LOOKUP-7).
        #[test]
        fn first_template_filter_reads_of_self_fail_before_study_access() {
            let filtered = |filter: &str| {
                WINDOWS.edit(
                    "  - id: first\n    dataset: SRC\n",
                    &format!("  - id: first\n    dataset: SRC\n    filter: \"{filter}\"\n"),
                )
            };
            for (filter, identifier, paths) in [
                ("P.DAY IS NULL", "P.DAY", vec!["rows.first.filter"]),
                (
                    "W.NAME IS NULL",
                    "P.DAY",
                    vec!["rows.first.filter", "intermediates.W.between.value"],
                ),
                (
                    "W2.NAME IS NULL AND DAY > 0",
                    "P.DAY",
                    vec![
                        "rows.first.filter",
                        "intermediates.W2.between.value",
                        "intermediates.W.between.value",
                    ],
                ),
            ] {
                let fixture = Fixture::new();
                assert_eq!(
                    issues(fixture.run(&filtered(filter))),
                    vec![yamaa_adapters::json!({
                        "phase":"validation", "condition":"phase_boundary", "rule":"LOOKUP-7",
                        "spec_paths":paths,
                        "context":{"identifier":identifier,"available_phase":"completed_prior_row_template","required_phase":"row_construction"}
                    })],
                    "{filter}"
                );
                assert!(!fixture.0.join("src.csv").exists());
            }
            // A grouped first template's filter reads no qualified name, which
            // binding reports as before (ROW-6).
            let fixture = Fixture::new();
            fs::write(fixture.0.join("src.csv"), "ID,DAY\na,3\n").unwrap();
            fs::write(fixture.0.join("win.csv"), "ID,LO,HI,NAME\na,1,5,early\n").unwrap();
            let grouped = filtered("P.DAY IS NULL").edit(
                "    filter: \"P.DAY IS NULL\"\n",
                "    group_by: [SRC.ID, SRC.DAY]\n    filter: \"P.DAY IS NULL\"\n",
            );
            assert_eq!(
                issues(fixture.run(&grouped)),
                vec![yamaa_adapters::json!({
                    "phase":"validation", "condition":"phase_boundary", "rule":"ROW-6",
                    "spec_paths":["rows.first.filter"],
                    "context":{"identifier":"P.DAY","row":"first","available_phase":"row_construction","required_phase":"grouped_row_filter"}
                })]
            );
            // In a later template the filter reads the completed first rows:
            // each second-template row is kept only when its subject has no
            // first-template row, so none is.
            let fixture = Fixture::new();
            fs::write(fixture.0.join("src.csv"), "ID,DAY\na,3\nb,12\n").unwrap();
            fs::write(fixture.0.join("win.csv"), "ID,LO,HI,NAME\na,1,5,early\n").unwrap();
            let later = WINDOWS.edit(
                "  - id: second\n    dataset: SRC\n",
                "  - id: second\n    dataset: SRC\n    filter: \"P.DAY IS NULL\"\n",
            );
            let mut result = fixture.run(&later).unwrap();
            assert!(result.issues().is_empty(), "{:?}", result.issues());
            assert!(result.save().unwrap());
            assert_eq!(
                fs::read_to_string(fixture.0.join("result.csv")).unwrap(),
                "ID,SECTION,DAY,WINDOW\na,first,3,none\nb,first,12,none\n"
            );
        }
        #[test]
        fn later_templates_read_completed_rows_through_between_values() {
            let fixture = Fixture::new();
            fs::write(fixture.0.join("src.csv"), "ID,DAY\na,3\na,8\nb,12\n").unwrap();
            fs::write(
                fixture.0.join("win.csv"),
                "ID,LO,HI,NAME\na,1,5,early\na,6,10,late\na,11,20,after\nb,1,10,early\nb,11,20,late\nb,21,30,after\n",
            )
            .unwrap();
            let mut result = fixture.run(WINDOWS).unwrap();
            assert!(result.issues().is_empty(), "{:?}", result.issues());
            assert!(result.save().unwrap());
            // P holds each subject's latest first-template day (a: 8, b: 12),
            // W the window holding it (a: late, b: late), and W2 the window
            // holding that window's last day (a: 10, b: 20).
            assert_eq!(
                fs::read_to_string(fixture.0.join("result.csv")).unwrap(),
                "ID,SECTION,DAY,WINDOW\na,first,3,none\na,first,8,none\nb,first,12,none\na,second,3,late\na,second,8,late\nb,second,12,late\n"
            );
        }
    }
    mod declarative_source_selection {
        //! Independently authored typed public results and ordered source-selection findings.
        use std::fs;
        use yamaa_adapters::{
            file_application, public_table::PublicTable, specification_report::Identity,
        };
        use yamaa_core::diagnostic::Diagnostic;
        use yamaa_core::domain_entry::Request;

        struct Fixture(crate::support::Scratch);
        impl Fixture {
            fn new(case: &yamaa_core::json::Value) -> Self {
                let root = crate::support::Scratch::new("source-selection");
                fs::write(
                    root.join("spec.yaml"),
                    case["specification"].as_str().unwrap(),
                )
                .unwrap();
                for (path, bytes) in case["inputs"].as_object().unwrap() {
                    fs::write(root.join(path), bytes.as_str().unwrap()).unwrap();
                }
                Self(root)
            }
        }
        fn issues(rows: &[Diagnostic]) -> yamaa_core::json::Value {
            yamaa_core::json::Value::Array(
                rows.iter()
                    .map(|issue| {
                        let mut row = yamaa_adapters::report_storage::diagnostic_value(issue);
                        assert!(row["context"].is_object());
                        row["context"] = yamaa_core::json::Value::Object(issue.context.clone());
                        row
                    })
                    .collect(),
            )
        }
        fn table(view: &PublicTable) -> yamaa_core::json::Value {
            use arrow_array::{Array, Int64Array, StringArray};
            let columns: Vec<_> = view
                .columns()
                .iter()
                .map(|field| {
                    let kind = match field.data_type() {
                        arrow_schema::DataType::Utf8 => "str",
                        arrow_schema::DataType::Int64 => "int",
                        _ => panic!("unexpected control type"),
                    };
                    yamaa_adapters::json!([field.name(), kind])
                })
                .collect();
            let rows: Vec<Vec<Option<String>>> = view
                .batches()
                .iter()
                .flat_map(|batch| {
                    (0..batch.num_rows()).map(move |row| {
                        batch
                            .columns()
                            .iter()
                            .map(|array| {
                                if array.is_null(row) {
                                    return None;
                                }
                                match array.data_type() {
                                    arrow_schema::DataType::Utf8 => Some(
                                        array
                                            .as_any()
                                            .downcast_ref::<StringArray>()
                                            .unwrap()
                                            .value(row)
                                            .to_owned(),
                                    ),
                                    arrow_schema::DataType::Int64 => Some(
                                        array
                                            .as_any()
                                            .downcast_ref::<Int64Array>()
                                            .unwrap()
                                            .value(row)
                                            .to_string(),
                                    ),
                                    _ => panic!("unexpected control type"),
                                }
                            })
                            .collect()
                    })
                })
                .collect();
            yamaa_adapters::json!({"columns": columns, "rows": rows})
        }
        fn identity(specification: &str) -> Identity<'_> {
            crate::support::study::identity("independent-source-selection", specification)
        }

        fn verify_truth(fixture: &str) {
            let truth: yamaa_core::json::Value =
                yamaa_adapters::json_wire::decode(fixture).unwrap();
            for (name, case) in truth["cases"].as_object().unwrap() {
                for _ in 0..2 {
                    let fixture = Fixture::new(case);
                    let entry = fixture.0.join("spec.yaml");
                    let written = entry.to_str().unwrap();
                    let request = || Request {
                        specification: written,
                        environment: None,
                    };
                    assert_eq!(
                        issues(&file_application::check(
                            yamaa_core::project_function::Language::Python,
                            request(),
                            identity(written)
                        )),
                        case["check_issues"],
                        "{name}: static check"
                    );
                    let mut result = crate::support::study::domain(
                        yamaa_core::project_function::Language::Python,
                        request(),
                        identity(written),
                    )
                    .unwrap();
                    assert_eq!(
                        issues(&result.issues()),
                        case["issues"],
                        "{name}: complete issues"
                    );
                    let report = result.observations();
                    assert_eq!(
                        report["handler_counts"], case["handlers"],
                        "{name}: handler ledger"
                    );
                    let reads: Vec<_> = report["source_reads"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|read| {
                            assert_eq!(read["outcome"], "captured", "{name}");
                            assert_eq!(read["snapshots_created"], 1, "{name}");
                            read["path"].clone()
                        })
                        .collect();
                    assert_eq!(
                        yamaa_adapters::json!(reads),
                        case["source_reads"],
                        "{name}: capture order"
                    );
                    assert_eq!(report["artifacts"], yamaa_adapters::json!([]));
                    if let Some(expected) = case.get("table") {
                        assert_eq!(
                            table(result.output_table().unwrap()),
                            *expected,
                            "{name}: typed output"
                        );
                        assert!(result.save().unwrap(), "{name}");
                        assert_eq!(
                            fs::read(fixture.0.join("result.csv")).unwrap(),
                            case["csv"].as_str().unwrap().as_bytes(),
                            "{name}: exact CSV"
                        );
                        fs::write(fixture.0.join("input.csv"), b"changed after build").unwrap();
                        assert!(result.save().unwrap(), "{name}: retained save");
                        assert_eq!(
                            fs::read(fixture.0.join("result.csv")).unwrap(),
                            case["csv"].as_str().unwrap().as_bytes(),
                            "{name}: retained bytes"
                        );
                    } else {
                        assert!(result.output().is_none(), "{name}");
                        assert!(
                            result.save().is_err(),
                            "{name}: failed build cannot publish"
                        );
                        assert!(!fixture.0.join("result.csv").exists(), "{name}");
                    }
                }
            }
        }

        #[test]
        fn every_source_selection_case_keeps_exact_types_diagnostics_counts_and_saved_bytes() {
            verify_truth(include_str!("fixtures/source_selection.json"));
        }
        #[test]
        fn group_key_filters_have_complete_normative_findings_and_record_controls() {
            verify_truth(include_str!("fixtures/source_filter_scope.json"));
        }

        #[test]
        fn mixed_grouped_and_record_templates_preserve_only_shared_group_key_scalars() {
            verify_truth(include_str!("fixtures/source_filter_mixed.json"));
        }
    }
    mod diagnostic_sites {
        //! Original public unknown-reference findings retain the authored diagnostic site.
        use std::fs;
        use yamaa_adapters::file_application;
        use yamaa_core::diagnostic::Diagnostic;
        use yamaa_core::domain_entry::Request;

        struct Fixture(crate::support::Scratch);
        fn rows(issues: &[Diagnostic]) -> yamaa_core::json::Value {
            issues
                .iter()
                .map(crate::support::study::issue_row)
                .collect()
        }
        #[test]
        fn unknown_references_preserve_original_identifiers_and_dataset_context() {
            let truth: yamaa_core::json::Value = yamaa_adapters::json_wire::decode(include_str!(
                "../../../tests/host-fixtures/cases.json"
            ))
            .unwrap();
            for case in truth["families"]["original-specifications"]["diagnostic_sites"]
                .as_array()
                .unwrap()
            {
                let fixture = Fixture(crate::support::Scratch::new("diagnostic-sites"));
                let path = fixture.0.join("spec.yaml");
                fs::write(&path, case["specification"].as_str().unwrap()).unwrap();
                let path = path.to_str().unwrap();
                let request = || Request {
                    specification: path,
                    environment: None,
                };
                let identity = || crate::support::study::identity("diagnostic-sites", path);
                assert_eq!(
                    rows(&file_application::check(
                        yamaa_core::project_function::Language::Python,
                        request(),
                        identity()
                    )),
                    case["check_issues"]
                );
                for (input, bytes) in case["inputs"].as_object().unwrap() {
                    let input = fixture.0.join(input);
                    fs::create_dir_all(input.parent().unwrap()).unwrap();
                    fs::write(input, bytes.as_str().unwrap()).unwrap();
                }
                for _ in 0..2 {
                    assert_eq!(
                        rows(&file_application::check(
                            yamaa_core::project_function::Language::Python,
                            request(),
                            identity()
                        )),
                        case["check_issues"],
                        "{}",
                        yamaa_adapters::json_wire::encode(&case["name"])
                    );
                    let issues = match crate::support::study::domain(
                        yamaa_core::project_function::Language::Python,
                        request(),
                        identity(),
                    ) {
                        Ok(mut result) => {
                            assert!(result.save().is_err());
                            result.issues().to_vec()
                        }
                        Err(issues) => issues,
                    };
                    assert_eq!(
                        rows(&issues),
                        case["issues"],
                        "{}",
                        yamaa_adapters::json_wire::encode(&case["name"])
                    );
                    assert!(!fixture.0.join(case["output"].as_str().unwrap()).exists());
                }
            }
        }
    }
    mod empty_child_condition {
        //! An empty child equals its parent (LAYER-5): for these negative
        //! benchmarks the child's first issue must match the parent's in phase,
        //! condition and rule, even though the layered path resolves column
        //! dependencies before final validation runs (#2292).
        use std::{
            fs,
            path::{Path, PathBuf},
        };
        use yamaa_core::domain_entry::Request;

        fn copy_dir(src: &Path, dst: &Path) {
            fs::create_dir_all(dst).unwrap();
            for entry in fs::read_dir(src).unwrap() {
                let entry = entry.unwrap();
                let target = dst.join(entry.file_name());
                if entry.file_type().unwrap().is_dir() {
                    if entry.file_name() == "expected" {
                        continue;
                    }
                    copy_dir(&entry.path(), &target);
                } else {
                    fs::copy(entry.path(), &target).unwrap();
                }
            }
        }

        /// The first issue of a build as `phase | condition | rule`.
        fn first_issue(spec: &Path) -> String {
            let path = spec.to_str().unwrap();
            let identity = crate::support::study::identity("empty-child-condition", path);
            let failure = match crate::support::study::domain(
                yamaa_core::project_function::Language::Python,
                Request {
                    specification: path,
                    environment: None,
                },
                identity,
            ) {
                Ok(domain) => domain.issues(),
                Err(failure) => failure,
            };
            let issue = failure.first().expect("benchmark must fail");
            format!(
                "{} | {} | {:?}",
                issue.phase(),
                issue.condition(),
                issue.rule()
            )
        }

        fn assert_empty_child_matches_parent(benchmark: &str, expected_first: &str) {
            let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../benchmarks");
            let dir = crate::support::Scratch::new("empty-child");
            let case = dir.join("case");
            copy_dir(&root.join(benchmark), &case);
            // The child is written in its parent's language, as every layer
            // of a chain is (LAYER-4).
            fs::write(
                case.join("__child.yaml"),
                "yamaa: \"1.0\"\ntype: spec\nparent: spec.yaml\n",
            )
            .unwrap();
            let parent = first_issue(&case.join("spec.yaml"));
            let child = first_issue(&case.join("__child.yaml"));
            assert_eq!(parent, expected_first, "{benchmark} parent first issue");
            assert_eq!(child, parent, "{benchmark} empty child first issue");
        }

        #[test]
        fn empty_child_of_negative_odm_context_reports_invalid_odm_context() {
            assert_empty_child_matches_parent(
                "negative-odm-context",
                "validation | invalid_odm_context | Some(\"NAME-11\")",
            );
        }

        #[test]
        fn empty_child_of_negative_review_unknown_date_reports_unknown_field() {
            assert_empty_child_matches_parent(
                "negative-review-unknown-date",
                "validation | unknown_field | Some(\"FORMULA-9\")",
            );
        }

        #[test]
        fn empty_child_of_negative_lookup_mismatched_reports_unknown_field() {
            assert_empty_child_matches_parent(
                "negative-lookup-mismatched",
                "validation | unknown_field | Some(\"LOOKUP-5\")",
            );
        }

        /// The child orders its columns before binding (LAYER-13), and an
        /// unbound reducer name of an aggregate with bindings is no column
        /// that ordering could miss (#3593).
        #[test]
        fn empty_child_of_negative_aggregate_unbound_variable_reports_unknown_derive_variable() {
            assert_empty_child_matches_parent(
                "negative-aggregate-unbound-variable",
                "validation | unknown_derive_variable | Some(\"LOOKUP-22\")",
            );
        }
    }
    mod inherited_aggregate_bindings {
        //! Aggregate derive bindings remain local when specifications are inherited.
        use std::fs;
        use yamaa_adapters::file_application;
        use yamaa_core::domain_entry::Request;
        struct Fixture(crate::support::Scratch);
        impl Fixture {
            fn new() -> Self {
                let path = crate::support::Scratch::new("inherited_aggregate_bindings");
                Self(path)
            }
            fn build(&self, name: &str) -> file_application::Domain {
                let path = self.0.join(name);
                let path = path.to_str().unwrap();
                let identity =
                    || crate::support::study::identity("inherited_aggregate_bindings", path);
                crate::support::study::domain(
                    yamaa_core::project_function::Language::Python,
                    Request {
                        specification: path,
                        environment: None,
                    },
                    identity(),
                )
                .unwrap()
            }
        }
        #[test]
        fn reducer_and_chained_binding_names_do_not_become_global_column_dependencies() {
            for (chained, collision) in [(false, false), (true, false), (false, true)] {
                let fixture = Fixture::new();
                fs::write(fixture.0.join("dm.csv"), "ID\n1\n2\n").unwrap();
                fs::write(
                    fixture.0.join("ex.csv"),
                    "ID,EXSTDTC\n1,2025-01-05\n1,2025-01-02\n2,2025-02-10\n",
                )
                .unwrap();
                // A chained binding reads the one written before it, so the
                // bindings map is written as YAML, which keeps their order.
                let (kind, expr, chain) = if chained {
                    (
                        "int",
                        "MIN(DAYNUM)",
                        "          DAYNUM: {type: int, derivation: {to_epoch_day: {source: STDT}}}\n",
                    )
                } else {
                    ("date", "MIN(STDT)", "")
                };
                let (output, collided) = if collision {
                    (
                        ", STDT",
                        "  - {name: STDT, type: date, derivation: RESULT}\n",
                    )
                } else {
                    ("", "")
                };
                let parent = format!(
                    "yamaa: \"1.0\"\n\
                     type: spec\n\
                     domain: OUT\n\
                     keys: [ID]\n\
                     base: DM\n\
                     input: {{DM: dm.csv, EX: {{path: ex.csv, key: [ID]}}}}\n\
                     output: {{path: result.csv, columns: [ID, RESULT{output}]}}\n\
                     columns:\n\
                     \x20 - {{name: ID, type: str, derivation: DM.ID}}\n\
                     \x20 - name: RESULT\n\
                     \x20   type: {kind}\n\
                     \x20   derivation:\n\
                     \x20     aggregate:\n\
                     \x20       expr: {expr}\n\
                     \x20       derivations:\n\
                     \x20         STDT: {{type: date, derivation: {{to_date: {{source: EX.EXSTDTC}}}}}}\n\
                     {chain}{collided}"
                );
                fs::write(fixture.0.join("parent.yaml"), parent).unwrap();
                fs::write(
                    fixture.0.join("child.yaml"),
                    "yamaa: \"1.0\"\ntype: spec\nparent: parent.yaml\n",
                )
                .unwrap();
                for entry in ["parent.yaml", "child.yaml"] {
                    let mut result = fixture.build(entry);
                    assert!(
                        result.issues().is_empty(),
                        "{chained} {collision} {entry}: {:?}",
                        result.issues()
                    );
                    assert!(result.save().unwrap());
                    let expected = if chained {
                        "ID,RESULT\n1,20090\n2,20129\n"
                    } else if collision {
                        "ID,RESULT,STDT\n1,2025-01-02,2025-01-02\n2,2025-02-10,2025-02-10\n"
                    } else {
                        "ID,RESULT\n1,2025-01-02\n2,2025-02-10\n"
                    };
                    assert_eq!(
                        fs::read_to_string(fixture.0.join("result.csv")).unwrap(),
                        expected
                    );
                }
            }
        }
    }
    mod inherited_identifier_collision {
        //! Inherited lookup/input collisions remain visible to semantic validation.
        use std::fs;
        use yamaa_adapters::file_application;
        use yamaa_core::domain_entry::Request;
        struct Fixture(crate::support::Scratch);
        impl Fixture {
            fn new() -> Self {
                let path = crate::support::Scratch::new("inherited_identifier_collision");
                Self(path)
            }
            fn issues(&self, name: &str) -> Vec<yamaa_core::diagnostic::Diagnostic> {
                let path = self.0.join(name);
                let path = path.to_str().unwrap();
                let identity =
                    || crate::support::study::identity("inherited_identifier_collision", path);
                let checked = file_application::check(
                    yamaa_core::project_function::Language::Python,
                    Request {
                        specification: path,
                        environment: None,
                    },
                    identity(),
                );
                let built = crate::support::study::domain(
                    yamaa_core::project_function::Language::Python,
                    Request {
                        specification: path,
                        environment: None,
                    },
                    identity(),
                )
                .err()
                .unwrap();
                assert_eq!(checked, built);
                built
            }
        }
        #[test]
        fn a_reachable_qualifier_collision_is_rejected_in_flat_and_composed_specs() {
            for inherited in [false, true] {
                let fixture = Fixture::new();
                fs::write(fixture.0.join("obs.csv"), "ID\n1\n2\n").unwrap();
                fs::write(fixture.0.join("ref.csv"), "ID,TRT\n1,DRUG A\n2,DRUG B\n").unwrap();
                let parent = "yamaa: \"1.0\"\ntype: spec\ndomain: OUT\nkeys: [ID]\nbase: OBS\ninput: {OBS: obs.csv, REF: ref.csv}\noutput: {path: result.csv, columns: [ID, TRT]}\ncolumns:\n  - {name: ID, type: str, derivation: OBS.ID}\n  - {name: TRT, type: str, derivation: REF.TRT}\n";
                let lookup = "intermediates:\n  - {id: REF, dataset: REF, key: [ID], filter: \"REF.TRT = 'DRUG A'\", absent: NONE}\n";
                fs::write(fixture.0.join("parent.yaml"), parent).unwrap();
                let entry = if inherited {
                    format!("yamaa: \"1.0\"\ntype: spec\nparent: parent.yaml\n{lookup}")
                } else {
                    format!("{parent}{lookup}")
                };
                fs::write(fixture.0.join("child.yaml"), entry).unwrap();
                let issues = fixture.issues("child.yaml");
                assert_eq!(issues.len(), 1, "{inherited}: {issues:?}");
                let issue = &issues[0];
                assert_eq!(issue.phase(), "validation");
                assert_eq!(issue.condition(), "duplicate_identifier");
                assert_eq!(issue.rule(), Some("LOOKUP-3"));
                assert_eq!(
                    yamaa_adapters::json_wire::encode_object(&issue.context),
                    "{\"identifier\":\"REF\"}"
                );
                assert!(issue
                    .spec_paths
                    .iter()
                    .any(|path| path == "intermediates.REF.id"));
                assert!(issue.spec_paths.iter().any(|path| path == "input.REF"));
                assert!(!fixture.0.join("result.csv").exists());
            }
        }
    }
    mod inherited_intermediate_verification {
        //! An intermediate verification remains load-bearing through inheritance.
        //!
        //! Diagnostic #2189: an assertion-only intermediate (one that no column reads)
        //! was pruned by the inheritance resolver, so an empty child of a parent
        //! that fails `duplicate_intermediate_records` (LOOKUP-17) built and saved
        //! with no issue. Intermediates carrying nonempty `verifications` are
        //! reachability roots (LAYER-11), so parent and child now agree.
        use std::fs;
        use yamaa_adapters::file_application;
        use yamaa_core::domain_entry::Request;
        struct Fixture(crate::support::Scratch);
        impl Fixture {
            fn new() -> Self {
                let path = crate::support::Scratch::new("inherited_intermediate_verification");
                Self(path)
            }
            fn build(&self, name: &str) -> file_application::Domain {
                let path = self.0.join(name);
                let path = path.to_str().unwrap();
                let identity =
                    || crate::support::study::identity("inherited_intermediate_verification", path);
                crate::support::study::domain(
                    yamaa_core::project_function::Language::Python,
                    Request {
                        specification: path,
                        environment: None,
                    },
                    identity(),
                )
                .unwrap()
            }
        }

        const PARENT_HEAD: &str = "yamaa: \"1.0\"\ntype: spec\ndomain: PV\nkeys: [ID]\ninput: {OBS: obs.csv, REF: ref.csv}\nbase: OBS\nintermediates:\n  - id: REFCHK\n    dataset: REF\n    key: {CODE: CODE}\n    filter: \"REF.KIND = 'X'\"\n    verifications:\n      - unique: [CODE]\n";

        #[test]
        fn assertion_only_intermediate_keeps_its_check_through_inheritance() {
            // `read` adds a column that reads the intermediate; the failure must be
            // identical whether or not anything reads it.
            for read in [false, true] {
                for duplicate in [false, true] {
                    let fixture = Fixture::new();
                    fs::write(fixture.0.join("obs.csv"), "ID,CODE\n1,A\n2,B\n").unwrap();
                    let donor = if duplicate {
                        "CODE,KIND,LABEL\nA,X,Alpha\nA,X,Alpha again\nB,X,Beta\nB,Y,Beta (Y)\n"
                    } else {
                        "CODE,KIND,LABEL\nA,X,Alpha\nB,X,Beta\nB,Y,Beta (Y)\n"
                    };
                    fs::write(fixture.0.join("ref.csv"), donor).unwrap();
                    let (output_columns, label_column) = if read {
                        (
                            "[ID, CODE, LABEL]",
                            "  - {name: LABEL, type: str, derivation: REFCHK.LABEL}\n",
                        )
                    } else {
                        ("[ID, CODE]", "")
                    };
                    fs::write(
                fixture.0.join("parent.yaml"),
                format!(
                    "{PARENT_HEAD}output: {{path: result.csv, columns: {output_columns}}}\ncolumns:\n  - {{name: ID, type: str, derivation: OBS.ID}}\n  - {{name: CODE, type: str, derivation: OBS.CODE}}\n{label_column}"
                ),
            )
            .unwrap();
                    fs::write(
                        fixture.0.join("child.yaml"),
                        "yamaa: \"1.0\"\ntype: spec\nparent: parent.yaml\n",
                    )
                    .unwrap();
                    let mut parent_issues = None;
                    for entry in ["parent.yaml", "child.yaml"] {
                        let mut result = fixture.build(entry);
                        if duplicate {
                            assert!(
                                result.output().is_none(),
                                "read={read} {entry}: a failed build has no output"
                            );
                            let issues = result.issues();
                            assert_eq!(issues.len(), 1, "read={read} {entry}: {issues:?}");
                            let issue = &issues[0];
                            assert_eq!(issue.phase(), "verification", "read={read} {entry}");
                            assert_eq!(
                                issue.condition(),
                                "duplicate_intermediate_records",
                                "read={read} {entry}"
                            );
                            assert_eq!(issue.rule(), Some("LOOKUP-17"), "read={read} {entry}");
                            assert_eq!(
                                issue.spec_paths,
                                ["intermediates.REFCHK.verifications[0].unique"],
                                "read={read} {entry}"
                            );
                            assert_eq!(
                        yamaa_adapters::json_wire::encode_object(&issue.context),
                        "{\"columns\":[\"CODE\"],\"dataset\":\"REF\",\"duplicate_count\":1,\"intermediate\":\"REFCHK\"}",
                        "read={read} {entry}"
                    );
                            match &parent_issues {
                                None => parent_issues = Some(issues),
                                Some(expected) => {
                                    // The child must report the parent's issue: same
                                    // phase, condition, rule and spec path. On
                                    // the read path the parent's context additionally
                                    // embeds its own spec path (pre-existing composed
                                    // provenance), so context is compared verbatim
                                    // only for the assertion-only case this issue is
                                    // about.
                                    assert_eq!(issues.len(), expected.len(), "read={read}");
                                    for (actual, want) in issues.iter().zip(expected.iter()) {
                                        assert_eq!(actual.phase(), want.phase(), "read={read}");
                                        assert_eq!(
                                            actual.condition(),
                                            want.condition(),
                                            "read={read}"
                                        );
                                        assert_eq!(actual.rule(), want.rule(), "read={read}");
                                        assert_eq!(
                                            actual.spec_paths, want.spec_paths,
                                            "read={read}"
                                        );
                                        if !read {
                                            assert_eq!(actual.context, want.context, "read={read}");
                                        }
                                    }
                                }
                            }
                            assert!(
                                result.save().is_err(),
                                "read={read} {entry}: a failed build cannot save"
                            );
                            assert!(
                                !fixture.0.join("result.csv").exists(),
                                "read={read} {entry}: a failed build publishes no artifact"
                            );
                        } else {
                            assert!(
                                result.issues().is_empty(),
                                "read={read} {entry}: {:?}",
                                result.issues()
                            );
                            assert!(result.save().unwrap(), "read={read} {entry}");
                            let expected = if read {
                                "ID,CODE,LABEL\n1,A,Alpha\n2,B,Beta\n"
                            } else {
                                "ID,CODE\n1,A\n2,B\n"
                            };
                            assert_eq!(
                                fs::read_to_string(fixture.0.join("result.csv")).unwrap(),
                                expected,
                                "read={read} {entry}"
                            );
                            fs::remove_file(fixture.0.join("result.csv")).unwrap();
                        }
                    }
                }
            }
        }

        #[test]
        fn empty_intermediate_verifications_do_not_keep_an_unread_lookup_or_input_live() {
            for verifications in ["", "    verifications: []\n"] {
                let fixture = Fixture::new();
                fs::write(fixture.0.join("obs.csv"), "ID\n1\n2\n").unwrap();
                fs::write(
            fixture.0.join("parent.yaml"),
            format!(
                "yamaa: \"1.0\"\ntype: spec\ndomain: OUT\nkeys: [ID]\nbase: OBS\ninput: {{OBS: obs.csv, UNUSED: absent.csv}}\noutput: {{path: result.csv, columns: [ID]}}\nintermediates:\n  - id: UNREAD\n    dataset: UNUSED\n    key: [ID]\n    require_match: true\n{verifications}columns:\n  - {{name: ID, type: str, derivation: OBS.ID}}\n"
            ),
        )
        .unwrap();
                fs::write(
                    fixture.0.join("child.yaml"),
                    "yamaa: \"1.0\"\ntype: spec\nparent: parent.yaml\n",
                )
                .unwrap();
                let mut result = fixture.build("child.yaml");
                assert!(
                    result.issues().is_empty(),
                    "{verifications:?}: {:?}",
                    result.issues()
                );
                assert!(result.save().unwrap());
                assert_eq!(
                    fs::read_to_string(fixture.0.join("result.csv")).unwrap(),
                    "ID\n1\n2\n"
                );
                assert!(!fixture.0.join("absent.csv").exists());
            }
        }
    }
    mod inherited_lookup_keys {
        //! An inherited lookup key retains its internal current-row dependencies.
        use std::fs;
        use yamaa_adapters::file_application;
        use yamaa_core::domain_entry::Request;
        struct Fixture(crate::support::Scratch);
        impl Fixture {
            fn new() -> Self {
                let path = crate::support::Scratch::new("inherited-keys");
                Self(path)
            }
            fn build(&self, name: &str) -> file_application::Domain {
                let path = self.0.join(name);
                let path = path.to_str().unwrap();
                let identity = || crate::support::study::identity("inherited-keys", path);
                assert!(file_application::check(
                    yamaa_core::project_function::Language::Python,
                    Request {
                        specification: path,
                        environment: None
                    },
                    identity()
                )
                .is_empty());
                crate::support::study::domain(
                    yamaa_core::project_function::Language::Python,
                    Request {
                        specification: path,
                        environment: None,
                    },
                    identity(),
                )
                .unwrap()
            }
        }
        #[test]
        fn lookup_and_aggregate_keys_survive_pruning_in_plain_case_and_row_derivations() {
            for key in ["[CODE]", "{CODE: CODE}"] {
                for shape in ["lookup", "case", "row", "aggregate"] {
                    let fixture = Fixture::new();
                    fs::write(fixture.0.join("obs.csv"), "ID,CODE\n1,A\n2,B\n").unwrap();
                    fs::write(fixture.0.join("ref.csv"), "CODE,LABEL\nA,Alpha\nB,Beta\n").unwrap();
                    let lookup =
                        format!("intermediates:\n  - {{id: REFLOOK, dataset: REF, key: {key}}}\n");
                    let derive = match shape {
                "case" => "{case: [{when: \"CODE = 'A'\", then: REFLOOK.LABEL}, {otherwise: REFLOOK.LABEL}]}".to_owned(),
                "aggregate" => format!("{{aggregate: {{key: {key}, expr: COUNT(REF.LABEL)}}}}"),
                _ => "REFLOOK.LABEL".to_owned(),
            };
                    let (declarations, rows) = if shape == "row" {
                        ("  - {name: ID, type: str}\n  - {name: CODE, type: str}\n  - {name: RESULT, type: str}\n".to_owned(), format!("rows:\n  - id: ROW\n    dataset: OBS\n    derivations: {{ID: OBS.ID, CODE: OBS.CODE, RESULT: {derive}}}\n"))
                    } else {
                        (format!("  - {{name: ID, type: str, derivation: OBS.ID}}\n  - {{name: CODE, type: str, derivation: OBS.CODE}}\n  - {{name: RESULT, type: {}, derivation: {derive}}}\n", if shape == "aggregate" {"int"} else {"str"}), String::new())
                    };
                    let parent = format!("yamaa: \"1.0\"\ntype: spec\ndomain: TEST\nkeys: [ID]\ninput: {{OBS: obs.csv, REF: ref.csv}}\nbase: OBS\noutput: {{path: result.csv, columns: [ID, RESULT]}}\n{}columns:\n{declarations}{rows}", if shape == "aggregate" {""} else {&lookup});
                    fs::write(fixture.0.join("parent.yaml"), parent).unwrap();
                    fs::write(
                        fixture.0.join("child.yaml"),
                        "yamaa: \"1.0\"\ntype: spec\nparent: parent.yaml\n",
                    )
                    .unwrap();
                    for entry in ["parent.yaml", "child.yaml"] {
                        let mut result = fixture.build(entry);
                        assert!(
                            result.issues().is_empty(),
                            "{key} {shape} {entry}: {:?}",
                            result.issues()
                        );
                        assert!(result.save().unwrap());
                        let expected = if shape == "aggregate" {
                            "ID,RESULT\n1,1\n2,1\n"
                        } else {
                            "ID,RESULT\n1,Alpha\n2,Beta\n"
                        };
                        assert_eq!(
                            fs::read_to_string(fixture.0.join("result.csv")).unwrap(),
                            expected,
                            "{key} {shape} {entry}"
                        );
                    }
                }
            }
        }
    }
    mod integer_literal_overflow {
        //! Signed-i64 runtime literals fail before study access, with the same complete
        //! issue in both shared host routes. Declaration count bounds stay mathematical.
        use std::fs;
        use yamaa_adapters::{file_application, specification_report::Identity};
        use yamaa_core::diagnostic::Diagnostic;
        use yamaa_core::domain_entry::Request;

        struct Fixture(crate::support::Scratch);
        impl Fixture {
            fn new(column: &str, extra: &str) -> Self {
                let root = crate::support::Scratch::new("integer-literal");
                fs::write(root.join("spec.yaml"), format!(
            "yamaa: \"1.0\"\ntype: spec\ndomain: ADSL\nkeys: [USUBJID]\ninput: {{DM: input.csv}}\noutput: {{path: out.csv, columns: [USUBJID, N]}}\ncolumns:\n  - {{name: USUBJID, type: str, derivation: DM.USUBJID}}\n{column}\n{extra}"
        )).unwrap();
                Self(root)
            }

            fn input(&self) {
                fs::write(self.0.join("input.csv"), "USUBJID,AGE\nS-001,34\n").unwrap();
            }

            fn specification(&self) -> String {
                self.0.join("spec.yaml").to_str().unwrap().to_owned()
            }
        }

        fn identity<'a>(runtime: &'a str, specification: &'a str) -> Identity<'a> {
            Identity {
                runtime,
                runtime_version: "test",
                engine_version: yamaa_core::VERSION,
                example: "integer-literal-overflow",
                specification,
                base_directory: ".",
            }
        }

        fn issue(value: &Diagnostic) -> yamaa_core::json::Value {
            crate::support::study::issue_row(value)
        }

        fn assert_overflow(fixture: &Fixture, path: &str, actual: &str) {
            let mut expected: yamaa_core::json::Value = yamaa_adapters::json_wire::decode(
                include_str!("fixtures/integer_literal_overflow.json"),
            )
            .unwrap();
            expected["spec_paths"] = yamaa_adapters::json!([path]);
            expected["context"]["actual"] = yamaa_adapters::json_wire::decode(actual).unwrap();
            let specification = fixture.specification();
            for runtime in ["python", "r"] {
                let request = || Request {
                    specification: &specification,
                    environment: None,
                };
                let checked = file_application::check(
                    crate::support::host(runtime),
                    request(),
                    identity(runtime, &specification),
                );
                let built = match crate::support::study::domain(
                    crate::support::host(runtime),
                    request(),
                    identity(runtime, &specification),
                ) {
                    Ok(domain) => domain.issues(),
                    Err(issues) => issues,
                };
                for (route, issues) in [("check", checked), ("domain", built)] {
                    assert_eq!(issues.len(), 1, "{runtime}/{route}: {issues:?}");
                    assert_eq!(issue(&issues[0]), expected, "{runtime}/{route}: {path}");
                }
            }
            assert!(!fixture.0.join("out.csv").exists());
        }

        #[test]
        fn wide_runtime_literals_report_validation_before_missing_study_input() {
            let huge = format!("1{}", "0".repeat(400));
            for (written, actual) in [
                ("9223372036854775808", "9223372036854775808"),
                ("-9223372036854775809", "-9223372036854775809"),
                ("18446744073709551616", "18446744073709551616"),
                ("0x8000000000000000", "9223372036854775808"),
                (huge.as_str(), huge.as_str()),
            ] {
                let cases = [
            (format!("  - {{name: N, type: int, derivation: {written}}}"), "columns.N.derivation"),
            (format!("  - {{name: N, type: float, derivation: {written}}}"), "columns.N.derivation"),
            (format!("  - {{name: N, type: str, derivation: {written}}}"), "columns.N.derivation"),
            (format!("  - {{name: N, type: int, derivation: DM.AGE, verifications: [{{allowed_values: {{values: [34, {written}]}}}}]}}"), "columns.N.verifications[0].allowed_values.values[1]"),
            (format!("  - {{name: N, type: int, derivation: DM.AGE, verifications: [{{range: {{min: {written}}}}}]}}"), "columns.N.verifications[0].range.min"),
            (format!("  - {{name: N, type: int, derivation: DM.AGE, verifications: [{{range: {{max: {written}}}}}]}}"), "columns.N.verifications[0].range.max"),
            (format!("  - {{name: N, type: str, derivation: DM.USUBJID, verifications: [{{max_length: {{max: {written}}}}}]}}"), "columns.N.verifications[0].max_length.max"),
            (format!("  - {{name: N, type: int, derivation: {{value: DM.AGE, unconvertible: {written}}}}}"), "columns.N.derivation.unconvertible"),
        ];
                for (column, path) in cases {
                    // No input.csv exists: preparation must report the literal, before IO.
                    assert_overflow(&Fixture::new(&column, ""), path, actual);
                }
            }
        }

        #[test]
        fn row_and_unselected_case_literals_keep_their_authored_paths() {
            let wide = "9223372036854775808";
            let row = Fixture::new(
                "  - {name: N, type: int}",
                &format!("rows:\n  - {{id: record, dataset: DM, derivations: {{N: {wide}}}}}\n"),
            );
            assert_overflow(&row, "rows.record.derivations.N", wide);
            let case = Fixture::new(
        &format!("  - {{name: N, type: int, derivation: {{case: [{{when: 'FALSE', then: {wide}}}, {{otherwise: 0}}]}}}}"),
        "",
    );
            assert_overflow(&case, "columns.N.derivation.case[0].then", wide);
            fs::rename(case.0.join("spec.yaml"), case.0.join("parent.yaml")).unwrap();
            fs::write(
                case.0.join("spec.yaml"),
                "yamaa: \"1.0\"\ntype: spec\nparent: parent.yaml\n",
            )
            .unwrap();
            assert_overflow(&case, "columns.N.derivation.case[0].then", wide);
        }

        #[test]
        fn integer_endpoints_quoted_identifiers_and_explicit_floats_still_build() {
            for (kind, written, expected) in [
                ("int", "-9223372036854775808", "-9223372036854775808"),
                ("int", "9223372036854775807", "9223372036854775807"),
                ("int", "0x7fffffffffffffff", "9223372036854775807"),
                ("str", "\"'9223372036854775808'\"", "9223372036854775808"),
                ("float", "9223372036854775808.0", "9223372036854776000"),
            ] {
                let fixture = Fixture::new(
                    &format!("  - {{name: N, type: {kind}, derivation: {written}}}"),
                    "",
                );
                fixture.input();
                let specification = fixture.specification();
                for runtime in ["python", "r"] {
                    let request = || Request {
                        specification: &specification,
                        environment: None,
                    };
                    assert!(file_application::check(
                        crate::support::host(runtime),
                        request(),
                        identity(runtime, &specification)
                    )
                    .is_empty());
                    let mut domain = crate::support::study::domain(
                        crate::support::host(runtime),
                        request(),
                        identity(runtime, &specification),
                    )
                    .unwrap();
                    assert!(
                        domain.issues().is_empty(),
                        "{written}: {:?}",
                        domain.issues()
                    );
                    assert!(domain.save().unwrap());
                    assert_eq!(
                        fs::read_to_string(fixture.0.join("out.csv")).unwrap(),
                        format!("USUBJID,N\nS-001,{expected}\n")
                    );
                }
            }
        }

        #[test]
        fn wide_mathematical_count_bounds_still_check_and_build() {
            for maximum in [
                "9223372036854775808".to_owned(),
                format!("1{}", "0".repeat(400)),
            ] {
                let fixture = Fixture::new(
                    "  - {name: N, type: int, derivation: DM.AGE}",
                    &format!("verifications:\n  - row_count: {{max: {maximum}}}\n"),
                );
                fixture.input();
                let specification = fixture.specification();
                for runtime in ["python", "r"] {
                    let request = || Request {
                        specification: &specification,
                        environment: None,
                    };
                    assert!(file_application::check(
                        crate::support::host(runtime),
                        request(),
                        identity(runtime, &specification)
                    )
                    .is_empty());
                    let domain = crate::support::study::domain(
                        crate::support::host(runtime),
                        request(),
                        identity(runtime, &specification),
                    )
                    .unwrap();
                    assert!(domain.issues().is_empty(), "{:?}", domain.issues());
                }
            }
        }
    }
    mod intermediate_dependencies {
        //! Original cyclic donor reads and valid dependency order through public routes.
        use std::fs;
        use yamaa_adapters::file_application;
        use yamaa_core::diagnostic::Diagnostic;
        use yamaa_core::domain_entry::Request;
        struct Fixture(crate::support::Scratch);
        fn rows(issues: &[Diagnostic]) -> yamaa_core::json::Value {
            issues
                .iter()
                .map(crate::support::study::issue_row)
                .collect()
        }
        #[test]
        fn cyclic_donor_reads_preserve_original_findings_and_forward_dependencies_execute() {
            let truth: yamaa_core::json::Value = yamaa_adapters::json_wire::decode(include_str!(
                "../../../tests/host-fixtures/cases.json"
            ))
            .unwrap();
            for case in truth["families"]["original-specifications"]["intermediate_dependencies"]
                .as_array()
                .unwrap()
            {
                let fixture = Fixture(crate::support::Scratch::new("intermediate-dependencies"));
                let path = fixture.0.join("spec.yaml");
                fs::write(&path, case["specification"].as_str().unwrap()).unwrap();
                let path = path.to_str().unwrap();
                let request = || Request {
                    specification: path,
                    environment: None,
                };
                let identity =
                    || crate::support::study::identity("intermediate-dependencies", path);
                let failing = !case["issues"].as_array().unwrap().is_empty();
                if failing {
                    assert_eq!(
                        rows(&file_application::check(
                            yamaa_core::project_function::Language::Python,
                            request(),
                            identity()
                        )),
                        case["issues"]
                    );
                    assert_eq!(
                        rows(
                            &crate::support::study::domain(
                                yamaa_core::project_function::Language::Python,
                                request(),
                                identity()
                            )
                            .err()
                            .unwrap()
                        ),
                        case["issues"]
                    );
                }
                for (input, bytes) in case["inputs"].as_object().unwrap() {
                    let input = fixture.0.join(input);
                    fs::create_dir_all(input.parent().unwrap()).unwrap();
                    fs::write(input, bytes.as_str().unwrap()).unwrap();
                }
                for _ in 0..2 {
                    let checked = file_application::check(
                        yamaa_core::project_function::Language::Python,
                        request(),
                        identity(),
                    );
                    assert_eq!(
                        rows(&checked),
                        case["issues"],
                        "{}",
                        yamaa_adapters::json_wire::encode(&case["name"])
                    );
                    if failing {
                        assert_eq!(
                            rows(
                                &crate::support::study::domain(
                                    yamaa_core::project_function::Language::Python,
                                    request(),
                                    identity()
                                )
                                .err()
                                .unwrap()
                            ),
                            case["issues"]
                        );
                        assert!(!fixture.0.join("out.csv").exists());
                    } else {
                        let mut result = crate::support::study::domain(
                            yamaa_core::project_function::Language::Python,
                            request(),
                            identity(),
                        )
                        .unwrap();
                        assert!(
                            result.issues().is_empty(),
                            "{}: {:?}",
                            yamaa_adapters::json_wire::encode(&case["name"]),
                            result.issues()
                        );
                        assert!(result.save().unwrap());
                        assert_eq!(
                            fs::read(fixture.0.join("out.csv")).unwrap(),
                            case["csv"].as_str().unwrap().as_bytes()
                        );
                        fs::remove_file(fixture.0.join("out.csv")).unwrap();
                    }
                }
            }
        }
    }
    mod intermediate_row_drivers {
        //! Named row drivers traverse filtered donor records without lookup keys.
        use std::fs;
        use yamaa_adapters::file_application;
        use yamaa_core::domain_entry::Request;

        struct Fixture(crate::support::Scratch);
        fn issues(rows: &[yamaa_core::diagnostic::Diagnostic]) -> yamaa_core::json::Value {
            yamaa_adapters::json!(rows
                .iter()
                .map(crate::support::study::issue_row)
                .collect::<Vec<_>>())
        }
        #[test]
        fn source_only_named_row_drivers_preserve_lookup_key_admission() {
            let truth: yamaa_core::json::Value = yamaa_adapters::json_wire::decode(include_str!(
                "../../../tests/host-fixtures/cases.json"
            ))
            .unwrap();
            for case in truth["families"]["original-specifications"]["intermediate_row_drivers"]
                .as_array()
                .unwrap()
            {
                let fixture = Fixture(crate::support::Scratch::new("intermediate-row-drivers"));
                for (path, bytes) in case["inputs"].as_object().unwrap() {
                    let path = fixture.0.join(path);
                    fs::create_dir_all(path.parent().unwrap()).unwrap();
                    fs::write(path, bytes.as_str().unwrap()).unwrap();
                }
                let path = fixture.0.join("spec.yaml");
                fs::write(&path, case["specification"].as_str().unwrap()).unwrap();
                let path = path.to_str().unwrap();
                let request = || Request {
                    specification: path,
                    environment: None,
                };
                let identity = || crate::support::study::identity("intermediate_row_drivers", path);
                assert_eq!(
                    issues(&file_application::check(
                        yamaa_core::project_function::Language::Python,
                        request(),
                        identity()
                    )),
                    case["check_issues"],
                    "{}",
                    yamaa_adapters::json_wire::encode(&case["name"])
                );
                let mut result = match crate::support::study::domain(
                    yamaa_core::project_function::Language::Python,
                    request(),
                    identity(),
                ) {
                    Ok(result) => result,
                    Err(rows) => {
                        assert_eq!(
                            issues(&rows),
                            case["issues"],
                            "{}",
                            yamaa_adapters::json_wire::encode(&case["name"])
                        );
                        assert!(case["csv"].is_null());
                        assert!(!fixture.0.join("out.csv").exists());
                        continue;
                    }
                };
                assert_eq!(
                    issues(&result.issues()),
                    case["issues"],
                    "{}",
                    yamaa_adapters::json_wire::encode(&case["name"])
                );
                if let Some(csv) = case["csv"].as_str() {
                    assert!(result.save().unwrap());
                    assert_eq!(fs::read(fixture.0.join("out.csv")).unwrap(), csv.as_bytes());
                } else {
                    assert!(result.output().is_none());
                    assert!(matches!(result.save(), Err(file_application::FailedBuild)));
                    assert!(!fixture.0.join("out.csv").exists());
                }
            }
        }
    }
    mod key_expressions {
        //! Independent current-row key operands through complete file check/build/save routes.
        use crate::support::Edit as _;
        use std::fs;
        use yamaa_adapters::file_application;
        use yamaa_core::diagnostic::Diagnostic;
        use yamaa_core::domain_entry::Request;
        struct Fixture(crate::support::Scratch);
        impl Fixture {
            fn new() -> Self {
                Self(crate::support::Scratch::new("key-expressions"))
            }
        }
        fn rows(issues: &[Diagnostic]) -> yamaa_core::json::Value {
            issues
                .iter()
                .map(crate::support::study::issue_row)
                .collect()
        }
        #[test]
        fn scalar_and_window_keys_reuse_current_operands_types_handlers_and_independent_saved_truth(
        ) {
            let truth: yamaa_core::json::Value = yamaa_adapters::json_wire::decode(include_str!(
                "../../../tests/host-fixtures/cases.json"
            ))
            .unwrap();
            for case in truth["families"]["original-specifications"]["key_expressions"]
                .as_array()
                .unwrap()
            {
                let fixture = Fixture::new();
                let entry = fixture.0.join("spec.yaml");
                fs::write(&entry, case["specification"].as_str().unwrap()).unwrap();
                let entry = entry.to_str().unwrap();
                let request = || Request {
                    specification: entry,
                    environment: None,
                };
                let identity = || crate::support::study::identity("key-expressions", entry);
                // A refusal binding decides from the declarations is reported
                // by the build only once it has captured the inputs.
                let after_capture = case
                    .get("check_after_capture")
                    .and_then(yamaa_core::json::Value::as_bool)
                    == Some(true);
                if !case["check_issues"].as_array().unwrap().is_empty() && !after_capture {
                    assert_eq!(
                        rows(&file_application::check(
                            yamaa_core::project_function::Language::Python,
                            request(),
                            identity()
                        )),
                        case["check_issues"],
                        "{} before input capture",
                        yamaa_adapters::json_wire::encode(&case["name"])
                    );
                    let issues = match crate::support::study::domain(
                        yamaa_core::project_function::Language::Python,
                        request(),
                        identity(),
                    ) {
                        Ok(mut result) => {
                            assert!(
                                result.output().is_none(),
                                "{}",
                                yamaa_adapters::json_wire::encode(&case["name"])
                            );
                            assert!(
                                !result.save().unwrap_or(false),
                                "{}",
                                yamaa_adapters::json_wire::encode(&case["name"])
                            );
                            result.issues()
                        }
                        Err(issues) => issues,
                    };
                    assert_eq!(
                        rows(&issues),
                        case["check_issues"],
                        "{} build before input capture",
                        yamaa_adapters::json_wire::encode(&case["name"])
                    );
                    assert!(
                        !fixture.0.join("out.csv").exists(),
                        "{}",
                        yamaa_adapters::json_wire::encode(&case["name"])
                    );
                }
                for (input, bytes) in case["inputs"].as_object().unwrap() {
                    fs::write(fixture.0.join(input), bytes.as_str().unwrap()).unwrap();
                }
                for _ in 0..2 {
                    assert_eq!(
                        rows(&file_application::check(
                            yamaa_core::project_function::Language::Python,
                            request(),
                            identity()
                        )),
                        case["check_issues"],
                        "{} check",
                        yamaa_adapters::json_wire::encode(&case["name"])
                    );
                    let result = crate::support::study::domain(
                        yamaa_core::project_function::Language::Python,
                        request(),
                        identity(),
                    );
                    if case["issues"].as_array().unwrap().is_empty() {
                        let mut result = result.unwrap_or_else(|issues| {
                            panic!(
                                "{}: {:?}",
                                yamaa_adapters::json_wire::encode(&case["name"]),
                                rows(&issues)
                            )
                        });
                        assert!(
                            result.issues().is_empty(),
                            "{}",
                            yamaa_adapters::json_wire::encode(&case["name"])
                        );
                        assert_eq!(
                            result.observations()["handler_counts"],
                            case["handlers"],
                            "{} handlers",
                            yamaa_adapters::json_wire::encode(&case["name"])
                        );
                        assert!(result.save().unwrap());
                        assert_eq!(
                            fs::read(fixture.0.join("out.csv")).unwrap(),
                            case["csv"].as_str().unwrap().as_bytes(),
                            "{}",
                            yamaa_adapters::json_wire::encode(&case["name"])
                        );
                        fs::remove_file(fixture.0.join("out.csv")).unwrap();
                    } else {
                        let issues = match result {
                            Ok(mut result) => {
                                let issues = result.issues();
                                assert!(!result.save().unwrap_or(false));
                                issues
                            }
                            Err(issues) => issues,
                        };
                        assert_eq!(
                            rows(&issues),
                            case["issues"],
                            "{} build",
                            yamaa_adapters::json_wire::encode(&case["name"])
                        );
                        assert!(!fixture.0.join("out.csv").exists());
                    }
                }
            }
        }

        #[test]
        fn missing_sources_keep_filter_types_but_other_resource_and_output_checkpoints_stay_separate(
        ) {
            let truth: yamaa_core::json::Value = yamaa_adapters::json_wire::decode(include_str!(
                "../../../tests/host-fixtures/cases.json"
            ))
            .unwrap();
            let case = truth["families"]["original-specifications"]["key_expressions"]
                .as_array()
                .unwrap()
                .iter()
                .find(|case| {
                    case["name"]
                        == "generic-key-valid-cardinality-retains-donor-filter-type-refusal"
                })
                .unwrap();
            let fixture = Fixture::new();
            let entry = fixture.0.join("spec.yaml");
            let specification = case["specification"].as_str().unwrap();
            fs::write(&entry, specification).unwrap();
            let refused = || {
                let entry = entry.to_str().unwrap();
                let mut result = crate::support::study::domain(
                    yamaa_core::project_function::Language::Python,
                    Request {
                        specification: entry,
                        environment: None,
                    },
                    crate::support::study::identity("missing-key-expression-input", entry),
                )
                .unwrap_or_else(|issues| panic!("{:?}", rows(&issues)));
                assert!(result.output().is_none());
                assert_eq!(
                    result.observations()["source_reads"],
                    yamaa_adapters::json!([])
                );
                assert!(!result.save().unwrap_or(false));
                assert!(!fixture.0.join("out.csv").exists());
                rows(&result.issues())
            };
            for present in ["src.csv", "tab.csv"] {
                fs::write(
                    fixture.0.join(present),
                    case["inputs"][present].as_str().unwrap(),
                )
                .unwrap();
                assert_eq!(refused(), case["check_issues"], "only {present} exists");
                fs::remove_file(fixture.0.join(present)).unwrap();
            }
            let missing_sources = yamaa_adapters::json!([
                {"phase": "validation", "condition": "resource_path_missing", "rule": "FILE-7",
                 "spec_paths": ["input.SRC.path"], "context": {"dataset": "SRC", "path": "src.csv"}},
                {"phase": "validation", "condition": "resource_path_missing", "rule": "FILE-7",
                 "spec_paths": ["input.TAB.path"], "context": {"dataset": "TAB", "path": "tab.csv"}},
            ]);
            fs::create_dir(fixture.0.join("tab.csv")).unwrap();
            let mut mixed_resources = missing_sources.clone();
            mixed_resources[1]["condition"] =
                yamaa_adapters::json!("resource_path_not_regular_file");
            assert_eq!(refused(), mixed_resources);
            fs::remove_dir(fixture.0.join("tab.csv")).unwrap();

            // A valid filter has no static refusal. Even an independently known output
            // declaration error retains its later checkpoint when sources are missing.
            fs::write(
                &entry,
                specification
                    .edit("TAB.KINT > 'bad'", "TAB.KINT > 0")
                    .edit("path: out.csv", "path: out.csv\n  decimals: 19"),
            )
            .unwrap();
            assert_eq!(refused(), missing_sources);
        }
    }
    mod namespace_admission {
        //! The original namespace obligations through complete public check and build.
        use std::fs;
        use yamaa_adapters::file_application;
        use yamaa_core::domain_entry::Request;

        struct Fixture(crate::support::Scratch);

        #[test]
        fn authored_namespace_refusals_precede_build_and_source_access() {
            let truth: yamaa_core::json::Value = yamaa_adapters::json_wire::decode(include_str!(
                "../../../tests/host-fixtures/cases.json"
            ))
            .unwrap();
            for case in truth["families"]["original-specifications"]["namespace_admission"]
                .as_array()
                .unwrap()
            {
                let fixture = Fixture(crate::support::Scratch::canonical("namespace"));
                let path = fixture.0.join("spec.yaml");
                fs::write(&path, case["specification"].as_str().unwrap()).unwrap();
                // Deliberately absent: an invalid namespace grants no source authority.
                let path = path.to_str().unwrap();
                // `{entry}` stands for the specification's absolute path as
                // the engine resolves it, with forward slashes.
                let expected = yamaa_adapters::json_wire::decode(
                    &yamaa_adapters::json_wire::encode(&case["issues"]).replace(
                        "\"{entry}\"",
                        &yamaa_adapters::json_wire::encode(&yamaa_adapters::json!(
                            path.replace('\\', "/")
                        )),
                    ),
                )
                .unwrap();
                let request = || Request {
                    specification: path,
                    environment: None,
                };
                let identity = || crate::support::study::identity("namespace_admission", path);
                let checked = file_application::check(
                    yamaa_core::project_function::Language::Python,
                    request(),
                    identity(),
                );
                let built = crate::support::study::domain(
                    yamaa_core::project_function::Language::Python,
                    request(),
                    identity(),
                )
                .err()
                .unwrap();
                assert_eq!(
                    checked,
                    built,
                    "{}",
                    yamaa_adapters::json_wire::encode(&case["name"])
                );
                let rows = checked
                    .iter()
                    .map(crate::support::study::issue_row)
                    .collect::<Vec<_>>();
                assert_eq!(
                    yamaa_adapters::json!(rows),
                    expected,
                    "{}",
                    yamaa_adapters::json_wire::encode(&case["name"])
                );
                assert!(!fixture.0.join("out.csv").exists());
            }
        }
    }
    mod nested_windows {
        use crate::support::Edit as _;
        use std::fs;
        use yamaa_adapters::file_application;
        use yamaa_core::domain_entry::Request;
        struct Study(crate::support::Scratch);
        impl Study {
            fn new(input: &str, specification: &str) -> Self {
                let path = crate::support::Scratch::new("nested-windows");
                fs::write(path.join("input.csv"), input).unwrap();
                fs::write(path.join("spec.yaml"), specification).unwrap();
                Self(path.resolved(std::fs::canonicalize))
            }
            fn build(&self) -> file_application::Domain {
                self.try_build().unwrap()
            }
            fn try_build(&self) -> Result<file_application::Domain, file_application::Failure> {
                let path = self.0.join("spec.yaml").to_str().unwrap().to_owned();
                crate::support::study::domain(
                    yamaa_core::project_function::Language::Python,
                    Request {
                        specification: &path,
                        environment: None,
                    },
                    crate::support::study::identity("nested-windows", &path),
                )
            }
            fn saved(&self) -> String {
                let mut result = self.build();
                assert!(result.issues().is_empty(), "{:?}", result.issues());
                assert!(result.save().unwrap());
                fs::read_to_string(self.0.join("out.csv")).unwrap()
            }
        }
        #[test]
        fn nested_case_windows_use_the_full_completed_context() {
            let study = Study::new(
                "I,G,O,V\n1,A,2,a2\n2,A,1,a1\n3,A,3,a3\n4,B,1,b1\n",
                r#"yamaa: "1.0"
type: spec
domain: TEST
keys: [I]
input: {SRC: {path: input.csv, types: {I: int, O: int}}}
columns:
  - {name: I, type: int, derivation: SRC.I}
  - name: N
    type: int
    derivation:
      case:
        - when: 'I > 1'
          then: {row_number: {window: {group_by: [SRC.G], order_by: [SRC.O]}}}
        - otherwise: -1
  - name: P
    type: str
    derivation:
      case:
        - when: 'I = 3'
          then: {row_value: {source: SRC.V, offset: -1, window: {group_by: [SRC.G], order_by: [SRC.O]}}}
        - otherwise: "'skip'"
  - name: C
    type: str
    derivation: {str_template: 'prior={P}'}
output: {path: out.csv, columns: [I, N, C]}
"#,
            );
            assert_eq!(
                study.saved(),
                "I,N,C\n1,-1,prior=skip\n2,1,prior=skip\n3,3,prior=a2\n4,1,prior=skip\n"
            );
        }
        #[test]
        fn nested_windows_remain_template_local_and_observe_pre_window_row_filtering() {
            let study = Study::new(
                "I,O\n1,2\n2,1\n3,3\n",
                r#"yamaa: "1.0"
type: spec
domain: TEST
keys: [T, I]
input: {SRC: {path: input.csv, types: {I: int, O: int}}}
columns:
  - {name: T, type: str}
  - {name: I, type: int, derivation: SRC.I}
  - {name: N, type: int}
rows:
  - id: first
    dataset: SRC
    derivations:
      T: "'A'"
      N: {case: [{when: 'I > 0', then: {row_number: {window: {order_by: [SRC.O]}}}}]}
  - id: second
    dataset: SRC
    filter: 'SRC.I > 1'
    derivations:
      T: "'B'"
      N: {case: [{when: 'I > 0', then: {row_number: {window: {order_by: [SRC.O]}}}}]}
output: {path: out.csv, columns: [T, I, N]}
"#,
            );
            assert_eq!(study.saved(), "T,I,N\nA,1,2\nA,2,1\nA,3,3\nB,2,1\nB,3,2\n");
        }
        #[test]
        fn output_key_template_keeps_transitive_window_defaults_over_all_constructed_rows() {
            let study = Study::new(
                "I\n1\n2\n3\n",
                r#"yamaa: "1.0"
type: spec
domain: TEST
keys: [T, K]
input: {SRC: {path: input.csv, types: {I: int}}}
columns:
  - {name: T, type: str}
  - {name: I, type: int, derivation: SRC.I}
  - name: N
    type: int
    derivation: {row_number: {window: {order_by: [T, I]}}}
  - {name: NC, type: str, derivation: N}
  - {name: K, type: str, derivation: {str_template: 'R{NC}'}}
rows:
  - id: first
    dataset: SRC
    derivations: {T: "'A'"}
  - id: second
    dataset: SRC
    filter: 'SRC.I > 1'
    derivations: {T: "'B'"}
output: {path: out.csv, columns: [T, K, I, N]}
"#,
            );
            assert_eq!(
                study.saved(),
                "T,K,I,N\nA,R1,1,1\nA,R2,2,2\nA,R3,3,3\nB,R4,2,4\nB,R5,3,5\n"
            );
        }
        #[test]
        fn unreachable_window_branch_has_no_runtime_failure_but_reached_branch_retains_it() {
            let specification = r#"yamaa: "1.0"
type: spec
domain: TEST
keys: [I]
input: {SRC: {path: input.csv, types: {I: int, D: date}}}
columns:
  - {name: I, type: int, derivation: SRC.I}
  - {name: D, type: date, derivation: SRC.D}
  - name: B
    type: str
    derivation:
      case:
        - when: 'I < 0'
          then: {baseline_flag: {date: D, reference: D, source: I}}
        - otherwise: "'skipped'"
output: {path: out.csv, columns: [I, B]}
"#;
            let input = "I,D\n1,2024-01-01\n2,2024-01-01\n";
            let skipped = Study::new(input, specification);
            assert_eq!(skipped.saved(), "I,B\n1,skipped\n2,skipped\n");
            let reached = Study::new(input, &specification.edit("I < 0", "I = 2"));
            let result = reached.build();
            assert!(result.output().is_none());
            assert_eq!(result.issues().len(), 1, "{:?}", result.issues());
            assert_eq!(result.issues()[0].condition(), "ambiguous_baseline");
            assert!(result.issues()[0]
                .spec_paths
                .iter()
                .any(|path| path.contains("case[0].then.baseline_flag")));
        }
        #[test]
        fn arbitrary_width_window_offsets_select_absent_neighbors_without_float_conversion() {
            let study = Study::new(
                "I,V\n1,a\n2,b\n",
                r#"yamaa: "1.0"
type: spec
domain: TEST
keys: [I]
input: {SRC: input.csv}
columns:
  - {name: I, type: int, derivation: SRC.I}
  - {name: V, type: str, derivation: SRC.V}
  - name: BEFORE
    type: str
    derivation: {row_value: {source: V, offset: -99999999999999999999999999999999999999, window: {order_by: [I]}}}
  - name: AFTER
    type: str
    derivation: {row_value: {source: V, offset: 99999999999999999999999999999999999999, window: {order_by: [I]}}}
output: {path: out.csv, columns: [I, BEFORE, AFTER]}
"#,
            );
            assert_eq!(study.saved(), "I,BEFORE,AFTER\n1,,\n2,,\n");
        }

        #[test]
        fn selected_window_partition_does_not_evaluate_unselected_partition_failures() {
            let study = Study::new(
                "I,G,D\n1,A,2024-01-01\n2,B,2024-01-01\n3,B,2024-01-01\n",
                r#"yamaa: "1.0"
type: spec
domain: TEST
keys: [I]
input: {SRC: {path: input.csv, types: {I: int, D: date}}}
columns:
  - {name: I, type: int, derivation: SRC.I}
  - {name: G, type: str, derivation: SRC.G}
  - {name: D, type: date, derivation: SRC.D}
  - name: B
    type: str
    derivation:
      case:
        - when: "G = 'A'"
          then: {baseline_flag: {date: D, reference: D, source: I, window: {group_by: [G]}}}
        - otherwise: "'skipped'"
output: {path: out.csv, columns: [I, B]}
"#,
            );
            assert_eq!(study.saved(), "I,B\n1,Y\n2,skipped\n3,skipped\n");
        }
    }
    mod original_named_windows {
        //! Original named-window witnesses retired with the Python evaluator in #2110.
        //! Inputs come from schema/test_named_windows.py at c7b88a4f; expected values
        //! are its independent row results or follow its asserted atomic replacement.
        use std::fs;
        use yamaa_adapters::file_application;
        use yamaa_adapters::json;
        use yamaa_core::diagnostic::Diagnostic;
        use yamaa_core::domain_entry::Request;
        use yamaa_core::json::Value;

        const INPUT: &str = "ID,G,SEQ,VAL\n01,a,1,10\n02,a,2,20\n03,b,1,30\n04,b,2,40\n";

        struct Study(crate::support::Scratch);
        impl Study {
            fn new() -> Self {
                let path = crate::support::Scratch::new("original-named-windows");
                Self(path)
            }
            fn write(&self, name: &str, document: &Value) {
                fs::write(
                    self.0.join(name),
                    yamaa_adapters::json_wire::encode(document).into_bytes(),
                )
                .unwrap();
            }
            fn build(&self) -> Result<file_application::Domain, Vec<Diagnostic>> {
                let entry = self.0.join("spec.yaml");
                let entry = entry.to_str().unwrap();
                crate::support::study::domain(
                    yamaa_core::project_function::Language::Python,
                    Request {
                        specification: entry,
                        environment: None,
                    },
                    crate::support::study::identity("original-named-windows", entry),
                )
            }
            fn saved(&self) -> String {
                let mut result = self.build().unwrap();
                assert!(result.issues().is_empty(), "{:?}", result.issues());
                assert!(!self.0.join("out.csv").exists());
                assert!(result.save().unwrap());
                let expected = fs::read_to_string(self.0.join("out.csv")).unwrap();
                // Save owns the completed bytes, including the evaluated window scope.
                fs::write(self.0.join("input.csv"), "changed after build").unwrap();
                assert!(result.save().unwrap());
                assert_eq!(
                    fs::read_to_string(self.0.join("out.csv")).unwrap(),
                    expected
                );
                expected
            }
            fn refused(&self) -> Vec<Diagnostic> {
                let issues = match self.build() {
                    Ok(mut result) => {
                        assert!(result.output().is_none());
                        assert!(result.save().is_err());
                        result.issues().to_vec()
                    }
                    Err(issues) => issues,
                };
                assert!(!issues.is_empty());
                assert!(!self.0.join("out.csv").exists());
                issues
            }
        }

        fn document() -> Value {
            json!({
                "yamaa":"1.0", "type":"spec", "domain":"OUT", "base":"SRC", "keys":["ID"],
                "input":{"SRC":{"path":"input.csv","types":{"SEQ":"int","VAL":"int"}}},
                "output":{"path":"out.csv","columns":["ID","PREV"]},
                "windows":{"VISITS":{"group_by":["G"],"order_by":["SEQ"]}},
                "columns":[
                    {"name":"ID","type":"str","label":"ID","derivation":"SRC.ID"},
                    {"name":"G","type":"str","label":"G","derivation":"SRC.G"},
                    {"name":"SEQ","type":"int","label":"SEQ","derivation":"SRC.SEQ"},
                    {"name":"VAL","type":"int","label":"VAL","derivation":"SRC.VAL"},
                    {"name":"PREV","type":"int","label":"Previous value","derivation":{"row_value":{"source":"VAL","offset":-1,"window":"VISITS"}}}
                ]
            })
        }

        #[test]
        fn shared_named_window_is_evaluated_in_each_filtered_row_template() {
            let study = Study::new();
            let mut authored = document();
            authored["windows"]["VISITS"] = json!({"order_by":["SEQ"]});
            let mut derivations = yamaa_core::json::Map::new();
            for column in authored["columns"].as_array_mut().unwrap() {
                let name = column["name"].as_str().unwrap().to_owned();
                derivations.insert(
                    name,
                    column
                        .as_object_mut()
                        .unwrap()
                        .remove("derivation")
                        .unwrap(),
                );
            }
            authored["rows"] = ["a", "b"]
        .map(|group| json!({"id":group,"dataset":"SRC","filter":format!("SRC.G = '{group}'"),"derivations":derivations}))
        .into();
            study.write("spec.yaml", &authored);
            fs::write(study.0.join("input.csv"), INPUT).unwrap();
            assert_eq!(study.saved(), "ID,PREV\n01,\n02,10\n03,\n04,30\n");
        }

        #[test]
        fn replacing_named_selection_with_inline_or_another_name_is_atomic() {
            for replacement in [json!("OTHER"), json!({"order_by":["VAL"]})] {
                let study = Study::new();
                let mut parent = document();
                parent["windows"]["OTHER"] = json!({"order_by":["VAL"]});
                study.write("parent.yaml", &parent);
                study.write(
            "spec.yaml",
            &json!({
                "yamaa":"1.0", "type":"spec", "parent":"parent.yaml",
                "columns":[{"name":"PREV","derivation":{"row_value":{"window":replacement}}}]
            }),
        );
                fs::write(study.0.join("input.csv"), INPUT).unwrap();
                // The replacement loses VISITS.group_by. Its VAL order therefore
                // has one partition: 10, 20, 30, 40 across both original groups.
                assert_eq!(study.saved(), "ID,PREV\n01,\n02,10\n03,20\n04,30\n");
            }
        }

        #[test]
        fn clearing_named_definitions_rejects_the_surviving_reference_before_input_access() {
            let study = Study::new();
            study.write("parent.yaml", &document());
            study.write(
                "spec.yaml",
                &json!({"yamaa":"1.0","type":"spec","parent":"parent.yaml","clear":["windows"]}),
            );
            let issues = study.refused();
            assert_eq!(issues[0].condition(), "unknown_window");
            assert_eq!(issues[0].rule(), Some("WINDOW-1"));
            assert_eq!(
                issues[0].spec_paths,
                ["columns.PREV.derivation.row_value.window"]
            );
            assert!(!study.0.join("input.csv").exists());
        }

        /// A named window keeps each operation's restrictions where it is
        /// expanded: a `row_number` over a window without `order_by` is refused
        /// (WINDOW-5).
        #[test]
        fn named_expansion_preserves_each_operations_window_restrictions() {
            let study = Study::new();
            let mut authored = document();
            authored["windows"]["VISITS"] = json!({});
            authored["columns"][4]["type"] = "int".into();
            authored["columns"][4]["derivation"] = json!({"row_number": {"window": "VISITS"}});
            study.write("spec.yaml", &authored);
            fs::write(study.0.join("input.csv"), INPUT).unwrap();
            let issues = study.refused();
            assert_eq!(
                issues[0].condition(),
                "window_order_by_required",
                "{issues:?}"
            );
        }
    }
    mod original_verification {
        //! Original verification witnesses from python/tests/verification/test_checks.py
        //! at c7b88a4f, before the reference evaluator retired in #2110.
        use std::fs;
        use yamaa_adapters::file_application;
        use yamaa_adapters::json;
        use yamaa_core::domain_entry::Request;
        use yamaa_core::json::Value;

        struct Study(crate::support::Scratch);
        impl Study {
            fn new() -> Self {
                let path = crate::support::Scratch::new("original-verification");
                Self(path)
            }
            fn build(
                &self,
                columns: Value,
                checks: Value,
                input: &str,
            ) -> file_application::Domain {
                let output = columns
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|column| column["name"].clone())
                    .collect::<Vec<_>>();
                let input_source = if output.iter().any(|name| name == "VAL") {
                    json!({"path":"input.csv","types":{"VAL":"int"}})
                } else {
                    json!("input.csv")
                };
                let document = json!({
                    "yamaa":"1.0", "type":"spec", "domain":"OUT", "base":"SRC",
                    "keys":["STUDYID","USUBJID"],
                    "input":{"SRC":input_source},
                    "columns":columns, "verifications":checks,
                    "output":{"path":"out.csv","columns":output}
                });
                let entry = self.0.join("spec.yaml");
                fs::write(
                    &entry,
                    yamaa_adapters::json_wire::encode(&document).into_bytes(),
                )
                .unwrap();
                fs::write(self.0.join("input.csv"), input).unwrap();
                let entry = entry.to_str().unwrap();
                crate::support::study::domain(
                    yamaa_core::project_function::Language::Python,
                    Request {
                        specification: entry,
                        environment: None,
                    },
                    crate::support::study::identity("original-verification", entry),
                )
                .unwrap()
            }
        }
        fn columns() -> Value {
            json!([
                {"name":"STUDYID","type":"str","derivation":"SRC.STUDYID"},
                {"name":"USUBJID","type":"str","derivation":"SRC.USUBJID"}
            ])
        }

        #[test]
        fn fractional_row_count_on_an_empty_artifact_is_zero() {
            for (bound, passes) in [
                (json!({"max_fraction":0}), true),
                (json!({"min_fraction":0.1}), false),
            ] {
                let study = Study::new();
                let mut result =
                    study.build(columns(), json!([{"row_count":bound}]), "STUDYID,USUBJID\n");
                if passes {
                    assert!(result.issues().is_empty(), "{:?}", result.issues());
                    assert!(result.output().is_some());
                    assert!(result.save().unwrap());
                    assert_eq!(
                        fs::read_to_string(study.0.join("out.csv")).unwrap(),
                        "STUDYID,USUBJID\n"
                    );
                } else {
                    assert!(result.output().is_none());
                    let issues = result.issues();
                    assert_eq!(issues.len(), 1, "{issues:?}");
                    assert_eq!(issues[0].condition(), "row_count_failed");
                    assert_eq!(issues[0].rule(), Some("CHECK-13"));
                    assert_eq!(issues[0].spec_paths, ["verifications[0].row_count"]);
                    let context = yamaa_core::json::Value::Object(issues[0].context.clone());
                    assert_eq!(context["denominator"], 0);
                    assert_eq!(context["count"], 0);
                    assert_eq!(context["keys"], json!([{}]));
                    assert!(result.save().is_err());
                    assert!(!study.0.join("out.csv").exists());
                }
            }
        }

        #[test]
        fn assertion_when_exempts_false_and_unknown_rows_but_require_must_be_true() {
            let study = Study::new();
            let mut columns = columns();
            columns.as_array_mut().unwrap().extend([
                json!({"name":"GATE","type":"str","derivation":"SRC.GATE"}),
                json!({"name":"VAL","type":"int","derivation":"SRC.VAL"}),
            ]);
            let mut result = study.build(
        columns,
        json!([{"assert":{"when":"GATE = 'Y'","require":"VAL > 0"}}]),
        "STUDYID,USUBJID,GATE,VAL\nS,S-01,Y,1\nS,S-02,Y,0\nS,S-03,Y,\nS,S-04,N,0\nS,S-05,,0\n",
    );
            let issues = result.issues();
            assert_eq!(issues.len(), 1, "{issues:?}");
            assert_eq!(issues[0].condition(), "assert_failed");
            assert_eq!(issues[0].rule(), Some("CHECK-12"));
            assert_eq!(issues[0].spec_paths, ["verifications[0].assert"]);
            let context = yamaa_core::json::Value::Object(issues[0].context.clone());
            assert_eq!(
                context["keys"],
                json!([
                    {"STUDYID":"S","USUBJID":"S-02"},
                    {"STUDYID":"S","USUBJID":"S-03"}
                ])
            );
            assert!(result.output().is_none());
            assert!(result.save().is_err());
            assert!(!study.0.join("out.csv").exists());
        }
    }
    mod predicate_suggestions {
        //! Original qualified-name suggestions and their unknown/ambiguous neighbors.
        use std::fs;
        use yamaa_adapters::file_application;
        use yamaa_core::domain_entry::Request;

        struct Fixture(crate::support::Scratch);
        fn issues(rows: &[yamaa_core::diagnostic::Diagnostic]) -> yamaa_core::json::Value {
            yamaa_adapters::json!(rows
                .iter()
                .map(crate::support::study::issue_row)
                .collect::<Vec<_>>())
        }
        #[test]
        fn original_predicate_suggestions_preserve_scopes_and_unknown_names() {
            let truth: yamaa_core::json::Value = yamaa_adapters::json_wire::decode(include_str!(
                "../../../tests/host-fixtures/cases.json"
            ))
            .unwrap();
            for case in truth["families"]["original-specifications"]["predicate_suggestions"]
                .as_array()
                .unwrap()
            {
                let fixture = Fixture(crate::support::Scratch::new("predicate-suggestions"));
                for (path, bytes) in case["inputs"].as_object().unwrap() {
                    let path = fixture.0.join(path);
                    fs::create_dir_all(path.parent().unwrap()).unwrap();
                    fs::write(path, bytes.as_str().unwrap()).unwrap();
                }
                let path = fixture.0.join("spec.yaml");
                fs::write(&path, case["specification"].as_str().unwrap()).unwrap();
                let path = path.to_str().unwrap();
                let request = || Request {
                    specification: path,
                    environment: None,
                };
                let identity = || crate::support::study::identity("predicate_suggestions", path);
                assert_eq!(
                    issues(&file_application::check(
                        yamaa_core::project_function::Language::Python,
                        request(),
                        identity()
                    )),
                    case["check_issues"],
                    "{}",
                    yamaa_adapters::json_wire::encode(&case["name"])
                );
                let mut result = crate::support::study::domain(
                    yamaa_core::project_function::Language::Python,
                    request(),
                    identity(),
                )
                .unwrap();
                assert_eq!(
                    issues(&result.issues()),
                    case["issues"],
                    "{}",
                    yamaa_adapters::json_wire::encode(&case["name"])
                );
                if let Some(csv) = case["csv"].as_str() {
                    assert!(result.save().unwrap());
                    assert_eq!(fs::read(fixture.0.join("out.csv")).unwrap(), csv.as_bytes());
                } else {
                    assert!(result.output().is_none());
                    assert!(matches!(result.save(), Err(file_application::FailedBuild)));
                    assert!(!fixture.0.join("out.csv").exists());
                }
            }
        }
    }
    mod missing_order {
        //! Every order term places an undeclared missing value lowest: first
        //! ascending and last descending. A declared `nulls` never flips.
        use crate::support::Edit as _;
        use std::fs;
        use yamaa_core::domain_entry::Request;
        struct Study(crate::support::Scratch);
        impl Study {
            fn new(input: &str, specification: &str) -> Self {
                let path = crate::support::Scratch::new("missing-order");
                fs::write(path.join("input.csv"), input).unwrap();
                fs::write(path.join("spec.yaml"), specification).unwrap();
                Self(path.resolved(std::fs::canonicalize))
            }
            fn saved(&self) -> String {
                let path = self.0.join("spec.yaml").to_str().unwrap().to_owned();
                let mut result = crate::support::study::domain(
                    yamaa_core::project_function::Language::Python,
                    Request {
                        specification: &path,
                        environment: None,
                    },
                    crate::support::study::identity("missing-order", &path),
                )
                .unwrap_or_else(|_| panic!("valid missing-order study"));
                assert!(result.issues().is_empty(), "{:?}", result.issues());
                assert!(result.save().unwrap());
                fs::read_to_string(self.0.join("out.csv")).unwrap()
            }
        }

        #[test]
        fn windows_and_presentation_sort_an_undeclared_missing_value_lowest() {
            let specification = r#"yamaa: "1.0"
type: spec
domain: TEST
keys: [I]
input: {SRC: {path: input.csv, types: {I: int, D: int}}}
columns:
  - {name: I, type: int, derivation: SRC.I}
  - {name: D, type: int, derivation: SRC.D}
  - name: ASC
    type: int
    derivation: {row_number: {window: {order_by: [D]}}}
  - name: DESC
    type: int
    derivation: {row_number: {window: {order_by: [{variable: D, direction: desc}]}}}
  - name: LAST
    type: int
    derivation: {row_number: {window: {order_by: [{variable: D, nulls: last}]}}}
  - name: FIRST
    type: int
    derivation:
      row_number: {window: {order_by: [{variable: D, direction: desc, nulls: first}]}}
output: {path: out.csv, columns: [I, D, ASC, DESC, LAST, FIRST], order_by: [D]}
"#;
            let input = "I,D\n1,3\n2,\n3,1\n";
            assert_eq!(
                Study::new(input, specification).saved(),
                "I,D,ASC,DESC,LAST,FIRST\n2,,1,3,3,1\n3,1,2,2,1,3\n1,3,3,1,2,2\n"
            );
            let descending = specification.edit(
                "order_by: [D]}\n",
                "order_by: [{variable: D, direction: desc}]}\n",
            );
            assert_ne!(descending, specification);
            assert_eq!(
                Study::new(input, &descending).saved(),
                "I,D,ASC,DESC,LAST,FIRST\n1,3,3,1,2,2\n3,1,2,2,1,3\n2,,1,3,3,1\n"
            );
        }

        #[test]
        fn source_and_lookup_selection_sort_an_undeclared_missing_value_lowest() {
            let study = Study::new(
                "G,D,S\nA,3,a3\nA,,amiss\nA,1,a1\nB,2,b2\nB,,bmiss\n",
                r#"yamaa: "1.0"
type: spec
domain: TEST
keys: [G]
input: {SRC: {path: input.csv, types: {D: int}}}
base: SRC
intermediates:
  - id: LATEST
    dataset: SRC
    key: [G]
    order_by: [{variable: SRC.D, direction: desc}]
    keep: last
columns:
  - {name: G, type: str, derivation: SRC.G}
  - name: LOWEST
    type: str
    derivation: {source: {variable: SRC.S, order_by: [SRC.D], keep: first}}
  - name: HIGHEST
    type: str
    derivation:
      source: {variable: SRC.S, order_by: [{variable: SRC.D, direction: desc}], keep: first}
  - name: DATED
    type: str
    derivation:
      source: {variable: SRC.S, order_by: [{variable: SRC.D, nulls: last}], keep: first}
  - {name: TRAILING, type: str, derivation: LATEST.S}
output: {path: out.csv, columns: [G, LOWEST, HIGHEST, DATED, TRAILING]}
"#,
            );
            assert_eq!(
                study.saved(),
                "G,LOWEST,HIGHEST,DATED,TRAILING\nA,amiss,a3,a1,amiss\nB,bmiss,b2,b2,bmiss\n"
            );
        }
    }
}
