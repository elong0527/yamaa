//! Inherited key shorthand retains its internal current-row dependencies.
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
            "yamaa-inherited-keys-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn build(&self, name: &str) -> file_application::Domain {
        let path = self.0.join(name);
        let path = path.to_str().unwrap();
        let identity = || Identity {
            runtime: "test",
            runtime_version: "test",
            engine_version: yamaa_core::VERSION,
            example: "inherited-keys",
            specification: path,
            base_directory: ".",
        };
        assert!(file_application::check(
            Request {
                specification: path,
                environment: None
            },
            identity()
        )
        .is_empty());
        file_application::domain(
            Request {
                specification: path,
                environment: None,
            },
            identity(),
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
fn lookup_and_aggregate_shorthands_survive_pruning_in_plain_case_and_row_derivations() {
    for key in ["CODE", "[CODE]", "{CODE: CODE}"] {
        for shape in ["lookup", "case", "row", "aggregate"] {
            let fixture = Fixture::new();
            fs::write(fixture.0.join("obs.csv"), "ID,CODE\n1,A\n2,B\n").unwrap();
            fs::write(fixture.0.join("ref.csv"), "CODE,LABEL\nA,Alpha\nB,Beta\n").unwrap();
            let lookup = format!("intermediates:\n  - {{id: REFLOOK, dataset: REF, key: {key}}}\n");
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
            let parent = format!("schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\ninput: {{OBS: obs.csv, REF: ref.csv}}\nbase: OBS\noutput: {{path: result.csv, columns: [ID, RESULT]}}\n{}columns:\n{declarations}{rows}", if shape == "aggregate" {""} else {&lookup});
            fs::write(fixture.0.join("parent.yaml"), parent).unwrap();
            fs::write(
                fixture.0.join("child.yaml"),
                "schema_version: '1.0'\nparents: parent.yaml\n",
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
