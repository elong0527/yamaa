//! Complete case/concat observations use the shared scalar source binder.
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
        let path = std::env::temp_dir().join(format!(
            "yamaa-case-intermediate-{}-{}",
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
                example: "independent-case-intermediate",
                specification,
                base_directory: ".",
            },
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
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
const BODY: &str = r#"schema_version: '1.0'
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
    order_by: [DONOR.SEQ]
    keep: first
    no_match: null
columns:
  - {name: ID, type: int, derivation: SRC.ID}
  - {name: SELECT, type: str, derivation: SRC.SELECT}
  - name: A
    type: str
    derivation:
      case:
        - when: "SELECT = 'Y'"
          then: P.A
        - otherwise: {literal: skip}
  - name: B
    type: str
    derivation:
      case:
        - when: "SELECT = 'Y'"
          then: P.B
        - otherwise: {literal: skip}
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
        serde_json::json!([
            {"spec_path":"columns.A.derivation.case[0].then.source.multiple_matches","handler":"multiple_matches","count":1},
            {"spec_path":"columns.A.derivation.case[0].then.source.no_match","handler":"no_match","count":1},
            {"spec_path":"columns.B.derivation.case[0].then.source.multiple_matches","handler":"multiple_matches","count":1},
            {"spec_path":"columns.B.derivation.case[0].then.source.no_match","handler":"no_match","count":1},
        ])
    );
}
#[test]
fn concat_nested_case_reuses_the_same_source_binding_and_authored_operand_path() {
    let fixture = Fixture::new();
    fixture.populate();
    fs::write(fixture.0.join("src.csv"), "ID,SELECT\n1,Y\n2,N\n").unwrap();
    let prefix = BODY
        .split("  - name: A")
        .next()
        .unwrap()
        .replace("[ID, A, B]", "[ID, C]");
    let body=format!("{prefix}  - name: C\n    type: str\n    derivation:\n      str_concat:\n        sources:\n          - {{literal: '['}}\n          - case:\n              - when: \"SELECT = 'Y'\"\n                then: P.A\n              - otherwise: {{literal: none}}\n          - {{literal: ']'}}\n");
    let mut result = fixture.run(&body).unwrap();
    assert!(result.issues().is_empty(), "{:?}", result.issues());
    assert!(result.save().unwrap());
    assert_eq!(
        fs::read(fixture.0.join("result.csv")).unwrap(),
        b"ID,C\n1,[first]\n2,[none]\n"
    );
    assert_eq!(
        result.observations()["handler_counts"],
        serde_json::json!([
            {"spec_path":"columns.C.derivation.str_concat.sources[1].case[0].then.source.multiple_matches","handler":"multiple_matches","count":1}
        ])
    );
}
#[test]
fn unchanged_first_ae_capture_matches_full_csv_and_complete_reached_handlers() {
    let fixture = Fixture::new();
    let original =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../benchmarks/adam-adtte-first-ae");
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
        serde_json::json!([
            {"spec_path":"columns.AEDT.derivation.source.multiple_matches","handler":"multiple_matches","count":2},
            {"spec_path":"columns.AEDT.derivation.source.no_match","handler":"no_match","count":3},
            {"spec_path":"columns.SRCSEQ.derivation.case[0].then.source.multiple_matches","handler":"multiple_matches","count":2}
        ])
    );
}
#[test]
fn nested_named_source_absence_handlers_are_lazy_and_preserve_existing_missing_values() {
    let fixture = Fixture::new();
    fixture.populate();
    let body = BODY.replace(
        "then: P.A",
        "then: {source: {variable: P.A, absent: fallback}}",
    );
    for (body, expected, count) in [
        (
            body.clone(),
            b"ID,A,B\n1,first,\n2,skip,skip\n3,,\n".as_slice(),
            0,
        ),
        (
            body.replace("variable: P.A,", "variable: P.ABSENT,"),
            b"ID,A,B\n1,fallback,\n2,skip,skip\n3,fallback,\n".as_slice(),
            2,
        ),
    ] {
        let mut result = fixture.run(&body).unwrap();
        assert!(result.issues().is_empty(), "{:?}", result.issues());
        assert!(result.save().unwrap());
        assert_eq!(fs::read(fixture.0.join("result.csv")).unwrap(), expected);
        assert_eq!(
            result.observations()["handler_counts"]
                .as_array()
                .unwrap()
                .iter()
                .find(
                    |entry| entry["spec_path"] == "columns.A.derivation.case[0].then.source.absent"
                )
                .unwrap(),
            &serde_json::json!({"spec_path":"columns.A.derivation.case[0].then.source.absent","handler":"absent","count":count})
        );
    }
}
