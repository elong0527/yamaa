//! Public calls refuse a file that declares the 1.x header (SCHEMA-2): the
//! engine reads no 1.x language version until it reads 1.0 files (#3030 D76),
//! so a `yamaa:` file fails as `unsupported_language_version` wherever a call
//! reads it, and every 0.5 file runs as before.
#![cfg(feature = "yaml")]

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use arrow_array::{Array, ListArray, StringArray};
use yamaa::{CancellationToken, Engine, EngineOptions, HostLanguage, Table};
use yamaa_core::json::Value;

static NEXT: AtomicUsize = AtomicUsize::new(0);

struct Study(PathBuf);
impl Study {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "yamaa-language-header-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&root).unwrap();
        Self(yamaa_adapters::canonical_path::canonicalize(root).unwrap())
    }
    fn write(&self, name: &str, contents: &str) {
        let path = self.0.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, contents).unwrap();
    }
    fn path(&self, name: &str) -> String {
        let path = self.0.join(name).to_str().unwrap().to_owned();
        #[cfg(windows)]
        let path = path.replace('\\', "/");
        path
    }
}
impl Drop for Study {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn engine() -> Engine {
    Engine::new(EngineOptions::new(
        HostLanguage::Python,
        "language-header-test".into(),
        "0".into(),
    ))
}

/// Condition, rule, spec paths and context of every issue row.
fn rows(table: &Table) -> Vec<(String, Option<String>, Vec<String>, Value)> {
    let mut rows = Vec::new();
    for batch in table.batches() {
        let text = |name: &str| {
            batch
                .column_by_name(name)
                .unwrap()
                .as_any()
                .downcast_ref::<StringArray>()
                .unwrap()
                .clone()
        };
        let (condition, requirement, context) =
            (text("condition"), text("requirement"), text("context"));
        let paths = batch
            .column_by_name("spec_paths")
            .unwrap()
            .as_any()
            .downcast_ref::<ListArray>()
            .unwrap()
            .clone();
        for row in 0..batch.num_rows() {
            let values = paths.value(row);
            let values = values.as_any().downcast_ref::<StringArray>().unwrap();
            rows.push((
                condition.value(row).to_owned(),
                (!requirement.is_null(row)).then(|| requirement.value(row).to_owned()),
                (0..values.len())
                    .map(|i| values.value(i).to_owned())
                    .collect(),
                yamaa_adapters::json_wire::decode(context.value(row)).unwrap(),
            ));
        }
    }
    rows
}

/// The refusal each call reports for a 1.x file it reads.
fn refused(table: &Table) {
    let rows = rows(table);
    assert_eq!(rows.len(), 1, "{rows:?}");
    let (condition, requirement, paths, context) = &rows[0];
    assert_eq!(condition, "unsupported_language_version");
    assert_eq!(requirement.as_deref(), Some("SCHEMA-2"));
    assert_eq!(paths, &["yamaa"]);
    assert_eq!(context["version"], Value::String("1.0".into()));
    assert_eq!(context["language_versions"], Value::Array(Vec::new()));
}

const SPEC: &str = "yamaa: \"1.0\"\ntype: spec\ndomain: DM\nkeys: [USUBJID]\ninput: {RAW: raw.csv}\ncolumns:\n  - {name: USUBJID, type: str, derivation: RAW.USUBJID}\noutput: {path: dm.csv, columns: [USUBJID]}\n";

#[test]
fn every_specification_call_refuses_a_1_x_specification() {
    let study = Study::new();
    study.write("raw.csv", "USUBJID\nS1\n");
    study.write("spec.yaml", SPEC);
    let spec = study.path("spec.yaml");
    let engine = engine();
    let token = CancellationToken::new();
    refused(&engine.spec_check(&spec, None, &token).unwrap().issues());
    let run = engine.spec_run(&spec, None, &token).unwrap();
    assert!(run.output().is_none());
    refused(&run.issues());
    refused(&engine.spec_resolve(&spec, None).unwrap_err().issues);
    // The same file written for 0.5 runs.
    study.write(
        "spec.yaml",
        &SPEC.replace("yamaa: \"1.0\"\ntype: spec\n", "schema_version: '1.0'\n"),
    );
    let run = engine.spec_run(&spec, None, &token).unwrap();
    assert!(rows(&run.issues()).is_empty());
    assert_eq!(run.output().unwrap().rows(), 1);
}

#[test]
fn every_project_call_refuses_a_1_x_project_file() {
    let study = Study::new();
    study.write(
        "project.yaml",
        "yamaa: \"1.0\"\ntype: project\nsdtm: {specs: [dm.yaml]}\n",
    );
    let project = study.path("project.yaml");
    let engine = engine();
    let token = CancellationToken::new();
    refused(&engine.project_check(&project, &token).unwrap().issues());
    refused(&engine.project_run(&project, None, &token).unwrap().issues());
    refused(
        &engine
            .project_submission(&project, None, None, "0", &token)
            .unwrap()
            .issues(),
    );
    refused(&engine.project_spec(&project, "DM").unwrap_err().issues);
}

#[test]
fn a_0_5_project_refuses_a_1_x_specification_it_lists() {
    let study = Study::new();
    study.write("raw.csv", "USUBJID\nS1\n");
    study.write("dm.yaml", SPEC);
    study.write(
        "project.yaml",
        "schema_version: '1.0'\nsdtm: {specs: [dm.yaml]}\n",
    );
    let engine = engine();
    let token = CancellationToken::new();
    let rows = rows(
        &engine
            .project_check(&study.path("project.yaml"), &token)
            .unwrap()
            .issues(),
    );
    assert_eq!(rows.len(), 1, "{rows:?}");
    let (condition, requirement, paths, context) = &rows[0];
    assert_eq!(condition, "unsupported_language_version");
    assert_eq!(requirement.as_deref(), Some("SCHEMA-2"));
    assert_eq!(paths, &["yamaa"]);
    assert_eq!(context["entry"], Value::String(study.path("dm.yaml")));
}
