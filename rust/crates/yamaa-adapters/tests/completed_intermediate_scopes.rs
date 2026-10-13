//! Original committed YAML and independent expected artifacts remain the oracle.
use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicUsize, Ordering},
};
use yamaa_adapters::{file_application, specification_report::Identity};
use yamaa_engine::domain_entry::Request;
static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!(
            "yamaa-intermediate-scopes-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&p).unwrap();
        Self(p)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
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
fn issue(issue: &yamaa_adapters::issue_rows::Issue) -> serde_json::Value {
    serde_json::json!({"phase":issue.phase,"condition":issue.condition,"requirement":issue.requirement,"spec_paths":issue.spec_paths,"context":serde_json::from_str::<serde_json::Value>(&issue.context).unwrap()})
}
fn expected(path: &Path) -> serde_json::Value {
    let text = fs::read_to_string(path).unwrap();
    let d = yamaa_adapters::yaml_decode::decode_yaml(text.as_bytes(), Default::default())
        .unwrap()
        .document;
    fn value(d: &yamaa_core::schema::Document, id: usize) -> serde_json::Value {
        use yamaa_core::schema::DocumentNode as N;
        match &d.nodes()[id] {
            N::Null => serde_json::Value::Null,
            N::Boolean(v) => (*v).into(),
            N::Integer(v) => serde_json::from_str(v).unwrap(),
            N::Float(v) => serde_json::json!(v),
            N::Text(v) => v.clone().into(),
            N::Sequence(ids) => ids.iter().map(|&id| value(d, id)).collect(),
            N::Mapping(pairs) => pairs
                .iter()
                .map(|&(key, item)| (value(d, key).as_str().unwrap().to_owned(), value(d, item)))
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
    let id = Identity {
        runtime: "test",
        runtime_version: "test",
        engine_version: yamaa_core::VERSION,
        example: name,
        specification: &spec,
        base_directory: ".",
    };
    let outcome = file_application::domain(
        Request {
            specification: &spec,
            environment: None,
        },
        id,
    );
    if let Ok(mut result) = outcome {
        if name.starts_with("negative-") {
            assert_eq!(result.issues().len(), 1, "{name}");
            let expected: serde_json::Value = expected(&original.join("expected/error.yaml"));
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
            let expected: serde_json::Value = expected(&original.join("expected/error.yaml"));
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
    let id = Identity {
        runtime: "test",
        runtime_version: "test",
        engine_version: yamaa_core::VERSION,
        example: "independent-intermediate",
        specification: &path,
        base_directory: ".",
    };
    let result = file_application::domain(
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
    let spec="schema_version: '1.0'\ndomain: OUT\nkeys: [ID]\nbase: SRC\ninput: {SRC: {path: source.csv, types: {ID: int}}, DONOR: {path: donor.csv, types: {ID: int, X: int}}}\noutput: {path: out.csv, columns: [ID]}\nintermediates:\n  - id: P\n    dataset: DONOR\n    verifications:\n      - unique: {columns: [ID], id: donor-id}\n      - unique: [X]\ncolumns:\n  - {name: ID, type: int, derivation: SRC.ID}\n";
    let (fixture, result) = independent(spec, "ID\n1\n", Some("ID,X\n1,2\n1,2\n"));
    let mut result = result.unwrap();
    let actual = result.issues().iter().map(issue).collect::<Vec<_>>();
    assert_eq!(
        actual,
        vec![
            serde_json::json!({"phase":"verification","condition":"duplicate_intermediate_records","requirement":"REQ-1245","spec_paths":["intermediates[0].verifications[0].unique"],"context":{"intermediate":"P","dataset":"DONOR","columns":["ID"],"duplicate_count":1,"verification_id":"donor-id"}}),
            serde_json::json!({"phase":"verification","condition":"duplicate_intermediate_records","requirement":"REQ-1245","spec_paths":["intermediates[0].verifications[1].unique"],"context":{"intermediate":"P","dataset":"DONOR","columns":["X"],"duplicate_count":1}})
        ]
    );
    assert!(result.save().is_err());
    assert!(!fixture.0.join("out.csv").exists());
}
#[test]
fn unused_donor_derivation_failure_is_not_hidden_by_an_unread_lookup() {
    let spec="schema_version: '1.0'\ndomain: OUT\nkeys: [ID]\nbase: SRC\ninput: {SRC: {path: source.csv, types: {ID: int}}, DONOR: {path: donor.csv, types: {ID: int, X: int}}}\noutput: {path: out.csv, columns: [ID]}\nintermediates:\n  - id: P\n    dataset: DONOR\n    derivations: {NEXT: {compute: {expr: 'X + 1'}}}\n    verifications: [{unique: [ID]}]\ncolumns:\n  - {name: ID, type: int, derivation: SRC.ID}\n";
    let (fixture, result) = independent(spec, "ID\n1\n", Some("ID,X\n1,9223372036854775807\n"));
    let mut result = result.unwrap();
    assert_eq!(result.issues().len(), 1);
    assert_eq!(result.issues()[0].condition, "integer_overflow");
    assert!(
        result.issues()[0].spec_paths[0].starts_with("intermediates[0].derivations.NEXT.compute")
    );
    assert!(result.save().is_err());
    assert!(!fixture.0.join("out.csv").exists());
}
#[test]
fn current_row_correlation_cannot_turn_a_unique_check_into_a_lazy_check() {
    let spec="schema_version: '1.0'\ndomain: OUT\nkeys: [ID]\nbase: SRC\ninput: {SRC: {path: source.csv, types: {ID: int}}, DONOR: {path: donor.csv, types: {ID: int, X: int}}}\noutput: {path: out.csv, columns: [ID]}\nintermediates:\n  - id: P\n    dataset: DONOR\n    filter: 'DONOR.X > SRC.ID'\n    derivations: {NEXT: {compute: {expr: 'X + SRC.ID'}}}\n    verifications: [{unique: [ID]}]\ncolumns:\n  - {name: ID, type: int, derivation: SRC.ID}\n";
    let (_fixture, result) = independent(spec, "ID\n1\n", Some("ID,X\n1,2\n"));
    let result = match result {
        Err(issues) => issues,
        Ok(domain) => domain.issues().to_vec(),
    };
    assert_eq!(
        result
            .iter()
            .map(|issue| (issue.condition.as_str(), issue.spec_paths[0].as_str()))
            .collect::<Vec<_>>(),
        vec![
            (
                "correlated_filter_with_unique_verification",
                "intermediates[0].verifications[0].unique"
            ),
            (
                "correlated_derivation_with_unique_verification",
                "intermediates[0].verifications[0].unique"
            )
        ]
    );
}

#[test]
fn an_earlier_selector_key_uses_the_later_donor_current_record_owner() {
    let spec = "schema_version: '1.0'\ndomain: OUT\nkeys: [ID]\nbase: SRC\ninput: {SRC: {path: source.csv, types: {ID: int, VAL: int}}, DONOR: {path: donor.csv, types: {KEY: int}}}\noutput: {path: out.csv, columns: [ID, VALUE]}\nintermediates:\n  - id: EARLIER\n    dataset: SRC\n    key: {ID: DONOR.KEY}\n  - id: LATER\n    dataset: DONOR\n    key: {KEY: ID}\n    derivations: {LOOKED_UP: EARLIER.VAL}\ncolumns:\n  - {name: ID, type: int, derivation: SRC.ID}\n  - {name: VALUE, type: int, derivation: LATER.LOOKED_UP}\n";
    for key in ["DONOR.KEY", "{source: DONOR.KEY}"] {
        let spec = spec.replace("key: {ID: DONOR.KEY}", &format!("key: {{ID: {key}}}"));
        // Different field names and reversed donor order expose owner/coordinate aliasing.
        let (fixture, result) = independent(&spec, "ID,VAL\n1,100\n2,200\n", Some("KEY\n2\n1\n"));
        let mut result = result.unwrap();
        assert!(result.issues().is_empty(), "{:?}", result.issues());
        assert!(result.save().unwrap());
        assert_eq!(
            fs::read(fixture.0.join("out.csv")).unwrap(),
            b"ID,VALUE\n1,100\n2,200\n"
        );
    }
}

#[test]
fn retained_foreign_key_is_valid_until_it_is_read_in_the_wrong_current_scope() {
    let declaration = "schema_version: '1.0'\ndomain: OUT\nkeys: [ID]\nbase: SRC\ninput: {SRC: {path: source.csv, types: {ID: int}}, DONOR: {path: donor.csv, types: {KEY: int}}}\noutput: {path: out.csv, columns: [ID]}\nintermediates:\n  - id: P\n    dataset: SRC\n    key: {ID: DONOR.KEY}\ncolumns:\n  - {name: ID, type: int, derivation: SRC.ID}\n";
    let (fixture, result) = independent(declaration, "ID\n1\n", Some("KEY\n2\n"));
    let mut result = result.unwrap();
    assert!(result.issues().is_empty(), "{:?}", result.issues());
    assert!(result.save().unwrap());
    assert_eq!(fs::read(fixture.0.join("out.csv")).unwrap(), b"ID\n1\n");
    let used = declaration.replace("columns: [ID]", "columns: [ID, VALUE]")
        + "  - {name: VALUE, type: int, derivation: P.ID}\n";
    let (fixture, result) = independent(&used, "ID\n1\n", Some("KEY\n2\n"));
    let mut result = result.unwrap();
    assert_eq!(
        result.issues().iter().map(issue).collect::<Vec<_>>(),
        vec![
            serde_json::json!({"phase":"validation","condition":"unknown_field","requirement":"REQ-0117","spec_paths":["intermediates[0].key"],"context":{"identifier":"DONOR.KEY","intermediate":"P"}})
        ]
    );
    assert!(result.save().is_err());
    assert!(!fixture.0.join("out.csv").exists());
}

#[test]
fn a_foreign_row_driver_owns_its_qualified_selector_keys() {
    let spec = "schema_version: '1.0'\ndomain: OUT\nkeys: [ID]\nbase: SRC\ninput: {SRC: {path: source.csv, types: {ID: int, VAL: int}}, DONOR: {path: donor.csv, types: {KEY: int}}}\noutput: {path: out.csv, columns: [ID, VALUE]}\nintermediates:\n  - id: P\n    dataset: SRC\n    key: {ID: DONOR.KEY}\ncolumns:\n  - {name: ID, type: int}\n  - {name: VALUE, type: int}\nrows:\n  - id: donor\n    dataset: DONOR\n    derivations: {ID: DONOR.KEY, VALUE: P.VAL}\n";
    let (fixture, result) = independent(spec, "ID,VAL\n1,100\n2,200\n", Some("KEY\n2\n1\n"));
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
    let spec = "schema_version: '1.0'\ndomain: OUT\nkeys: [ID]\nbase: SRC\ninput: {SRC: {path: source.csv, types: {ID: int, VAL: int}}, DONOR: {path: donor.csv, types: {KEY: int, MIN: int}}}\noutput: {path: out.csv, columns: [ID, VALUE]}\nintermediates:\n  - id: EARLIER\n    dataset: SRC\n    key: {ID: DONOR.KEY}\n    filter: 'SRC.VAL > DONOR.MIN'\n    no_match: -1\n  - id: LATER\n    dataset: DONOR\n    key: {KEY: ID}\n    derivations: {LOOKED_UP: EARLIER.VAL}\ncolumns:\n  - {name: ID, type: int, derivation: SRC.ID}\n  - {name: VALUE, type: int, derivation: LATER.LOOKED_UP}\n";
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
    let spec = "schema_version: '1.0'\ndomain: OUT\nkeys: [ID]\nbase: SRC\ninput: {SRC: {path: source.csv, types: {ID: int, VAL: int}}, DONOR: {path: donor.csv, types: {KEY: int}}}\noutput: {path: out.csv, columns: [ID, VALUE]}\nintermediates:\n  - id: EARLIER\n    dataset: SRC\n    key: {ID: DONOR.NEWKEY}\n  - id: LATER\n    dataset: DONOR\n    key: {KEY: ID}\n    derivations:\n      NEWKEY: KEY\n      LOOKED_UP: EARLIER.VAL\ncolumns:\n  - {name: ID, type: int, derivation: SRC.ID}\n  - {name: VALUE, type: int, derivation: LATER.LOOKED_UP}\n";
    let (fixture, result) = independent(spec, "ID,VAL\n1,100\n2,200\n", Some("KEY\n2\n1\n"));
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
    let base = "schema_version: '1.0'\ndomain: OUT\nkeys: [ID]\nbase: SRC\ninput: {SRC: {path: source.csv, types: {ID: int, VAL: int}}, DONOR: {path: donor.csv, types: {KEY: int}}}\noutput: {path: out.csv, columns: [ID, VALUE]}\nintermediates:\n  - id: EARLIER\n    dataset: SRC\n    key: {ID: DONOR.NEWKEY}\n  - id: LATER\n    dataset: DONOR\n    key: {KEY: ID}\n    derivations:\n      NEWKEY: KEY\n      LOOKED_UP: EARLIER.VAL\ncolumns:\n  - {name: ID, type: int, derivation: SRC.ID}\n  - {name: VALUE, type: int, derivation: LATER.LOOKED_UP}\n";
    let unknown = |name: &str| serde_json::json!({"phase":"validation","condition":"unknown_field","requirement":"REQ-1263","spec_paths":["intermediates[1].derivations.LOOKED_UP.source"],"context":{"identifier":name,"intermediate":"LATER","read":"EARLIER"}});
    for (spec, first) in [
        (
            base.replace(
                "      NEWKEY: KEY\n      LOOKED_UP: EARLIER.VAL",
                "      LOOKED_UP: EARLIER.VAL\n      NEWKEY: KEY",
            ),
            unknown("DONOR.NEWKEY"),
        ),
        (
            base.replace("ID: DONOR.NEWKEY", "ID: DONOR.ABSENT"),
            unknown("DONOR.ABSENT"),
        ),
        (
            base.replace("ID: DONOR.NEWKEY", "ID: WRONG.NEWKEY"),
            unknown("WRONG.NEWKEY"),
        ),
        (
            base.replace("NEWKEY: KEY", "NEWKEY: {literal: wrong-type}"),
            serde_json::json!({"phase":"validation","condition":"incompatible_input_type","requirement":"REQ-0323","spec_paths":["intermediates[0].key"],"context":{"intermediate":"EARLIER","source":"DONOR.NEWKEY","expected":"str","actual":"int"}}),
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
    "schema_version: '1.0'\ndomain: OUT\nkeys: [ID, STAGE]\nbase: SRC\ninput: {SRC: {path: source.csv, types: {ID: int, VAL: int}}}\noutput: {path: out.csv, columns: [ID, STAGE, VALUE]}\nintermediates:\n  - id: P\n    dataset: SELF\n    key: [ID]\n    derivations:\n      TWICE: {compute: {expr: 'SELF.VALUE * 2'}}\n      PLUS: {compute: {expr: 'TWICE + 1'}}\n    order_by: [SELF.STAGE]\n    keep: last\ncolumns:\n  - {name: ID, type: int, derivation: SRC.ID}\n  - {name: STAGE, type: int}\n  - {name: VALUE, type: int}\nrows:\n  - id: first\n    derivations: {STAGE: {literal: 1}, VALUE: SRC.VAL}\n  - id: second\n    derivations: {STAGE: {literal: 2}, VALUE: P.PLUS}\n  - id: third\n    derivations: {STAGE: {literal: 3}, VALUE: P.PLUS}\n"
}

#[test]
fn self_derivations_reject_later_and_unknown_fields() {
    for reference in ["SELF.PLUS", "PLUS", "SELF.ABSENT", "ABSENT"] {
        let spec = self_derivation_spec().replace("SELF.VALUE * 2", reference);
        let (fixture, result) = independent(&spec, "ID,VAL\n1,10\n", None);
        let mut result = result.unwrap();
        assert_eq!(
            result.issues().iter().map(issue).collect::<Vec<_>>(),
            vec![serde_json::json!({
                "phase":"validation", "condition":"unknown_field", "requirement":"REQ-1185",
                "spec_paths":["intermediates[0].derivations.TWICE.compute.expr"],
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
        .replace("TWICE:", "VALUE:")
        .replace("TWICE + 1", "VALUE + 1");
    let (fixture, result) = independent(&spec, "ID,VAL\n1,10\n", None);
    let mut result = result.unwrap();
    assert_eq!(
        result.issues().iter().map(issue).collect::<Vec<_>>(),
        vec![serde_json::json!({
            "phase":"validation", "condition":"duplicate_derivation", "requirement":"REQ-1185",
            "spec_paths":["intermediates[0].derivations.VALUE"],
            "context":{"intermediate":"P", "derivation":"VALUE", "identifier":"SELF.VALUE"}
        })]
    );
    assert!(result.save().is_err());
    assert!(!fixture.0.join("out.csv").exists());
}
#[test]
fn self_derived_unique_checks_are_eager_after_a_completed_template() {
    let spec = self_derivation_spec()
        .replace(
            "    key: [ID]",
            "    key: [ID]\n    verifications: [{unique: [TWICE]}]",
        )
        .replace(
            "VALUE: P.PLUS",
            "VALUE: {compute: {expr: '9223372036854775807 + 1'}}",
        );
    let (fixture, result) = independent(&spec, "ID,VAL\n1,10\n2,10\n", None);
    let mut result = result.unwrap();
    assert_eq!(
        result.issues().iter().map(issue).collect::<Vec<_>>(),
        vec![serde_json::json!({
            "phase":"verification", "condition":"duplicate_intermediate_records", "requirement":"REQ-1245",
            "spec_paths":["intermediates[0].verifications[0].unique"],
            "context":{"intermediate":"P", "dataset":"SELF", "columns":["TWICE"], "duplicate_count":1}
        })]
    );
    assert!(result.save().is_err());
    assert!(!fixture.0.join("out.csv").exists());
}
#[test]
fn a_first_template_cannot_read_a_self_derived_field_from_future_rows() {
    let spec = self_derivation_spec().replace("VALUE: SRC.VAL", "VALUE: P.PLUS");
    let (fixture, result) = independent(&spec, "ID,VAL\n1,10\n", None);
    let issues = result
        .err()
        .expect("the first template is refused at preparation");
    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0].condition, "phase_boundary");
    assert_eq!(issues[0].spec_paths, ["rows[0].derivations.VALUE.source"]);
    assert!(!fixture.0.join("out.csv").exists());
}

#[test]
fn a_self_donor_derived_key_can_read_an_input_backed_selector() {
    let spec = self_derivation_spec()
        .replace(
            "  - id: P",
            "  - id: INPUT\n    dataset: SRC\n    key: {ID: SELF.NEWKEY}\n  - id: P",
        )
        .replace(
            "      TWICE: {compute: {expr: 'SELF.VALUE * 2'}}",
            "      NEWKEY: ID\n      TWICE: {compute: {expr: 'INPUT.VAL * 2'}}",
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
