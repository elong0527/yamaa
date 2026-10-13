//! Native index paths resolve only through the exact held composed member IDs.
#![cfg(any(unix, windows))]
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};
use yamaa_adapters::{file_resources::Resources, file_workflow, shipped_schema, source_metadata};
use yamaa_core::project_function::Language;
use yamaa_engine::build::Prepared;

static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Study(PathBuf);
impl Study {
    fn new() -> Self {
        let directory = std::env::temp_dir().join(format!(
            "yamaa-diagnostic-origins-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(directory.join("parent")).unwrap();
        Self(fs::canonicalize(directory).unwrap())
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

#[test]
fn physical_row_indices_preserve_literal_parent_ids_and_refuse_invalid_slots() {
    for id in ["ordinary", "dot.id", "bracket[999]", "both.[999]"] {
        let study = Study::new();
        fs::write(study.0.join("parent/base.yaml"), format!(
            "schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\ninput: {{SRC: input.csv}}\noutput: {{path: out.csv, columns: [ID]}}\ncolumns: [{{name: ID, type: str}}]\nrows:\n  - {{id: clean, dataset: SRC, derivations: {{ID: SRC.ID}}}}\n  - {{id: '{id}', dataset: SRC, filter: 'ID >', derivations: {{ID: SRC.ID}}}}\n")).unwrap();
        fs::write(
            study.0.join("spec.yaml"),
            "schema_version: '1.0'\nparents: parent/base.yaml\n",
        )
        .unwrap();
        let owner = file_workflow::select_with_optional_resources(
            Resources::for_entry(study.0.to_str().unwrap()).unwrap(),
            "spec.yaml",
            None,
            Language::Python,
            shipped_schema::capture().unwrap(),
            shipped_schema::capture_environment().unwrap(),
            Default::default(),
        )
        .unwrap();
        let Prepared::Plain(prepared) = &owner else {
            panic!("plain owner");
        };
        let document = prepared.run().document();
        let parent = study.identity("parent/base.yaml");
        assert_eq!(
            document.declaring_source("rows[1].filter"),
            Some(parent.as_str())
        );
        assert_eq!(
            document.declaring_source(&format!("rows.{id}.filter")),
            Some(parent.as_str())
        );
        for invalid in [
            "rows[2].filter",
            "rows[-1].filter",
            "rows[01].filter",
            "rows[+1].filter",
            "rows[1]x",
        ] {
            assert_eq!(document.declaring_source(invalid), None, "{invalid}");
        }
        assert_eq!(prepared.capture_reads(), 2);
        assert_eq!(
            source_metadata::selected_origins(&owner).unwrap(),
            [("rows[1].filter".into(), parent.clone())]
        );
        let issues = source_metadata::selected_issues(&owner).unwrap();
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].phase, "validation");
        assert_eq!(issues[0].condition, "invalid_predicate");
        assert_eq!(issues[0].requirement.as_deref(), Some("REQ-0188"));
        assert_eq!(issues[0].spec_paths, ["rows[1].filter"]);
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&issues[0].context).unwrap(),
            serde_json::json!({"predicate": "ID >", "position": 4})
        );
        assert_eq!(prepared.capture_reads(), 2);
        assert!(!study.0.join("parent/input.csv").exists());
        assert!(!study.0.join("parent/out.csv").exists());
    }
}

#[test]
fn ready_static_owner_returns_contributor_without_changing_public_issue_context() {
    let study = Study::new();
    fs::write(study.0.join("parent/base.yaml"),
        "schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\ninput: {SRC: input.csv}\noutput: {path: out.csv, columns: [ID]}\ncolumns:\n  - name: ID\n    type: str\n    label: Identifier\n    derivation: SRC.ID\n    verifications: [{matches: {pattern: '('}}]\n").unwrap();
    fs::write(
        study.0.join("spec.yaml"),
        "schema_version: '1.0'\nparents: parent/base.yaml\n",
    )
    .unwrap();
    let owner = file_workflow::select_with_optional_resources(
        Resources::for_entry(study.0.to_str().unwrap()).unwrap(),
        "spec.yaml",
        None,
        Language::Python,
        shipped_schema::capture().unwrap(),
        shipped_schema::capture_environment().unwrap(),
        Default::default(),
    )
    .unwrap();
    let before = source_metadata::selected_reads(&owner);
    assert_eq!(
        source_metadata::selected_origins(&owner).unwrap(),
        [(
            "columns.ID.verifications[0].matches.pattern".into(),
            study.identity("parent/base.yaml")
        )]
    );
    let issues = source_metadata::selected_issues(&owner).unwrap();
    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0].context, "{\"pattern\":\"(\"}");
    assert_eq!(source_metadata::selected_reads(&owner), before);
    assert!(!study.0.join("parent/input.csv").exists());
    assert!(!study.0.join("parent/out.csv").exists());
}
