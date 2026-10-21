//! An input whose path ends in `.xml` is read by the ODM source profile
//! (INPUT-1, ODM-1, #3030 D26): a 1.0 specification reads an ODM 1.3 or 2.0
//! snapshot into the clinical-item table that `read_odm` gives, with no host
//! script converting it first. An `odm` read binds the table's eleven fields
//! (NAME-8), an input that no `odm` read names reads every column, and the
//! profile's refusals name the input at `input.<dataset>.path`.
#![cfg(all(feature = "yaml", feature = "odm"))]

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
            "yamaa-odm-input-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&root).unwrap();
        Self(yamaa_adapters::canonical_path::canonicalize(root).unwrap())
    }
    fn write(&self, name: &str, contents: impl AsRef<[u8]>) {
        std::fs::write(self.0.join(name), contents).unwrap();
    }
    fn read(&self, name: &str) -> String {
        String::from_utf8(std::fs::read(self.0.join(name)).unwrap()).unwrap()
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
        "odm-input-test".into(),
        "0".into(),
    ))
}

/// One issue row: condition, rule, spec paths, message and context.
#[derive(Debug)]
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

fn context(json: &str) -> Value {
    yamaa_adapters::json_wire::decode(json).unwrap()
}

/// An EDC snapshot of two subjects. The first subject's sex carries the audit
/// record of its correction, and its item group was updated; the second
/// subject's age is explicitly null.
const SNAPSHOT: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<ODM xmlns="http://www.cdisc.org/ns/odm/v1.3" ODMVersion="1.3.2"
     FileType="Snapshot" FileOID="EXPORT.1">
  <Study OID="S1">
    <MetaDataVersion OID="V1" Name="Version 1">
      <FormDef OID="F.DM" Name="Demographics"/>
      <ItemDef OID="IT.SEX" Name="Sex" DataType="text"/>
    </MetaDataVersion>
  </Study>
  <ClinicalData StudyOID="S1" MetaDataVersionOID="V1">
    <SubjectData SubjectKey="P1">
      <StudyEventData StudyEventOID="SCREENING">
        <FormData FormOID="F.DM">
          <ItemGroupData ItemGroupOID="IG.DM" TransactionType="Update">
            <ItemData ItemOID="IT.SEX" Value="F">
              <AuditRecord>
                <UserRef UserOID="U1"/>
                <LocationRef LocationOID="L1"/>
                <DateTimeStamp>2026-01-05T10:00:00</DateTimeStamp>
                <ReasonForChange>Corrected from the source document</ReasonForChange>
              </AuditRecord>
            </ItemData>
            <ItemData ItemOID="IT.AGE" Value="34"/>
          </ItemGroupData>
        </FormData>
      </StudyEventData>
    </SubjectData>
    <SubjectData SubjectKey="P2">
      <StudyEventData StudyEventOID="SCREENING">
        <FormData FormOID="F.DM">
          <ItemGroupData ItemGroupOID="IG.DM">
            <ItemData ItemOID="IT.SEX" Value="M"/>
            <ItemData ItemOID="IT.AGE" IsNull="Yes"/>
          </ItemGroupData>
        </FormData>
      </StudyEventData>
    </SubjectData>
  </ClinicalData>
</ODM>
"#;

/// One record per subject from `odm` reads of an `.xml` input.
const DEMOGRAPHICS: &str = "yamaa: \"1.0\"\ntype: spec\ndomain: DM\nkeys: [STUDYID, USUBJID]\ninput:\n  ODM: odm.xml\noutput:\n  path: dm.csv\n  columns: [STUDYID, USUBJID, SEX, AGE]\n  report: dm-report.csv\ncolumns:\n  - {name: STUDYID, type: str, label: Study, derivation: ODM.StudyOID}\n  - {name: USUBJID, type: str, label: Subject, derivation: ODM.SubjectKey}\n  - {name: SEX, type: str, label: Sex, derivation: {odm: ODM.IT.SEX}}\n  - {name: AGE, type: int, label: Age, derivation: {odm: ODM.IT.AGE}}\n";

#[test]
fn odm_reads_bind_the_item_table_of_an_xml_input() {
    let study = Study::new();
    study.write("odm.xml", SNAPSHOT);
    study.write("spec.yaml", DEMOGRAPHICS);
    let spec = study.path("spec.yaml");
    let engine = engine();
    let token = CancellationToken::new();
    // Checking reads no input.
    let checked = issues(&engine.spec_check(&spec, None, &token).unwrap().issues());
    assert!(checked.is_empty(), "{checked:?}");
    let mut run = engine.spec_run(&spec, None, &token).unwrap();
    let ran = issues(&run.issues());
    assert!(ran.is_empty(), "{ran:?}");
    run.save(&token).unwrap();
    // The audit record is skipped, an update reads as the present value and
    // an explicit null reads as missing.
    assert_eq!(
        study.read("dm.csv"),
        "STUDYID,USUBJID,SEX,AGE\nS1,P1,F,34\nS1,P2,M,\n"
    );
    // The run report records the profile, the items decoded and the bytes
    // read (CHECK-24).
    let report = study.read("dm-report.csv");
    let input = format!(
        "dm.csv,input,input.ODM,ODM,,,,,,,4,,,\"{{\"\"bytes\"\":{},\"\"path\"\":\"\"odm.xml\"\",\"\"profile\"\":\"\"odm\"\"}}\"\n",
        SNAPSHOT.len()
    );
    assert!(report.contains(&input), "{report}");
}

#[test]
fn an_xml_input_no_odm_read_names_reads_every_column() {
    let study = Study::new();
    study.write("odm.xml", SNAPSHOT);
    // The vendor fields an `odm` read never sees, such as the item group's
    // transaction type, and the integer source ordinal.
    study.write(
        "spec.yaml",
        "yamaa: \"1.0\"\ntype: spec\ndomain: IT\nkeys: [SEQ]\ninput:\n  ODM: odm.xml\noutput:\n  path: items.csv\n  columns: [SEQ, ITEM, VALUE, NAME, TRANSACTION, ISNULL]\ncolumns:\n  - {name: SEQ, type: int, label: Sequence, derivation: ODM.SourceOrdinal}\n  - {name: ITEM, type: str, label: Item, derivation: ODM.ItemOID}\n  - {name: VALUE, type: str, label: Value, derivation: ODM.Value}\n  - {name: NAME, type: str, label: Name, derivation: ODM.ItemName}\n  - {name: TRANSACTION, type: str, label: Transaction, derivation: ODM.TransactionType}\n  - {name: ISNULL, type: str, label: Explicitly null, derivation: ODM.IsNull}\n",
    );
    let mut run = engine()
        .spec_run(&study.path("spec.yaml"), None, &CancellationToken::new())
        .unwrap();
    let ran = issues(&run.issues());
    assert!(ran.is_empty(), "{ran:?}");
    run.save(&CancellationToken::new()).unwrap();
    assert_eq!(
        study.read("items.csv"),
        "SEQ,ITEM,VALUE,NAME,TRANSACTION,ISNULL\n\
         1,IT.SEX,F,Sex,Update,false\n\
         2,IT.AGE,34,,Update,false\n\
         3,IT.SEX,M,Sex,,false\n\
         4,IT.AGE,,,,true\n"
    );
}

#[test]
fn an_xml_input_no_odm_read_names_may_declare_an_ordinal() {
    let study = Study::new();
    study.write("odm.xml", SNAPSHOT);
    let spec = |ordinal: &str| {
        format!("yamaa: \"1.0\"\ntype: spec\ndomain: IT\nkeys: [POSITION]\ninput:\n  ODM: {{path: odm.xml, ordinal: {ordinal}}}\noutput:\n  path: items.csv\n  columns: [POSITION, SEQ]\ncolumns:\n  - {{name: POSITION, type: int, label: Position, derivation: ODM.{ordinal}}}\n  - {{name: SEQ, type: int, label: Sequence, derivation: ODM.SourceOrdinal}}\n")
    };
    // The generated ordinal follows the stored fields and counts the items
    // as `SourceOrdinal` does (INPUT-3).
    study.write("spec.yaml", spec("ITEMPOS"));
    let engine = engine();
    let token = CancellationToken::new();
    let mut run = engine
        .spec_run(&study.path("spec.yaml"), None, &token)
        .unwrap();
    let ran = issues(&run.issues());
    assert!(ran.is_empty(), "{ran:?}");
    run.save(&token).unwrap();
    assert_eq!(
        study.read("items.csv"),
        "POSITION,SEQ\n1,1\n2,2\n3,3\n4,4\n"
    );
    // It must not collide with a stored field (INPUT-4).
    study.write("spec.yaml", spec("SourceOrdinal"));
    let found = issues(
        &engine
            .spec_run(&study.path("spec.yaml"), None, &token)
            .unwrap()
            .issues(),
    );
    let [issue] = found.as_slice() else {
        panic!("one issue: {found:?}")
    };
    assert_eq!(issue.condition, "ordinal_field_collision");
    assert_eq!(issue.paths, ["input.ODM.ordinal"]);
}

/// The one issue of a run of [`DEMOGRAPHICS`] over `odm`.
fn run_issue(odm: &[u8]) -> Issue {
    let study = Study::new();
    study.write("odm.xml", odm);
    study.write("spec.yaml", DEMOGRAPHICS);
    let run = engine()
        .spec_run(&study.path("spec.yaml"), None, &CancellationToken::new())
        .unwrap();
    let mut found = issues(&run.issues());
    assert_eq!(found.len(), 1, "{found:?}");
    found.remove(0)
}

#[test]
fn the_profile_refuses_a_transactional_document_at_the_input() {
    let issue = run_issue(
        SNAPSHOT
            .replace("FileType=\"Snapshot\"", "FileType=\"Transactional\"")
            .as_bytes(),
    );
    assert_eq!(issue.condition, "source_odm_transactional");
    assert_eq!(issue.rule.as_deref(), Some("ODM-1"));
    assert_eq!(issue.paths, ["input.ODM.path"]);
    assert_eq!(
        issue.context,
        context(
            "{\"attribute\":\"FileType\",\"dataset\":\"ODM\",\"element\":\"ODM\",\"path\":\"odm.xml\",\"value\":\"Transactional\"}"
        )
    );
    assert_eq!(
        issue.message,
        "odm.xml is not a snapshot: ODM declares FileType Transactional"
    );
    // A removal on an element read is transactional too.
    let removal = run_issue(
        SNAPSHOT
            .replace("TransactionType=\"Update\"", "TransactionType=\"Remove\"")
            .as_bytes(),
    );
    assert_eq!(removal.condition, "source_odm_transactional");
    assert_eq!(
        removal.context,
        context(
            "{\"attribute\":\"TransactionType\",\"dataset\":\"ODM\",\"element\":\"ItemGroupData\",\"path\":\"odm.xml\",\"value\":\"Remove\"}"
        )
    );
}

#[test]
fn the_profile_refuses_a_document_without_clinical_data_at_the_input() {
    let metadata = SNAPSHOT.split("  <ClinicalData").next().unwrap().to_owned() + "</ODM>\n";
    let issue = run_issue(metadata.as_bytes());
    assert_eq!(issue.condition, "source_odm_no_clinical_data");
    assert_eq!(issue.rule.as_deref(), Some("ODM-1"));
    assert_eq!(issue.paths, ["input.ODM.path"]);
    assert_eq!(
        issue.context,
        context("{\"dataset\":\"ODM\",\"path\":\"odm.xml\"}")
    );
    assert_eq!(issue.message, "odm.xml holds no ClinicalData");
}

#[test]
fn nothing_is_sniffed_so_an_archive_named_xml_is_refused_as_xml() {
    // An empty TAR archive: one zero block, which `read_odm` opens as an
    // archive and finds no XML member in.
    let archive = [0_u8; 512];
    let study = Study::new();
    study.write("odm.xml", archive);
    let Err(yamaa::Error::Rejected(report)) =
        engine().read_odm(&study.path("odm.xml"), &yamaa::OdmOptions::new())
    else {
        panic!("read_odm refuses an archive without an XML member");
    };
    assert_eq!(report.diagnostics[0].code, "source_odm_archive_member");
    // The input's extension selected XML, which the bytes are not.
    let issue = run_issue(&archive);
    assert_eq!(issue.condition, "source_odm_xml_malformed");
    assert_eq!(issue.paths, ["input.ODM.path"]);
    assert_eq!(issue.context["dataset"], Value::String("ODM".into()));
}

#[test]
fn an_xml_input_declares_no_types() {
    let study = Study::new();
    study.write(
        "spec.yaml",
        DEMOGRAPHICS
            .replace("ODM: odm.xml", "ODM: {path: odm.xml, types: {Value: str}}")
            .replace("{odm: ODM.IT.SEX}", "ODM.Value")
            .replace("{odm: ODM.IT.AGE}", "ODM.Value"),
    );
    let path = study.path("spec.yaml");
    let engine = engine();
    let token = CancellationToken::new();
    // No input file exists: the declaration fails first.
    for found in [
        issues(&engine.spec_check(&path, None, &token).unwrap().issues()),
        issues(&engine.spec_run(&path, None, &token).unwrap().issues()),
    ] {
        let [issue] = found.as_slice() else {
            panic!("one issue: {found:?}")
        };
        assert_eq!(issue.condition, "redundant_field_type");
        assert_eq!(issue.rule.as_deref(), Some("INPUT-6"));
        assert_eq!(issue.paths, ["input.ODM.types.Value"]);
        assert_eq!(
            issue.context,
            context("{\"dataset\":\"ODM\",\"field\":\"Value\",\"type\":\"str\"}")
        );
    }
}

#[test]
fn a_0_5_file_keeps_refusing_an_xml_input() {
    let study = Study::new();
    study.write(
        "spec.yaml",
        DEMOGRAPHICS
            .replace("yamaa: \"1.0\"\ntype: spec\n", "schema_version: '1.0'\n")
            .replace("{odm: ODM.IT.SEX}", "ODM.Value")
            .replace("{odm: ODM.IT.AGE}", "ODM.Value"),
    );
    let found = issues(
        &engine()
            .spec_check(&study.path("spec.yaml"), None, &CancellationToken::new())
            .unwrap()
            .issues(),
    );
    let [issue] = found.as_slice() else {
        panic!("one issue: {found:?}")
    };
    assert_eq!(issue.condition, "source_profile_unknown");
    assert_eq!(issue.paths, ["input.ODM.path"]);
}
