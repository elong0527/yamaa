//! Complete independent reports for inclusive ranges and visible donor columns.
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};
use yamaa_adapters::{file_application, specification_report::Identity};
use yamaa_engine::domain_entry::Request;
static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "yamaa-intermediate-ranges-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn run(&self, body: &str) -> file_application::Domain {
        let path = self.0.join("spec.yaml");
        fs::write(&path, body).unwrap();
        let path = path.to_str().unwrap();
        file_application::domain(
            Request {
                specification: path,
                environment: None,
            },
            Identity {
                runtime: "test",
                runtime_version: "test",
                engine_version: yamaa_core::VERSION,
                example: "independent-intermediate-range",
                specification: path,
                base_directory: ".",
            },
        )
        .unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
const BODY: &str = r#"schema_version: '1.0'
domain: TEST
base: SRC
keys: [ID, SEQ]
input:
  SRC: {path: src.csv, types: {ID: int, SEQ: int, X: int}}
  D: {path: donor.csv, types: {ID: int, LOW: int, HIGH: int, ORD: int}}
output: {path: result.csv, columns: [ID, SEQ, X, A, B]}
intermediates:
  - id: P
    dataset: D
    key: ID
    filter: "D.ELIG = 'Y'"
    between: {value: X, lower: LOW, upper: HIGH}
    columns: [V, W, V]
    order_by: [D.ORD]
    keep: first
    no_match: absent
columns:
  - {name: ID, type: int, derivation: SRC.ID}
  - {name: SEQ, type: int, derivation: SRC.SEQ}
  - {name: X, type: int, derivation: SRC.X}
  - {name: A, type: str, derivation: P.V}
  - {name: B, type: str, derivation: P.W}
"#;
fn populate(f: &Fixture) {
    fs::write(
        f.0.join("src.csv"),
        "ID,SEQ,X\n1,1,3\n1,2,4\n1,3,\n1,4,6\n2,1,3\n1,5,2\n",
    )
    .unwrap();
    fs::write(f.0.join("donor.csv"), "ID,LOW,HIGH,ORD,ELIG,V,W\n1,1,3,2,Y,edge,\n1,3,5,1,Y,later,paired\n1,,10,3,Y,missing-bound,hidden\n1,1,10,0,N,filtered,hidden\n").unwrap();
}
#[test]
fn inclusive_ranges_keep_one_record_choice_and_project_only_named_reads() {
    let f = Fixture::new();
    populate(&f);
    let mut result = f.run(BODY);
    assert!(result.issues().is_empty(), "{:?}", result.issues());
    assert!(result.save().unwrap());
    assert_eq!(fs::read(f.0.join("result.csv")).unwrap(), b"ID,SEQ,X,A,B\n1,1,3,later,paired\n1,2,4,later,paired\n1,3,,absent,absent\n1,4,6,absent,absent\n2,1,3,absent,absent\n1,5,2,edge,\n");
    assert_eq!(
        result.observations()["handler_counts"],
        serde_json::json!([
            {"spec_path":"columns.A.derivation.source.multiple_matches","handler":"multiple_matches","count":1},
            {"spec_path":"columns.A.derivation.source.no_match","handler":"no_match","count":3},
            {"spec_path":"columns.B.derivation.source.multiple_matches","handler":"multiple_matches","count":1},
            {"spec_path":"columns.B.derivation.source.no_match","handler":"no_match","count":3}
        ])
    );
}
#[test]
fn unstated_lower_bound_never_turns_a_missing_unused_donor_field_into_absence() {
    let f = Fixture::new();
    populate(&f);
    let mut result =
        f.run(&BODY.replace("value: X, lower: LOW, upper: HIGH", "value: X, upper: HIGH"));
    assert!(result.issues().is_empty(), "{:?}", result.issues());
    assert!(result.save().unwrap());
    assert_eq!(fs::read(f.0.join("result.csv")).unwrap(), b"ID,SEQ,X,A,B\n1,1,3,later,paired\n1,2,4,later,paired\n1,3,,absent,absent\n1,4,6,missing-bound,hidden\n2,1,3,absent,absent\n1,5,2,later,paired\n");
    assert_eq!(
        result.observations()["handler_counts"],
        serde_json::json!([
            {"spec_path":"columns.A.derivation.source.multiple_matches","handler":"multiple_matches","count":3},
            {"spec_path":"columns.A.derivation.source.no_match","handler":"no_match","count":2},
            {"spec_path":"columns.B.derivation.source.multiple_matches","handler":"multiple_matches","count":3},
            {"spec_path":"columns.B.derivation.source.no_match","handler":"no_match","count":2}
        ])
    );
}
#[test]
fn a_hidden_read_has_the_complete_original_reference_finding_and_no_output() {
    let f = Fixture::new();
    populate(&f);
    let mut result = f.run(&BODY.replace("derivation: P.W", "derivation: P.LOW"));
    assert_eq!(
        result
            .issues()
            .iter()
            .map(|issue| {
                let mut value = issue.as_value();
                value["context"] = serde_json::from_str(&issue.context).unwrap();
                value
            })
            .collect::<Vec<_>>(),
        [
            serde_json::json!({"phase":"validation","condition":"unknown_field","requirement":"REQ-0125","spec_paths":["columns.B.derivation.source"],"context":{"identifier":"P.LOW"}})
        ]
    );
    assert_eq!(
        result.observations()["handler_counts"],
        serde_json::json!([])
    );
    assert!(matches!(result.save(), Err(file_application::FailedBuild)));
    assert!(!f.0.join("result.csv").exists());
}
#[test]
fn integer_absence_replacement_retains_full_signed_integer_value_and_ledger() {
    let f = Fixture::new();
    fs::write(f.0.join("src.csv"), "ID,SEQ,X\n2,1,3\n").unwrap();
    fs::write(
        f.0.join("donor.csv"),
        "ID,LOW,HIGH,ORD,ELIG,V,W\n1,1,4,1,Y,3,4\n",
    )
    .unwrap();
    let body = BODY
        .replace("no_match: absent", "no_match: -9223372036854775808")
        .replace("name: A, type: str", "name: A, type: int")
        .replace("name: B, type: str", "name: B, type: int");
    let mut result = f.run(&body);
    assert!(result.issues().is_empty(), "{:?}", result.issues());
    assert!(result.save().unwrap());
    assert_eq!(
        fs::read(f.0.join("result.csv")).unwrap(),
        b"ID,SEQ,X,A,B\n2,1,3,-9223372036854775808,-9223372036854775808\n"
    );
    assert_eq!(
        result.observations()["handler_counts"],
        serde_json::json!([
            {"spec_path":"columns.A.derivation.source.no_match","handler":"no_match","count":1},
            {"spec_path":"columns.B.derivation.source.no_match","handler":"no_match","count":1}
        ])
    );
}

#[test]
fn range_current_outputs_and_scalar_keys_promote_their_defaults_before_row_reads() {
    let f = Fixture::new();
    populate(&f);
    let body = BODY
        .replace("\ncolumns:\n", "\nrows:\n  - id: selected\n    dataset: SRC\n    derivations: {A: P.V, B: P.W}\ncolumns:\n")
        .replace("name: A, type: str, derivation: P.V", "name: A, type: str")
        .replace("name: B, type: str, derivation: P.W", "name: B, type: str");
    let mut result = f.run(&body);
    assert!(result.issues().is_empty(), "{:?}", result.issues());
    assert!(result.save().unwrap());
    assert_eq!(fs::read(f.0.join("result.csv")).unwrap(), b"ID,SEQ,X,A,B\n1,1,3,later,paired\n1,2,4,later,paired\n1,3,,absent,absent\n1,4,6,absent,absent\n2,1,3,absent,absent\n1,5,2,edge,\n");
    assert_eq!(
        result.observations()["handler_counts"],
        serde_json::json!([
            {"spec_path":"rows[0].derivations.A.source.multiple_matches","handler":"multiple_matches","count":1},
            {"spec_path":"rows[0].derivations.B.source.multiple_matches","handler":"multiple_matches","count":1},
            {"spec_path":"rows[0].derivations.A.source.no_match","handler":"no_match","count":3},
            {"spec_path":"rows[0].derivations.B.source.no_match","handler":"no_match","count":3}
        ])
    );
}

#[test]
fn float_absence_replacement_is_a_value_with_complete_per_read_observations() {
    let f = Fixture::new();
    fs::write(f.0.join("src.csv"), "ID,SEQ,X\n2,1,3\n").unwrap();
    fs::write(
        f.0.join("donor.csv"),
        "ID,LOW,HIGH,ORD,ELIG,V,W\n1,1,4,1,Y,3,4\n",
    )
    .unwrap();
    let body = BODY
        .replace("no_match: absent", "no_match: 0.5")
        .replace("name: A, type: str", "name: A, type: float")
        .replace("name: B, type: str", "name: B, type: float");
    let mut result = f.run(&body);
    assert!(result.issues().is_empty(), "{:?}", result.issues());
    assert!(result.save().unwrap());
    assert_eq!(
        fs::read(f.0.join("result.csv")).unwrap(),
        b"ID,SEQ,X,A,B\n2,1,3,0.5,0.5\n"
    );
    assert_eq!(
        result.observations()["handler_counts"],
        serde_json::json!([
            {"spec_path":"columns.A.derivation.source.no_match","handler":"no_match","count":1},
            {"spec_path":"columns.B.derivation.source.no_match","handler":"no_match","count":1}
        ])
    );
}

#[test]
fn boolean_absence_replacement_reaches_a_null_test_without_conversion() {
    let f = Fixture::new();
    fs::write(f.0.join("src.csv"), "ID,SEQ,X\n2,1,3\n").unwrap();
    fs::write(
        f.0.join("donor.csv"),
        "ID,LOW,HIGH,ORD,ELIG,V,W\n1,1,4,1,Y,3,4\n",
    )
    .unwrap();
    let body = BODY.replace("no_match: absent", "no_match: true")
        .replace("derivation: P.V", "derivation: {flag: {condition: 'P.V IS NOT NULL', true_value: Y, false_value: N, missing: null}}")
        .replace("derivation: P.W", "derivation: {flag: {condition: 'P.W IS NOT NULL', true_value: Y, false_value: N, missing: null}}");
    let mut result = f.run(&body);
    assert!(result.issues().is_empty(), "{:?}", result.issues());
    assert!(result.save().unwrap());
    assert_eq!(
        fs::read(f.0.join("result.csv")).unwrap(),
        b"ID,SEQ,X,A,B\n2,1,3,Y,Y\n"
    );
    // Predicate normalization retains the literal value and discards recovery sites.
    assert_eq!(
        result.observations()["handler_counts"],
        serde_json::json!([])
    );
}

#[test]
fn integer_current_values_do_not_round_into_a_float_donor_endpoint() {
    let f = Fixture::new();
    fs::write(
        f.0.join("src.csv"),
        "ID,SEQ,X\n1,1,9007199254740993\n1,2,9007199254740992\n",
    )
    .unwrap();
    fs::write(
        f.0.join("donor.csv"),
        "ID,LOW,HIGH,ORD,ELIG,V,W\n1,9007199254740992,9007199254740992,1,Y,edge,paired\n",
    )
    .unwrap();
    let body = BODY.replace("LOW: int, HIGH: int", "LOW: float, HIGH: float");
    let mut result = f.run(&body);
    assert!(result.issues().is_empty(), "{:?}", result.issues());
    assert!(result.save().unwrap());
    assert_eq!(
        fs::read(f.0.join("result.csv")).unwrap(),
        b"ID,SEQ,X,A,B\n1,1,9007199254740993,absent,absent\n1,2,9007199254740992,edge,paired\n"
    );
    assert_eq!(
        result.observations()["handler_counts"],
        serde_json::json!([
            {"spec_path":"columns.A.derivation.source.no_match","handler":"no_match","count":1},
            {"spec_path":"columns.B.derivation.source.no_match","handler":"no_match","count":1}
        ])
    );
}

#[test]
fn unchanged_bad_range_document_preserves_its_single_complete_historical_finding() {
    let f = Fixture::new();
    let original = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../benchmarks/negative-lookup-bad-range");
    fs::create_dir(f.0.join("input")).unwrap();
    for name in ["vs.csv", "epoch.csv"] {
        fs::copy(
            original.join("input").join(name),
            f.0.join("input").join(name),
        )
        .unwrap();
    }
    let mut result = f.run(&fs::read_to_string(original.join("spec.yaml")).unwrap());
    let findings = result
        .issues()
        .iter()
        .map(|issue| {
            let mut value = issue.as_value();
            value["context"] = serde_json::from_str(&issue.context).unwrap();
            value
        })
        .collect::<Vec<_>>();
    assert_eq!(
        findings,
        [
            serde_json::json!({"phase":"validation","condition":"incomparable_range_types","requirement":"REQ-0121","spec_paths":["intermediates[0].between"],"context":{"intermediate":"EPOCHDEF","value_type":"date","lower_type":"int","upper_type":"int"}})
        ]
    );
    assert_eq!(
        result.observations()["handler_counts"],
        serde_json::json!([])
    );
    assert!(matches!(result.save(), Err(file_application::FailedBuild)));
    assert!(!f.0.join("vs.csv").exists());
}
