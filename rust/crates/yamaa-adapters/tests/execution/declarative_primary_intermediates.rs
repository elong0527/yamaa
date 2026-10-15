//! Same-input donor selection retains one captured relation across driver scopes.
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
            "yamaa-primary-intermediate-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn run(
        &self,
        body: &str,
    ) -> Result<file_application::Domain, Vec<yamaa_adapters::issue_rows::Issue>> {
        let specification = self.0.join("spec.yaml");
        fs::write(&specification, body).unwrap();
        let specification = specification.to_str().unwrap();
        file_application::domain(
            Request {
                specification,
                environment: None,
            },
            Identity {
                runtime: "test",
                runtime_version: "test",
                engine_version: yamaa_core::VERSION,
                example: "independent-primary-intermediate",
                specification,
                base_directory: ".",
            },
        )
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
  SRC: {path: src.csv, types: {ID: int, SEQ: int}}
output: {path: result.csv, columns: [ID, SEQ, A, B]}
intermediates:
  - id: P
    dataset: SRC
    key: [ID]
    filter: "SRC.ELIG = 'Y'"
    order_by: [SRC.SEQ]
    keep: last
    no_match: null
columns:
  - {name: ID, type: int, derivation: SRC.ID}
  - {name: SEQ, type: int, derivation: SRC.SEQ}
  - {name: A, type: str, derivation: P.A}
  - {name: B, type: str, derivation: P.B}
"#;
fn populate(fixture: &Fixture) {
    fs::write(
        fixture.0.join("src.csv"),
        "ID,SEQ,ELIG,A,B\n1,1,Y,early,x\n1,2,Y,last,\n2,1,N,hidden,hidden\n",
    )
    .unwrap();
}
#[test]
fn driver_and_donor_share_the_input_but_selection_stays_at_the_donor_grain() {
    let fixture = Fixture::new();
    populate(&fixture);
    let mut result = fixture.run(BODY).unwrap();
    assert!(result.issues().is_empty(), "{:?}", result.issues());
    assert!(result.save().unwrap());
    assert_eq!(
        fs::read_to_string(fixture.0.join("result.csv")).unwrap(),
        "ID,SEQ,A,B\n1,1,last,\n1,2,last,\n2,1,,\n"
    );
    assert_eq!(
        result.observations()["handler_counts"],
        serde_json::json!([
            {"spec_path":"columns.A.derivation.source.multiple_matches","handler":"multiple_matches","count":2},
            {"spec_path":"columns.A.derivation.source.no_match","handler":"no_match","count":1},
            {"spec_path":"columns.B.derivation.source.multiple_matches","handler":"multiple_matches","count":2},
            {"spec_path":"columns.B.derivation.source.no_match","handler":"no_match","count":1},
        ])
    );
}
#[test]
fn a_different_template_driver_still_reads_the_original_primary_donors() {
    let fixture = Fixture::new();
    populate(&fixture);
    fs::write(fixture.0.join("alt.csv"), "ID\n1\n3\n2\n").unwrap();
    let body = BODY.replace("keys: [ID, SEQ]", "keys: [ID]")
        .replace("  SRC: {path: src.csv, types: {ID: int, SEQ: int}}", "  SRC: {path: src.csv, types: {ID: int, SEQ: int}}\n  ALT: {path: alt.csv, types: {ID: int}}")
        .replace("output: {path: result.csv, columns: [ID, SEQ, A, B]}", "output: {path: result.csv, columns: [ID, A, B]}")
        .replace("columns:\n  - {name: ID, type: int, derivation: SRC.ID}\n  - {name: SEQ, type: int, derivation: SRC.SEQ}\n  - {name: A, type: str, derivation: P.A}\n  - {name: B, type: str, derivation: P.B}", "rows:\n  - id: alternate\n    dataset: ALT\n    derivations: {ID: ALT.ID, A: P.A, B: P.B}\ncolumns:\n  - {name: ID, type: int}\n  - {name: A, type: str}\n  - {name: B, type: str}");
    let mut result = fixture.run(&body).unwrap();
    assert!(result.issues().is_empty(), "{:?}", result.issues());
    assert!(result.save().unwrap());
    assert_eq!(
        fs::read_to_string(fixture.0.join("result.csv")).unwrap(),
        "ID,A,B\n1,last,\n3,,\n2,,\n"
    );
    assert_eq!(
        result.observations()["handler_counts"],
        serde_json::json!([
            {"spec_path":"rows[0].derivations.A.source.multiple_matches","handler":"multiple_matches","count":1},
            {"spec_path":"rows[0].derivations.B.source.multiple_matches","handler":"multiple_matches","count":1},
            {"spec_path":"rows[0].derivations.A.source.no_match","handler":"no_match","count":2},
            {"spec_path":"rows[0].derivations.B.source.no_match","handler":"no_match","count":2},
        ])
    );
}
