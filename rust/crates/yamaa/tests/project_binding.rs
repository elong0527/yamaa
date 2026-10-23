//! A specification checked or run without a project file binds nothing a
//! project supplies (CT-13, #3030 D32 and D85): each codelist, inventory
//! vocabulary and project function a 1.0 specification names fails
//! validation as `project_binding_missing`, before any input is read. With
//! a project file, a binding to an item the project does not declare keeps
//! that item's condition.
#![cfg(feature = "yaml")]

use arrow_array::{Array, ListArray, StringArray};
use yamaa::{CancellationToken, Engine, EngineOptions, HostLanguage, Table};
use yamaa_core::json::Value;

struct Study(crate::support::Scratch);
impl Study {
    fn new() -> Self {
        let root = crate::support::Scratch::new("project-binding");
        Self(root)
    }
    fn write(&self, name: &str, contents: &str) {
        std::fs::write(self.0.join(name), contents).unwrap();
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
        "project-binding-test".into(),
        "0".into(),
    ))
}

/// One issue row: condition, rule, spec paths, message and context.
#[derive(Clone, Debug, PartialEq)]
struct Issue {
    condition: String,
    rule: Option<String>,
    paths: Vec<String>,
    message: String,
    context: Value,
}

fn issues(table: &Table) -> Vec<Issue> {
    let mut issues = Vec::new();
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
        let (condition, rule, message, context) = (
            text("condition"),
            text("rule"),
            text("message"),
            text("context"),
        );
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
            issues.push(Issue {
                condition: condition.value(row).to_owned(),
                rule: (!rule.is_null(row)).then(|| rule.value(row).to_owned()),
                paths: (0..values.len())
                    .map(|i| values.value(i).to_owned())
                    .collect(),
                message: message.value(row).to_owned(),
                context: yamaa_adapters::json_wire::decode(context.value(row)).unwrap(),
            });
        }
    }
    issues
}

/// `project_binding_missing` at `path`, with the context `context` writes
/// as canonical JSON, whose `binding` the message names.
fn missing(path: &str, context: &str) -> Issue {
    let context = yamaa_adapters::json_wire::decode(context).unwrap();
    let binding = context["binding"].as_str().expect("a binding").to_owned();
    Issue {
        condition: "project_binding_missing".into(),
        rule: Some("CT-13".into()),
        paths: vec![path.into()],
        message: format!("the {binding} at {path} needs a project file, and none is given"),
        context,
    }
}

/// A 1.0 ADaM specification whose `PARAMCD` binds the codelist `PARAMCD`,
/// whose `AVALU` binds `GLUCU` on its glucose rows, and whose `SRCDOM` binds
/// the dataset inventory on its glucose rows, which `inventory` turns on or
/// off; with `call`, its published `BMI` calls the project function `bmi`.
fn spec(inventory: bool, call: bool) -> String {
    let (published, column) = if call {
        (
            ", BMI",
            "  - name: BMI\n    type: float\n    label: Body Mass Index\n    derivation:\n      function: {name: bmi, args: {weight_kg: RAW.WEIGHT}}\n",
        )
    } else {
        ("", "")
    };
    format!(
        "yamaa: \"1.0\"
type: spec
model: ADaM
domain: ADLB
keys: [USUBJID, PARAMCD]
input: {{RAW: raw.csv}}
output: {{path: adlb.csv, columns: [USUBJID, PARAMCD, AVALU, SRCDOM{published}]}}
columns:
  - {{name: USUBJID, type: str, label: Subject, derivation: RAW.USUBJID}}
  - name: PARAMCD
    type: str
    label: Parameter Code
    derivation: RAW.PARAMCD
    submission:
      core: Req
      length: 8
      origins: [{{type: Assigned}}]
      codelist: PARAMCD
  - name: AVALU
    type: str
    label: Analysis Value Unit
    derivation: RAW.AVALU
    submission:
      core: Perm
      length: 8
      origins: [{{type: Assigned}}]
      value_level:
        - {{id: GLUC, where: {{PARAMCD: [GLUC]}}, codelist: GLUCU}}
  - name: SRCDOM
    type: str
    label: Source Data
    derivation: RAW.SRCDOM
    submission:
      core: Perm
      length: 4
      origins: [{{type: Assigned}}]
      value_level:
        - {{id: GLUC, where: {{PARAMCD: [GLUC]}}, inventory_vocabulary: {inventory}}}
{column}"
    )
}

/// The issues of a check and of a run of `spec` without a project file, as
/// `engine` gives them, after asserting that the run built nothing.
fn without_project(engine: &Engine, study: &Study, spec: &str) -> [Vec<Issue>; 2] {
    study.write("spec.yaml", spec);
    let path = study.path("spec.yaml");
    let token = CancellationToken::new();
    let checked = issues(&engine.spec_check(&path, None, &token).unwrap().issues());
    let run = engine.spec_run(&path, None, &token).unwrap();
    assert!(run.output().is_none());
    [checked, issues(&run.issues())]
}

#[test]
fn each_binding_of_a_specification_without_its_project_fails_before_any_input_is_read() {
    // No input file exists: every binding fails first.
    let study = Study::new();
    let codelists = [
        missing(
            "columns.PARAMCD.submission.codelist",
            r#"{"binding":"codelist","codelist":"PARAMCD","column":"PARAMCD"}"#,
        ),
        missing(
            "columns.AVALU.submission.value_level.GLUC.codelist",
            r#"{"binding":"codelist","codelist":"GLUCU","column":"AVALU"}"#,
        ),
    ];
    let inventory = missing(
        "columns.SRCDOM.submission.value_level.GLUC.inventory_vocabulary",
        r#"{"binding":"inventory_vocabulary","column":"SRCDOM"}"#,
    );
    let call = missing(
        "columns.BMI.derivation.function.name",
        r#"{"binding":"function","function":"bmi"}"#,
    );
    // Each project function first, with the other findings of the calls;
    // then each column's terminology, in column order.
    let expected = [vec![call], codelists.into(), vec![inventory]].concat();
    for found in without_project(&engine(), &study, &spec(true, true)) {
        assert_eq!(found, expected);
    }
    // An engine without project functions, such as the command line's,
    // reports the call the same way (#3030 D110, D123).
    let neutral = Engine::new(EngineOptions::neutral("cli".into(), "0".into()));
    for found in without_project(&neutral, &study, &spec(true, true)) {
        assert_eq!(found, expected);
    }
    // An entry that turns the inventory off binds nothing.
    for found in without_project(&engine(), &study, &spec(false, false)) {
        assert_eq!(found, expected[1..3]);
    }
    // A column-level inventory vocabulary is a binding at the column.
    let column = spec(false, false).replace(
        "      length: 4\n",
        "      length: 4\n      inventory_vocabulary: true\n",
    );
    for found in without_project(&engine(), &study, &column) {
        assert_eq!(
            found,
            [
                &expected[1..3],
                &[missing(
                    "columns.SRCDOM.submission.inventory_vocabulary",
                    r#"{"binding":"inventory_vocabulary","column":"SRCDOM"}"#,
                )],
            ]
            .concat()
        );
    }
}

/// A 1.0 project whose `adam` section lists `spec.yaml` and that declares
/// the codelists `codelists` writes.
fn project(codelists: &str) -> String {
    format!(
        "yamaa: \"1.0\"\ntype: project\ncodelists:\n  - codelists:\n{codelists}adam:\n  specs: [spec.yaml]\n"
    )
}

const PARAMCD: &str = "      - {id: PARAMCD, name: Parameter Code, data_type: text, items: [{value: GLUC}, {value: CREAT}]}\n";
const GLUCU: &str =
    "      - {id: GLUCU, name: Glucose Unit, data_type: text, items: [{value: mg/dL}]}\n";

#[test]
fn with_its_project_a_binding_names_what_the_project_declares() {
    let study = Study::new();
    study.write("spec.yaml", &spec(true, false));
    let (path, project_path) = (study.path("spec.yaml"), study.path("project.yaml"));
    let check = |codelists: &str| {
        study.write("project.yaml", &project(codelists));
        issues(
            &engine()
                .spec_check(&path, Some(&project_path), &CancellationToken::new())
                .unwrap()
                .issues(),
        )
    };
    // The project declares every codelist and lists the specification, so
    // its inventory exists: nothing fails before data.
    assert_eq!(check(&format!("{PARAMCD}{GLUCU}")), []);
    // A codelist the project does not declare keeps CT-1's condition.
    let found = check(PARAMCD);
    let [issue] = found.as_slice() else {
        panic!("one issue: {found:?}")
    };
    assert_eq!(issue.condition, "unknown_codelist");
    assert_eq!(issue.rule.as_deref(), Some("CT-1"));
    assert_eq!(
        issue.paths,
        ["columns.AVALU.submission.value_level.GLUC.codelist"]
    );
}

#[test]
fn a_where_value_is_never_left_unchecked_against_its_codelist() {
    // `OTHER` is no item of `PARAMCD`, the where column's codelist (SUB-12).
    let study = Study::new();
    let spec = spec(false, false).replace(
        "        - {id: GLUC, where: {PARAMCD: [GLUC]}, codelist: GLUCU}\n",
        "        - {id: GLUC, where: {PARAMCD: [GLUC]}, codelist: GLUCU}\n        - {id: OTHER, where: {PARAMCD: [OTHER]}, codelist: GLUCU}\n",
    );
    study.write("spec.yaml", &spec);
    study.write("project.yaml", &project(&format!("{PARAMCD}{GLUCU}")));
    let found = issues(
        &engine()
            .spec_check(
                &study.path("spec.yaml"),
                Some(&study.path("project.yaml")),
                &CancellationToken::new(),
            )
            .unwrap()
            .issues(),
    );
    let [issue] = found.as_slice() else {
        panic!("one issue: {found:?}")
    };
    assert_eq!(issue.condition, "value_level_where_invalid");
    assert_eq!(issue.rule.as_deref(), Some("SUB-12"));
    assert_eq!(
        issue.paths,
        ["columns.AVALU.submission.value_level.OTHER.where.PARAMCD"]
    );
    assert_eq!(
        issue.context["reason"],
        Value::String("not_in_codelist".into())
    );
    // Without the project, the where column's codelist is not known, and
    // the specification fails at that binding instead of passing unchecked.
    for found in without_project(&engine(), &study, &spec) {
        assert_eq!(
            found.first(),
            Some(&missing(
                "columns.PARAMCD.submission.codelist",
                r#"{"binding":"codelist","codelist":"PARAMCD","column":"PARAMCD"}"#,
            ))
        );
        assert!(found
            .iter()
            .all(|issue| issue.condition == "project_binding_missing"));
    }
}
