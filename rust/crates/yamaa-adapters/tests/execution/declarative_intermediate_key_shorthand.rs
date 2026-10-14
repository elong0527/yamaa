//! REQ-0115 scalar key shorthand retains exact integers and shared donor choice.
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
            "yamaa-scalar-intermediate-key-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn run(&self) -> file_application::Domain {
        let path = self.0.join("spec.yaml");
        fs::write(
            &path,
            r#"schema_version: '1.0'
domain: TEST
base: SRC
keys: [ID]
input:
  SRC: {path: src.csv, types: {ID: int}}
  D: {path: donor.csv, types: {ID: int, SEQ: int}}
output: {path: result.csv, columns: [ID, A, B]}
intermediates:
  - id: P
    dataset: D
    key: ID
    order_by: [D.SEQ]
    keep: first
    no_match: null
columns:
  - {name: ID, type: int, derivation: SRC.ID}
  - {name: A, type: str, derivation: P.V}
  - {name: B, type: str, derivation: P.V}
"#,
        )
        .unwrap();
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
                example: "independent-key-shorthand",
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
#[test]
fn scalar_shorthand_preserves_distinct_large_integer_keys_and_per_read_handlers() {
    let fixture = Fixture::new();
    fs::write(
        fixture.0.join("src.csv"),
        "ID\n9007199254740992\n9007199254740993\n-9223372036854775808\n",
    )
    .unwrap();
    fs::write(
        fixture.0.join("donor.csv"),
        "ID,SEQ,V\n9007199254740992,1,lower\n9007199254740993,2,later\n9007199254740993,1,exact\n",
    )
    .unwrap();
    let mut result = fixture.run();
    assert!(result.issues().is_empty(), "{:?}", result.issues());
    assert!(result.save().unwrap());
    assert_eq!(fs::read(fixture.0.join("result.csv")).unwrap(),b"ID,A,B\n9007199254740992,lower,lower\n9007199254740993,exact,exact\n-9223372036854775808,,\n");
    assert_eq!(
        result.observations()["handler_counts"],
        serde_json::json!([
            {"spec_path":"columns.A.derivation.source.multiple_matches","handler":"multiple_matches","count":1},
            {"spec_path":"columns.A.derivation.source.no_match","handler":"no_match","count":1},
            {"spec_path":"columns.B.derivation.source.multiple_matches","handler":"multiple_matches","count":1},
            {"spec_path":"columns.B.derivation.source.no_match","handler":"no_match","count":1}
        ])
    );
}
