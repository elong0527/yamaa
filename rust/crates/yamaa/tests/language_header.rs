//! Public calls read language version 1.0 (SCHEMA-2, #3030 D76): a file that
//! declares `yamaa: "1.0"` passes `check`, `run`, `resolve`, `submission` and
//! `lint`. A newer version fails as `unsupported_language_version`. Every 1.0
//! form is built, so none is refused as `unsupported_operation` any longer
//! (D76).
#![cfg(feature = "yaml")]

use arrow_array::{Array, ListArray, StringArray};
use yamaa::{
    CancellationToken, ContextValue, Engine, EngineOptions, HostLanguage, SubmissionOptions, Table,
};
use yamaa_core::json::Value;

struct Study(crate::support::Scratch);
impl Study {
    fn new() -> Self {
        let root = crate::support::Scratch::new("language-header");
        Self(root)
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
        rows(
            &engine
                .spec(&spec)
                .cancel_with(&token)
                .check()
                .unwrap()
                .issues()
        ),
        []
    );
    let mut run = engine.spec(&spec).cancel_with(&token).run().unwrap();
    assert_eq!(rows(&run.issues()), []);
    assert_eq!(run.output().unwrap().rows(), 2);
    assert_eq!(
        run.save(&CancellationToken::new()).unwrap(),
        [study.path("dm.csv")]
    );
    assert_eq!(study.read("dm.csv"), b"USUBJID,SITE\nP1,S01\nP2,S01\n");
    // The resolved artifact records the language version, never
    // `schema_version` (D84).
    let resolved = engine.spec(&spec).resolve().unwrap();
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
        rows(
            &engine
                .project(&project)
                .cancel_with(&token)
                .check()
                .unwrap()
                .issues()
        ),
        []
    );
    let run = engine.project(&project).cancel_with(&token).run().unwrap();
    assert_eq!(rows(&run.issues()), []);
    assert_eq!(
        engine.project(&project).spec("DM").unwrap().path(),
        study.path("dm.yaml")
    );
    // A project file that declares `schema_version` is written in language
    // 0.5, which the engine does not read: it is refused at its own
    // `schema_version`, as a finding of the project file (SCHEMA-2).
    study.write(
        "project.yaml",
        "schema_version: '1.0'\nsdtm: {specs: [dm.yaml]}\n",
    );
    assert_eq!(
        rows(
            &engine
                .project(&project)
                .cancel_with(&token)
                .check()
                .unwrap()
                .issues()
        ),
        [(
            "unsupported_language_version".to_owned(),
            Some("SCHEMA-2".to_owned()),
            vec!["schema_version".to_owned()],
            yamaa_adapters::json!({"version": "0.5", "language_versions": ["1.0"],
                "source": project, "entry": project, "environment_path": "$"}),
        )]
    );
}

#[test]
fn a_newer_language_version_fails_at_yamaa() {
    let study = Study::new();
    study.write("spec.yaml", &SPEC.replace("\"1.0\"", "\"1.1\""));
    let issues = rows(
        &engine()
            .spec(&study.path("spec.yaml"))
            .check()
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

/// D67 on a public run (D102): `absent` answers only a lookup that finds no
/// record, so a column that reads a variable the input does not hold fails
/// at the read, whatever its `absent` declares (NAME-3).
#[test]
fn absent_does_not_answer_a_variable_the_input_does_not_hold() {
    let study = Study::new();
    study.write("raw.csv", "USUBJID\nP1\n");
    study.write(
        "spec.yaml",
        "yamaa: \"1.0\"\ntype: spec\ndomain: DM\nkeys: [USUBJID]\ninput: {RAW: raw.csv}\ncolumns:\n  - {name: USUBJID, type: str, label: Subject, derivation: RAW.USUBJID}\n  - {name: V, type: int, label: Value, derivation: {source: {variable: RAW.UNKNOWN, absent: 7}}}\noutput: {path: dm.csv, columns: [USUBJID, V]}\n",
    );
    let issues = rows(
        &engine()
            .spec(&study.path("spec.yaml"))
            .run()
            .unwrap()
            .issues(),
    );
    let [(condition, rule, paths, context)] = issues.as_slice() else {
        panic!("one issue: {issues:?}")
    };
    assert_eq!(condition, "unknown_field");
    assert_eq!(rule.as_deref(), Some("NAME-3"));
    assert_eq!(paths, &["columns.V.derivation.source"]);
    assert_eq!(context["identifier"], Value::String("RAW.UNKNOWN".into()));
}

/// A 1.0 project whose `sdtm` section composes one dataset, published as
/// `dm.csv`. Every benchmark's Define documents are compared with their
/// expected bytes by the installed host suites.
fn define_study() -> Study {
    let study = Study::new();
    study.write(
        "project.yaml",
        "yamaa: \"1.0\"\ntype: project\nstudy: {id: S01, name: S01, description: Example study, protocol_name: S01}\nsdtm:\n  standards:\n    - {name: SDTMIG, version: \"3.4\", status: Final}\n  specs: [dm.yaml]\n  define: define.xml\n  context: Other\n",
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
            .project(&project)
            .section("sdtm")
            .cancel_with(&token)
            .submission(&SubmissionOptions::new("0.4.0").with_created("2026-09-11T00:00:00"))
            .unwrap()
    };
    let mut submission = submit();
    assert_eq!(rows(&submission.issues()), []);
    assert_eq!(
        submission.save(&CancellationToken::new()).unwrap(),
        [study.path("define.xml")]
    );
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
            .spec(&study.path("dm.yaml"))
            .cancel_with(&token)
            .check()
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
            .project(&study.path("project.yaml"))
            .section("sdtm")
            .cancel_with(&token)
            .submission(&SubmissionOptions::new("0.4.0").with_created("2026-09-11T00:00:00"))
            .unwrap()
            .issues(),
    );
    assert_eq!(listed, issues);
    // A specification without submission metadata follows no model.
    study.write("spec.yaml", SPEC);
    assert_eq!(
        rows(
            &engine
                .spec(&study.path("spec.yaml"))
                .cancel_with(&token)
                .check()
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
                .project(&project)
                .section("sdtm")
                .cancel_with(&token)
                .submission(&SubmissionOptions::new("0.4.0").with_created("2026-09-11T00:00:00"))
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
                .spec(&study.path("dm.yaml"))
                .cancel_with(&token)
                .check()
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
        .project(&study.path("project.yaml"))
        .section("sdtm")
        .cancel_with(&token)
        .submission(&SubmissionOptions::new("0.4.0").with_created("2026-09-11T00:00:00"))
        .unwrap();
    assert_eq!(rows(&submission.issues()), []);
    submission.save(&CancellationToken::new()).unwrap();
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
/// SEND column keeps.
#[test]
fn not_available_is_an_origin_of_send_alone() {
    let study = define_study();
    let dm = String::from_utf8(study.read("dm.yaml")).unwrap();
    let engine = engine();
    let token = CancellationToken::new();
    let check = |name: &str| {
        rows(
            &engine
                .spec(&study.path(name))
                .cancel_with(&token)
                .check()
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

/// SUB-12 (D66, P-VLM-1): a value-level `NE` or `NOTIN` selects the rows
/// whose where column holds none of its values, a missing value included,
/// and Define-XML writes it as the `RangeCheck` comparator (DEFINE-27).
#[test]
fn a_value_level_entry_selects_the_rows_that_hold_none_of_its_values() {
    let study = Study::new();
    study.write(
        "project.yaml",
        "yamaa: \"1.0\"\ntype: project\nstudy: {id: S01, name: S01, description: Example study, protocol_name: S01}\nsdtm:\n  standards:\n    - {name: SDTMIG, version: \"3.4\", status: Final}\n  specs: [xx.yaml]\n  define: define.xml\n  context: Other\n",
    );
    let spec = |entries: &str| {
        format!(
            "yamaa: \"1.0\"\ntype: spec\nmodel: SDTM\ndomain: XX\nkeys: [USUBJID]\ninput: {{RAW: raw.csv}}\noutput: {{path: xx.csv, columns: [USUBJID, KIND, VALUE]}}\nsubmission: {{label: Example, class: FINDINGS, structure: One record per subject, repeating: false}}\ncolumns:\n  - name: USUBJID\n    type: str\n    label: Subject\n    derivation: RAW.USUBJID\n    submission: {{core: Req, role: Identifier, length: 2, origins: [{{type: Assigned, source: Sponsor}}]}}\n  - name: KIND\n    type: str\n    label: Kind\n    derivation: RAW.KIND\n    submission: {{core: Perm, role: Record Qualifier, length: 1, origins: [{{type: Assigned, source: Sponsor}}]}}\n  - name: VALUE\n    type: str\n    label: Value\n    derivation: RAW.VALUE\n    submission:\n      core: Perm\n      role: Result Qualifier\n      length: 1\n      origins: [{{type: Assigned, source: Sponsor}}]\n      value_level:\n{entries}"
        )
    };
    let entries = |other: &str, listed: &str| {
        format!(
            "        - {{id: OTHER, where: {{KIND: {other}}}, mandatory: true}}\n        - {{id: LISTED, where: {{KIND: {listed}}}}}\n"
        )
    };
    let engine = engine();
    let token = CancellationToken::new();
    let check = || {
        rows(
            &engine
                .spec(&study.path("xx.yaml"))
                .cancel_with(&token)
                .check()
                .unwrap()
                .issues(),
        )
    };
    // `NE [X]` and `[X]` on one column share no value, so they do not overlap.
    study.write(
        "xx.yaml",
        &spec(&entries("{comparator: NE, values: [X]}", "[X]")),
    );
    assert_eq!(check(), []);
    // The NE entry selects the row whose kind is missing, and requires its
    // value there.
    study.write("raw.csv", "USUBJID,KIND,VALUE\nP1,X,A\nP2,,B\nP3,Y,C\n");
    let mut run = engine
        .spec(&study.path("xx.yaml"))
        .cancel_with(&token)
        .run()
        .unwrap();
    assert_eq!(rows(&run.issues()), []);
    run.save(&CancellationToken::new()).unwrap();
    study.write("raw.csv", "USUBJID,KIND,VALUE\nP1,X,A\nP2,,\nP3,Y,C\n");
    let issues = rows(
        &engine
            .spec(&study.path("xx.yaml"))
            .cancel_with(&token)
            .run()
            .unwrap()
            .issues(),
    );
    assert!(
        issues
            .iter()
            .any(|(condition, rule, _, _)| condition == "not_missing_failed"
                && rule.as_deref() == Some("SUB-11")),
        "{issues:?}"
    );
    // The document writes each comparator.
    let mut submission = engine
        .project(&study.path("project.yaml"))
        .section("sdtm")
        .cancel_with(&token)
        .submission(&SubmissionOptions::new("0.4.0").with_created("2026-09-11T00:00:00"))
        .unwrap();
    assert_eq!(rows(&submission.issues()), []);
    submission.save(&CancellationToken::new()).unwrap();
    let define = String::from_utf8(study.read("define.xml")).unwrap();
    for comparator in ["NE", "EQ"] {
        assert!(
            define.contains(&format!(
                "<RangeCheck Comparator=\"{comparator}\" SoftHard=\"Soft\" def:ItemOID=\"IT.XX.KIND\">"
            )),
            "{define}"
        );
    }
    // NOTIN takes two or more values and NE one.
    for (other, comparator) in [
        ("{comparator: NOTIN, values: [X]}", "NOTIN"),
        ("{comparator: NE, values: [X, Y]}", "NE"),
    ] {
        study.write("xx.yaml", &spec(&entries(other, "[Z]")));
        let issues = check();
        let [(condition, rule, paths, context)] = issues.as_slice() else {
            panic!("one issue: {issues:?}")
        };
        assert_eq!(condition, "value_level_where_invalid");
        assert_eq!(rule.as_deref(), Some("SUB-12"));
        assert_eq!(
            paths,
            &["columns.VALUE.submission.value_level.OTHER.where.KIND"]
        );
        assert_eq!(context["reason"], Value::String("value_count".into()));
        assert_eq!(context["comparator"], Value::String(comparator.into()));
    }
    // NOTIN [X, Y] and [X] share no value; two exclusions on one column
    // always share the missing value.
    study.write(
        "xx.yaml",
        &spec(&entries("{comparator: NOTIN, values: [X, Y]}", "[X]")),
    );
    assert_eq!(check(), []);
    study.write(
        "xx.yaml",
        &spec(&entries(
            "{comparator: NE, values: [X]}",
            "{comparator: NE, values: [Y]}",
        )),
    );
    let issues = check();
    let [(condition, rule, _, context)] = issues.as_slice() else {
        panic!("one issue: {issues:?}")
    };
    assert_eq!(condition, "value_level_overlap");
    assert_eq!(rule.as_deref(), Some("SUB-12"));
    assert_eq!(
        context["entries"],
        Value::Array(vec![
            Value::String("OTHER".into()),
            Value::String("LISTED".into())
        ])
    );
    // A comparison class without a comparator restates the list (SCHEMA-15).
    study.write("xx.yaml", &spec(&entries("{values: [X]}", "[Y]")));
    let issues = check();
    let [(condition, _, paths, _)] = issues.as_slice() else {
        panic!("one issue: {issues:?}")
    };
    assert_eq!(condition, "redundant_long_form");
    assert_eq!(
        paths,
        &["columns.VALUE.submission.value_level.OTHER.where.KIND"]
    );
}

/// SUB-2: a dataset's `standard` is governed in 1.0, so a 1.0 root
/// `metadata` map does not carry it.
#[test]
fn a_1_0_root_metadata_reserves_standard() {
    let study = Study::new();
    study.write("raw.csv", "USUBJID\nP1\n");
    study.write(
        "spec.yaml",
        &format!("{SPEC}metadata: {{standard: SDTMIG, note: kept}}\n"),
    );
    let engine = engine();
    let token = CancellationToken::new();
    let check = |spec: &str| {
        rows(
            &engine
                .spec(&study.path(spec))
                .cancel_with(&token)
                .check()
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
            .project(&study.path("project.yaml"))
            .section("sdtm")
            .cancel_with(&token)
            .submission(&SubmissionOptions::new("0.4.0").with_created("2026-09-11T00:00:00"))
            .unwrap();
        assert_eq!(rows(&submission.issues()), []);
        submission.save(&CancellationToken::new()).unwrap();
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
                .spec(&study.path("dm.yaml"))
                .cancel_with(&token)
                .check()
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
            .project(&study.path("project.yaml"))
            .section("sdtm")
            .cancel_with(&token)
            .submission(&SubmissionOptions::new("0.4.0").with_created("2026-09-11T00:00:00"))
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

/// DEFINE-7: a listed specification without its submission metadata fails at
/// its listing, and an output column without its `submission` or its `label`
/// at the column's `submission`, naming in `detail` the domain, the column or
/// the missing `label`, and in `field` the metadata it lacks. A column
/// without both fails once, for its `submission`.
#[test]
fn missing_submission_metadata_is_reported_at_the_listing_or_the_column_submission() {
    let study = define_study();
    let dm = String::from_utf8(study.read("dm.yaml")).unwrap();
    let engine = engine();
    let token = CancellationToken::new();
    let label = "    label: Study Identifier\n";
    let column = "    submission:\n      core: Req\n      role: Identifier\n      length: 8\n      origins:\n        - {type: Protocol, source: Sponsor}\n";
    let root = "submission: {label: Demographics, class: SPECIAL PURPOSE, structure: One record per subject, repeating: false}\n";
    for (removed, path, context, case) in [
        (
            &[label][..],
            "sdtm.specs[0].columns.STUDYID.submission",
            r#"{"detail":"label","field":"label"}"#,
            "a column without its label",
        ),
        (
            &[column],
            "sdtm.specs[0].columns.STUDYID.submission",
            r#"{"detail":"STUDYID","field":"submission"}"#,
            "a column without its submission",
        ),
        (
            &[label, column],
            "sdtm.specs[0].columns.STUDYID.submission",
            r#"{"detail":"STUDYID","field":"submission"}"#,
            "a column without its label and its submission",
        ),
        (
            &[root],
            "sdtm.specs[0]",
            r#"{"detail":"DM","field":"submission"}"#,
            "a specification without its submission",
        ),
    ] {
        let mut edited = dm.clone();
        for written in removed {
            assert_eq!(edited.matches(written).count(), 1, "{case}");
            edited = edited.replacen(written, "", 1);
        }
        study.write("dm.yaml", &edited);
        let submission = engine
            .project(&study.path("project.yaml"))
            .section("sdtm")
            .cancel_with(&token)
            .submission(&SubmissionOptions::new("0.4.0").with_created("2026-09-11T00:00:00"))
            .unwrap();
        let issues: Vec<_> = submission
            .diagnostics()
            .into_iter()
            .map(|issue| {
                (
                    issue.phase.as_str().to_owned(),
                    issue.code.as_str().to_owned(),
                    issue.rule,
                    issue.spec_path,
                    issue.context.to_json().to_owned(),
                    issue.source_span,
                    issue.operand_route,
                )
            })
            .collect();
        assert_eq!(
            issues,
            [(
                "validation".to_owned(),
                "submission_metadata_missing".to_owned(),
                Some("DEFINE-7".to_owned()),
                vec![path.to_owned()],
                context.to_owned(),
                None,
                None,
            )],
            "{case}"
        );
    }
}
