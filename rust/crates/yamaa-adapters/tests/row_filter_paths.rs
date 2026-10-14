//! Static and build diagnostics use declaration indices, never arbitrary row IDs.
#![cfg(any(unix, windows))]
use serde_json::{json, Value};
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};
use yamaa_adapters::{
    file_application, file_preparation::FileSpecification, file_resources::Resources,
    issue_rows::Issue, specification_report::Identity,
};
use yamaa_engine::domain_entry::Request;

static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Study(PathBuf);
impl Study {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "yamaa-row-filter-path-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(path.join("parent")).unwrap();
        Self(fs::canonicalize(path).unwrap())
    }
    fn identity(&self, name: &str) -> String {
        let value = self.0.join(name).to_string_lossy().into_owned();
        #[cfg(windows)]
        {
            value
                .strip_prefix(r"\\?\")
                .unwrap_or(&value)
                .replace('\\', "/")
        }
        #[cfg(unix)]
        {
            value
        }
    }
}
impl Drop for Study {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
fn issues(rows: &[Issue]) -> Value {
    Value::Array(
        rows.iter()
            .map(|issue| {
                let mut value = issue.as_value();
                value["context"] = serde_json::from_str(&issue.context).unwrap();
                value
            })
            .collect(),
    )
}
#[test]
fn check_and_build_retain_index_paths_and_literal_id_inherited_provenance() {
    for inherited in [false, true] {
        for row_id in ["ordinary", "dot.id", "bracket[999]"] {
            let study = Study::new();
            let entry = study.0.join("spec.yaml");
            let text = format!("schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\ninput: {{SRC: {{path: input.csv}}}}\noutput: {{path: result.csv, columns: [ID]}}\ncolumns: [{{name: ID, type: str}}]\nrows:\n  - {{id: clean, dataset: SRC, derivations: {{ID: SRC.ID}}}}\n  - {{id: '{row_id}', dataset: SRC, filter: 'ID >', derivations: {{ID: SRC.ID}}}}\n");
            let owner = if inherited {
                study.0.join("parent/base.yaml")
            } else {
                entry.clone()
            };
            fs::write(&owner, text).unwrap();
            fs::write(owner.parent().unwrap().join("input.csv"), "ID\na\n").unwrap();
            if inherited {
                fs::write(
                    &entry,
                    "schema_version: '1.0'\nparents: [parent/base.yaml]\n",
                )
                .unwrap();
            }
            // Frozen pre-static row-filter grammar truth carries these two public
            // context fields. Human declaring-file attribution uses held provenance.
            let context = json!({"predicate":"ID >","position":4});
            let prepared = FileSpecification::prepare(
                Resources::new(study.0.to_str().unwrap(), study.0.to_str().unwrap(), &[]).unwrap(),
                "spec.yaml",
            )
            .unwrap();
            let owner_identity = study.identity(if inherited {
                "parent/base.yaml"
            } else {
                "spec.yaml"
            });
            assert_eq!(
                prepared
                    .run()
                    .document()
                    .declaring_source(&format!("rows.{row_id}.filter")),
                Some(owner_identity.as_str())
            );
            assert_eq!(prepared.capture_reads(), if inherited { 2 } else { 1 });
            let expected = json!([{"phase":"validation","condition":"invalid_predicate","requirement":"REQ-0188","spec_paths":["rows[1].filter"],"context":context}]);
            let written = entry.to_str().unwrap();
            let request = || Request {
                specification: written,
                environment: None,
            };
            let identity = || Identity {
                runtime: "test",
                runtime_version: "test",
                engine_version: yamaa_core::VERSION,
                example: "independent-row-filter-attribution",
                specification: written,
                base_directory: ".",
            };
            assert_eq!(
                issues(&file_application::check(request(), identity())),
                expected,
                "{row_id}: check inherited={inherited}"
            );
            let built = file_application::domain(request(), identity()).unwrap();
            assert_eq!(
                issues(&built.issues()),
                expected,
                "{row_id}: build inherited={inherited}"
            );
            assert!(built.output().is_none());
            assert!(!study.0.join("result.csv").exists());
        }
    }
}
