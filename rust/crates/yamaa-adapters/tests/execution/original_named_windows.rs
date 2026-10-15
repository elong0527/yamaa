//! Original named-window witnesses retired with the Python evaluator in #2110.
//! Inputs come from schema/test_named_windows.py at c7b88a4f; expected values
//! are its independent row results or follow its asserted atomic replacement.
use serde_json::{json, Value};
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};
use yamaa_adapters::{file_application, issue_rows::Issue, specification_report::Identity};
use yamaa_core::domain_entry::Request;

static NEXT: AtomicUsize = AtomicUsize::new(0);
const INPUT: &str = "ID,G,SEQ,VAL\n01,a,1,10\n02,a,2,20\n03,b,1,30\n04,b,2,40\n";

struct Study(PathBuf);
impl Study {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "yamaa-original-named-windows-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn write(&self, name: &str, document: &Value) {
        fs::write(self.0.join(name), serde_json::to_vec(document).unwrap()).unwrap();
    }
    fn build(&self) -> Result<file_application::Domain, Vec<Issue>> {
        let entry = self.0.join("spec.yaml");
        let entry = entry.to_str().unwrap();
        file_application::domain(
            Request {
                specification: entry,
                environment: None,
            },
            Identity {
                runtime: "test",
                runtime_version: "test",
                engine_version: yamaa_core::VERSION,
                example: "original-named-windows",
                specification: entry,
                base_directory: ".",
            },
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
    fn refused(&self) -> Vec<Issue> {
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
impl Drop for Study {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

fn document() -> Value {
    json!({
        "schema_version":"1.0", "domain":"OUT", "base":"SRC", "keys":["ID"],
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
    let mut derivations = serde_json::Map::new();
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
                "schema_version":"1.0", "parents":"parent.yaml",
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
        &json!({"schema_version":"1.0","parents":"parent.yaml","windows":null}),
    );
    let issues = study.refused();
    assert_eq!(issues[0].condition, "unknown_window");
    assert_eq!(issues[0].requirement.as_deref(), Some("REQ-1253"));
    assert_eq!(
        issues[0].spec_paths,
        ["columns.PREV.derivation.row_value.window"]
    );
    assert!(!study.0.join("input.csv").exists());
}

#[test]
fn named_expansion_preserves_each_operations_window_restrictions() {
    for (operation, window, kind, condition) in [
        ("row_number", json!({}), "int", "window_order_by_required"),
        (
            "baseline_flag",
            json!({"order_by":["SEQ"]}),
            "str",
            "window_order_by_forbidden",
        ),
    ] {
        let study = Study::new();
        let mut authored = document();
        authored["windows"]["VISITS"] = window;
        let mut payload = json!({"window":"VISITS"});
        if operation == "baseline_flag" {
            payload["date"] = "SEQ".into();
            payload["reference_date"] = "SEQ".into();
        }
        authored["columns"][4]["type"] = kind.into();
        authored["columns"][4]["derivation"] = json!({operation:payload});
        study.write("spec.yaml", &authored);
        fs::write(study.0.join("input.csv"), INPUT).unwrap();
        let issues = study.refused();
        assert_eq!(issues[0].condition, condition, "{operation}: {issues:?}");
    }
}
