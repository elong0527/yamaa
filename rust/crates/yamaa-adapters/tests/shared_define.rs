//! Independent fixed-byte DM and structural AE/VS shared-generation witnesses.
#![cfg(any(unix, windows))]
use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};
use yamaa_adapters::file_define::{Define, PublicationState};
use yamaa_core::project_function::Language;
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "yamaa-define-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        Self(fs::canonicalize(path).unwrap())
    }
    fn fixture(name: &str, published: bool) -> Self {
        let this = Self::new();
        let original = root().join("benchmarks").join(name);
        for file in ["spec.yaml", "environment.yaml"] {
            fs::copy(original.join(file), this.0.join(file)).unwrap();
        }
        if published {
            fs::copy(original.join("expected/dm.csv"), this.0.join("dm.csv")).unwrap();
        }
        this
    }
    fn run(&self, created: &str) -> Define {
        Define::prepare(
            self.0.join("environment.yaml").to_str().unwrap(),
            "sdtm",
            Some(created),
            "0.3.0",
            Language::Python,
        )
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .unwrap()
}
#[test]
fn dm_both_goldens_without_input_execution_and_save_uses_held_bytes() {
    let temp = Temp::fixture("sdtm-dm-metadata", true);
    let mut run = temp.run("2026-09-11T00:00:00");
    assert!(run.issues().is_empty(), "{:?}", run.issues());
    let expected = root().join("benchmarks/sdtm-dm-metadata/expected");
    let artifacts: Vec<_> = run.artifacts().collect();
    assert_eq!(artifacts.len(), 2);
    assert_eq!(
        artifacts[0],
        (
            "dm.json",
            fs::read(expected.join("dm.json")).unwrap().as_slice(),
            PublicationState::Ready
        )
    );
    assert_eq!(
        artifacts[1],
        (
            "define.xml",
            fs::read(expected.join("define.xml")).unwrap().as_slice(),
            PublicationState::Ready
        )
    );
    assert!(!temp.0.join("input").exists());
    assert!(!temp.0.join("define.xml").exists());
    assert!(!temp.0.join("dm.json").exists());
    let reads = run.capture_reads();
    fs::remove_file(temp.0.join("spec.yaml")).unwrap();
    fs::remove_file(temp.0.join("environment.yaml")).unwrap();
    fs::write(temp.0.join("dm.csv"), b"changed after preparation").unwrap();
    assert!(run.save().is_ok());
    assert_eq!(reads, run.capture_reads());
    assert_eq!(
        fs::read(temp.0.join("define.xml")).unwrap(),
        fs::read(expected.join("define.xml")).unwrap()
    );
    assert_eq!(
        fs::read(temp.0.join("dm.json")).unwrap(),
        fs::read(expected.join("dm.json")).unwrap()
    );
    assert!(!run.save().unwrap());
}
#[test]
fn ae_and_vs_metadata_only_fixed_time_projection_has_independent_structure() {
    for (name, id, count, has_acrf, codelist) in [
        ("sdtm-ae-coding", "AE", 7, true, "MEDDRA"),
        (
            "sdtm-vs-epoch-from-subject-elements",
            "VS",
            8,
            false,
            "EPOCH",
        ),
    ] {
        let temp = Temp::fixture(name, false);
        let run = temp.run("2026-09-30T00:00:00");
        assert!(run.issues().is_empty(), "{:?}", run.issues());
        let artifacts: Vec<_> = run.artifacts().collect();
        assert_eq!(artifacts.len(), 1);
        let xml = std::str::from_utf8(artifacts[0].1).unwrap();
        assert!(xml.contains("CreationDateTime=\"2026-09-30T00:00:00\""));
        assert!(xml.contains("SourceSystemVersion=\"1.0\""));
        assert!(xml.contains(&format!("OID=\"IG.{id}\" Name=\"{id}\"")));
        assert_eq!(xml.matches("<ItemDef ").count(), count);
        assert_eq!(xml.contains("<def:AnnotatedCRF>"), has_acrf);
        assert!(xml.contains(&format!("OID=\"CL.{codelist}\"")));
        assert!(!temp
            .0
            .join(format!("{}.csv", id.to_ascii_lowercase()))
            .exists());
    }
}
#[test]
fn semantic_failure_and_target_admission_have_zero_writes() {
    let temp = Temp::fixture("sdtm-dm-metadata", true);
    let spec = fs::read_to_string(temp.0.join("spec.yaml"))
        .unwrap()
        .replace("length: 2", "length: 0");
    fs::write(temp.0.join("spec.yaml"), spec).unwrap();
    let mut run = temp.run("2026-09-11T00:00:00");
    assert!(run
        .issues()
        .iter()
        .any(|i| i.condition == "submission_length_invalid"));
    assert!(run.save().is_err());
    assert!(!temp.0.join("define.xml").exists());
    assert!(!temp.0.join("dm.json").exists());
    let temp = Temp::fixture("sdtm-dm-metadata", true);
    let mut run = temp.run("2026-09-11T00:00:00");
    assert!(run.issues().is_empty());
    fs::create_dir(temp.0.join("define.xml")).unwrap();
    assert!(run.save().is_err());
    assert!(run.failure().is_some());
    assert!(!temp.0.join("dm.json").exists());
    assert!(run.issues().iter().any(|i| i.phase == "engine"
        && i.condition == "engine_rejected"
        && i.requirement.is_none()
        && i.context.contains("publication_target_admission")));
    assert!(matches!(
        run.failure(),
        Some(yamaa_adapters::file_define::Failure::TargetAdmission { index: 1, .. })
    ));
    let states: Vec<_> = run.artifacts().map(|(_, _, state)| state).collect();
    assert_eq!(
        states,
        [PublicationState::Stopped, PublicationState::Failed]
    );
}
#[test]
fn case_alias_targets_are_refused_before_overwriting_a_sentinel() {
    let temp = Temp::fixture("sdtm-dm-metadata", true);
    let target = temp.0.join("dm.json");
    fs::write(&target, b"original publication sentinel").unwrap();
    if !temp.0.join("DM.JSON").exists() {
        // This witness concerns an actual case-insensitive volume; distinct
        // names on case-sensitive volumes are distinct publication targets.
        return;
    }
    let environment = temp.0.join("environment.yaml");
    fs::write(
        &environment,
        fs::read_to_string(&environment)
            .unwrap()
            .replace("  - spec.yaml", "  - spec.yaml\n  - second.yaml"),
    )
    .unwrap();
    fs::write(
        temp.0.join("second.yaml"),
        fs::read_to_string(temp.0.join("spec.yaml"))
            .unwrap()
            .replace("domain: DM", "domain: DM2")
            .replace("path: dm.csv", "path: DM.csv"),
    )
    .unwrap();
    fs::write(temp.0.join("define.xml"), b"original Define sentinel").unwrap();
    let mut names: Vec<_> = fs::read_dir(&temp.0)
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    names.sort();
    let mut result = temp.run("2026-09-11T00:00:00");
    assert_eq!(result.issues().len(), 1, "{:?}", result.issues());
    let issue = &result.issues()[0];
    assert_eq!(issue.phase, "validation");
    assert_eq!(issue.condition, "dataset_json_path_collision");
    assert_eq!(issue.requirement.as_deref(), Some("REQ-1226"));
    assert!(issue.context.contains("dm.json") && issue.context.contains("DM.json"));
    let reads = result.capture_reads();
    assert!(result.save().is_err());
    assert_eq!(reads, result.capture_reads());
    assert_eq!(fs::read(target).unwrap(), b"original publication sentinel");
    assert_eq!(
        fs::read(temp.0.join("define.xml")).unwrap(),
        b"original Define sentinel"
    );
    assert!(result
        .artifacts()
        .all(|(_, _, state)| state == PublicationState::Stopped));
    let mut after: Vec<_> = fs::read_dir(&temp.0)
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    after.sort();
    assert_eq!(after, names, "no publication residue");
}
#[test]
fn explicit_section_selector_datetime_and_missing_published_artifact_are_refused() {
    let temp = Temp::fixture("sdtm-dm-metadata", false);
    let mut run = temp.run("2026-09-11T00:00:00");
    assert!(run
        .issues()
        .iter()
        .any(|i| i.condition == "producer_contract_mismatch"));
    assert!(run.save().is_err());
    let run = temp.run("2026-09-11");
    assert!(run.issues().iter().any(|i| i.spec_paths == ["created"]));
    let run = Define::prepare(
        temp.0.join("environment.yaml").to_str().unwrap(),
        "SDTM",
        None,
        "0.3.0",
        Language::Python,
    );
    assert!(run.issues().iter().any(|i| i.spec_paths == ["standard"]));
}
#[test]
fn inherited_original_output_supplies_both_json_capture_and_publication_target() {
    let temp = Temp::fixture("sdtm-dm-metadata", false);
    for directory in ["parents", "children", "artifacts"] {
        fs::create_dir(temp.0.join(directory)).unwrap();
    }
    let parent = fs::read_to_string(temp.0.join("spec.yaml"))
        .unwrap()
        .replace("path: dm.csv", "path: ../artifacts/dm.csv");
    fs::write(temp.0.join("parents/base.yaml"), parent).unwrap();
    fs::write(
        temp.0.join("children/spec.yaml"),
        "schema_version: '1.0'\nparents: ../parents/base.yaml\n",
    )
    .unwrap();
    let environment = fs::read_to_string(temp.0.join("environment.yaml"))
        .unwrap()
        .replace("  - spec.yaml", "  - children/spec.yaml")
        .replace("define: define.xml", "define: artifacts/define.xml");
    fs::write(temp.0.join("environment.yaml"), environment).unwrap();
    fs::copy(
        root().join("benchmarks/sdtm-dm-metadata/expected/dm.csv"),
        temp.0.join("artifacts/dm.csv"),
    )
    .unwrap();
    let mut result = temp.run("2026-09-11T00:00:00");
    assert!(result.issues().is_empty(), "{:?}", result.issues());
    let xml = std::str::from_utf8(result.artifacts().nth(1).unwrap().1).unwrap();
    assert!(xml.contains("xlink:href=\"dm.json\""));
    assert!(std::str::from_utf8(result.artifacts().next().unwrap().1)
        .unwrap()
        .contains("\"metaDataRef\": \"define.xml\""));
    let reads = result.capture_reads();
    assert!(result.save().unwrap());
    assert!(temp.0.join("artifacts/dm.json").is_file());
    assert!(!temp.0.join("children/artifacts/dm.json").exists());
    assert!(!temp.0.join("dm.json").exists());
    assert_eq!(result.capture_reads(), reads);
}
#[test]
fn mutual_references_and_published_contract_fail_before_any_publication() {
    // REQ1206/1229 forbid a JSON-to-Define reference needing parent traversal.
    let temp = Temp::fixture("sdtm-dm-metadata", false);
    fs::create_dir(temp.0.join("artifacts")).unwrap();
    let spec = fs::read_to_string(temp.0.join("spec.yaml"))
        .unwrap()
        .replace("path: dm.csv", "path: artifacts/dm.csv");
    fs::write(temp.0.join("spec.yaml"), spec).unwrap();
    let mut run = temp.run("2026-09-11T00:00:00");
    assert!(run
        .issues()
        .iter()
        .any(|i| i.condition == "artifact_outside_document"
            && i.requirement.as_deref() == Some("REQ-1229")));
    assert!(run.save().is_err());
    assert!(!temp.0.join("dm.json").exists());
    // REQ1230 assigns a stale output contract to REQ0535, before JSON schema.
    let temp = Temp::fixture("sdtm-dm-metadata", true);
    let csv = fs::read_to_string(temp.0.join("dm.csv")).unwrap();
    fs::write(
        temp.0.join("dm.csv"),
        csv.replace("STUDYID,USUBJID", "USUBJID,STUDYID"),
    )
    .unwrap();
    let mut run = temp.run("2026-09-11T00:00:00");
    assert!(run
        .issues()
        .iter()
        .any(|i| i.condition == "producer_contract_mismatch"
            && i.requirement.as_deref() == Some("REQ-0535")));
    assert!(run.save().is_err());
    assert!(!temp.0.join("define.xml").exists());
    // A missing declared field is the same contract cause, before typed CSV
    // conversion can misclassify the absent field as a scalar/lookup problem.
    fs::write(temp.0.join("dm.csv"), csv.replace("STUDYID", "WRONG")).unwrap();
    let mut run = temp.run("2026-09-11T00:00:00");
    assert!(run
        .issues()
        .iter()
        .any(|issue| issue.requirement.as_deref() == Some("REQ-0535")));
    assert!(run.save().is_err());
    assert!(!temp.0.join("define.xml").exists());
    // REQ0957 preserves the verification phase, offending value and row key.
    fs::write(temp.0.join("dm.csv"), csv.replace(",F,USA", ",Z,USA")).unwrap();
    let mut run = temp.run("2026-09-11T00:00:00");
    let issue = run
        .issues()
        .iter()
        .find(|i| i.requirement.as_deref() == Some("REQ-0957"))
        .unwrap();
    assert_eq!(issue.phase, "verification");
    let context: serde_json::Value = serde_json::from_str(&issue.context).unwrap();
    assert_eq!(context["value"], "Z");
    assert!(context["key_values"]
        .as_array()
        .unwrap()
        .iter()
        .any(|value| value == "CATH-UCSD-0001"));
    assert!(run.save().is_err());
    assert!(!temp.0.join("dm.json").exists());
}
#[test]
fn integer_terminology_items_admit_equal_float_published_values() {
    let temp = Temp::fixture("sdtm-dm-metadata", true);
    let spec = fs::read_to_string(temp.0.join("spec.yaml"))
        .unwrap()
        .replace("name: AGE\n    type: int", "name: AGE\n    type: float")
        .replace(
            "core: Exp\n      role: Record Qualifier\n      length: 3",
            "core: Exp\n      role: Record Qualifier\n      length: 3\n      significant_digits: 3\n      codelist: AGE_VALUES",
        );
    fs::write(temp.0.join("spec.yaml"), spec).unwrap();
    let environment = fs::read_to_string(temp.0.join("environment.yaml")).unwrap().replace("sdtm:\n", "- codelists:\n  - id: AGE_VALUES\n    name: Ages\n    data_type: float\n    items: [{value: 34}, {value: 67}, {value: 52}]\nsdtm:\n");
    fs::write(temp.0.join("environment.yaml"), environment).unwrap();
    let result = temp.run("2026-09-11T00:00:00");
    assert!(result.issues().is_empty(), "{:?}", result.issues());
    let data: serde_json::Value =
        serde_json::from_slice(result.artifacts().next().unwrap().1).unwrap();
    assert_eq!(data["columns"][5]["dataType"], "float");
    assert_eq!(data["rows"][0][5].as_f64(), Some(34.0));
}
#[test]
fn unused_terminology_is_omitted_and_missing_document_links_do_not_read_study_files() {
    let temp = Temp::fixture("sdtm-dm-metadata", false);
    let environment = fs::read_to_string(temp.0.join("environment.yaml")).unwrap().replace("dataset_json: true", "dataset_json: false").replace("sdtm:\n", "- codelists:\n  - id: UNUSED\n    name: Used only in another submission\n    data_type: text\n    items: [{value: OTHER}]\nsdtm:\n");
    fs::write(temp.0.join("environment.yaml"), environment).unwrap();
    let result = temp.run("2026-09-11T00:00:00");
    assert!(result.issues().is_empty(), "{:?}", result.issues());
    let xml = std::str::from_utf8(result.artifacts().next().unwrap().1).unwrap();
    assert!(!xml.contains("UNUSED"));
    assert_eq!(xml.matches("<CodeList ").count(), 4);
    assert!(xml.contains("xlink:href=\"acrf.pdf\""));
    assert!(!temp.0.join("acrf.pdf").exists());
    assert!(!temp.0.join("input").exists());
}
#[test]
fn independent_metadata_refutations_preserve_shared_conditions_and_zero_writes() {
    for (from, to, condition, requirement) in [
        (
            "values: [F, M, U]",
            "values: [F, M]",
            "codelist_values_conflict",
            "REQ-0955",
        ),
        (
            "type: Collected",
            "type: Derived",
            "origin_contradicts_derivation",
            "REQ-0922",
        ),
        (
            "domain: DM",
            "domain: DM\nmetadata: {label: duplicate}",
            "reserved_metadata_key",
            "REQ-0910",
        ),
    ] {
        let temp = Temp::fixture("sdtm-dm-metadata", false);
        let spec = fs::read_to_string(temp.0.join("spec.yaml"))
            .unwrap()
            .replace(from, to);
        fs::write(temp.0.join("spec.yaml"), spec).unwrap();
        let mut result = temp.run("2026-09-11T00:00:00");
        assert!(
            result
                .issues()
                .iter()
                .any(|i| i.condition == condition && i.requirement.as_deref() == Some(requirement)),
            "{:?}",
            result.issues()
        );
        assert!(result.save().is_err());
        assert!(!temp.0.join("define.xml").exists());
        assert!(!temp.0.join("dm.json").exists());
    }
    let temp = Temp::fixture("sdtm-ae-coding", false);
    let spec = fs::read_to_string(temp.0.join("spec.yaml"))
        .unwrap()
        .replace(
            "codelist: MEDDRA\n      origin:\n        type: Assigned\n        source: Sponsor",
            "codelist: MEDDRA\n      origin:\n        type: Collected\n        source: Vendor",
        );
    fs::write(temp.0.join("spec.yaml"), spec).unwrap();
    let result = temp.run("2026-09-30T00:00:00");
    assert!(
        result
            .issues()
            .iter()
            .any(|i| i.condition == "origin_contradicts_derivation"
                && i.spec_paths[0].contains("AEDECOD")),
        "{:?}",
        result.issues()
    );
}
