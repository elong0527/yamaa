//! The copy entry (SPEC-9, #3030 D19 and D70): a 1.0 `columns` entry
//! `copy: {dataset: D, columns: [...]}` declares one ordinary column per
//! listed name, reading the same-named column of input `D` and taking the
//! type, label and content metadata of the producer `D` links through
//! `schema`. The entry expands before the layers compose, so `resolve()`
//! shows ordinary columns and a later layer overrides or clears a copied
//! column as it would one written out (P8).
#![cfg(feature = "yaml")]

use arrow_array::{Array, ListArray, StringArray};
use yamaa::{CancellationToken, ContextValue, Engine, EngineOptions, HostLanguage, Table};
use yamaa_core::json::Value;

struct Study(crate::support::Scratch);
impl Study {
    fn new() -> Self {
        let root = crate::support::Scratch::new("copy-entry");
        Self(root)
    }
    fn write(&self, name: &str, contents: &str) {
        std::fs::write(self.0.join(name), contents).unwrap();
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
        "copy-entry-test".into(),
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

/// The condition, rule and spec paths of each issue a check of `spec` finds.
fn checked(study: &Study, spec: &str) -> Vec<(String, Option<String>, Vec<String>)> {
    rows(
        &engine()
            .spec_check(&study.path(spec), None, &CancellationToken::new())
            .unwrap()
            .issues(),
    )
    .into_iter()
    .map(|(condition, rule, paths, _)| (condition, rule, paths))
    .collect()
}

fn issue(condition: &str, rule: &str, path: &str) -> (String, Option<String>, Vec<String>) {
    (condition.into(), Some(rule.into()), vec![path.into()])
}

/// The value at `key` of a resolved tree's map.
fn field<'a>(value: &'a ContextValue, key: &str) -> &'a ContextValue {
    let ContextValue::Map(map) = value else {
        panic!("a map: {value:?}")
    };
    map.get(key)
        .unwrap_or_else(|| panic!("no {key} in {value:?}"))
}

/// The resolved specification of `spec`.
fn resolved(study: &Study, spec: &str) -> ContextValue {
    let artifact = engine().spec_resolve(&study.path(spec), None).unwrap();
    let ContextValue::List(entries) = field(&artifact, "entries") else {
        panic!("entries")
    };
    field(&entries[0], "specification").clone()
}

/// The resolved column `name` of `specification`.
fn column<'a>(specification: &'a ContextValue, name: &str) -> &'a ContextValue {
    let ContextValue::List(columns) = field(specification, "columns") else {
        panic!("columns")
    };
    columns
        .iter()
        .find(|column| field(column, "name") == &ContextValue::Text(name.into()))
        .unwrap_or_else(|| panic!("no column {name}"))
}

fn text(value: &str) -> ContextValue {
    ContextValue::Text(value.into())
}

/// A subject-level producer: `AGE` declares its length, `SEX` its length
/// through a `max_length` verification, and `TRTSDT` an ADaM date's data
/// type and display format.
const ADSL: &str = "yamaa: \"1.0\"
type: spec
model: ADaM
domain: ADSL
keys: [STUDYID, USUBJID]
input: {DM: dm.csv}
output:
  path: adsl.csv
  columns: [STUDYID, USUBJID, AGE, SEX, TRTSDT]
columns:
  - {name: STUDYID, type: str, label: Study Identifier, derivation: DM.STUDYID}
  - {name: USUBJID, type: str, label: Unique Subject Identifier, derivation: DM.USUBJID}
  - name: AGE
    type: int
    label: Age
    derivation: DM.AGE
    submission:
      length: 8
      display_format: '3.'
      origins: [{type: Predecessor, description: DM.AGE}]
  - name: SEX
    type: str
    label: Sex
    derivation: DM.SEX
    verifications: [{max_length: {max: 1}}]
  - name: TRTSDT
    type: date
    label: Date of First Exposure to Treatment
    derivation: DM.RFXSTDTC
    submission:
      data_type: integer
      display_format: DATE9.
      origins: [{type: Predecessor, description: DM.RFXSTDTC}]
";

/// An event-level consumer that copies from the subject-level producer.
fn adae(copy: &str) -> String {
    format!(
        "yamaa: \"1.0\"
type: spec
model: ADaM
domain: ADAE
keys: [STUDYID, USUBJID, AESEQ]
base: AE
input:
  AE: ae.csv
  ADSL: {{path: adsl.csv, schema: adsl.yaml, key: [STUDYID, USUBJID]}}
output:
  path: adae.csv
  columns: [STUDYID, USUBJID, AESEQ, AGE, SEX, TRTSDT]
columns:
  - {{name: STUDYID, type: str, label: Study Identifier, derivation: AE.STUDYID}}
  - {{name: USUBJID, type: str, label: Unique Subject Identifier, derivation: AE.USUBJID}}
  - {{name: AESEQ, type: int, label: Sequence Number, derivation: AE.AESEQ}}
{copy}"
    )
}

const COPY: &str = "  - copy:
      dataset: ADSL
      columns: [AGE, SEX, TRTSDT]
      submission: {core: Cond}
";

fn study() -> Study {
    let study = Study::new();
    study.write(
        "dm.csv",
        "STUDYID,USUBJID,AGE,SEX,RFXSTDTC\nS1,S1-01,34,F,2026-01-05\nS1,S1-02,51,M,2026-01-09\n",
    );
    study.write(
        "ae.csv",
        "STUDYID,USUBJID,AESEQ\nS1,S1-01,1\nS1,S1-01,2\nS1,S1-02,1\n",
    );
    study.write("adsl.yaml", ADSL);
    study.write("adae.yaml", &adae(COPY));
    study
}

#[test]
fn a_copy_entry_declares_each_listed_producer_column() {
    let study = study();
    let engine = engine();
    let token = CancellationToken::new();
    let mut producer = engine
        .spec_run(&study.path("adsl.yaml"), None, &token)
        .unwrap();
    assert_eq!(rows(&producer.issues()), []);
    producer.save().unwrap();
    assert_eq!(checked(&study, "adae.yaml"), []);
    let specification = resolved(&study, "adae.yaml");
    let age = column(&specification, "AGE");
    assert_eq!(field(age, "type"), &text("int"));
    assert_eq!(field(age, "label"), &text("Age"));
    assert_eq!(field(field(age, "derivation"), "value"), &text("ADSL.AGE"));
    let submission = field(age, "submission");
    assert_eq!(field(submission, "core"), &text("Cond"));
    assert_eq!(field(submission, "length"), &ContextValue::Int(8));
    assert_eq!(field(submission, "display_format"), &text("3."));
    let ContextValue::List(origins) = field(submission, "origins") else {
        panic!("origins")
    };
    assert_eq!(origins.len(), 1);
    assert_eq!(field(&origins[0], "type"), &text("Predecessor"));
    assert_eq!(field(&origins[0], "description"), &text("ADSL.AGE"));
    // A length derives from the producer's max_length verification (SUB-8).
    let sex = field(column(&specification, "SEX"), "submission");
    assert_eq!(field(sex, "length"), &ContextValue::Int(1));
    // A date is submitted as the consumer's model requires (SUB-6, SUB-25).
    let ContextValue::Map(date) = field(column(&specification, "TRTSDT"), "submission") else {
        panic!("a date's submission")
    };
    assert_eq!(
        date.keys().map(String::as_str).collect::<Vec<_>>(),
        ["core", "origins"]
    );
    let mut run = engine
        .spec_run(&study.path("adae.yaml"), None, &token)
        .unwrap();
    assert_eq!(rows(&run.issues()), []);
    run.save().unwrap();
    assert_eq!(
        study.read("adae.csv"),
        b"STUDYID,USUBJID,AESEQ,AGE,SEX,TRTSDT\nS1,S1-01,1,34,F,2026-01-05\nS1,S1-01,2,34,F,2026-01-05\nS1,S1-02,1,51,M,2026-01-09\n"
    );
}

/// The copy entry and the columns it declares written out give one resolved
/// specification, under a later layer that overrides one copied column and
/// clears a field of another (P8, LAYER-5, LAYER-9).
#[test]
fn a_later_layer_overrides_and_clears_a_copied_column_as_one_written_out() {
    let study = study();
    let written = "  - name: AGE
    type: int
    label: Age
    derivation: ADSL.AGE
    submission:
      core: Cond
      length: 8
      display_format: '3.'
      origins: [{type: Predecessor, description: ADSL.AGE}]
  - name: SEX
    type: str
    label: Sex
    derivation: ADSL.SEX
    submission:
      core: Cond
      length: 1
      origins: [{type: Predecessor, description: ADSL.SEX}]
  - name: TRTSDT
    type: date
    label: Date of First Exposure to Treatment
    derivation: ADSL.TRTSDT
    submission:
      core: Cond
      origins: [{type: Predecessor, description: ADSL.TRTSDT}]
";
    study.write("written.yaml", &adae(written));
    let child = |parent: &str| {
        format!(
            "yamaa: \"1.0\"
type: spec
parent: {parent}
clear: [columns.SEX.submission.length]
columns:
  - {{name: AGE, label: Age at Onset}}
"
        )
    };
    study.write("copied_child.yaml", &child("adae.yaml"));
    study.write("written_child.yaml", &child("written.yaml"));
    let copied = resolved(&study, "copied_child.yaml");
    assert_eq!(copied, resolved(&study, "written_child.yaml"));
    assert_eq!(
        field(column(&copied, "AGE"), "label"),
        &text("Age at Onset")
    );
    let ContextValue::Map(sex) = field(column(&copied, "SEX"), "submission") else {
        panic!("SEX's submission")
    };
    assert!(!sex.contains_key("length"));
    assert_eq!(
        resolved(&study, "adae.yaml"),
        resolved(&study, "written.yaml")
    );
}

/// A copied column `output.columns` does not list is internal: it carries
/// no submission metadata (SUB-1), and another column may read it; without
/// a model, a published one carries none either.
#[test]
fn an_unpublished_copied_column_is_internal() {
    let study = study();
    study.write(
        "adae.yaml",
        &adae(COPY_INTERNAL).replace(
            "columns: [STUDYID, USUBJID, AESEQ, AGE, SEX, TRTSDT]",
            "columns: [STUDYID, USUBJID, AESEQ, AGE, AGE1]",
        ),
    );
    assert_eq!(checked(&study, "adae.yaml"), []);
    let specification = resolved(&study, "adae.yaml");
    let ContextValue::Map(age) = column(&specification, "AGE") else {
        panic!("AGE")
    };
    assert!(age.contains_key("submission"));
    let ContextValue::Map(trtsdt) = column(&specification, "TRTSDT") else {
        panic!("TRTSDT")
    };
    assert!(!trtsdt.contains_key("submission"));
    // SEX is read by nothing and published nowhere, so it is pruned
    // (LAYER-11).
    let ContextValue::List(columns) = field(&specification, "columns") else {
        panic!("columns")
    };
    assert!(columns
        .iter()
        .all(|column| field(column, "name") != &text("SEX")));
    study.write(
        "adae.yaml",
        &adae("  - copy: {dataset: ADSL, columns: [AGE]}\n")
            .replace("model: ADaM\n", "")
            .replace("AESEQ, AGE, SEX, TRTSDT]", "AESEQ, AGE]"),
    );
    assert_eq!(checked(&study, "adae.yaml"), []);
    let plain = resolved(&study, "adae.yaml");
    let ContextValue::Map(age) = column(&plain, "AGE") else {
        panic!("AGE")
    };
    assert!(!age.contains_key("submission"));
}

const COPY_INTERNAL: &str = "  - copy: {dataset: ADSL, columns: [AGE, SEX, TRTSDT]}
  - name: AGE1
    type: int
    label: Age at Day 1
    derivation:
      case:
        - when: TRTSDT IS NOT NULL
          then: AGE
";

/// A producer that copies in turn is read with its own producer; copies that
/// lead back to a specification being resolved fail.
#[test]
fn a_copy_reads_a_producer_that_copies_in_turn_and_refuses_a_cycle() {
    let study = study();
    study.write(
        "adsl2.yaml",
        "yamaa: \"1.0\"
type: spec
domain: ADSL
keys: [STUDYID, USUBJID]
input: {ADSL: {path: adsl.csv, schema: adsl.yaml}}
output: {path: adsl2.csv, columns: [STUDYID, USUBJID, AGE]}
columns:
  - copy: {dataset: ADSL, columns: [STUDYID, USUBJID, AGE]}
",
    );
    study.write(
        "adae.yaml",
        &adae("  - copy: {dataset: ADSL, columns: [AGE]}\n")
            .replace("schema: adsl.yaml", "schema: adsl2.yaml")
            .replace("AESEQ, AGE, SEX, TRTSDT]", "AESEQ, AGE]"),
    );
    assert_eq!(checked(&study, "adae.yaml"), []);
    let specification = resolved(&study, "adae.yaml");
    assert_eq!(field(column(&specification, "AGE"), "label"), &text("Age"));
    // The producer's producer copies from the consumer.
    study.write(
        "adsl2.yaml",
        &String::from_utf8(study.read("adsl2.yaml"))
            .unwrap()
            .replace("schema: adsl.yaml", "schema: adae.yaml"),
    );
    let issues = rows(
        &engine()
            .spec_check(&study.path("adae.yaml"), None, &CancellationToken::new())
            .unwrap()
            .issues(),
    );
    let [(condition, rule, paths, context)] = issues.as_slice() else {
        panic!("one issue: {issues:?}")
    };
    assert_eq!(condition, "producer_workflow_cycle");
    assert_eq!(rule.as_deref(), Some("SPEC-9"));
    assert_eq!(paths, &["input.ADSL.schema"]);
    assert_eq!(
        context["cycle"],
        Value::Array(
            ["adae.yaml", "adsl2.yaml", "adae.yaml"]
                .map(|name| Value::String(study.path(name)))
                .to_vec()
        )
    );
}

#[test]
fn each_copy_refusal_names_what_it_refuses() {
    let study = study();
    let refused = |copy: &str, edit: &dyn Fn(String) -> String| {
        study.write("case.yaml", &edit(adae(copy)));
        checked(&study, "case.yaml")
    };
    let keep = |spec: String| spec;
    // A name the producer does not output (SPEC-9).
    assert_eq!(
        refused("  - copy: {dataset: ADSL, columns: [AGE, BMI]}\n", &keep),
        [issue(
            "unknown_field",
            "SPEC-9",
            "columns[3].copy.columns[1]"
        )]
    );
    // A dataset that is no input, and an input without a schema link.
    assert_eq!(
        refused("  - copy: {dataset: XX, columns: [AGE]}\n", &keep),
        [issue("unknown_field", "SPEC-9", "columns[3].copy.dataset")]
    );
    assert_eq!(
        refused("  - copy: {dataset: AE, columns: [AGE]}\n", &keep),
        [issue(
            "prohibited_construct",
            "SPEC-9",
            "columns[3].copy.dataset"
        )]
    );
    // A copied name another column of the layer declares (SPEC-2).
    assert_eq!(
        refused("  - copy: {dataset: ADSL, columns: [AESEQ]}\n", &keep),
        [issue(
            "duplicate_identifier",
            "SPEC-9",
            "columns[3].copy.columns[0]"
        )]
    );
    assert_eq!(
        refused(
            "  - copy: {dataset: ADSL, columns: [AGE]}\n  - {name: AGE, type: int, label: Age, derivation: AE.AGE}\n",
            &keep
        ),
        [issue("duplicate_identifier", "SPEC-9", "columns.AGE.name")]
    );
    // A submission field other than core, mandatory and role.
    assert_eq!(
        refused(
            "  - copy: {dataset: ADSL, columns: [AGE], submission: {length: 8}}\n",
            &keep
        ),
        [issue(
            "unknown_field",
            "SCHEMA-13",
            "columns[3].copy.submission.length"
        )]
    );
    // The producer is read as its link is (INPUT-10).
    assert_eq!(
        refused("  - copy: {dataset: ADSL, columns: [AGE]}\n", &|spec| spec
            .replace("schema: adsl.yaml", "schema: missing.yaml")),
        [issue(
            "resource_path_missing",
            "FILE-7",
            "input.ADSL.schema"
        )]
    );
    assert_eq!(
        refused("  - copy: {dataset: ADSL, columns: [AGE]}\n", &|spec| spec
            .replace("schema: adsl.yaml", "schema: case.yaml")),
        [issue(
            "invalid_producer_contract",
            "INPUT-10",
            "input.ADSL.schema"
        )]
    );
    // Copied through an input named otherwise than the producer's domain,
    // the column reads another dataset than its Predecessor names (SUB-20).
    let renamed = |spec: String| {
        spec.replace("  ADSL: {path", "  SL: {path")
            .replace("dataset: ADSL", "dataset: SL")
    };
    assert_eq!(
        refused("  - copy: {dataset: ADSL, columns: [AGE]}\n", &renamed),
        [issue(
            "origin_contradicts_derivation",
            "SUB-19",
            "columns.AGE.submission"
        )]
    );
    // A copied codelist is a project binding (CT-13).
    study.write(
        "adsl.yaml",
        &ADSL.replace(
            "      length: 8\n",
            "      length: 8\n      codelist: AGEU\n",
        ),
    );
    assert_eq!(
        refused("  - copy: {dataset: ADSL, columns: [AGE]}\n", &keep),
        [issue(
            "project_binding_missing",
            "CT-13",
            "columns.AGE.submission.codelist"
        )]
    );
}

/// A copy from an input that does not drive the rows is an implicit join,
/// which needs the input's declared `key` (LOOKUP-5, D111).
#[test]
fn a_copy_from_a_joined_input_needs_its_key() {
    let study = study();
    let engine = engine();
    let token = CancellationToken::new();
    engine
        .spec_run(&study.path("adsl.yaml"), None, &token)
        .unwrap()
        .save()
        .unwrap();
    study.write(
        "adae.yaml",
        &adae("  - copy: {dataset: ADSL, columns: [AGE]}\n")
            .replace(", key: [STUDYID, USUBJID]", "")
            .replace("AESEQ, AGE, SEX, TRTSDT]", "AESEQ, AGE]"),
    );
    let issues = rows(
        &engine
            .spec_run(&study.path("adae.yaml"), None, &token)
            .unwrap()
            .issues(),
    );
    let [(condition, rule, paths, _)] = issues.as_slice() else {
        panic!("one issue: {issues:?}")
    };
    assert_eq!(condition, "no_applicable_keys");
    assert_eq!(rule.as_deref(), Some("LOOKUP-5"));
    assert_eq!(paths, &["columns.AGE.derivation.source"]);
}
