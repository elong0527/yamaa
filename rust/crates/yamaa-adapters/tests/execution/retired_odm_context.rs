//! Original retired ODM-context operands through the shared public file engine.
//! Generic source reads keep their ordinary relation semantics; only `odm`
//! expressions select the fixed eleven-field ODM interpretation.
use serde_json::{json, Value};
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};
use yamaa_adapters::{file_application, issue_rows::Issue, specification_report::Identity};
use yamaa_core::domain_entry::Request;

struct Fixture(PathBuf);
impl Fixture {
    fn new(specification: &str, input: &str) -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let root = std::env::temp_dir().join(format!(
            "yamaa-retired-odm-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        fs::write(root.join("spec.yaml"), specification).unwrap();
        fs::write(root.join("odm.csv"), input).unwrap();
        Self(root)
    }
    fn run(&self) -> Result<file_application::Domain, Vec<Issue>> {
        let specification = self.0.join("spec.yaml");
        let specification = specification.to_str().unwrap();
        file_application::domain(
            Request {
                specification,
                environment: None,
            },
            Identity {
                runtime: "test",
                runtime_version: "test",
                engine_version: yamaa_core::VERSION,
                example: "retired-odm-context",
                specification,
                base_directory: ".",
            },
        )
    }
    fn saved(&self, expected: &str) -> Value {
        let mut domain = self.run().unwrap();
        assert!(domain.issues().is_empty(), "{:?}", domain.issues());
        let report = domain.observations();
        assert!(domain.save().unwrap());
        assert_eq!(
            fs::read(self.0.join("out.csv")).unwrap(),
            expected.as_bytes()
        );
        report
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

fn specification(key: &str, kind: &str, expression: &str, input: &str) -> String {
    format!(
        "schema_version: '1.0'\ndomain: OUT\nkeys: [{key}]\ninput: {{ODM: {input}}}\noutput: {{path: out.csv, columns: [{key}, VALUE]}}\ncolumns:\n  - {{name: {key}, type: str, derivation: ODM.{key}}}\n  - {{name: VALUE, type: {kind}, derivation: {expression}}}\n"
    )
}
fn issue_rows(issues: &[Issue]) -> Value {
    Value::Array(
        issues
            .iter()
            .map(|issue| {
                let mut value = issue.as_value();
                value["context"] = serde_json::from_str(&issue.context).unwrap();
                value
            })
            .collect(),
    )
}

const ITEMS: &str = "StudyOID,SubjectKey,ItemOID,Value\nS1,001,IT.DM.SEX,Male\nS1,001,IT.DM.AGE,34\nS1,001,IT.DM.ARM,Placebo\n";

#[test]
fn original_filters_mapping_and_first_available_read_their_own_collected_records() {
    for (expression, expected) in [
        ("{source: {variable: ODM.Value, filter: \"ODM.ItemOID = 'IT.DM.AGE'\"}}", "34"),
        ("{mapping: {source: {variable: ODM.Value, filter: \"ODM.ItemOID = 'IT.DM.SEX'\"}, dict: {Male: M, Female: F}, missing: U}}", "M"),
        ("{first_available: {sources: [{variable: ODM.Value, filter: \"ODM.ItemOID = 'IT.DM.ARM'\"}], missing: Unassigned}}", "Placebo"),
        ("{first_available: {sources: [{variable: ODM.Value, filter: \"ODM.ItemOID = 'IT.DM.RACE'\"}], missing: Unassigned}}", "Unassigned"),
    ] {
        let fixture = Fixture::new(&specification("SubjectKey", "str", expression, "odm.csv"), ITEMS);
        fixture.saved(&format!("SubjectKey,VALUE\n001,{expected}\n"));
    }
    let fixture = Fixture::new(
        &specification("SubjectKey", "str", "{source: {variable: ODM.Value, filter: \"ODM.ItemOID = 'IT.DM.RACE'\", absent: fallback}}", "odm.csv"),
        ITEMS,
    );
    let report = fixture.saved("SubjectKey,VALUE\n001,\n");
    assert_eq!(
        report["handler_counts"],
        json!([{
            "spec_path":"columns.VALUE.derivation.source.absent", "handler":"absent", "count":0
        }])
    );
}

#[test]
fn original_collected_conflicts_count_values_after_the_source_filter() {
    for (expression, input, count) in [
        ("ODM.Value", ITEMS, 3),
        (
            "{source: {variable: ODM.Value, filter: \"ODM.ItemOID = 'IT.DM.SEX'\"}}",
            "StudyOID,SubjectKey,ItemOID,Value\nS1,001,IT.DM.SEX,Male\nS1,001,IT.DM.SEX,Female\n",
            2,
        ),
    ] {
        let fixture = Fixture::new(
            &specification("SubjectKey", "str", expression, "odm.csv"),
            input,
        );
        let mut domain = fixture.run().unwrap();
        assert_eq!(
            issue_rows(&domain.issues()),
            json!([{
                "phase":"derivation", "condition":"multiple_values_per_key", "requirement":"REQ-0075",
                "spec_paths":["columns.VALUE.derivation.source"],
                "context":{"identifier":"ODM.Value", "value_count":count, "keys":[{"SubjectKey":"001"}]}
            }])
        );
        assert!(domain.output().is_none());
        assert!(domain.save().is_err());
        assert!(!fixture.0.join("out.csv").exists());
    }
}

#[test]
fn original_selected_missing_values_never_fall_back_to_unselected_records() {
    let expression = "{source: {variable: ODM.Value, filter: \"ODM.RecordKind = 'TARGET'\"}}";
    for (rows, expected) in [
        ("G1,UNRELATED,unrelated\nG1,TARGET,\n", ""),
        ("G1,UNRELATED,unrelated\nG1,TARGET,\nG1,TARGET,\n", ""),
        (
            "G1,UNRELATED,unrelated\nG1,TARGET,\nG1,TARGET,target\n",
            "target",
        ),
        ("G1,OTHER,other\nG1,TARGET,\nG1,UNRELATED,x\n", ""),
        ("G1,UNRELATED,x\nG1,TARGET,\nG1,OTHER,other\n", ""),
        ("G1,TARGET,\nG1,UNRELATED,x\nG1,OTHER,other\n", ""),
    ] {
        let fixture = Fixture::new(
            &specification("GroupID", "str", expression, "odm.csv"),
            &format!("GroupID,RecordKind,Value\n{rows}"),
        );
        fixture.saved(&format!("GroupID,VALUE\nG1,{expected}\n"));
    }
}

#[test]
fn original_source_filter_cannot_read_another_relation() {
    let fixture = Fixture::new(
        &specification(
            "SubjectKey",
            "str",
            "{source: {variable: ODM.Value, filter: \"OTHER.ItemOID = 'IT.DM.AGE'\"}}",
            "odm.csv",
        ),
        ITEMS,
    );
    let issues = match fixture.run() {
        Ok(domain) => {
            assert!(domain.output().is_none());
            domain.issues().to_vec()
        }
        Err(issues) => issues,
    };
    assert_eq!(
        issue_rows(&issues),
        json!([{
            "phase":"validation", "condition":"unknown_field", "requirement":"REQ-0132",
            "spec_paths":["columns.VALUE.derivation.source.filter"],
            "context":{"identifier":"OTHER.ItemOID", "dataset":"ODM"}
        }])
    );
    assert!(!fixture.0.join("out.csv").exists());
}

#[test]
fn original_repeated_dates_are_one_value_and_distinct_dates_keep_the_exact_count() {
    let specification = specification(
        "SubjectKey",
        "date",
        "ODM.VISITDT",
        "{path: odm.csv, types: {VISITDT: date}}",
    );
    let first = "StudyOID,SubjectKey,VISITDT,ItemOID,Value\nS1,001,2025-01-02,IT.A,a\nS1,001,2025-01-02,IT.B,b\n";
    let fixture = Fixture::new(&specification, first);
    fixture.saved("SubjectKey,VALUE\n001,2025-01-02\n");
    let fixture = Fixture::new(
        &specification,
        &first.replace("2025-01-02,IT.B", "2025-01-03,IT.B"),
    );
    let domain = fixture.run().unwrap();
    assert_eq!(
        issue_rows(&domain.issues()),
        json!([{
            "phase":"derivation", "condition":"multiple_values_per_key", "requirement":"REQ-0075",
            "spec_paths":["columns.VALUE.derivation.source"],
            "context":{"identifier":"ODM.VISITDT", "value_count":2, "keys":[{"SubjectKey":"001"}]}
        }])
    );
}

const HEADER: &str = "StudyOID,MetaDataVersionOID,SubjectKey,StudyEventOID,StudyEventRepeatKey,FormOID,FormRepeatKey,ItemGroupOID,ItemGroupRepeatKey,ItemOID,Value";
const RECORD: &str = "S1,M1,001,SCREENING,1,FO.DM,1,IG.DM,1,IT.DM.AGE,34";

#[test]
fn original_binding_fixture_keeps_output_names_literal_fields_and_complete_source_count() {
    use yamaa_adapters::{
        shipped_schema, specification_run::PreparedRun, specification_source::Source,
    };
    use yamaa_core::{
        reference_binding::{Binding, Catalog, Dataset, Field},
        specification::BindError,
        table::{Column, TableSchema},
        value::ColumnType,
    };
    let original = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../benchmarks/schema-odm-form-items/spec.yaml");
    let document = shipped_schema::capture()
        .unwrap()
        .prepare_standalone(Source {
            identity: "spec.yaml".into(),
            bytes: fs::read(original).unwrap(),
        })
        .unwrap();
    let run = PreparedRun::prepare(document).unwrap();
    let compiled = run.compiled();
    let source = TableSchema::new(
        HEADER
            .split(',')
            .map(|name| Column {
                name: name.into(),
                kind: ColumnType::Str,
            })
            .collect(),
    )
    .unwrap();
    assert!(matches!(
        compiled.bind_sources(&[]),
        Err(BindError::SourceCount)
    ));
    assert!(matches!(
        compiled.bind_sources(&[&source, &source]),
        Err(BindError::SourceCount)
    ));
    compiled.bind_sources(&[&source]).unwrap();
    let outputs = compiled
        .output()
        .columns()
        .iter()
        .map(|column| Field {
            name: &column.name,
            column_type: column.kind,
        })
        .collect::<Vec<_>>();
    let fields = source
        .columns()
        .iter()
        .map(|column| Field {
            name: &column.name,
            column_type: column.kind,
        })
        .collect::<Vec<_>>();
    let catalog = Catalog::compile(
        &outputs,
        &[Dataset {
            name: "ODM",
            fields: &fields,
        }],
        Default::default(),
    )
    .unwrap();
    assert_eq!(
        catalog.bind("LBDTC").unwrap(),
        Some(Binding::Output {
            column: 7,
            column_type: ColumnType::Date
        })
    );
    assert_eq!(
        catalog.bind("ODM.StudyOID").unwrap(),
        Some(Binding::Dataset {
            dataset: 0,
            field: 0,
            column_type: ColumnType::Str
        })
    );
    for name in ["UNKNOWN", "OTHER.Value", "ODM.IT.LB.LBDTC", "ODM.AGE"] {
        assert_eq!(catalog.bind(name).unwrap(), None, "{name}");
    }
}

#[test]
fn original_fixed_odm_csv_canonicalizes_only_schema_names_and_ignores_vendor_fields() {
    let fixture = Fixture::new(
        &specification("SubjectKey", "str", "{odm: ODM.IT.DM.AGE}", "odm.csv"),
        &format!("{},SITEID\n{RECORD},SITE-9\n", HEADER.to_ascii_uppercase()),
    );
    let report = fixture.saved("SubjectKey,VALUE\n001,34\n");
    let source = &report["tables"][0];
    assert_eq!(
        source["columns"],
        json!(HEADER.split(',').collect::<Vec<_>>())
    );
    assert_eq!(source["types"], json!(vec!["str"; 11]));
    assert_eq!(
        source["rows"][0],
        json!(RECORD
            .split(',')
            .map(|value| json!({"type":"str", "value":value}))
            .collect::<Vec<_>>())
    );
}

#[test]
fn original_missing_identifier_reports_first_csv_record_and_complete_failure_count() {
    let missing = RECORD.replacen("001", "", 1);
    let fixture = Fixture::new(
        &specification("SubjectKey", "str", "{odm: ODM.IT.DM.AGE}", "odm.csv"),
        &format!("{HEADER}\n{RECORD}\n{missing}\n{missing}\n"),
    );
    let issues = match fixture.run() {
        Ok(domain) => {
            assert!(domain.output().is_none());
            domain.issues().to_vec()
        }
        Err(issues) => issues,
    };
    assert_eq!(
        issue_rows(&issues),
        json!([{
            "phase":"ingest", "condition":"odm_schema_value_missing", "requirement":"REQ-1276",
            "spec_paths":["input.ODM.path"],
            "context":{"dataset":"ODM", "field":"SubjectKey", "record":3, "records":2}
        }])
    );
    assert!(!fixture.0.join("out.csv").exists());
}
