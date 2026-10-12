#![cfg(any(unix, windows))]
use serde_json::{json, Value};
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};
use yamaa_adapters::{
    file_preparation::{Error, FileSpecification},
    file_resources::Resources,
};
static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Study(PathBuf);
impl Study {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "yamaa-file-preparation-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        for name in ["entry", "parent", "outside"] {
            fs::create_dir(path.join(name)).unwrap();
        }
        Self(fs::canonicalize(path).unwrap())
    }
    fn text(&self, name: &str) -> String {
        self.0.join(name).to_str().unwrap().into()
    }
    fn resource_key(&self, name: &str) -> String {
        let path = self.text(name);
        #[cfg(windows)]
        {
            // Protocol identities use a local-drive path with forward separators,
            // rather than the Windows extended-path spelling used for native IO.
            path.strip_prefix(r"\\?\")
                .unwrap_or(&path)
                .replace('\\', "/")
        }
        #[cfg(unix)]
        {
            path
        }
    }
    fn prepare(&self, source: &str) -> Result<FileSpecification, Error> {
        fs::write(self.0.join("entry/spec.yaml"), source).unwrap();
        FileSpecification::prepare(
            Resources::new(&self.text(""), &self.text("entry"), &[]).unwrap(),
            "spec.yaml",
        )
    }
}
impl Drop for Study {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
const VALID: &str = "schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\ninput: {SRC: input.csv}\noutput: {path: output.csv, columns: [ID]}\ncolumns:\n  - {name: ID, type: int, derivation: {source: SRC.ID}}\n";
fn failure(study: &Study, source: &str) -> String {
    match study.prepare(source) {
        Ok(_) => panic!("expected preparation failure"),
        Err(error) => error.into_message(),
    }
}
#[test]
fn standalone_retains_entry_and_checks_without_study_capture() {
    let study = Study::new();
    let mut prepared = study.prepare(VALID).unwrap();
    assert_eq!(prepared.capture_reads(), 1);
    assert_eq!(prepared.run().document().source().bytes, VALID.as_bytes());
    assert_eq!(
        yamaa_adapters::specification_check::issues(prepared.run()).unwrap(),
        "[]"
    );
    assert_eq!(prepared.capture_reads(), 1);
    // Compilation uses the retained original model rather than reopening entry YAML.
    fs::write(study.0.join("entry/spec.yaml"), b"[changed").unwrap();
    fs::write(study.0.join("entry/input.csv"), b"ID\n1\n").unwrap();
    assert!(prepared.build().result.is_ok());
    assert_eq!(prepared.capture_reads(), 2);
    assert!(prepared.build().result.is_ok());
    assert_eq!(prepared.capture_reads(), 2);
}

#[cfg(unix)]
#[test]
fn canonical_entry_identity_retains_a_selected_root_with_a_backslash_name() {
    let study = Study::new();
    let root = study.0.join("named\\root");
    fs::create_dir(&root).unwrap();
    fs::write(root.join("spec.yaml"), VALID).unwrap();
    fs::write(root.join("input.csv"), b"ID\n1\n").unwrap();
    let resources = Resources::new(root.to_str().unwrap(), root.to_str().unwrap(), &[]).unwrap();
    let mut prepared = FileSpecification::prepare(resources, "spec.yaml").unwrap();
    assert_eq!(
        prepared.run().document().source().identity,
        fs::canonicalize(root.join("spec.yaml"))
            .unwrap()
            .to_str()
            .unwrap()
    );
    assert_eq!(prepared.capture_reads(), 1);
    assert!(prepared.build().result.is_ok());
    assert_eq!(prepared.capture_reads(), 2);
}
#[test]
fn entry_version_finding_precedes_parent_and_study_authority() {
    let study = Study::new();
    let actual: Value = serde_json::from_str(&failure(
        &study,
        &format!(
            "{}parents: ../outside/missing.yaml\n",
            VALID.replace("'1.0'", "'99.0'")
        ),
    ))
    .unwrap();
    assert_eq!(
        actual,
        json!({"protocol":"specification/prototype","outcome":{"status":"invalid","diagnostics":[{"phase":"validation","condition":"schema_version_mismatch","requirement":"REQ-0245","spec_paths":["schema_version"],"context":{"expected":"1.0","actual":"99.0","source":study.resource_key("entry/spec.yaml"),"entry":study.resource_key("entry/spec.yaml")}}]}})
    );
}
#[test]
fn missing_parent_retains_complete_authored_reference_finding() {
    let study = Study::new();
    let actual: Value =
        serde_json::from_str(&failure(&study, &format!("{VALID}parents: absent.yaml\n"))).unwrap();
    assert_eq!(
        actual,
        json!({"protocol":"specification/prototype","outcome":{"status":"invalid","diagnostics":[{"phase":"validation","condition":"parent_not_found","requirement":"REQ-0654","spec_paths":["parents"],"context":{"path":"absent.yaml","source":study.resource_key("entry/spec.yaml")}}]}})
    );
}
#[test]
fn invalid_parent_preserves_complete_layer_context() {
    let study = Study::new();
    fs::write(
        study.0.join("parent/base.yaml"),
        "schema_version: '1.0'\ndomain: bad-name\n",
    )
    .unwrap();
    let actual: Value = serde_json::from_str(&failure(
        &study,
        &format!("{VALID}parents: ../parent/base.yaml\n"),
    ))
    .unwrap();
    assert_eq!(
        actual,
        json!({"protocol":"specification/prototype","outcome":{"status":"invalid","diagnostics":[{"phase":"validation","condition":"pattern_mismatch","requirement":"REQ-0287","spec_paths":["domain"],"context":{"value":"bad-name","pattern":"^[A-Za-z_][A-Za-z0-9_]*$","source":study.resource_key("parent/base.yaml"),"entry":study.resource_key("entry/spec.yaml")}}]}})
    );
}
#[test]
fn inherited_paths_rebase_lexically_and_shared_parent_bytes_are_retained_once() {
    let study = Study::new();
    fs::write(study.0.join("parent/base.yaml"), VALID).unwrap();
    let mut prepared = study
        .prepare("schema_version: '1.0'\nparents: [../parent/base.yaml, ../parent/./base.yaml]\n")
        .unwrap();
    assert_eq!(prepared.capture_reads(), 2);
    assert_eq!(prepared.run().document().parents().len(), 1);
    assert_eq!(
        prepared.run().document().parents()[0].source().bytes,
        VALID.as_bytes()
    );
    assert_eq!(prepared.run().source().path, "../parent/input.csv");
    // No data file was needed for model preparation or declaration-only checking.
    assert_eq!(
        yamaa_adapters::specification_check::issues(prepared.run()).unwrap(),
        "[]"
    );
    fs::write(study.0.join("parent/input.csv"), b"ID\n1\n").unwrap();
    assert!(prepared.build().result.is_ok());
    assert_eq!(prepared.capture_reads(), 3);
    assert!(prepared.build().result.is_ok());
    assert_eq!(prepared.capture_reads(), 3);
}
#[cfg(unix)]
#[test]
fn selected_read_roots_and_no_link_authority_remain_enforced() {
    let study = Study::new();
    fs::write(study.0.join("parent/base.yaml"), VALID).unwrap();
    std::os::unix::fs::symlink(
        study.0.join("parent/base.yaml"),
        study.0.join("entry/link.yaml"),
    )
    .unwrap();
    assert_eq!(
        failure(&study, &format!("{VALID}parents: link.yaml\n")),
        "resource path contains a symbolic link"
    );
    let resources = Resources::new(&study.text("entry"), &study.text("entry"), &[]).unwrap();
    fs::write(
        study.0.join("entry/spec.yaml"),
        format!("{VALID}parents: ../parent/base.yaml\n"),
    )
    .unwrap();
    let error = match FileSpecification::prepare(resources, "spec.yaml") {
        Ok(_) => panic!("root escape accepted"),
        Err(error) => error,
    };
    assert_eq!(error.into_message(), "resource path outside approved roots");
}

#[cfg(unix)]
#[test]
fn unreadable_parent_is_unavailable_without_fallback_or_study_capture() {
    use std::os::unix::fs::PermissionsExt;
    let study = Study::new();
    let parent = study.0.join("entry/blocked.yaml");
    fs::write(&parent, VALID).unwrap();
    fs::write(study.0.join("blocked.yaml"), VALID).unwrap();
    let permissions = fs::metadata(&parent).unwrap().permissions();
    fs::set_permissions(&parent, fs::Permissions::from_mode(0o0)).unwrap();
    if fs::File::open(&parent).is_ok() {
        fs::set_permissions(&parent, permissions).unwrap();
        eprintln!("process bypasses DAC; unreadable-parent case not exercised");
        return;
    }
    let result = study.prepare(&format!("{VALID}parents: blocked.yaml\n"));
    fs::set_permissions(&parent, permissions).unwrap();
    let error = match result {
        Ok(_) => panic!("unreadable parent accepted through fallback"),
        Err(error) => error,
    };
    let actual: Value = serde_json::from_str(&error.into_message()).unwrap();
    assert_eq!(
        actual,
        json!({"protocol":"specification/prototype","outcome":{"status":"invalid","diagnostics":[{"phase":"validation","condition":"parent_not_found","requirement":"REQ-0654","spec_paths":["parents"],"context":{"path":"blocked.yaml","source":study.resource_key("entry/spec.yaml")}}]}})
    );
}
#[test]
fn all_original_file_builds_preserve_full_reports_and_cached_counters() {
    use yamaa_adapters::specification_report::{self, Identity};
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../benchmarks");
    let names = [
        "negative-zero-division",
        "negative-integer-overflow",
        "adam-adlb-ordered-sum",
        "schema-window-functions",
        "schema-inheritance",
        "schema-lookup",
        "negative-formula-flag",
        "negative-row-aggregate",
        "negative-row-no-prior",
        "negative-source-missing-field",
        "negative-source-trivial-filter",
        "negative-paired-dates",
        "negative-not-missing-age",
        "negative-implausible-age",
        "negative-invalid-sex",
        "negative-sex-code",
        "negative-matches-bad-pattern",
    ];
    for name in names {
        let case = fs::canonicalize(root.join(name)).unwrap();
        let entry = if name == "schema-inheritance" {
            "spec_study.yaml"
        } else {
            "spec.yaml"
        };
        let mut prepared = FileSpecification::prepare(
            Resources::new(case.to_str().unwrap(), case.to_str().unwrap(), &[]).unwrap(),
            entry,
        )
        .unwrap();
        let preparation_reads = if name == "schema-inheritance" { 3 } else { 1 };
        assert_eq!(prepared.capture_reads(), preparation_reads, "{name}");
        let mut expected: Value = serde_json::from_slice(
            &fs::read(
                PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join(format!("tests/fixtures/specs/{name}.json")),
            )
            .unwrap(),
        )
        .unwrap();
        expected["artifacts"] = json!([]);
        let expected_reads = expected["source_reads"]
            .as_array()
            .unwrap()
            .iter()
            .map(|read| read["snapshots_created"].as_u64().unwrap_or(0) as usize)
            .sum::<usize>();
        for repeated in [false, true] {
            if repeated {
                for read in expected["source_reads"].as_array_mut().unwrap() {
                    if !read["snapshots_created"].is_null() {
                        read["snapshots_created"] = json!(0);
                    }
                }
            }
            let attempt = prepared.build();
            let result = specification_report::build_result(
                prepared.run(),
                &attempt,
                Identity {
                    runtime: "python",
                    runtime_version: "fixture-runtime",
                    engine_version: "fixture-engine",
                    example: name,
                    specification: entry,
                    base_directory: ".",
                },
            )
            .unwrap();
            assert_eq!(
                result.observations(),
                expected,
                "{name} repeated={repeated}"
            );
            assert_eq!(
                prepared.capture_reads(),
                preparation_reads + expected_reads,
                "{name}"
            );
        }
    }
}

const DICTIONARY_SPEC: &str = "schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\ninput: {SRC: input.csv}\noutput: {path: output.csv, columns: [ID, X, Y]}\ncolumns:\n  - {name: ID, type: int, derivation: SRC.ID}\n  - {name: X, type: str, derivation: {mapping: {source: SRC.CODE, dict: dict.yaml}}}\n  - {name: Y, type: str, derivation: {mapping: {source: SRC.CODE, dict: ./dict.yaml}}}\n";
#[test]
fn dictionary_resources_are_held_once_and_original_authored_model_is_retained() {
    let study = Study::new();
    fs::write(study.0.join("entry/dict.yaml"), "A: answer\n").unwrap();
    let prepared = study.prepare(DICTIONARY_SPEC).unwrap();
    assert_eq!(prepared.capture_reads(), 2);
    assert_eq!(prepared.run().document().dictionary_sources().count(), 1);
    assert_eq!(
        prepared
            .run()
            .document()
            .dictionary_sources()
            .next()
            .unwrap()
            .bytes,
        b"A: answer\n"
    );
    let document = prepared.run().document().model().document();
    let columns = document.field(document.root(), "columns").unwrap();
    let yamaa_core::schema::DocumentNode::Sequence(columns) = &document.nodes()[columns] else {
        panic!()
    };
    let column = columns[1];
    let derivation = document.field(column, "derivation").unwrap();
    let value = document.field(derivation, "value").unwrap();
    let mapping = document.field(value, "mapping").unwrap();
    let dict = document.field(mapping, "dict").unwrap();
    assert_eq!(
        document.nodes()[dict],
        yamaa_core::schema::DocumentNode::Text("dict.yaml".into())
    );
    fs::write(study.0.join("entry/dict.yaml"), b"A: changed\n").unwrap();
    let response = prepared.run().execute_csv(b"ID,CODE\n1,A\n").unwrap();
    assert!(response.table.is_some());
    let outcome: Value = serde_json::from_str(
        &yamaa_adapters::table_transport::table_snapshot(response.table.as_ref().unwrap()).unwrap(),
    )
    .unwrap();
    assert_eq!(
        outcome["rows"],
        json!([[{"int":"1"},{"str":"answer"},{"str":"answer"}]])
    );
    assert_eq!(prepared.capture_reads(), 2);
    assert!(!study.0.join("entry/output.csv").exists());
}
#[test]
fn inherited_dictionary_resources_start_at_original_declaring_layer() {
    let study = Study::new();
    fs::write(study.0.join("parent/base.yaml"), DICTIONARY_SPEC).unwrap();
    fs::write(study.0.join("parent/dict.yaml"), "A: declared-parent\n").unwrap();
    fs::write(study.0.join("entry/dict.yaml"), "A: wrong-entry\n").unwrap();
    let prepared = study
        .prepare("schema_version: '1.0'\nparents: [../parent/base.yaml]\n")
        .unwrap();
    assert_eq!(prepared.capture_reads(), 3);
    let dictionary = prepared
        .run()
        .document()
        .dictionary_sources()
        .next()
        .unwrap();
    assert_eq!(dictionary.bytes, b"A: declared-parent\n");
    assert!(dictionary.identity.ends_with("/parent/dict.yaml"));
    let response = prepared.run().execute_csv(b"ID,CODE\n1,A\n").unwrap();
    let outcome: Value = serde_json::from_str(
        &yamaa_adapters::table_transport::table_snapshot(response.table.as_ref().unwrap()).unwrap(),
    )
    .unwrap();
    assert_eq!(
        outcome["rows"],
        json!([[{"int":"1"},{"str":"declared-parent"},{"str":"declared-parent"}]])
    );
    assert!(!study.0.join("parent/input.csv").exists());
    assert!(!study.0.join("entry/output.csv").exists());
}
#[test]
fn dictionary_missing_and_directory_failures_name_authored_field_without_host_identity() {
    let study = Study::new();
    for (path, condition) in [
        ("absent.yaml", "resource_path_missing"),
        (".", "resource_path_not_regular_file"),
    ] {
        let actual: Value = serde_json::from_str(&failure(
            &study,
            &DICTIONARY_SPEC.replace("dict.yaml", path),
        ))
        .unwrap();
        assert_eq!(
            actual,
            json!({"protocol":"specification/prototype","outcome":{"status":"invalid","diagnostics":[{"phase":"validation","condition":condition,"requirement":"REQ-0785","spec_paths":["columns.X.derivation.mapping.dict"],"context":{"path":path}}]}})
        );
    }
}
#[test]
fn dictionary_invalid_content_retains_original_typed_schema_context_before_study() {
    let study = Study::new();
    fs::write(study.0.join("entry/dict.yaml"), "7: answer\n").unwrap();
    let actual: Value = serde_json::from_str(&failure(&study, DICTIONARY_SPEC)).unwrap();
    let diagnostics = actual["outcome"]["diagnostics"].as_array().unwrap();
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0]["condition"], "invalid_field_type");
    assert_eq!(
        diagnostics[0]["spec_paths"],
        json!(["columns.X.derivation.mapping.dict.key(7)"])
    );
    assert_eq!(
        diagnostics[0]["context"],
        json!({"expected":"str","actual":"int"})
    );
    assert!(!study.0.join("entry/input.csv").exists());
}
#[cfg(unix)]
#[test]
fn dictionary_symlink_policy_retains_original_resource_error() {
    let study = Study::new();
    fs::write(study.0.join("parent/dict.yaml"), "A: answer\n").unwrap();
    std::os::unix::fs::symlink(
        study.0.join("parent/dict.yaml"),
        study.0.join("entry/dict.yaml"),
    )
    .unwrap();
    let error = match study.prepare(DICTIONARY_SPEC) {
        Ok(_) => panic!(),
        Err(error) => error,
    };
    assert_eq!(
        error.into_message(),
        "resource path contains a symbolic link"
    );
}
