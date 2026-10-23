#![cfg(feature = "yaml")]
//! Execution contracts: completed intermediate scopes, completed lookup
//! assertions and declarative case intermediates.
mod execution {
    mod completed_intermediate_scopes {
        //! Original committed YAML and independent expected artifacts remain the oracle.
        use crate::support::Edit as _;
        use std::{fs, path::Path};
        use yamaa_adapters::file_application;
        use yamaa_core::domain_entry::Request;
        struct Fixture(crate::support::Scratch);
        impl Fixture {
            fn new() -> Self {
                let p = crate::support::Scratch::new("intermediate-scopes");
                Self(p)
            }
        }
        fn copy(source: &Path, destination: &Path) {
            fs::create_dir_all(destination).unwrap();
            for item in fs::read_dir(source).unwrap() {
                let item = item.unwrap();
                if item.file_type().unwrap().is_dir() {
                    copy(&item.path(), &destination.join(item.file_name()));
                } else {
                    fs::copy(item.path(), destination.join(item.file_name())).unwrap();
                }
            }
        }
        fn issue(issue: &yamaa_core::diagnostic::Diagnostic) -> yamaa_core::json::Value {
            crate::support::study::issue_row(issue)
        }
        fn expected(path: &Path) -> yamaa_core::json::Value {
            let text = fs::read_to_string(path).unwrap();
            let d = yamaa_adapters::yaml_decode::decode_yaml(text.as_bytes())
                .unwrap()
                .document;
            fn value(d: &yamaa_core::schema::Document, id: usize) -> yamaa_core::json::Value {
                use yamaa_core::schema::DocumentNode as N;
                match &d.nodes()[id] {
                    N::Null => yamaa_core::json::Value::Null,
                    N::Boolean(v) => (*v).into(),
                    N::Integer(v) => yamaa_adapters::json_wire::decode(v).unwrap(),
                    N::Float(v) => yamaa_adapters::json!(v),
                    N::Text(v) => v.clone().into(),
                    N::Sequence(ids) => ids.iter().map(|&id| value(d, id)).collect(),
                    N::Mapping(pairs) => pairs
                        .iter()
                        .map(|&(key, item)| {
                            (value(d, key).as_str().unwrap().to_owned(), value(d, item))
                        })
                        .collect(),
                }
            }
            value(&d, d.root())
        }
        fn unchanged_original(name: &str) {
            let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");

            let original = root.join("benchmarks").join(name);
            let fixture = Fixture::new();
            copy(&original.join("input"), &fixture.0.join("input"));
            fs::copy(original.join("spec.yaml"), fixture.0.join("spec.yaml")).unwrap();
            let spec = fixture.0.join("spec.yaml").to_str().unwrap().to_owned();
            let id = crate::support::study::identity(name, &spec);
            let outcome = crate::support::study::domain(
                yamaa_core::project_function::Language::Python,
                Request {
                    specification: &spec,
                    environment: None,
                },
                id,
            );
            if let Ok(mut result) = outcome {
                if name.starts_with("negative-") {
                    assert_eq!(result.issues().len(), 1, "{name}");
                    let expected: yamaa_core::json::Value =
                        expected(&original.join("expected/error.yaml"));
                    assert_eq!(issue(&result.issues()[0]), expected, "{name}");
                    assert!(result.save().is_err());
                } else {
                    assert!(result.issues().is_empty(), "{name}: {:?}", result.issues());
                    assert!(result.save().unwrap());
                    for file in fs::read_dir(original.join("expected")).unwrap() {
                        let file = file.unwrap();
                        assert_eq!(
                            fs::read(fixture.0.join(file.file_name())).unwrap(),
                            fs::read(file.path()).unwrap(),
                            "{name}"
                        );
                    }
                }
            } else {
                let actual = outcome.err().unwrap().iter().map(issue).collect::<Vec<_>>();
                if name == "negative-lookup-id-collision" {
                    let expected: yamaa_core::json::Value =
                        expected(&original.join("expected/error.yaml"));
                    assert_eq!(actual, vec![expected], "{name}");
                } else {
                    panic!("{name}: {actual:?}");
                }
            }
        }

        macro_rules! original {
            ($test:ident,$case:literal) => {
                #[test]
                fn $test() {
                    unchanged_original($case)
                }
            };
        }
        original!(unique, "schema-lookup-intermediate-unique");
        original!(duplicate, "negative-lookup-duplicate-intermediate");
        original!(collision, "negative-lookup-id-collision");
        original!(ae, "sdtm-ae-meddra-hierarchy");
        original!(cm, "sdtm-cm-whodrug-coding");
        original!(vs, "sdtm-vs-collected-form");
        original!(lb, "sdtm-lb-last-observation-before-exposure");
        original!(timepoints, "sdtm-vs-planned-time-points");
        original!(qs, "sdtm-qs-questionnaire-items");
        original!(fa, "sdtm-fa-event-findings");
        original!(ex, "sdtm-ex-from-ec");
        original!(derived_filter, "schema-lookup-intermediate-derived-filter");
        original!(
            correlated_derive,
            "schema-lookup-intermediate-correlated-derive"
        );
        original!(derived_window, "schema-window-intermediate-rank");
        original!(eot, "adam-adlb-end-of-treatment");
        original!(serious, "adam-adae-serious-events");
        original!(self_unique, "adam-adae-prior-serious-event");

        original!(advs, "adam-advs-windows");

        fn independent(
            spec: &str,
            source: &str,
            donor: Option<&str>,
        ) -> (
            Fixture,
            Result<file_application::Domain, file_application::Failure>,
        ) {
            let fixture = Fixture::new();
            fs::write(fixture.0.join("spec.yaml"), spec).unwrap();
            fs::write(fixture.0.join("source.csv"), source).unwrap();
            if let Some(donor) = donor {
                fs::write(fixture.0.join("donor.csv"), donor).unwrap();
            }
            let path = fixture.0.join("spec.yaml").to_str().unwrap().to_owned();
            let id = crate::support::study::identity("independent-intermediate", &path);
            let result = crate::support::study::domain(
                yamaa_core::project_function::Language::Python,
                Request {
                    specification: &path,
                    environment: None,
                },
                id,
            );
            (fixture, result)
        }
        #[test]
        fn unused_unique_checks_are_eager_and_retain_all_failed_declarations() {
            let spec="yamaa: \"1.0\"\ntype: spec\ndomain: OUT\nkeys: [ID]\nbase: SRC\ninput: {SRC: {path: source.csv, types: {ID: int}}, DONOR: {path: donor.csv, types: {ID: int, X: int}}}\noutput: {path: out.csv, columns: [ID]}\nintermediates:\n  - id: P\n    dataset: DONOR\n    key: [ID]\n    verifications:\n      - unique: {columns: [ID], id: donor-id}\n      - unique: [X]\ncolumns:\n  - {name: ID, type: int, derivation: SRC.ID}\n";
            let (fixture, result) = independent(spec, "ID\n1\n", Some("ID,X\n1,2\n1,2\n"));
            let mut result = result.unwrap();
            let actual = result.issues().iter().map(issue).collect::<Vec<_>>();
            assert_eq!(
                actual,
                vec![
                    yamaa_adapters::json!({"phase":"verification","condition":"duplicate_intermediate_records","rule":"LOOKUP-17","spec_paths":["intermediates.P.verifications[0].unique"],"context":{"intermediate":"P","dataset":"DONOR","columns":["ID"],"duplicate_count":1,"verification_id":"donor-id"}}),
                    yamaa_adapters::json!({"phase":"verification","condition":"duplicate_intermediate_records","rule":"LOOKUP-17","spec_paths":["intermediates.P.verifications[1].unique"],"context":{"intermediate":"P","dataset":"DONOR","columns":["X"],"duplicate_count":1}})
                ]
            );
            assert!(result.save().is_err());
            assert!(!fixture.0.join("out.csv").exists());
        }
        #[test]
        fn unused_donor_derivation_failure_is_not_hidden_by_an_unread_lookup() {
            let spec="yamaa: \"1.0\"\ntype: spec\ndomain: OUT\nkeys: [ID]\nbase: SRC\ninput: {SRC: {path: source.csv, types: {ID: int}}, DONOR: {path: donor.csv, types: {ID: int, X: int}}}\noutput: {path: out.csv, columns: [ID]}\nintermediates:\n  - id: P\n    dataset: DONOR\n    key: [ID]\n    derivations: {NEXT: 'X + 1'}\n    verifications: [{unique: [ID]}]\ncolumns:\n  - {name: ID, type: int, derivation: SRC.ID}\n";
            let (fixture, result) =
                independent(spec, "ID\n1\n", Some("ID,X\n1,9223372036854775807\n"));
            let mut result = result.unwrap();
            assert_eq!(result.issues().len(), 1);
            assert_eq!(result.issues()[0].condition(), "integer_overflow");
            assert!(
                result.issues()[0].spec_paths[0].starts_with("intermediates.P.derivations.NEXT")
            );
            assert!(result.save().is_err());
            assert!(!fixture.0.join("out.csv").exists());
        }
        #[test]
        fn current_row_correlation_cannot_turn_a_unique_check_into_a_lazy_check() {
            let spec="yamaa: \"1.0\"\ntype: spec\ndomain: OUT\nkeys: [ID]\nbase: SRC\ninput: {SRC: {path: source.csv, types: {ID: int}}, DONOR: {path: donor.csv, types: {ID: int, X: int}}}\noutput: {path: out.csv, columns: [ID]}\nintermediates:\n  - id: P\n    dataset: DONOR\n    key: [ID]\n    filter: 'DONOR.X > SRC.ID'\n    derivations: {NEXT: 'X + SRC.ID'}\n    verifications: [{unique: [ID]}]\ncolumns:\n  - {name: ID, type: int, derivation: SRC.ID}\n";
            let (_fixture, result) = independent(spec, "ID\n1\n", Some("ID,X\n1,2\n"));
            let result = match result {
                Err(issues) => issues,
                Ok(domain) => domain.issues().to_vec(),
            };
            assert_eq!(
                result
                    .iter()
                    .map(|issue| (issue.condition(), issue.spec_paths[0].as_str()))
                    .collect::<Vec<_>>(),
                vec![
                    (
                        "correlated_filter_with_unique_verification",
                        "intermediates.P.verifications[0].unique"
                    ),
                    (
                        "correlated_derivation_with_unique_verification",
                        "intermediates.P.verifications[0].unique"
                    )
                ]
            );
        }

        #[test]
        fn an_earlier_selector_key_uses_the_later_donor_current_record_owner() {
            let spec = "yamaa: \"1.0\"\ntype: spec\ndomain: OUT\nkeys: [ID]\nbase: SRC\ninput: {SRC: {path: source.csv, types: {ID: int, VAL: int}}, DONOR: {path: donor.csv, types: {KEY: int}}}\noutput: {path: out.csv, columns: [ID, VALUE]}\nintermediates:\n  - id: EARLIER\n    dataset: SRC\n    key: {ID: DONOR.KEY}\n  - id: LATER\n    dataset: DONOR\n    key: {KEY: ID}\n    derivations: {LOOKED_UP: EARLIER.VAL}\ncolumns:\n  - {name: ID, type: int, derivation: SRC.ID}\n  - {name: VALUE, type: int, derivation: LATER.LOOKED_UP}\n";
            // Different field names and reversed donor order expose owner/coordinate aliasing.
            let (fixture, result) =
                independent(spec, "ID,VAL\n1,100\n2,200\n", Some("KEY\n2\n1\n"));
            let mut result = result.unwrap();
            assert!(result.issues().is_empty(), "{:?}", result.issues());
            assert!(result.save().unwrap());
            assert_eq!(
                fs::read(fixture.0.join("out.csv")).unwrap(),
                b"ID,VALUE\n1,100\n2,200\n"
            );
        }

        #[test]
        fn retained_foreign_key_is_valid_until_it_is_read_in_the_wrong_current_scope() {
            let declaration = "yamaa: \"1.0\"\ntype: spec\ndomain: OUT\nkeys: [ID]\nbase: SRC\ninput: {SRC: {path: source.csv, types: {ID: int}}, DONOR: {path: donor.csv, types: {KEY: int}}}\noutput: {path: out.csv, columns: [ID]}\nintermediates:\n  - id: P\n    dataset: SRC\n    key: {ID: DONOR.KEY}\ncolumns:\n  - {name: ID, type: int, derivation: SRC.ID}\n";
            let (fixture, result) = independent(declaration, "ID\n1\n", Some("KEY\n2\n"));
            let mut result = result.unwrap();
            assert!(result.issues().is_empty(), "{:?}", result.issues());
            assert!(result.save().unwrap());
            assert_eq!(fs::read(fixture.0.join("out.csv")).unwrap(), b"ID\n1\n");
            let used = declaration.edit("columns: [ID]", "columns: [ID, VALUE]")
                + "  - {name: VALUE, type: int, derivation: P.ID}\n";
            let (fixture, result) = independent(&used, "ID\n1\n", Some("KEY\n2\n"));
            let mut result = result.unwrap();
            assert_eq!(
                result.issues().iter().map(issue).collect::<Vec<_>>(),
                vec![
                    yamaa_adapters::json!({"phase":"validation","condition":"unknown_field","rule":"LOOKUP-5","spec_paths":["intermediates.P.key"],"context":{"identifier":"DONOR.KEY","intermediate":"P"}})
                ]
            );
            assert!(result.save().is_err());
            assert!(!fixture.0.join("out.csv").exists());
        }

        #[test]
        fn a_foreign_row_driver_owns_its_qualified_selector_keys() {
            let spec = "yamaa: \"1.0\"\ntype: spec\ndomain: OUT\nkeys: [ID]\nbase: SRC\ninput: {SRC: {path: source.csv, types: {ID: int, VAL: int}}, DONOR: {path: donor.csv, types: {KEY: int}}}\noutput: {path: out.csv, columns: [ID, VALUE]}\nintermediates:\n  - id: P\n    dataset: SRC\n    key: {ID: DONOR.KEY}\ncolumns:\n  - {name: ID, type: int}\n  - {name: VALUE, type: int}\nrows:\n  - id: donor\n    dataset: DONOR\n    derivations: {ID: DONOR.KEY, VALUE: P.VAL}\n";
            let (fixture, result) =
                independent(spec, "ID,VAL\n1,100\n2,200\n", Some("KEY\n2\n1\n"));
            let mut result = result.unwrap();
            assert!(result.issues().is_empty(), "{:?}", result.issues());
            assert!(result.save().unwrap());
            assert_eq!(
                fs::read(fixture.0.join("out.csv")).unwrap(),
                b"ID,VALUE\n2,200\n1,100\n"
            );
        }

        #[test]
        fn an_earlier_selectors_filter_uses_the_reading_donor_current_fields() {
            let spec = "yamaa: \"1.0\"\ntype: spec\ndomain: OUT\nkeys: [ID]\nbase: SRC\ninput: {SRC: {path: source.csv, types: {ID: int, VAL: int}}, DONOR: {path: donor.csv, types: {KEY: int, MIN: int}}}\noutput: {path: out.csv, columns: [ID, VALUE]}\nintermediates:\n  - id: EARLIER\n    dataset: SRC\n    key: {ID: DONOR.KEY}\n    filter: 'SRC.VAL > DONOR.MIN'\n    absent: -1\n  - id: LATER\n    dataset: DONOR\n    key: {KEY: ID}\n    derivations: {LOOKED_UP: EARLIER.VAL}\ncolumns:\n  - {name: ID, type: int, derivation: SRC.ID}\n  - {name: VALUE, type: int, derivation: LATER.LOOKED_UP}\n";
            let (fixture, result) = independent(
                spec,
                "ID,VAL\n1,100\n2,200\n",
                Some("KEY,MIN\n2,50\n1,150\n"),
            );
            let mut result = result.unwrap();
            assert!(result.issues().is_empty(), "{:?}", result.issues());
            assert!(result.save().unwrap());
            assert_eq!(
                fs::read(fixture.0.join("out.csv")).unwrap(),
                b"ID,VALUE\n1,-1\n2,200\n"
            );
        }

        #[test]
        fn named_match_keys_bind_an_earlier_derived_field_of_the_reading_donor() {
            let spec = "yamaa: \"1.0\"\ntype: spec\ndomain: OUT\nkeys: [ID]\nbase: SRC\ninput: {SRC: {path: source.csv, types: {ID: int, VAL: int}}, DONOR: {path: donor.csv, types: {KEY: int}}}\noutput: {path: out.csv, columns: [ID, VALUE]}\nintermediates:\n  - id: EARLIER\n    dataset: SRC\n    key: {ID: DONOR.NEWKEY}\n  - id: LATER\n    dataset: DONOR\n    key: {KEY: ID}\n    derivations:\n      NEWKEY: KEY\n      LOOKED_UP: EARLIER.VAL\ncolumns:\n  - {name: ID, type: int, derivation: SRC.ID}\n  - {name: VALUE, type: int, derivation: LATER.LOOKED_UP}\n";
            let (fixture, result) =
                independent(spec, "ID,VAL\n1,100\n2,200\n", Some("KEY\n2\n1\n"));
            let mut result = result.unwrap();
            assert!(result.issues().is_empty(), "{:?}", result.issues());
            assert!(result.save().unwrap());
            assert_eq!(
                fs::read(fixture.0.join("out.csv")).unwrap(),
                b"ID,VALUE\n1,100\n2,200\n"
            );
        }

        #[test]
        fn contextual_keys_reject_later_unknown_wrong_owner_and_known_type_at_binding() {
            let base = "yamaa: \"1.0\"\ntype: spec\ndomain: OUT\nkeys: [ID]\nbase: SRC\ninput: {SRC: {path: source.csv, types: {ID: int, VAL: int}}, DONOR: {path: donor.csv, types: {KEY: int}}}\noutput: {path: out.csv, columns: [ID, VALUE]}\nintermediates:\n  - id: EARLIER\n    dataset: SRC\n    key: {ID: DONOR.NEWKEY}\n  - id: LATER\n    dataset: DONOR\n    key: {KEY: ID}\n    derivations:\n      NEWKEY: KEY\n      LOOKED_UP: EARLIER.VAL\ncolumns:\n  - {name: ID, type: int, derivation: SRC.ID}\n  - {name: VALUE, type: int, derivation: LATER.LOOKED_UP}\n";
            let unknown = |name: &str| yamaa_adapters::json!({"phase":"validation","condition":"unknown_field","rule":"LOOKUP-16","spec_paths":["intermediates.LATER.derivations.LOOKED_UP.source"],"context":{"identifier":name,"intermediate":"LATER","read":"EARLIER"}});
            for (spec, first) in [
                (
                    base.edit(
                        "      NEWKEY: KEY\n      LOOKED_UP: EARLIER.VAL",
                        "      LOOKED_UP: EARLIER.VAL\n      NEWKEY: KEY",
                    ),
                    unknown("DONOR.NEWKEY"),
                ),
                (
                    base.edit("ID: DONOR.NEWKEY", "ID: DONOR.ABSENT"),
                    unknown("DONOR.ABSENT"),
                ),
                (
                    base.edit("ID: DONOR.NEWKEY", "ID: WRONG.NEWKEY"),
                    unknown("WRONG.NEWKEY"),
                ),
                (
                    base.edit("NEWKEY: KEY", "NEWKEY: \"'wrong-type'\""),
                    yamaa_adapters::json!({"phase":"validation","condition":"incompatible_input_type","rule":"LOOKUP-9","spec_paths":["intermediates.EARLIER.key"],"context":{"intermediate":"EARLIER","source":"DONOR.NEWKEY","expected":"str","actual":"int"}}),
                ),
            ] {
                let (fixture, result) = independent(&spec, "ID,VAL\n1,100\n", Some("KEY\n2\n"));
                let mut result = result.unwrap();
                assert_eq!(
                    result.issues().iter().map(issue).collect::<Vec<_>>(),
                    vec![first],
                    "{spec}"
                );
                assert!(result.save().is_err());
                assert!(!fixture.0.join("out.csv").exists());
            }
        }

        #[test]
        fn self_derivations_read_completed_rows_and_publish_each_template_snapshot() {
            let spec = self_derivation_spec();
            let (fixture, result) = independent(spec, "ID,VAL\n1,10\n", None);
            let mut result = result.unwrap();
            assert!(result.issues().is_empty(), "{:?}", result.issues());
            assert!(result.save().unwrap());
            assert_eq!(
                fs::read(fixture.0.join("out.csv")).unwrap(),
                b"ID,STAGE,VALUE\n1,1,10\n1,2,21\n1,3,43\n"
            );
        }

        fn self_derivation_spec() -> &'static str {
            "yamaa: \"1.0\"\ntype: spec\ndomain: OUT\nkeys: [ID, STAGE]\nbase: SRC\ninput: {SRC: {path: source.csv, types: {ID: int, VAL: int}}}\noutput: {path: out.csv, columns: [ID, STAGE, VALUE]}\nintermediates:\n  - id: P\n    dataset: SELF\n    key: [ID]\n    derivations:\n      TWICE: 'SELF.VALUE * 2'\n      PLUS: 'TWICE + 1'\n    order_by: [SELF.STAGE]\n    keep: last\ncolumns:\n  - {name: ID, type: int, derivation: SRC.ID}\n  - {name: STAGE, type: int}\n  - {name: VALUE, type: int}\nrows:\n  - id: first\n    derivations: {STAGE: 1, VALUE: SRC.VAL}\n  - id: second\n    derivations: {STAGE: 2, VALUE: P.PLUS}\n  - id: third\n    derivations: {STAGE: 3, VALUE: P.PLUS}\n"
        }

        #[test]
        fn self_derivations_reject_later_and_unknown_fields() {
            for reference in ["SELF.PLUS", "PLUS", "SELF.ABSENT", "ABSENT"] {
                let spec = self_derivation_spec().edit("SELF.VALUE * 2", reference);
                let (fixture, result) = independent(&spec, "ID,VAL\n1,10\n", None);
                let mut result = result.unwrap();
                assert_eq!(
                    result.issues().iter().map(issue).collect::<Vec<_>>(),
                    vec![yamaa_adapters::json!({
                        "phase":"validation", "condition":"unknown_field", "rule":"LOOKUP-15",
                        "spec_paths":["intermediates.P.derivations.TWICE.source"],
                        "context":{"intermediate":"P", "identifier":reference}
                    })],
                    "{reference}"
                );
                assert!(result.save().is_err());
                assert!(!fixture.0.join("out.csv").exists());
            }
        }
        #[test]
        fn self_derivations_cannot_shadow_completed_stored_fields() {
            let spec = self_derivation_spec()
                .edit("TWICE:", "VALUE:")
                .edit("TWICE + 1", "VALUE + 1");
            let (fixture, result) = independent(&spec, "ID,VAL\n1,10\n", None);
            let mut result = result.unwrap();
            assert_eq!(
                result.issues().iter().map(issue).collect::<Vec<_>>(),
                vec![yamaa_adapters::json!({
                    "phase":"validation", "condition":"duplicate_derivation", "rule":"LOOKUP-15",
                    "spec_paths":["intermediates.P.derivations.VALUE"],
                    "context":{"intermediate":"P", "derivation":"VALUE", "identifier":"SELF.VALUE"}
                })]
            );
            assert!(result.save().is_err());
            assert!(!fixture.0.join("out.csv").exists());
        }
        #[test]
        fn self_derived_unique_checks_are_eager_after_a_completed_template() {
            let spec = self_derivation_spec()
                .edit(
                    "    key: [ID]",
                    "    key: [ID]\n    verifications: [{unique: [TWICE]}]",
                )
                .replace("VALUE: P.PLUS", "VALUE: '9223372036854775807 + 1'");
            let (fixture, result) = independent(&spec, "ID,VAL\n1,10\n2,10\n", None);
            let mut result = result.unwrap();
            assert_eq!(
                result.issues().iter().map(issue).collect::<Vec<_>>(),
                vec![yamaa_adapters::json!({
                    "phase":"verification", "condition":"duplicate_intermediate_records", "rule":"LOOKUP-17",
                    "spec_paths":["intermediates.P.verifications[0].unique"],
                    "context":{"intermediate":"P", "dataset":"SELF", "columns":["TWICE"], "duplicate_count":1}
                })]
            );
            assert!(result.save().is_err());
            assert!(!fixture.0.join("out.csv").exists());
        }
        #[test]
        fn a_first_template_cannot_read_a_self_derived_field_from_future_rows() {
            let spec = self_derivation_spec().edit("VALUE: SRC.VAL", "VALUE: P.PLUS");
            let (fixture, result) = independent(&spec, "ID,VAL\n1,10\n", None);
            let issues = result
                .err()
                .expect("the first template is refused at preparation");
            assert_eq!(issues.len(), 1);
            assert_eq!(issues[0].condition(), "phase_boundary");
            assert_eq!(
                issues[0].spec_paths,
                ["rows.first.derivations.VALUE.source"]
            );
            assert!(!fixture.0.join("out.csv").exists());
        }

        #[test]
        fn a_self_donor_derived_key_can_read_an_input_backed_selector() {
            let spec = self_derivation_spec()
                .edit(
                    "  - id: P",
                    "  - id: INPUT\n    dataset: SRC\n    key: {ID: SELF.NEWKEY}\n  - id: P",
                )
                .edit(
                    "      TWICE: 'SELF.VALUE * 2'",
                    "      NEWKEY: ID\n      TWICE: 'INPUT.VAL * 2'",
                );
            let (fixture, result) = independent(&spec, "ID,VAL\n1,10\n", None);
            let mut result = result.unwrap();
            assert!(result.issues().is_empty(), "{:?}", result.issues());
            assert!(result.save().unwrap());
            assert_eq!(
                fs::read(fixture.0.join("out.csv")).unwrap(),
                b"ID,STAGE,VALUE\n1,1,10\n1,2,21\n1,3,21\n"
            );
        }

        fn between_spec(intermediates: &str, x: &str, kind: &str, donor_time: bool) -> String {
            let dm = if donor_time {
                "{path: dm.csv, types: {ADTM: datetime}}"
            } else {
                "dm.csv"
            };
            format!("yamaa: \"1.0\"\ntype: spec\ndomain: ADPC\nkeys: [USUBJID, PCSEQ]\nbase: PC\ninput:\n  PC: {{path: pc.csv, types: {{PCSEQ: int, PCDTC: datetime}}}}\n  EX: {{path: ex.csv, types: {{EXSTDTC: datetime}}}}\n  DM: {dm}\nintermediates:\n{intermediates}columns:\n  - {{name: USUBJID, type: str, derivation: PC.USUBJID}}\n  - {{name: PCSEQ, type: int, derivation: PC.PCSEQ}}\n  - {{name: ADTM, type: datetime, derivation: PC.PCDTC}}\n  - {{name: LDOSEDTM, type: datetime, derivation: LAST_DOSE.EXSTDTC}}\n  - {{name: X, type: {kind}, derivation: {x}}}\noutput: {{path: out.csv, columns: [USUBJID, PCSEQ, ADTM, LDOSEDTM, X]}}\n")
        }
        fn between_fixture(spec: &str, donor_time: bool) -> Fixture {
            let fixture = Fixture::new();
            fs::write(fixture.0.join("spec.yaml"), spec).unwrap();
            fs::write(fixture.0.join("pc.csv"), "USUBJID,PCSEQ,PCDTC\nS1,1,2030-01-01T09:00:00\nS1,2,2030-01-02T09:00:00\nS2,1,2030-01-01T10:00:00\n").unwrap();
            fs::write(fixture.0.join("ex.csv"), "USUBJID,EXTRT,EXSTDTC\nS1,DRUG A,2030-01-01T07:00:00\nS1,DRUG B,2030-01-02T07:00:00\nS2,DRUG A,2030-01-01T07:30:00\n").unwrap();
            let dm = if donor_time {
                "USUBJID,SEX,ADTM\nS1,F,2030-01-02T09:00:00\nS2,M,2030-01-01T10:00:00\n"
            } else {
                "USUBJID,SEX\nS1,F\nS2,M\n"
            };
            fs::write(fixture.0.join("dm.csv"), dm).unwrap();
            fixture
        }
        fn between_run(fixture: &Fixture) -> file_application::Domain {
            let path = fixture.0.join("spec.yaml").to_str().unwrap().to_owned();
            crate::support::study::domain(
                yamaa_core::project_function::Language::Python,
                Request {
                    specification: &path,
                    environment: None,
                },
                crate::support::study::identity("between-donor-scope", &path),
            )
            .unwrap_or_else(|issues| panic!("preparation {issues:?}"))
        }
        const LAST_DOSE: &str = "  - id: LAST_DOSE\n    dataset: EX\n    key: [USUBJID]\n    between: {value: ADTM, lower: EXSTDTC}\n    order_by: [EX.EXSTDTC]\n    keep: last\n";
        const SUBJ: &str = "  - id: SUBJ\n    dataset: DM\n    key: [USUBJID]\n    derivations:\n      SEXL: {str_case: {source: SEX, to: lower}}\n";
        const LAST_TRT: &str = "  - id: LAST_TRT\n    dataset: EX\n    key: [USUBJID]\n    between: {value: ADTM, lower: EXSTDTC}\n    derivations:\n      TRT: EX.EXTRT\n    order_by: [EX.EXSTDTC]\n    keep: last\n";

        #[test]
        fn unrelated_donor_derivations_do_not_rebase_earlier_between_values() {
            for (intermediates, x, expected) in [
                (format!("{LAST_DOSE}{SUBJ}"), "SUBJ.SEXL", ["f", "f", "m"]),
                (format!("{SUBJ}{LAST_DOSE}"), "SUBJ.SEXL", ["f", "f", "m"]),
                (
                    format!("{}{SUBJ}", LAST_DOSE.edit("value: ADTM", "value: PC.PCDTC")),
                    "SUBJ.SEXL",
                    ["f", "f", "m"],
                ),
                (
                    format!("{LAST_DOSE}  - id: SUBJ\n    dataset: DM\n    key: [USUBJID]\n"),
                    "SUBJ.SEX",
                    ["F", "F", "M"],
                ),
                (
                    format!("{LAST_DOSE}{LAST_TRT}"),
                    "LAST_TRT.TRT",
                    ["DRUG A", "DRUG B", "DRUG A"],
                ),
                (
                    format!("{LAST_TRT}{LAST_DOSE}"),
                    "LAST_TRT.TRT",
                    ["DRUG A", "DRUG B", "DRUG A"],
                ),
            ] {
                let spec = between_spec(&intermediates, x, "str", false);
                let fixture = between_fixture(&spec, false);
                let mut result = between_run(&fixture);
                assert!(result.issues().is_empty(), "{spec}: {:?}", result.issues());
                assert!(result.save().unwrap());
                let expected = format!("USUBJID,PCSEQ,ADTM,LDOSEDTM,X\nS1,1,2030-01-01T09:00:00,2030-01-01T07:00:00,{}\nS1,2,2030-01-02T09:00:00,2030-01-02T07:00:00,{}\nS2,1,2030-01-01T10:00:00,2030-01-01T07:30:00,{}\n", expected[0], expected[1], expected[2]);
                assert_eq!(
                    fs::read_to_string(fixture.0.join("out.csv")).unwrap(),
                    expected
                );
            }
        }

        #[test]
        fn a_read_of_between_requires_its_value_in_the_reading_donor_scope() {
            let reader = "  - id: SUBJ\n    dataset: DM\n    key: [USUBJID]\n    derivations:\n      LD: LAST_DOSE.EXSTDTC\n";
            let spec = between_spec(
                &format!("{LAST_DOSE}{reader}"),
                "SUBJ.LD",
                "datetime",
                false,
            );
            let fixture = between_fixture(&spec, false);
            let mut result = between_run(&fixture);
            assert_eq!(
                result.issues().iter().map(issue).collect::<Vec<_>>(),
                vec![yamaa_adapters::json!({
                    "phase": "validation", "condition": "unknown_field", "rule": "LOOKUP-16",
                    "spec_paths": ["intermediates.SUBJ.derivations.LD.source"],
                    "context": {"intermediate": "SUBJ", "identifier": "ADTM", "read": "LAST_DOSE"}
                })]
            );
            assert!(result.output().is_none());
            assert!(result.save().is_err());
            assert!(!fixture.0.join("out.csv").exists());

            let spec = between_spec(&format!("{LAST_DOSE}{reader}"), "SUBJ.LD", "datetime", true);
            let fixture = between_fixture(&spec, true);
            let mut result = between_run(&fixture);
            assert!(result.issues().is_empty(), "{:?}", result.issues());
            assert!(result.save().unwrap());
            assert_eq!(fs::read_to_string(fixture.0.join("out.csv")).unwrap(), "USUBJID,PCSEQ,ADTM,LDOSEDTM,X\nS1,1,2030-01-01T09:00:00,2030-01-01T07:00:00,2030-01-02T07:00:00\nS1,2,2030-01-02T09:00:00,2030-01-02T07:00:00,2030-01-02T07:00:00\nS2,1,2030-01-01T10:00:00,2030-01-01T07:30:00,2030-01-01T07:30:00\n");
        }

        #[test]
        fn rebasing_retains_transitive_selector_dependencies_and_missing_names() {
            let second = "  - id: SECOND\n    dataset: EX\n    key: [USUBJID]\n    between: {value: LAST_DOSE.EXSTDTC, lower: EXSTDTC}\n    order_by: [EX.EXSTDTC]\n    keep: last\n";
            let reader = "  - id: SUBJ\n    dataset: DM\n    key: [USUBJID]\n    derivations:\n      LD: SECOND.EXSTDTC\n";
            let declarations = format!("{LAST_DOSE}{second}{reader}");
            let spec = between_spec(&declarations, "SUBJ.LD", "datetime", false);
            let fixture = between_fixture(&spec, false);
            let result = between_run(&fixture);
            assert_eq!(
                result.issues().iter().map(issue).collect::<Vec<_>>(),
                vec![yamaa_adapters::json!({
                    "phase": "validation", "condition": "unknown_field", "rule": "LOOKUP-16",
                    "spec_paths": ["intermediates.SUBJ.derivations.LD.source"],
                    "context": {"intermediate": "SUBJ", "identifier": "ADTM", "read": "SECOND"}
                })]
            );
            let spec = between_spec(&declarations, "SUBJ.LD", "datetime", true);
            let fixture = between_fixture(&spec, true);
            let mut result = between_run(&fixture);
            assert!(result.issues().is_empty(), "{:?}", result.issues());
            assert!(result.save().unwrap());
            assert_eq!(fs::read_to_string(fixture.0.join("out.csv")).unwrap(), "USUBJID,PCSEQ,ADTM,LDOSEDTM,X\nS1,1,2030-01-01T09:00:00,2030-01-01T07:00:00,2030-01-02T07:00:00\nS1,2,2030-01-02T09:00:00,2030-01-02T07:00:00,2030-01-02T07:00:00\nS2,1,2030-01-01T10:00:00,2030-01-01T07:30:00,2030-01-01T07:30:00\n");
        }

        /// A `between.value` qualified by an input reads that input's record
        /// only where it drives the reading row or donor record. Elsewhere it
        /// names no current-row value, however the intermediate is reached.
        #[test]
        fn a_qualified_between_value_reads_only_its_own_driver_record() {
            const WINDOW: &str = "  - id: W\n    dataset: WIN\n    key: [USUBJID]\n    between: {value: REF.REFDAY, lower: LO, upper: HI}\n    order_by: [WIN.LO]\n    keep: first\n";
            let run = |body: &str| {
                let fixture = Fixture::new();
                fs::write(fixture.0.join("spec.yaml"), format!("yamaa: \"1.0\"\ntype: spec\ndomain: OUT\ninput:\n  SUBJ: {{path: subj.csv, types: {{SUBJDAY: int}}}}\n  REF: {{path: ref.csv, types: {{REFDAY: int}}}}\n  WIN: {{path: win.csv, types: {{LO: int, HI: int}}}}\n  EVT: {{path: evt.csv, key: [USUBJID], types: {{EVDAY: int}}}}\n{body}")).unwrap();
                // REF orders subjects the other way and keeps its day in the
                // column where SUBJ keeps a different day.
                fs::write(fixture.0.join("subj.csv"), "USUBJID,SUBJDAY\nS1,5\nS2,50\n").unwrap();
                fs::write(fixture.0.join("ref.csv"), "USUBJID,REFDAY\nS2,5\nS1,50\n").unwrap();
                fs::write(fixture.0.join("win.csv"), "USUBJID,NAME,LO,HI\nS1,EARLY,1,10\nS1,LATE,11,100\nS2,EARLY,1,10\nS2,LATE,11,100\n").unwrap();
                fs::write(fixture.0.join("evt.csv"), "USUBJID,EVDAY\nS1,3\nS2,3\n").unwrap();
                let path = fixture.0.join("spec.yaml").to_str().unwrap().to_owned();
                let mut result = match crate::support::study::domain(
                    yamaa_core::project_function::Language::Python,
                    Request {
                        specification: &path,
                        environment: None,
                    },
                    crate::support::study::identity("between-current-driver", &path),
                ) {
                    Ok(result) => result,
                    Err(failure) => return Err(failure.iter().map(issue).collect::<Vec<_>>()),
                };
                let issues = result.issues().iter().map(issue).collect::<Vec<_>>();
                if !issues.is_empty() {
                    assert!(result.save().is_err());
                    return Err(issues);
                }
                assert!(result.save().unwrap());
                Ok(fs::read_to_string(fixture.0.join("out.csv")).unwrap())
            };
            let base = "keys: [USUBJID]\nbase: SUBJ\noutput: {path: out.csv, columns: [USUBJID, WINDOW]}\nintermediates:\n";
            let column = |derivation: &str| {
                format!("columns:\n  - {{name: USUBJID, type: str, derivation: SUBJ.USUBJID}}\n  - {{name: WINDOW, type: str, derivation: {derivation}}}\n")
            };
            // The base driver's rows name no REF record: read directly, through
            // another intermediate's value, from an aggregate's derive step, or
            // through an aggregate's range value.
            let direct = format!("{base}{WINDOW}{}", column("W.NAME"));
            let chained = WINDOW.edit("id: W\n", "id: W2\n")
                + &WINDOW.edit("value: REF.REFDAY", "value: W2.LO");
            let chained = format!("{base}{chained}{}", column("W.NAME"));
            let aggregated = |derivation: &str| {
                format!("{base}{WINDOW}{}", column(derivation)).edit(
                    "type: str, derivation: {aggregate",
                    "type: int, derivation: {aggregate",
                )
            };
            let derived = aggregated("{aggregate: {key: [USUBJID], derivations: {DAY: {type: int, derivation: EVT.EVDAY}, LOW: {type: int, derivation: W.LO}}, expr: 'MAX(LOW)'}}");
            let ranged = aggregated(
                "{aggregate: {between: {value: W.LO, lower: EVDAY}, expr: 'COUNT(EVT.EVDAY)'}}",
            );
            let refused = |intermediate: &str| {
                Err(vec![yamaa_adapters::json!({
                    "phase": "validation", "condition": "unknown_field", "rule": "LOOKUP-9",
                    "spec_paths": [format!("intermediates.{intermediate}.between.value")],
                    "context": {"intermediate": intermediate, "identifier": "REF.REFDAY"}
                })])
            };
            assert_eq!(run(&direct), refused("W"));
            assert_eq!(run(&chained), refused("W2"));
            assert_eq!(run(&derived), refused("W"));
            assert_eq!(run(&ranged), refused("W"));
            // A donor record of REF supplies the value under LOOKUP-16.
            let reader = "  - id: R\n    dataset: REF\n    key: [USUBJID]\n    derivations: {WINNAME: W.NAME}\n";
            assert_eq!(
                run(&format!("{base}{WINDOW}{reader}{}", column("R.WINNAME"))),
                Ok("USUBJID,WINDOW\nS1,LATE\nS2,EARLY\n".into())
            );
            // So does REF's record in the row template it drives, read by the
            // range or by a correlated filter.
            let rows = "keys: [USUBJID, ORIGIN]\noutput: {path: out.csv, columns: [USUBJID, ORIGIN, WINDOW]}\ncolumns:\n  - {name: USUBJID, type: str}\n  - {name: ORIGIN, type: str}\n  - {name: WINDOW, type: str}\nrows:\n  - id: subj\n    dataset: SUBJ\n    derivations: {USUBJID: SUBJ.USUBJID, ORIGIN: \"'SUBJ'\", WINDOW: \"'NONE'\"}\n  - id: ref\n    dataset: REF\n    derivations: {USUBJID: REF.USUBJID, ORIGIN: \"'REF'\", WINDOW: W.NAME}\nintermediates:\n";
            let filtered = WINDOW.edit(
                "between: {value: REF.REFDAY, lower: LO, upper: HI}",
                "filter: 'WIN.LO <= REF.REFDAY AND REF.REFDAY <= WIN.HI'",
            );
            for window in [WINDOW.to_owned(), filtered] {
                assert_eq!(
                    run(&format!("{rows}{window}")),
                    Ok("USUBJID,ORIGIN,WINDOW\nS1,SUBJ,NONE\nS2,SUBJ,NONE\nS2,REF,EARLY\nS1,REF,LATE\n".into())
                );
            }
        }

        /// An aggregate's `between.value` qualified by an input reads that
        /// input's record only where it drives every row the column derives.
        /// `check` refuses any other input from the declarations alone.
        #[test]
        fn an_aggregate_between_value_reads_only_its_own_driver_record() {
            const WINDOW: &str = "intermediates:\n  - id: W\n    dataset: WIN\n    key: [USUBJID]\n    between: {value: REF.REFDAY, lower: LO, upper: HI}\n    order_by: [WIN.LO]\n    keep: first\n";
            // The check issues, and the domain issues or the saved artifact.
            let run = |body: &str| {
                let fixture = Fixture::new();
                fs::write(fixture.0.join("spec.yaml"), format!("yamaa: \"1.0\"\ntype: spec\ndomain: OUT\ninput:\n  SUBJ: {{path: subj.csv, types: {{SUBJDAY: int}}}}\n  REF: {{path: ref.csv, key: [USUBJID], types: {{REFDAY: int}}}}\n  WIN: {{path: win.csv, types: {{LO: int, HI: int}}}}\n  EVT: {{path: evt.csv, key: [USUBJID], types: {{EVDAY: int}}}}\n{body}")).unwrap();
                // REF orders subjects the other way and keeps its day in the
                // column where SUBJ keeps a different day.
                fs::write(fixture.0.join("subj.csv"), "USUBJID,SUBJDAY\nS1,5\nS2,50\n").unwrap();
                fs::write(fixture.0.join("ref.csv"), "USUBJID,REFDAY\nS2,5\nS1,50\n").unwrap();
                fs::write(fixture.0.join("win.csv"), "USUBJID,NAME,LO,HI\nS1,EARLY,1,10\nS1,LATE,11,100\nS2,EARLY,1,10\nS2,LATE,11,100\n").unwrap();
                fs::write(
                    fixture.0.join("evt.csv"),
                    "USUBJID,EVDAY\nS1,3\nS1,10\nS1,60\nS2,3\nS2,10\nS2,60\n",
                )
                .unwrap();
                let path = fixture.0.join("spec.yaml").to_str().unwrap().to_owned();
                let request = || Request {
                    specification: &path,
                    environment: None,
                };
                let identity =
                    || crate::support::study::identity("aggregate-between-current-driver", &path);
                let checked = file_application::check(
                    yamaa_core::project_function::Language::Python,
                    request(),
                    identity(),
                )
                .iter()
                .map(issue)
                .collect::<Vec<_>>();
                let built = match crate::support::study::domain(
                    yamaa_core::project_function::Language::Python,
                    request(),
                    identity(),
                ) {
                    Ok(mut result) => {
                        let issues = result.issues().iter().map(issue).collect::<Vec<_>>();
                        if issues.is_empty() {
                            assert!(result.save().unwrap());
                            Ok(fs::read_to_string(fixture.0.join("out.csv")).unwrap())
                        } else {
                            assert!(result.save().is_err());
                            Err(issues)
                        }
                    }
                    Err(failure) => Err(failure.iter().map(issue).collect::<Vec<_>>()),
                };
                (checked, built)
            };
            // Events on or after the day the value names.
            let count = |value: &str| {
                format!("{{aggregate: {{between: {{value: {value}, upper: EVDAY}}, expr: 'COUNT(EVT.*)'}}}}")
            };
            let base = |derivation: &str| {
                format!("keys: [USUBJID]\nbase: SUBJ\noutput: {{path: out.csv, columns: [USUBJID, N]}}\ncolumns:\n  - {{name: USUBJID, type: str, derivation: SUBJ.USUBJID}}\n  - {{name: RDAY, type: int, derivation: REF.REFDAY}}\n  - {{name: N, type: int, derivation: {derivation}}}\n")
            };
            let refused_at = |path: &str, identifier: &str| {
                vec![yamaa_adapters::json!({
                    "phase": "validation", "condition": "unknown_field", "rule": "LOOKUP-9",
                    "spec_paths": [path], "context": {"identifier": identifier}
                })]
            };
            let refused = |identifier: &str| {
                refused_at("columns.N.derivation.aggregate.between.value", identifier)
            };
            // The base driver's rows name no REF record, read directly or from
            // an alternative of a case.
            let alternative = format!(
                "{{case: [{{when: 'USUBJID IS NOT NULL', then: {}}}, {{otherwise: null}}]}}",
                count("REF.REFDAY")
            );
            for (derivation, expected) in [
                (count("REF.REFDAY"), refused("REF.REFDAY")),
                (
                    alternative,
                    refused_at(
                        "columns.N.derivation.case[0].then.aggregate.between.value",
                        "REF.REFDAY",
                    ),
                ),
            ] {
                assert_eq!(
                    run(&base(&derivation)),
                    (expected.clone(), Err(expected)),
                    "{derivation}"
                );
            }
            // An output-row reduction narrows by no range at all: binding refuses
            // the range itself, and `check` reports that refusal, not its value.
            let refusal = vec![yamaa_adapters::json!({
                "phase": "validation", "condition": "invalid_aggregate_context", "rule": "LOOKUP-18",
                "spec_paths": ["columns.N.derivation.aggregate.between"],
                "context": {"expr": "MAX(RDAY)", "reason": "between narrows a qualified right side only"}
            })];
            assert_eq!(
                run(&base(
                    "{aggregate: {group_by: [USUBJID], between: {value: REF.REFDAY, upper: RDAY}, expr: 'MAX(RDAY)'}}",
                )),
                (refusal.clone(), Err(refusal))
            );
            // The base driver's own record supplies the value, and so does a
            // column built for the row, here REF's day joined on the subject.
            for (value, expected) in [("SUBJ.SUBJDAY", "S1,2\nS2,1\n"), ("RDAY", "S1,1\nS2,2\n")] {
                assert_eq!(
                    run(&base(&count(value))),
                    (Vec::new(), Ok(format!("USUBJID,N\n{expected}")))
                );
            }
            // A range value that reads a named intermediate runs that
            // intermediate for the same row, so its own value is refused there.
            assert_eq!(
                run(&format!("{WINDOW}{}", base(&count("W.LO")))).1,
                Err(vec![yamaa_adapters::json!({
                    "phase": "validation", "condition": "unknown_field", "rule": "LOOKUP-9",
                    "spec_paths": ["intermediates.W.between.value"],
                    "context": {"intermediate": "W", "identifier": "REF.REFDAY"}
                })])
            );
            // A column-phase aggregate evaluates for the rows of every template:
            // with templates driven by SUBJ and by REF, neither input drives all
            // of them, and each refusal is reported once.
            let rows = |templates: &str, value: &str| {
                format!("keys: [USUBJID, ORIGIN]\noutput: {{path: out.csv, columns: [USUBJID, ORIGIN, N]}}\ncolumns:\n  - {{name: USUBJID, type: str}}\n  - {{name: ORIGIN, type: str}}\n  - {{name: N, type: int, derivation: {}}}\nrows:\n{templates}", count(value))
            };
            let subj = "  - id: subj\n    dataset: SUBJ\n    derivations: {USUBJID: SUBJ.USUBJID, ORIGIN: \"'SUBJ'\"}\n";
            let both = format!("{subj}  - id: ref\n    dataset: REF\n    derivations: {{USUBJID: REF.USUBJID, ORIGIN: \"'REF'\"}}\n");
            for value in ["SUBJ.SUBJDAY", "REF.REFDAY"] {
                assert_eq!(
                    run(&rows(&both, value)),
                    (refused(value), Err(refused(value))),
                    "{value}"
                );
            }
            assert_eq!(
                run(&rows(subj, "SUBJ.SUBJDAY")),
                (
                    Vec::new(),
                    Ok("USUBJID,ORIGIN,N\nS1,SUBJ,2\nS2,SUBJ,1\n".into())
                )
            );
        }
    }

    mod completed_lookup_assertions {
        //! Independent completed-row assertion scope, order and whole-record truth.
        use std::fs;
        use yamaa_adapters::file_application;
        use yamaa_core::diagnostic::Diagnostic;
        use yamaa_core::domain_entry::Request;
        struct Fixture(crate::support::Scratch);
        fn issues(rows: &[Diagnostic]) -> yamaa_core::json::Value {
            yamaa_core::json::Value::Array(
                rows.iter()
                    .map(|issue| {
                        let mut value = yamaa_adapters::report_storage::diagnostic_value(issue);
                        value["context"] = yamaa_core::json::Value::Object(issue.context.clone());
                        value
                    })
                    .collect(),
            )
        }
        #[test]
        fn complete_independent_lookup_assertions_keep_declaring_sites_and_prior_records() {
            let truth: yamaa_core::json::Value = yamaa_adapters::json_wire::decode(include_str!(
                "fixtures/completed_lookup_assertions.json"
            ))
            .unwrap();
            for (name, case) in truth["cases"].as_object().unwrap() {
                for _ in 0..2 {
                    let fixture = Fixture(crate::support::Scratch::new("completed-assert"));
                    let entry = fixture.0.join("spec.yaml");
                    fs::write(&entry, case["specification"].as_str().unwrap()).unwrap();
                    for (path, content) in case["inputs"].as_object().unwrap() {
                        fs::write(fixture.0.join(path), content.as_str().unwrap()).unwrap();
                    }
                    let written = entry.to_str().unwrap();
                    let identity = || crate::support::study::identity("completed-assert", written);
                    let request = || Request {
                        specification: written,
                        environment: None,
                    };
                    assert_eq!(
                        issues(&file_application::check(
                            yamaa_core::project_function::Language::Python,
                            request(),
                            identity()
                        )),
                        case["check_issues"],
                        "{name}: check"
                    );
                    let mut result = crate::support::study::domain(
                        yamaa_core::project_function::Language::Python,
                        request(),
                        identity(),
                    )
                    .unwrap_or_else(|issues| panic!("{name}: {issues:?}"));
                    assert_eq!(
                        issues(&result.issues()),
                        case["issues"],
                        "{name}: complete findings"
                    );
                    let report = result.observations();
                    assert_eq!(
                        report["handler_counts"], case["handlers"],
                        "{name}: declared handler ledger"
                    );
                    let mut expected = case["verifications"].clone();
                    for record in expected.as_array_mut().unwrap() {
                        record["specification"] = written.into();
                    }
                    assert_eq!(
                        report["verifications"], expected,
                        "{name}: complete ordered verification records"
                    );
                    assert_eq!(report["artifacts"], yamaa_adapters::json!([]));
                    if let Some(csv) = case.get("csv") {
                        assert!(result.output().is_some(), "{name}");
                        assert!(result.save().unwrap(), "{name}");
                        assert_eq!(
                            fs::read(fixture.0.join("result.csv")).unwrap(),
                            csv.as_str().unwrap().as_bytes(),
                            "{name}: exact CSV"
                        );
                        assert!(result.save().unwrap());
                    } else {
                        assert!(result.output().is_none(), "{name}");
                        assert!(result.save().is_err(), "{name}");
                        assert!(!fixture.0.join("result.csv").exists());
                    }
                }
            }
        }
    }

    mod declarative_case_intermediates {
        //! Complete case observations use the shared scalar source binder.
        use std::{fs, path::Path};
        use yamaa_adapters::file_application;
        use yamaa_core::domain_entry::Request;
        struct Fixture(crate::support::Scratch);
        impl Fixture {
            fn new() -> Self {
                let path = crate::support::Scratch::new("case-intermediate");
                Self(path)
            }
            fn run(
                &self,
                body: &str,
            ) -> Result<file_application::Domain, Vec<yamaa_core::diagnostic::Diagnostic>>
            {
                let specification = self.0.join("spec.yaml");
                fs::write(&specification, body).unwrap();
                let specification = specification.to_str().unwrap();
                crate::support::study::domain(
                    yamaa_core::project_function::Language::Python,
                    Request {
                        specification,
                        environment: None,
                    },
                    crate::support::study::identity("independent-case-intermediate", specification),
                )
            }
            fn populate(&self) {
                fs::write(self.0.join("src.csv"), "ID,SELECT\n1,Y\n2,N\n3,Y\n").unwrap();
                fs::write(
                    self.0.join("donor.csv"),
                    "ID,SEQ,A,B\n1,1,first,\n1,2,second,present\n2,1,hidden,hidden\n",
                )
                .unwrap();
            }
        }
        const BODY: &str = r#"yamaa: "1.0"
type: spec
domain: TEST
base: SRC
keys: [ID]
input:
  SRC: {path: src.csv, types: {ID: int}}
  DONOR: {path: donor.csv, types: {ID: int, SEQ: int}}
output: {path: result.csv, columns: [ID, A, B]}
intermediates:
  - id: P
    dataset: DONOR
    key: [ID]
    order_by: [DONOR.SEQ]
    keep: first
columns:
  - {name: ID, type: int, derivation: SRC.ID}
  - {name: SELECT, type: str, derivation: SRC.SELECT}
  - name: A
    type: str
    derivation:
      case:
        - when: "SELECT = 'Y'"
          then: P.A
        - otherwise: "'skip'"
  - name: B
    type: str
    derivation:
      case:
        - when: "SELECT = 'Y'"
          then: P.B
        - otherwise: "'skip'"
"#;
        #[test]
        fn selected_case_fields_share_kept_donors_and_unselected_branches_have_no_handlers() {
            let fixture = Fixture::new();
            fixture.populate();
            let mut result = fixture.run(BODY).unwrap();
            assert!(result.issues().is_empty(), "{:?}", result.issues());
            assert!(result.save().unwrap());
            assert_eq!(
                fs::read(fixture.0.join("result.csv")).unwrap(),
                b"ID,A,B\n1,first,\n2,skip,skip\n3,,\n"
            );
            assert_eq!(
                result.observations()["handler_counts"],
                yamaa_adapters::json!([
                    {"spec_path":"columns.A.derivation.case[0].then.source.multiple_matches","handler":"multiple_matches","count":1},
                    {"spec_path":"columns.A.derivation.case[0].then.source.absent","handler":"absent","count":1},
                    {"spec_path":"columns.B.derivation.case[0].then.source.multiple_matches","handler":"multiple_matches","count":1},
                    {"spec_path":"columns.B.derivation.case[0].then.source.absent","handler":"absent","count":1},
                ])
            );
        }
        #[test]
        fn unchanged_first_ae_capture_matches_full_csv_and_complete_reached_handlers() {
            let fixture = Fixture::new();
            let original = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../../benchmarks/adam-adtte-first-ae");
            fs::create_dir(fixture.0.join("input")).unwrap();
            for name in ["adsl.csv", "adae.csv"] {
                fs::copy(
                    original.join("input").join(name),
                    fixture.0.join("input").join(name),
                )
                .unwrap();
            }
            let written = fs::read(original.join("spec.yaml")).unwrap();
            let mut result = fixture.run(std::str::from_utf8(&written).unwrap()).unwrap();
            assert!(result.issues().is_empty(), "{:?}", result.issues());
            assert!(result.save().unwrap());
            assert_eq!(
                fs::read(fixture.0.join("adtte.csv")).unwrap(),
                fs::read(original.join("expected/adtte.csv")).unwrap()
            );
            assert_eq!(fs::read(original.join("spec.yaml")).unwrap(), written);
            assert_eq!(
                result.observations()["handler_counts"],
                yamaa_adapters::json!([
                    {"spec_path":"columns.AEDT.derivation.source.multiple_matches","handler":"multiple_matches","count":2},
                    {"spec_path":"columns.AEDT.derivation.source.absent","handler":"absent","count":3},
                    {"spec_path":"columns.SRCSEQ.derivation.case[0].then.source.multiple_matches","handler":"multiple_matches","count":2},
                    {"spec_path":"columns.SRCSEQ.derivation.case[0].then.source.absent","handler":"absent","count":0}
                ])
            );
        }
    }
}
