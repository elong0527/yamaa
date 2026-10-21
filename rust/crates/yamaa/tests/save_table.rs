//! A table saved to a path its caller names (#3330), as the command line's
//! `yamaa read-odm SOURCE OUT` saves the item table `read_odm` returns: the
//! extension selects the profile from OUTPUT-5's closed mapping, the file
//! holds the bytes a run saves for the same table, it is published through
//! the one save's publisher, and a target that cannot be written is refused
//! as a run's is.
#![cfg(all(feature = "yaml", feature = "odm"))]

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use yamaa::{
    CancellationToken, ContextValue, Diagnostic, Engine, EngineOptions, Failure, MemoryResolver,
    OdmOptions, Status, Table,
};

static NEXT: AtomicUsize = AtomicUsize::new(0);

struct Study(PathBuf);
impl Study {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "yamaa-save-table-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&root).unwrap();
        Self(yamaa_adapters::canonical_path::canonicalize(root).unwrap())
    }
    fn write(&self, name: &str, contents: &str) {
        std::fs::write(self.0.join(name), contents).unwrap();
    }
    fn read(&self, name: &str) -> Vec<u8> {
        std::fs::read(self.0.join(name)).unwrap()
    }
    /// The absolute path of `name`, spelled as a save returns it: with
    /// forward slashes.
    fn path(&self, name: &str) -> String {
        let path = self.0.join(name).to_str().unwrap().to_owned();
        #[cfg(windows)]
        let path = path.replace('\\', "/");
        path
    }
    /// Every entry of the directory, by name in order.
    fn entries(&self) -> Vec<String> {
        let mut names = std::fs::read_dir(&self.0)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        names.sort();
        names
    }
}
impl Drop for Study {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// The command line's engine: the local file resolver, no host language.
fn engine() -> Engine {
    Engine::new(EngineOptions::neutral("save-table-test".into(), "0".into()))
}

fn text(value: &str) -> ContextValue {
    ContextValue::Text(value.into())
}

/// The one issue of `failure`.
fn issue(failure: &Failure) -> &Diagnostic {
    let [issue] = failure.diagnostics.as_slice() else {
        panic!("one issue: {:?}", failure.diagnostics)
    };
    assert_eq!(failure.issues.rows(), 1);
    issue
}

/// Assert `failure` is the engine fault `code` in the output stage, at no
/// spec path, with nothing written.
#[track_caller]
fn assert_output_fault(failure: &Failure, code: &str) {
    let issue = issue(failure);
    assert_eq!(issue.code, "engine_rejected");
    assert_eq!(issue.rule, None);
    assert!(issue.spec_path.is_empty(), "{:?}", issue.spec_path);
    assert_eq!(issue.context.get("stage"), Some(&text("output")));
    assert_eq!(issue.context.get("code"), Some(&text(code)));
    assert!(failure.paths.is_empty());
}

/// A hand-authored ODM 1.3 file: a value the csv profile quotes, a collected
/// empty value and an explicit null, under the names its metadata defines.
const ODM: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<ODM xmlns="http://www.cdisc.org/ns/odm/v1.3" ODMVersion="1.3" FileOID="FILE.1">
  <Study OID="S.1">
    <MetaDataVersion OID="M.1" Name="Metadata">
      <StudyEventDef OID="E.1" Name="Screening"/>
      <FormDef OID="F.1" Name="Comments"/>
      <ItemGroupDef OID="G.1" Name="Comments, general"/>
      <ItemDef OID="I.COMMENT" Name="Comment" DataType="text"/>
      <ItemDef OID="I.EMPTY" Name="Collected empty" DataType="text"/>
      <ItemDef OID="I.NULL" Name="Explicit null" DataType="text"/>
    </MetaDataVersion>
  </Study>
  <ClinicalData StudyOID="S.1" MetaDataVersionOID="M.1">
    <SubjectData SubjectKey="SUBJECT-1">
      <StudyEventData StudyEventOID="E.1" StudyEventRepeatKey="2">
        <FormData FormOID="F.1">
          <ItemGroupData ItemGroupOID="G.1" ItemGroupRepeatKey="7" TransactionType="Insert">
            <ItemData ItemOID="I.COMMENT" Value="said &quot;no&quot;, twice"/>
            <ItemData ItemOID="I.EMPTY" Value=""/>
            <ItemData ItemOID="I.NULL" IsNull="Yes"/>
          </ItemGroupData>
        </FormData>
      </StudyEventData>
    </SubjectData>
  </ClinicalData>
</ODM>
"#;

/// The item table of [`ODM`] in the csv profile, written by hand from the
/// item table's columns and the csv cards: a header of the column names
/// (CSV-2), a field quoted exactly when it holds a quote or a comma or is
/// empty (CSV-3), missing as an empty bare field and the empty string as
/// `""` (CSV-4), an integer as its digits (CSV-5), and a line feed after
/// every record (CSV-1).
const ITEMS_CSV: &str = concat!(
    "ODMVersion,FileOID,StudyOID,MetaDataVersionOID,SubjectKey,StudySubjectID,",
    "SubjectStatus,StudyEventOID,StudyEventRepeatKey,EventName,StartDate,EventStatus,",
    "EventWorkflowStatus,FormOID,FormRepeatKey,FormName,FormLayoutOID,FormStatus,",
    "FormWorkflowStatus,OpenQueries,ItemGroupOID,ItemGroupRepeatKey,ItemGroupName,",
    "TransactionType,ItemOID,ItemName,Value,ValuePresent,IsNull,SourceOrdinal\n",
    "1.3,FILE.1,S.1,M.1,SUBJECT-1,,,E.1,2,Screening,,,,F.1,,Comments,,,,,G.1,7,",
    "\"Comments, general\",Insert,I.COMMENT,Comment,\"said \"\"no\"\", twice\",true,false,1\n",
    "1.3,FILE.1,S.1,M.1,SUBJECT-1,,,E.1,2,Screening,,,,F.1,,Comments,,,,,G.1,7,",
    "\"Comments, general\",Insert,I.EMPTY,Collected empty,\"\",true,false,2\n",
    "1.3,FILE.1,S.1,M.1,SUBJECT-1,,,E.1,2,Screening,,,,F.1,,Comments,,,,,G.1,7,",
    "\"Comments, general\",Insert,I.NULL,Explicit null,,false,true,3\n",
);

/// [`ODM`] written into `study` and read back as its item table.
fn items(study: &Study, engine: &Engine) -> Table {
    study.write("odm.xml", ODM);
    engine
        .read_odm(&study.path("odm.xml"), &OdmOptions::new())
        .unwrap()
}

#[test]
fn a_saved_item_table_holds_the_bytes_the_csv_cards_fix() {
    let study = Study::new();
    let engine = engine();
    let table = items(&study, &engine);
    // The extension selects the profile whatever its case (OUTPUT-5), and the
    // save returns the absolute path it wrote (D38).
    let out = study.path("items.CSV");
    assert_eq!(engine.save_table(&table, &out).unwrap(), [out.as_str()]);
    assert_eq!(
        String::from_utf8(study.read("items.CSV")).unwrap(),
        ITEMS_CSV
    );
    // A save replaces an existing file whole and leaves nothing beside it.
    study.write("items.csv", "an older file\n");
    let out = study.path("items.csv");
    assert_eq!(engine.save_table(&table, &out).unwrap(), [out.as_str()]);
    assert_eq!(
        String::from_utf8(study.read("items.csv")).unwrap(),
        ITEMS_CSV
    );
    assert_eq!(study.entries(), ["items.CSV", "items.csv", "odm.xml"]);
}

/// A local path is the caller's own, as `read_odm` reads its source: its
/// file name need not be a written path (FILE-5), so on Unix it may hold a
/// `:`, a `\` or a character outside ASCII.
#[cfg(unix)]
#[test]
fn a_local_file_name_need_not_be_a_written_path() {
    let study = Study::new();
    let engine = engine();
    let table = items(&study, &engine);
    for name in ["items:2026.csv", "odm\\items.csv", "donn\u{e9}es.csv"] {
        let out = study.path(name);
        assert_eq!(engine.save_table(&table, &out).unwrap(), [out.as_str()]);
        assert_eq!(study.read(name), ITEMS_CSV.as_bytes());
    }
}

/// A 1.0 specification that copies every column of the item table, served
/// as the frame `odm.parquet`, to `output`. The input keeps a stored empty
/// string as the collected empty value the table holds (INPUT-9).
fn copying_spec(output: &str) -> String {
    let mut spec = format!(
        "yamaa: \"1.0\"\ntype: spec\ndomain: ITEMS\nkeys: [SourceOrdinal]\ninput: {{ODM: {{path: odm.parquet, empty_string: present}}}}\noutput:\n  path: {output}\n  columns: [{}]\ncolumns:\n",
        yamaa::ODM_COLUMNS.join(", ")
    );
    for name in yamaa::ODM_COLUMNS {
        let kind = if name == "SourceOrdinal" {
            "int"
        } else {
            "str"
        };
        spec.push_str(&format!(
            "  - {{name: {name}, type: {kind}, label: {name}, derivation: ODM.{name}}}\n"
        ));
    }
    spec
}

/// The bytes of a saved item table are those a run saves for the same table
/// under each profile: a specification that copies every column of the table,
/// read unchanged as an input frame, saves the same file.
#[test]
fn a_saved_item_table_holds_the_bytes_a_run_saves_for_it() {
    let resolver = Arc::new(MemoryResolver::new());
    resolver.insert("/", "odm.xml", "odm:1", Arc::from(ODM.as_bytes()));
    let engine = Engine::new(
        EngineOptions::neutral("save-table-test".into(), "0".into())
            .with_resolver(resolver.clone()),
    );
    let table = engine.read_odm("odm.xml", &OdmOptions::new()).unwrap();
    let frame = yamaa::InputFrame::import(table.stream()).unwrap();
    let extensions: &[&str] = if cfg!(feature = "parquet") {
        &["csv", "parquet"]
    } else {
        &["csv"]
    };
    for extension in extensions {
        let spec = format!("copy-{extension}.yaml");
        let spec_text = copying_spec(&format!("run.{extension}"));
        resolver.insert("/", &spec, "spec:1", Arc::from(spec_text.as_bytes()));
        resolver.insert_frame(&format!("/{spec}"), "odm.parquet", "frame:1", &frame);
        let mut run = engine
            .spec_run(&spec, None, &CancellationToken::new())
            .unwrap();
        assert_eq!(run.status(), Status::Succeeded, "{:?}", run.diagnostics());
        let ran = format!("memory:/run.{extension}");
        assert_eq!(run.save().unwrap(), [ran.as_str()]);
        let saved = format!("memory:/saved.{extension}");
        assert_eq!(
            engine
                .save_table(&table, &format!("saved.{extension}"))
                .unwrap(),
            [saved.as_str()]
        );
        assert_eq!(resolver.published(&saved), resolver.published(&ran));
        assert_eq!(resolver.staged_count(), 0);
    }
    let csv = resolver.published("memory:/saved.csv").unwrap();
    assert_eq!(String::from_utf8(csv.to_vec()).unwrap(), ITEMS_CSV);
}

const RAW: &str = "ID,NAME,SCORE,VISITDT,SEENAT\n1,\"Smith, \"\"Jo\"\"\",2.5,2024-01-15,2024-01-15T08:30:00\n2,,0.1,,\n3,plain,-0.0,2023-12-31,2023-12-31T23:59:59\n";

/// A 1.0 specification over [`RAW`] whose output has a column of each type.
fn typed_spec(output: &str) -> String {
    format!(
        "yamaa: \"1.0\"\ntype: spec\ndomain: TYPED\nkeys: [ID]\ninput:\n  RAW: {{path: raw.csv, types: {{ID: int, SCORE: float, VISITDT: date, SEENAT: datetime}}}}\noutput: {{path: {output}, columns: [ID, NAME, SCORE, RATIO, VISITDT, SEENAT]}}\ncolumns:\n  - {{name: ID, type: int, label: Identifier, derivation: RAW.ID}}\n  - {{name: NAME, type: str, label: Name, derivation: RAW.NAME}}\n  - {{name: SCORE, type: float, label: Score, derivation: RAW.SCORE}}\n  - {{name: RATIO, type: float, label: Ratio, derivation: \"SCORE / 3\"}}\n  - {{name: VISITDT, type: date, label: Visit date, derivation: RAW.VISITDT}}\n  - {{name: SEENAT, type: datetime, label: Seen at, derivation: RAW.SEENAT}}\n"
    )
}

/// A run's output table, saved again, holds the bytes the run saved, for
/// every column type and under each profile.
#[test]
fn a_run_output_saved_as_a_table_holds_the_bytes_the_run_saved() {
    let study = Study::new();
    study.write("raw.csv", RAW);
    let engine = engine();
    let extensions: &[&str] = if cfg!(feature = "parquet") {
        &["csv", "parquet"]
    } else {
        &["csv"]
    };
    for extension in extensions {
        study.write("spec.yaml", &typed_spec(&format!("run.{extension}")));
        let mut run = engine
            .spec_run(&study.path("spec.yaml"), None, &CancellationToken::new())
            .unwrap();
        assert_eq!(run.status(), Status::Succeeded, "{:?}", run.diagnostics());
        run.save().unwrap();
        let copy = format!("copy.{extension}");
        let out = study.path(&copy);
        assert_eq!(
            engine.save_table(run.output().unwrap(), &out).unwrap(),
            [out.as_str()]
        );
        assert_eq!(study.read(&copy), study.read(&format!("run.{extension}")));
    }
}

#[test]
fn an_extension_outside_the_closed_mapping_is_refused_before_anything_is_written() {
    let study = Study::new();
    let engine = engine();
    let table = items(&study, &engine);
    for name in [
        "items.txt",
        "items",
        "items.xpt",
        "items.json",
        ".csv",
        "items.csv.tmp",
    ] {
        let out = study.path(name);
        let failure = engine.save_table(&table, &out).unwrap_err();
        let issue = issue(&failure);
        assert_eq!(issue.code, "unknown_artifact_profile");
        assert_eq!(issue.rule.as_deref(), Some("OUTPUT-5"));
        assert_eq!(issue.phase, "validation");
        assert!(issue.spec_path.is_empty());
        assert_eq!(issue.context.get("path"), Some(&text(&out)));
        assert_eq!(
            issue.context.get("permitted"),
            Some(&ContextValue::List(vec![text(".csv"), text(".parquet")]))
        );
        assert_eq!(
            failure.message,
            format!(
                "cannot save {out}: the extension of {out} is not one of .csv, .parquet \
                 [unknown_artifact_profile, OUTPUT-5]"
            )
        );
        assert!(failure.paths.is_empty());
    }
    assert_eq!(study.entries(), ["odm.xml"]);
}

/// A target that cannot be written is refused as a run's save refuses one
/// (OUTPUT-8, #3030 D99): a missing directory, a directory, a trailing
/// separator and a link name no file the publisher may replace.
#[test]
fn a_target_that_cannot_be_written_is_refused_as_a_run_refuses_it() {
    let study = Study::new();
    let engine = engine();
    let table = items(&study, &engine);
    std::fs::create_dir(study.0.join("folder.csv")).unwrap();
    #[cfg(unix)]
    {
        study.write("kept.csv", "kept\n");
        std::os::unix::fs::symlink("kept.csv", study.0.join("link.csv")).unwrap();
    }
    let link = cfg!(unix).then_some("link.csv");
    for name in ["absent/items.csv", "folder.csv", "items.csv/"]
        .into_iter()
        .chain(link)
    {
        let out = study.path(name);
        let failure = engine.save_table(&table, &out).unwrap_err();
        assert_output_fault(&failure, "invalid_target");
        assert_eq!(
            failure.message,
            format!(
                "cannot save {out}: the engine refused the operation in its output stage: \
                 invalid_target [engine_rejected]"
            )
        );
    }
    #[cfg(unix)]
    {
        assert_eq!(study.read("kept.csv"), b"kept\n");
        assert!(std::fs::symlink_metadata(study.0.join("link.csv"))
            .unwrap()
            .file_type()
            .is_symlink());
        assert_eq!(
            study.entries(),
            ["folder.csv", "kept.csv", "link.csv", "odm.xml"]
        );
    }
    assert!(std::fs::read_dir(study.0.join("folder.csv"))
        .unwrap()
        .next()
        .is_none());
}

/// A file that cannot be staged, in a directory that admits no new entry,
/// stops the save as it stops a run's, naming the path and the attempt, and
/// leaves nothing behind (OUTPUT-8, #3030 D99).
#[cfg(unix)]
#[test]
fn a_file_that_cannot_be_staged_is_refused_and_leaves_nothing() {
    use std::os::unix::fs::PermissionsExt;
    let study = Study::new();
    let engine = engine();
    let table = items(&study, &engine);
    let locked = study.0.join("locked");
    std::fs::create_dir(&locked).unwrap();
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o500)).unwrap();
    let probe = std::fs::write(locked.join("probe"), b"");
    let out = study.path("locked/items.csv");
    let saved = engine.save_table(&table, &out);
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o700)).unwrap();
    if probe.is_ok() {
        // A privileged user is not refused; nothing to observe here.
        return;
    }
    let failure = saved.unwrap_err();
    assert_output_fault(&failure, "publication_failed");
    let issue = issue(&failure);
    assert_eq!(issue.context.get("path"), Some(&text(&out)));
    assert_eq!(issue.context.get("entered"), Some(&ContextValue::Int(1)));
    assert_eq!(issue.context.get("completed"), Some(&ContextValue::Int(0)));
    assert_eq!(std::fs::read_dir(&locked).unwrap().count(), 0);
    // Once the directory admits it, the same call saves.
    assert_eq!(engine.save_table(&table, &out).unwrap(), [out.as_str()]);
    assert_eq!(study.read("locked/items.csv"), ITEMS_CSV.as_bytes());
}

/// An issues table holds list columns, which neither profile writes.
#[test]
fn a_table_neither_profile_writes_is_refused() {
    let study = Study::new();
    let engine = engine();
    let issues = engine
        .project_spec(&study.path("absent.yaml"), "DM")
        .unwrap_err()
        .issues;
    let out = study.path("issues.csv");
    let failure = engine.save_table(&issues, &out).unwrap_err();
    assert_output_fault(&failure, "output_schema");
    assert_eq!(
        issue(&failure).context.get("column"),
        Some(&text("spec_paths"))
    );
    assert!(study.entries().is_empty());
}

/// Through a caller's resolver, a target the resolver refuses is refused as
/// a run's target is.
#[test]
fn a_target_a_resolver_refuses_is_refused_as_a_run_refuses_it() {
    let resolver = Arc::new(MemoryResolver::new());
    resolver.insert("/", "odm.xml", "odm:1", Arc::from(ODM.as_bytes()));
    let engine = Engine::new(
        EngineOptions::neutral("save-table-test".into(), "0".into())
            .with_resolver(resolver.clone()),
    );
    let table = engine.read_odm("odm.xml", &OdmOptions::new()).unwrap();
    resolver.cancel();
    let failure = engine.save_table(&table, "items.csv").unwrap_err();
    assert_output_fault(&failure, "host_resource_failure");
    assert!(resolver.published("memory:/items.csv").is_none());
    assert_eq!(resolver.staged_count(), 0);
}
