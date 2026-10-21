//! Public calls read language version 1.0 (SCHEMA-2, #3030 D76): a file that
//! declares `yamaa: "1.0"` passes `check`, `run`, `resolve`, `submission` and
//! `lint` beside the 0.5 files, which keep their meaning. A newer version
//! fails as `unsupported_language_version`, and a 1.0 feature the engine does
//! not build yet as `unsupported_operation`, naming it.
#![cfg(feature = "yaml")]

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use arrow_array::{Array, ListArray, StringArray};
use yamaa::{CancellationToken, ContextValue, Engine, EngineOptions, HostLanguage, Table};
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
    fn read(&self, name: &str) -> Vec<u8> {
        std::fs::read(self.0.join(name)).unwrap()
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
        let (condition, rule, context) = (text("condition"), text("rule"), text("context"));
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
                (!rule.is_null(row)).then(|| rule.value(row).to_owned()),
                (0..values.len())
                    .map(|i| values.value(i).to_owned())
                    .collect(),
                yamaa_adapters::json_wire::decode(context.value(row)).unwrap(),
            ));
        }
    }
    rows
}

const SPEC: &str = "yamaa: \"1.0\"\ntype: spec\ndomain: DM\nkeys: [USUBJID]\ninput: {RAW: raw.csv}\ncolumns:\n  - {name: USUBJID, type: str, label: Subject, derivation: RAW.USUBJID}\n  - {name: SITE, type: str, label: Site, derivation: \"'S01'\"}\noutput: {path: dm.csv, columns: [USUBJID, SITE]}\n";
/// [`SPEC`] written for 0.5.
const LEGACY: &str = "schema_version: '1.0'\ndomain: DM\nkeys: [USUBJID]\ninput: {RAW: raw.csv}\ncolumns:\n  - {name: USUBJID, type: str, label: Subject, derivation: RAW.USUBJID}\n  - {name: SITE, type: str, label: Site, derivation: {literal: S01}}\noutput: {path: dm.csv, columns: [USUBJID, SITE]}\n";

/// The value at `key` of a resolved tree's map.
fn field<'a>(value: &'a ContextValue, key: &str) -> &'a ContextValue {
    let ContextValue::Map(map) = value else {
        panic!("a map: {value:?}")
    };
    &map[key]
}

#[test]
fn every_specification_call_reads_a_1_0_specification() {
    let study = Study::new();
    study.write("raw.csv", "USUBJID\nP1\nP2\n");
    study.write("spec.yaml", SPEC);
    let spec = study.path("spec.yaml");
    let engine = engine();
    let token = CancellationToken::new();
    assert_eq!(
        rows(&engine.spec_check(&spec, None, &token).unwrap().issues()),
        []
    );
    let mut run = engine.spec_run(&spec, None, &token).unwrap();
    assert_eq!(rows(&run.issues()), []);
    assert_eq!(run.output().unwrap().rows(), 2);
    assert_eq!(run.save().unwrap(), [study.path("dm.csv")]);
    let written = study.read("dm.csv");
    assert_eq!(written, b"USUBJID,SITE\nP1,S01\nP2,S01\n");
    // The resolved artifact records the language version, never
    // `schema_version` (D84).
    let resolved = engine.spec_resolve(&spec, None).unwrap();
    let ContextValue::List(entries) = field(&resolved, "entries") else {
        panic!("entries")
    };
    let entry = &entries[0];
    assert_eq!(
        field(entry, "language_version"),
        &ContextValue::Text("1.0".into())
    );
    let ContextValue::Map(entry_fields) = entry else {
        panic!("an entry")
    };
    assert!(!entry_fields.contains_key("schema_version"));
    assert_eq!(
        field(field(entry, "specification"), "yamaa"),
        &ContextValue::Text("1.0".into())
    );
    // The same specification written for 0.5 writes the same bytes.
    study.write("spec.yaml", LEGACY);
    let mut run = engine.spec_run(&spec, None, &token).unwrap();
    assert_eq!(rows(&run.issues()), []);
    run.save().unwrap();
    assert_eq!(study.read("dm.csv"), written);
}

#[test]
fn every_project_call_reads_a_1_0_project() {
    let study = Study::new();
    study.write("raw.csv", "USUBJID\nP1\n");
    study.write("dm.yaml", SPEC);
    study.write(
        "project.yaml",
        "yamaa: \"1.0\"\ntype: project\nsdtm: {specs: [dm.yaml]}\n",
    );
    let project = study.path("project.yaml");
    let engine = engine();
    let token = CancellationToken::new();
    assert_eq!(
        rows(&engine.project_check(&project, &token).unwrap().issues()),
        []
    );
    let run = engine.project_run(&project, None, &token).unwrap();
    assert_eq!(rows(&run.issues()), []);
    assert_eq!(
        engine.project_spec(&project, "DM").unwrap(),
        study.path("dm.yaml")
    );
    // A 0.5 project lists a 1.0 specification, which is read by its own header.
    study.write(
        "project.yaml",
        "schema_version: '1.0'\nsdtm: {specs: [dm.yaml]}\n",
    );
    assert_eq!(
        rows(&engine.project_check(&project, &token).unwrap().issues()),
        []
    );
}

#[test]
fn a_newer_language_version_fails_at_yamaa() {
    let study = Study::new();
    study.write("spec.yaml", &SPEC.replace("\"1.0\"", "\"1.1\""));
    let issues = rows(
        &engine()
            .spec_check(&study.path("spec.yaml"), None, &CancellationToken::new())
            .unwrap()
            .issues(),
    );
    let [(condition, rule, paths, context)] = issues.as_slice() else {
        panic!("one issue: {issues:?}")
    };
    assert_eq!(condition, "unsupported_language_version");
    assert_eq!(rule.as_deref(), Some("SCHEMA-2"));
    assert_eq!(paths, &["yamaa"]);
    assert_eq!(context["version"], Value::String("1.1".into()));
    assert_eq!(
        context["language_versions"],
        Value::Array(vec![Value::String("1.0".into())])
    );
}

#[test]
fn lint_reads_a_1_0_file() {
    let study = Study::new();
    study.write("spec.yaml", SPEC);
    let lint = yamaa::lint(&[study.path("spec.yaml")], false).unwrap();
    assert_eq!(lint.files(), 1);
}

/// D67 on a public run (D102): in 0.5 `absent` answers a variable the input
/// does not hold; the same column written for 1.0, as the converter writes
/// it, fails at the read, since there `absent` answers only a lookup that
/// finds no record (NAME-3).
#[test]
fn absent_answers_a_variable_the_input_does_not_hold_only_in_0_5() {
    let study = Study::new();
    study.write("raw.csv", "USUBJID\nP1\n");
    let spec = |header: &str| {
        format!(
            "{header}domain: DM\nkeys: [USUBJID]\ninput: {{RAW: raw.csv}}\ncolumns:\n  - {{name: USUBJID, type: str, label: Subject, derivation: RAW.USUBJID}}\n  - {{name: V, type: int, label: Value, derivation: {{source: {{variable: RAW.UNKNOWN, absent: 7}}}}}}\noutput: {{path: dm.csv, columns: [USUBJID, V]}}\n"
        )
    };
    let path = study.path("spec.yaml");
    let engine = engine();
    let token = CancellationToken::new();
    study.write("spec.yaml", &spec("schema_version: '1.0'\n"));
    let mut run = engine.spec_run(&path, None, &token).unwrap();
    assert_eq!(rows(&run.issues()), []);
    run.save().unwrap();
    assert_eq!(study.read("dm.csv"), b"USUBJID,V\nP1,7\n");
    study.write("spec.yaml", &spec("yamaa: \"1.0\"\ntype: spec\n"));
    let issues = rows(&engine.spec_run(&path, None, &token).unwrap().issues());
    let [(condition, rule, paths, context)] = issues.as_slice() else {
        panic!("one issue: {issues:?}")
    };
    assert_eq!(condition, "unknown_field");
    assert_eq!(rule.as_deref(), Some("NAME-3"));
    assert_eq!(paths, &["columns.V.derivation.source"]);
    assert_eq!(context["identifier"], Value::String("RAW.UNKNOWN".into()));
}

/// The `unsupported_operation` refusals of a check of `spec`, with `project`.
fn refusals(study: &Study, spec: &str, project: Option<&str>) -> Vec<(String, Vec<String>)> {
    let project = project.map(|project| study.path(project));
    rows(
        &engine()
            .spec_check(
                &study.path(spec),
                project.as_deref(),
                &CancellationToken::new(),
            )
            .unwrap()
            .issues(),
    )
    .into_iter()
    .map(|(condition, rule, paths, context)| {
        assert_eq!(
            condition, "unsupported_operation",
            "{spec}: {paths:?} {context:?}"
        );
        assert_eq!(rule, None);
        let Value::String(operation) = &context["operation"] else {
            panic!("an operation: {context:?}")
        };
        (operation.clone(), paths)
    })
    .collect()
}

fn refusal(operation: &str, path: &str) -> (String, Vec<String>) {
    (operation.into(), vec![path.into()])
}

#[test]
fn each_1_0_feature_not_built_yet_is_refused_as_unsupported_operation() {
    let study = Study::new();
    let submission = "\n    submission:\n      core: Req\n      length: 8\n      origins:\n        - {type: Assigned, source: Sponsor}\n";
    let spec = |columns: &str| {
        format!("yamaa: \"1.0\"\ntype: spec\nmodel: SDTM\ndomain: DM\nkeys: [USUBJID]\ninput: {{RAW: raw.csv}}\noutput: {{path: dm.csv, columns: [USUBJID]}}\ncolumns:\n  - name: USUBJID\n    type: str\n    label: Subject\n    derivation: RAW.USUBJID{columns}")
    };
    // A value-level comparator (P1-14).
    study.write(
        "spec.yaml",
        &spec(&format!(
            "{submission}      value_level:\n        - {{id: OTHER, where: {{USUBJID: {{comparator: NE, values: [P1]}}}}}}\n"
        )),
    );
    assert_eq!(
        refusals(&study, "spec.yaml", None),
        [refusal(
            "value_level_comparator",
            "columns.USUBJID.submission.value_level.OTHER.where.USUBJID.comparator"
        )]
    );
    // An ODM XML input (P1-15).
    study.write("spec.yaml", &spec("").replace("raw.csv", "raw.xml"));
    assert_eq!(
        refusals(&study, "spec.yaml", None),
        [refusal("odm_xml_input", "input.RAW.path")]
    );
}

/// A 1.0 project whose `sdtm` section composes one dataset, published as
/// `dm.csv`. Every benchmark's Define documents, converted to 1.0, are
/// compared with their expected bytes by `make check-conversion`.
fn define_study() -> Study {
    let study = Study::new();
    study.write(
        "project.yaml",
        "yamaa: \"1.0\"\ntype: project\nstudy: {id: S01, name: S01, description: Example study, protocol_name: S01}\nsdtm:\n  standards:\n    - {name: SDTMIG, version: \"3.4\", status: Final}\n  specs: [dm.yaml]\n  define: define.xml\n",
    );
    study.write(
        "dm.yaml",
        "yamaa: \"1.0\"\ntype: spec\nmodel: SDTM\ndomain: DM\nkeys: [STUDYID]\ninput: {RAW: raw.csv}\noutput: {path: dm.csv, columns: [STUDYID]}\nsubmission: {label: Demographics, class: SPECIAL PURPOSE, structure: One record per subject, repeating: false}\ncolumns:\n  - name: STUDYID\n    type: str\n    label: Study Identifier\n    derivation: RAW.STUDYID\n    submission:\n      core: Req\n      role: Identifier\n      length: 8\n      origins:\n        - {type: Protocol, source: Sponsor}\n",
    );
    study.write("dm.csv", "STUDYID\nS01\n");
    study
}

#[test]
fn a_1_0_project_composes_its_submission() {
    let study = define_study();
    let project = study.path("project.yaml");
    let engine = engine();
    let token = CancellationToken::new();
    let submit = || {
        engine
            .project_submission(
                &project,
                Some("sdtm"),
                Some("2026-09-11T00:00:00"),
                "0.4.0",
                &token,
            )
            .unwrap()
    };
    let mut submission = submit();
    assert_eq!(rows(&submission.issues()), []);
    assert_eq!(submission.save().unwrap(), [study.path("define.xml")]);
    let define = String::from_utf8(study.read("define.xml")).unwrap();
    assert!(define.contains("Name=\"DM\""), "{define}");
    assert!(define.contains("Origin Type=\"Protocol\""), "{define}");
}

/// SUB-3 (D25): a specification that declares submission metadata names its
/// model, before any input is read, whether it is checked alone or listed.
#[test]
fn submission_metadata_needs_a_model() {
    let study = define_study();
    let dm = String::from_utf8(study.read("dm.yaml")).unwrap();
    study.write("dm.yaml", &dm.replace("model: SDTM\n", ""));
    let engine = engine();
    let token = CancellationToken::new();
    let issues = rows(
        &engine
            .spec_check(&study.path("dm.yaml"), None, &token)
            .unwrap()
            .issues(),
    );
    let [(condition, rule, paths, context)] = issues.as_slice() else {
        panic!("one issue: {issues:?}")
    };
    assert_eq!(condition, "missing_required_field");
    assert_eq!(rule.as_deref(), Some("SUB-3"));
    assert_eq!(paths, &["model"]);
    assert_eq!(context["field"], Value::String("model".into()));
    assert_eq!(context["class"], Value::String("root_class".into()));
    // Listed, the specification reports the same issue when its section
    // composes it, as it reports any other fault of its own.
    let listed = rows(
        &engine
            .project_submission(
                &study.path("project.yaml"),
                Some("sdtm"),
                Some("2026-09-11T00:00:00"),
                "0.4.0",
                &token,
            )
            .unwrap()
            .issues(),
    );
    assert_eq!(listed, issues);
    // A specification without submission metadata follows no model.
    study.write("spec.yaml", SPEC);
    assert_eq!(
        rows(
            &engine
                .spec_check(&study.path("spec.yaml"), None, &token)
                .unwrap()
                .issues()
        ),
        []
    );
}

/// SUB-3 (D25, D58): a section lists only specifications of its model, and
/// each guide it lists belongs to that model under a name of its own
/// (DEFINE-6). Each fault fails before any published dataset is read.
#[test]
fn a_section_lists_only_specifications_of_its_model() {
    let study = define_study();
    let project = study.path("project.yaml");
    let engine = engine();
    let token = CancellationToken::new();
    let submit = || {
        rows(
            &engine
                .project_submission(
                    &project,
                    Some("sdtm"),
                    Some("2026-09-11T00:00:00"),
                    "0.4.0",
                    &token,
                )
                .unwrap()
                .issues(),
        )
    };
    let one = |issues: Vec<(String, Option<String>, Vec<String>, Value)>| {
        let [issue] = <[_; 1]>::try_from(issues).unwrap_or_else(|issues| panic!("{issues:?}"));
        issue
    };
    let dm = String::from_utf8(study.read("dm.yaml")).unwrap();
    // A specification of another model, at its `model`.
    study.write("dm.yaml", &dm.replace("model: SDTM", "model: ADaM"));
    let (condition, rule, paths, context) = one(submit());
    assert_eq!(condition, "standard_model_mismatch");
    assert_eq!(rule.as_deref(), Some("SUB-3"));
    assert_eq!(paths, ["sdtm.specs[0].model"]);
    assert_eq!(context["value"], Value::String("ADaM".into()));
    assert_eq!(context["model"], Value::String("SDTM".into()));
    assert_eq!(context["section"], Value::String("sdtm".into()));
    // A listed specification without `model` or submission metadata.
    study.write("dm.yaml", SPEC);
    let (condition, rule, paths, context) = one(submit());
    assert_eq!(condition, "missing_required_field");
    assert_eq!(rule.as_deref(), Some("SUB-3"));
    assert_eq!(paths, ["sdtm.specs[0].model"]);
    assert_eq!(context["field"], Value::String("model".into()));
    assert_eq!(context["model"], Value::String("SDTM".into()));
    assert_eq!(context["section"], Value::String("sdtm".into()));
    // A guide of another model, at the section's first standard.
    study.write("dm.yaml", &dm);
    let declared = String::from_utf8(study.read("project.yaml")).unwrap();
    study.write(
        "project.yaml",
        &declared.replace(
            "{name: SDTMIG, version: \"3.4\"",
            "{name: ADaMIG, version: \"1.3\"",
        ),
    );
    let (condition, rule, paths, context) = one(submit());
    assert_eq!(condition, "standard_model_mismatch");
    assert_eq!(rule.as_deref(), Some("DEFINE-6"));
    assert_eq!(paths, ["sdtm.standards[0].name"]);
    assert_eq!(context["value"], Value::String("ADaMIG".into()));
    assert_eq!(context["model"], Value::String("SDTM".into()));
    assert_eq!(context["section"], Value::String("sdtm".into()));
    // Every guide the section lists is checked, not only its first.
    let second = |guide: &str| {
        declared.replace(
            "status: Final}\n",
            &format!("status: Final}}\n    - {guide}\n"),
        )
    };
    study.write(
        "project.yaml",
        &second("{name: SENDIG, version: \"3.1\", status: Final}"),
    );
    let (condition, rule, paths, context) = one(submit());
    assert_eq!(condition, "standard_model_mismatch");
    assert_eq!(rule.as_deref(), Some("DEFINE-6"));
    assert_eq!(paths, ["sdtm.standards[1].name"]);
    assert_eq!(context["value"], Value::String("SENDIG".into()));
    // A name an earlier entry takes, at the later entry.
    study.write(
        "project.yaml",
        &second("{name: SDTMIG, version: \"3.3\", status: Final}"),
    );
    let (condition, rule, paths, context) = one(submit());
    assert_eq!(condition, "duplicate_identifier");
    assert_eq!(rule.as_deref(), Some("DEFINE-6"));
    assert_eq!(paths, ["sdtm.standards[1].name"]);
    assert_eq!(context["identifier"], Value::String("SDTMIG".into()));
}

/// P-ORIGIN-1 (D16, D70): a column lists one or more origins, and the
/// document writes one `def:Origin` per origin, in declaration order
/// (DEFINE-19); conflicting origins fail before any input is read (SUB-14,
/// D69).
#[test]
fn a_column_declares_several_origins() {
    let study = define_study();
    let dm = String::from_utf8(study.read("dm.yaml")).unwrap();
    let engine = engine();
    let token = CancellationToken::new();
    let check = || {
        rows(
            &engine
                .spec_check(&study.path("dm.yaml"), None, &token)
                .unwrap()
                .issues(),
        )
    };
    let several = |origins: &str| {
        dm.replace(
            "      origins:\n        - {type: Protocol, source: Sponsor}\n",
            &format!("      origins:\n{origins}"),
        )
    };
    study.write(
        "dm.yaml",
        &several(
            "        - {type: Protocol, source: Sponsor}\n        - {type: Collected, source: Vendor}\n",
        ),
    );
    assert_eq!(check(), []);
    let mut submission = engine
        .project_submission(
            &study.path("project.yaml"),
            Some("sdtm"),
            Some("2026-09-11T00:00:00"),
            "0.4.0",
            &token,
        )
        .unwrap();
    assert_eq!(rows(&submission.issues()), []);
    submission.save().unwrap();
    let define = String::from_utf8(study.read("define.xml")).unwrap();
    assert!(
        define.contains(
            "        <def:Origin Type=\"Protocol\" Source=\"Sponsor\"/>\n        <def:Origin Type=\"Collected\" Source=\"Vendor\"/>\n"
        ),
        "{define}"
    );
    // A repeated type and source, and an origin beside a Predecessor.
    for (origins, condition_path, origin_type) in [
        (
            "        - {type: Protocol, source: Sponsor}\n        - {type: Protocol, source: Sponsor}\n",
            "columns.STUDYID.submission.origins[1]",
            "Protocol",
        ),
        (
            "        - {type: Predecessor, description: RAW.STUDYID}\n        - {type: Protocol, source: Sponsor}\n",
            "columns.STUDYID.submission.origins[1]",
            "Protocol",
        ),
    ] {
        study.write("dm.yaml", &several(origins));
        let issues = check();
        let [(condition, rule, paths, context)] = issues.as_slice() else {
            panic!("one issue: {issues:?}")
        };
        assert_eq!(condition, "conflicting_origins");
        assert_eq!(rule.as_deref(), Some("SUB-14"));
        assert_eq!(paths, &[condition_path]);
        assert_eq!(context["column"], Value::String("STUDYID".into()));
        assert_eq!(context["origin_type"], Value::String(origin_type.into()));
    }
    // An empty list declares no origin.
    study.write(
        "dm.yaml",
        &several("        []\n").replace(":\n        []", ": []"),
    );
    let issues = check();
    let [(condition, rule, paths, _)] = issues.as_slice() else {
        panic!("one issue: {issues:?}")
    };
    assert_eq!(condition, "origin_missing");
    assert_eq!(rule.as_deref(), Some("SUB-14"));
    assert_eq!(paths, &["columns.STUDYID.submission"]);
}

/// D65: a 1.0 SDTM or ADaM column takes no `Not Available` origin, which a
/// SEND column keeps; a 0.5 specification keeps the pairs of 0.5.0 (D75).
#[test]
fn not_available_is_an_origin_of_send_alone() {
    let study = define_study();
    let dm = String::from_utf8(study.read("dm.yaml")).unwrap();
    let engine = engine();
    let token = CancellationToken::new();
    let check = |name: &str| {
        rows(
            &engine
                .spec_check(&study.path(name), None, &token)
                .unwrap()
                .issues(),
        )
    };
    let unavailable = dm.replace(
        "{type: Protocol, source: Sponsor}",
        "{type: Not Available, description: Not recorded at this site}",
    );
    study.write("dm.yaml", &unavailable);
    let issues = check("dm.yaml");
    let [(condition, rule, paths, context)] = issues.as_slice() else {
        panic!("one issue: {issues:?}")
    };
    assert_eq!(condition, "origin_source_not_admitted");
    assert_eq!(rule.as_deref(), Some("SUB-15"));
    assert_eq!(paths, &["columns.STUDYID.submission.origins[0]"]);
    assert_eq!(
        context["origin_type"],
        Value::String("Not Available".into())
    );
    assert_eq!(context["model"], Value::String("SDTM".into()));
    study.write(
        "dm.yaml",
        &unavailable.replace("model: SDTM", "model: SEND"),
    );
    assert_eq!(check("dm.yaml"), []);
}

/// SUB-2: a dataset's `standard` is governed in 1.0, so a 1.0 root
/// `metadata` map does not carry it; a 0.5 one still may.
#[test]
fn a_1_0_root_metadata_reserves_standard() {
    let study = Study::new();
    study.write("raw.csv", "USUBJID\nP1\n");
    study.write(
        "spec.yaml",
        &format!("{SPEC}metadata: {{standard: SDTMIG, note: kept}}\n"),
    );
    study.write(
        "legacy.yaml",
        &format!("{LEGACY}metadata: {{standard: SDTMIG}}\n"),
    );
    let engine = engine();
    let token = CancellationToken::new();
    let check = |spec: &str| {
        rows(
            &engine
                .spec_check(&study.path(spec), None, &token)
                .unwrap()
                .issues(),
        )
    };
    let issues = check("spec.yaml");
    let [(condition, rule, paths, context)] = issues.as_slice() else {
        panic!("one issue: {issues:?}")
    };
    assert_eq!(condition, "reserved_metadata_key");
    assert_eq!(rule.as_deref(), Some("SUB-2"));
    assert_eq!(paths, &["metadata.standard"]);
    assert_eq!(context["key"], Value::String("standard".into()));
    assert_eq!(check("legacy.yaml"), []);
}

/// D16 (P-STD-1, DEFINE-6, DEFINE-8): the document lists every guide of the
/// section in declaration order, a `Provisional` one too, and each dataset
/// binds the guide its `submission.standard` selects, or the first.
#[test]
fn a_dataset_binds_the_section_standard_it_selects() {
    let study = define_study();
    let declared = String::from_utf8(study.read("project.yaml")).unwrap();
    study.write(
        "project.yaml",
        &declared.replace(
            "status: Final}\n",
            "status: Final}\n    - {name: SDTMIG-MD, version: \"1.0\", status: Provisional}\n",
        ),
    );
    let dm = String::from_utf8(study.read("dm.yaml")).unwrap();
    let engine = engine();
    let token = CancellationToken::new();
    let define = || {
        let mut submission = engine
            .project_submission(
                &study.path("project.yaml"),
                Some("sdtm"),
                Some("2026-09-11T00:00:00"),
                "0.4.0",
                &token,
            )
            .unwrap();
        assert_eq!(rows(&submission.issues()), []);
        submission.save().unwrap();
        String::from_utf8(study.read("define.xml")).unwrap()
    };
    let standards = "<def:Standard OID=\"STD.SDTMIG\" Name=\"SDTMIG\" Type=\"IG\" Version=\"3.4\" Status=\"Final\"/>\n        <def:Standard OID=\"STD.SDTMIG-MD\" Name=\"SDTMIG-MD\" Type=\"IG\" Version=\"1.0\" Status=\"Provisional\"/>\n";
    // Without a selection the dataset binds the first guide, and the second
    // is still listed.
    let document = define();
    assert!(document.contains(standards), "{document}");
    assert!(
        document.contains("def:StandardOID=\"STD.SDTMIG\""),
        "{document}"
    );
    // A selection is checked only where a section composes the dataset.
    let selecting = dm.replace(
        "repeating: false}",
        "repeating: false, standard: SDTMIG-MD}",
    );
    study.write("dm.yaml", &selecting);
    assert_eq!(
        rows(
            &engine
                .spec_check(&study.path("dm.yaml"), None, &token)
                .unwrap()
                .issues()
        ),
        []
    );
    let document = define();
    assert!(document.contains(standards), "{document}");
    assert!(
        document.contains("def:StandardOID=\"STD.SDTMIG-MD\""),
        "{document}"
    );
    assert!(
        !document.contains("def:StandardOID=\"STD.SDTMIG\""),
        "{document}"
    );
    // A name the section does not list fails before any document is written.
    std::fs::remove_file(study.0.join("define.xml")).unwrap();
    study.write(
        "dm.yaml",
        &dm.replace(
            "repeating: false}",
            "repeating: false, standard: SDTMIG-AP}",
        ),
    );
    let issues = rows(
        &engine
            .project_submission(
                &study.path("project.yaml"),
                Some("sdtm"),
                Some("2026-09-11T00:00:00"),
                "0.4.0",
                &token,
            )
            .unwrap()
            .issues(),
    );
    let [(condition, rule, paths, context)] = issues.as_slice() else {
        panic!("one issue: {issues:?}")
    };
    assert_eq!(condition, "unknown_standard");
    assert_eq!(rule.as_deref(), Some("DEFINE-8"));
    assert_eq!(paths, &["sdtm.specs[0].submission.standard"]);
    assert_eq!(context["value"], Value::String("SDTMIG-AP".into()));
    assert_eq!(context["section"], Value::String("sdtm".into()));
    assert_eq!(
        context["standards"],
        Value::Array(vec![
            Value::String("SDTMIG".into()),
            Value::String("SDTMIG-MD".into())
        ])
    );
    assert!(!study.0.join("define.xml").exists());
}
