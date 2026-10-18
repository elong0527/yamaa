//! Project operations through the public engine: names, project checks, run
//! scope and order, saved producers, failures, saves and submissions.
#![cfg(feature = "yaml")]

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use arrow_array::{Array, ListArray, StringArray};
use yamaa::{
    Argument, CallError, CancellationToken, Engine, EngineOptions, FunctionError, FunctionHandle,
    FunctionIdentity, HostLanguage, LockContext, ProjectFunctions, Scalar, Signature, Table,
};
use yamaa_core::json::Value;

struct NoFunctions;
impl ProjectFunctions for NoFunctions {
    fn verify_lock(
        &self,
        _: &LockContext<'_>,
        _: &[FunctionIdentity],
    ) -> Result<(), FunctionError> {
        Ok(())
    }
    fn bind(&self, _: &FunctionIdentity, _: &Signature) -> Result<FunctionHandle, FunctionError> {
        unreachable!("these specifications call no project function")
    }
    fn invoke(&self, _: &FunctionHandle, _: &[Argument<'_>]) -> Result<Scalar, CallError> {
        unreachable!("these specifications call no project function")
    }
}

fn engine() -> Engine {
    Engine::new(
        EngineOptions::new(HostLanguage::Python, "project-test".into(), "0".into())
            .with_functions(Arc::new(NoFunctions)),
    )
}

static NEXT: AtomicUsize = AtomicUsize::new(0);

struct Study(PathBuf);
impl Study {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "yamaa-project-operations-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&root).unwrap();
        // The engine's spelling: no Windows verbatim prefix.
        Self(yamaa_adapters::canonical_path::canonicalize(root).unwrap())
    }
    fn write(&self, name: &str, contents: impl AsRef<[u8]>) {
        let path = self.0.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, contents).unwrap();
    }
    /// A file's identity as the engine reports it, with forward slashes.
    fn path(&self, name: &str) -> String {
        let path = self.0.join(name).to_str().unwrap().to_owned();
        #[cfg(windows)]
        let path = path.replace('\\', "/");
        path
    }
    fn read(&self, name: &str) -> Option<String> {
        std::fs::read_to_string(self.0.join(name)).ok()
    }
    fn exists(&self, name: &str) -> bool {
        self.0.join(name).exists()
    }
    fn project(&self) -> String {
        self.path("project.yaml")
    }
}
impl Drop for Study {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

const DM: &str = "schema_version: '1.0'\ndomain: DM\nkeys: [USUBJID]\ninput: {RAW: {path: ../raw/dm.csv, types: {AGE: int}}}\ncolumns:\n  - {name: USUBJID, type: str, label: Subject, derivation: RAW.USUBJID}\n  - {name: AGE, type: int, label: Age, derivation: RAW.AGE}\noutput: {path: ../out/dm.csv, columns: [USUBJID, AGE], report: ../out/dm-report.csv}\n";
const AE: &str = "schema_version: '1.0'\ndomain: AE\nkeys: [USUBJID, AESEQ]\ninput: {RAW: {path: ../raw/ae.csv, types: {AESEQ: int}}}\ncolumns:\n  - {name: USUBJID, type: str, label: Subject, derivation: RAW.USUBJID}\n  - {name: AESEQ, type: int, label: Sequence, derivation: RAW.AESEQ}\noutput: {path: ../out/ae.csv, columns: [USUBJID, AESEQ]}\n";
const ADSL: &str = "schema_version: '1.0'\ndomain: ADSL\nkeys: [USUBJID]\ninput: {DM: {path: ../out/dm.csv, schema: ../sdtm/dm.yaml}}\ncolumns:\n  - {name: USUBJID, type: str, label: Subject, derivation: DM.USUBJID}\n  - {name: AGE, type: int, label: Age, derivation: DM.AGE}\noutput: {path: ../out/adsl.csv, columns: [USUBJID, AGE], report: ../out/adsl-report.csv}\n";
const PROJECT: &str =
    "schema_version: '1.0'\nsdtm:\n  specs: [sdtm/dm.yaml, sdtm/ae.yaml]\nadam:\n  specs: [adam/adsl.yaml]\n";
const DM_CSV: &str = "USUBJID,AGE\nS1,30\nS2,41\n";

/// DM and AE in SDTM, ADSL in ADaM reading DM through its `schema` link.
fn study() -> Study {
    let study = Study::new();
    study.write("raw/dm.csv", "USUBJID,AGE\nS1,30\nS2,41\n");
    study.write("raw/ae.csv", "USUBJID,AESEQ\nS1,1\nS2,1\n");
    study.write("sdtm/dm.yaml", DM);
    study.write("sdtm/ae.yaml", AE);
    study.write("adam/adsl.yaml", ADSL);
    study.write("project.yaml", PROJECT);
    std::fs::create_dir_all(study.0.join("out")).unwrap();
    study
}

#[derive(Debug)]
struct Row {
    condition: String,
    requirement: Option<String>,
    spec_paths: Vec<String>,
    context: Value,
}

fn rows(table: &Table) -> Vec<Row> {
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
            rows.push(Row {
                condition: condition.value(row).to_owned(),
                requirement: (!requirement.is_null(row)).then(|| requirement.value(row).to_owned()),
                spec_paths: (0..values.len())
                    .map(|i| values.value(i).to_owned())
                    .collect(),
                context: yamaa_adapters::json_wire::decode(context.value(row)).unwrap(),
            });
        }
    }
    rows
}

fn conditions(table: &Table) -> Vec<String> {
    rows(table).into_iter().map(|row| row.condition).collect()
}

/// The `ARTIFACT` cell of every run report row.
fn artifacts(table: &Table) -> Vec<String> {
    let mut cells = Vec::new();
    for batch in table.batches() {
        let column = batch
            .column_by_name("ARTIFACT")
            .unwrap()
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap();
        cells.extend((0..column.len()).map(|row| column.value(row).to_owned()));
    }
    cells
}

fn text(value: &str) -> Value {
    Value::String(value.into())
}

#[test]
fn names_select_listed_specifications_and_refuse_unknown_or_shared_names() {
    let study = study();
    let engine = engine();
    let path = engine.project_spec(&study.project(), "ADSL").unwrap();
    assert_eq!(Path::new(&path), study.0.join("adam/adsl.yaml"));
    let unknown = engine.project_spec(&study.project(), "ADXX").unwrap_err();
    assert!(
        unknown.message.contains("available: ADSL, AE, DM"),
        "{}",
        unknown.message
    );
    let issues = rows(&unknown.issues);
    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0].condition, "unknown_specification_name");
    assert_eq!(issues[0].requirement.as_deref(), Some("REQ-1339"));
    assert_eq!(issues[0].context["name"], text("ADXX"));
    assert_eq!(
        issues[0].context["available"],
        Value::Array(vec![text("ADSL"), text("AE"), text("DM")])
    );

    // A second DM, listed in ADaM, makes the name ambiguous everywhere.
    study.write("adam/dm.yaml", DM.replace("../out/dm", "../out/dm-copy"));
    study.write(
        "project.yaml",
        PROJECT.replace("[adam/adsl.yaml]", "[adam/adsl.yaml, adam/dm.yaml]"),
    );
    let token = CancellationToken::new();
    let duplicate = engine.project_spec(&study.project(), "AE").unwrap_err();
    let issues = rows(&duplicate.issues);
    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0].condition, "duplicate_specification_name");
    assert_eq!(issues[0].requirement.as_deref(), Some("REQ-1338"));
    assert_eq!(issues[0].spec_paths, ["sdtm.specs[0]", "adam.specs[1]"]);
    assert_eq!(issues[0].context["name"], text("DM"));
    let check = conditions(&engine.project_check(&study.project(), &token));
    assert!(check.iter().any(|c| c == "duplicate_specification_name"));
    let mut run = engine.project_run(&study.project(), None, &token).unwrap();
    assert!(run.names().is_empty());
    assert_eq!(conditions(&run.issues()), ["duplicate_specification_name"]);
    assert!(run.save().is_err());
    let mut submission = engine
        .project_submission(&study.project(), None, None, "0.4.0", &token)
        .unwrap();
    assert_eq!(
        conditions(&submission.issues()),
        ["duplicate_specification_name"]
    );
    assert!(submission.save().is_err());
    assert!(!study.exists("out/dm.csv"));
}

/// The caller's spelling of its project file keeps the authority of the
/// directory it names, as an entry path's does, though the engine names each
/// listed specification by its canonical identity. Here the spelling passes
/// through a linked directory, as every macOS temporary path passes through
/// `/var`, a link to `/private/var`.
#[cfg(unix)]
#[test]
fn a_project_spelled_through_a_linked_directory_keeps_its_authority() {
    struct Link(PathBuf);
    impl Drop for Link {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }
    let study = study();
    let mut name = study.0.file_name().unwrap().to_os_string();
    name.push("-link");
    let link = Link(study.0.with_file_name(name));
    std::os::unix::fs::symlink(&study.0, &link.0).unwrap();
    let project = link.0.join("project.yaml").to_str().unwrap().to_owned();
    let engine = engine();
    let token = CancellationToken::new();
    let check = engine.project_check(&project, &token);
    assert_eq!(check.rows(), 0, "{:?}", rows(&check));
    let run = engine.project_run(&project, None, &token).unwrap();
    assert_eq!(run.names(), ["DM", "AE", "ADSL"]);
    assert_eq!(run.issues().rows(), 0, "{:?}", rows(&run.issues()));
    // A selected specification is checked and run as the caller named it.
    let path = engine.project_spec(&project, "DM").unwrap();
    assert_eq!(path, study.path("sdtm/dm.yaml"));
    let check = engine.spec_check(&path, Some(&project), &token);
    assert_eq!(check.rows(), 0, "{:?}", rows(&check));
    let run = engine.spec_run(&path, Some(&project), &token).unwrap();
    assert_eq!(run.issues().rows(), 0, "{:?}", rows(&run.issues()));
    assert_eq!(run.output().unwrap().rows(), 2);
    // The authority is the caller's own: a specification that declares the
    // linked spelling still walks it from the filesystem root and refuses it.
    let linked = link.0.join("raw/dm.csv").to_str().unwrap().to_owned();
    study.write("sdtm/dm.yaml", DM.replace("../raw/dm.csv", &linked));
    let run = engine.spec_run(&path, None, &token).unwrap();
    assert!(run.output().is_none());
    assert_eq!(conditions(&run.issues()), ["resource_path_symlink"]);
}

#[test]
fn unknown_sections_are_refused_arguments() {
    let study = study();
    let token = CancellationToken::new();
    let error = engine()
        .project_run(&study.project(), Some("adm"), &token)
        .err()
        .unwrap();
    let yamaa::Error::Rejected(report) = error else {
        panic!("an unknown section is a refused argument")
    };
    assert_eq!(report.diagnostics[0].code, "engine_rejected");
    assert_eq!(
        report.diagnostics[0].context.get("section"),
        Some(&yamaa::ContextValue::Text("adm".into()))
    );
}

#[test]
fn project_check_reports_every_specification_and_the_project_findings() {
    let study = study();
    study.write(
        "sdtm/ae.yaml",
        AE.replace(
            "columns: [USUBJID, AESEQ]}",
            "columns: [USUBJID, AESEQ, AETERM]}",
        ),
    );
    // A second ADaM specification reads DM's artifact without a link.
    study.write(
        "adam/adae.yaml",
        "schema_version: '1.0'\ndomain: ADAE\nkeys: [USUBJID]\ninput: {DM: {path: ../out/dm.csv, types: {AGE: int}}}\ncolumns:\n  - {name: USUBJID, type: str, label: Subject, derivation: DM.USUBJID}\noutput: {path: ../out/adae.csv, columns: [USUBJID]}\n",
    );
    study.write(
        "project.yaml",
        PROJECT.replace("[adam/adsl.yaml]", "[adam/adsl.yaml, adam/adae.yaml]"),
    );
    let table = engine().project_check(&study.project(), &CancellationToken::new());
    let issues = rows(&table);
    let ae = study.path("sdtm/ae.yaml");
    let adae = study.path("adam/adae.yaml");
    let unlinked = issues
        .iter()
        .find(|row| row.condition == "unlinked_producer")
        .expect("unlinked producer finding");
    assert_eq!(unlinked.requirement.as_deref(), Some("REQ-1344"));
    assert_eq!(unlinked.spec_paths, ["input.DM.path"]);
    assert_eq!(unlinked.context["path"], text("../out/dm.csv"));
    assert_eq!(
        unlinked.context["producer"],
        text(&study.path("sdtm/dm.yaml"))
    );
    assert_eq!(unlinked.context["entry"], text(&adae));
    let unknown = issues
        .iter()
        .find(|row| row.condition == "undeclared_column")
        .unwrap_or_else(|| panic!("AE's own static finding: {issues:?}"));
    assert_eq!(unknown.context["entry"], text(&ae));
    assert_eq!(unknown.context["source"], text(&ae));
    assert_eq!(issues.len(), 2, "{issues:?}");
    // A check reads no study data and writes nothing.
    assert!(!study.exists("out/dm.csv"));
}

#[test]
fn a_section_run_builds_only_its_scope_and_a_full_run_hands_producers_over_in_memory() {
    let study = study();
    let engine = engine();
    let token = CancellationToken::new();
    let sdtm = engine
        .project_run(&study.project(), Some("sdtm"), &token)
        .unwrap();
    assert_eq!(sdtm.names(), ["DM", "AE"]);
    assert!(sdtm.get("ADSL").is_none());
    assert!(sdtm.get("DM").unwrap().output().is_some());
    assert_eq!(sdtm.issues().rows(), 0);

    // ADADD reads ADSL, which reads DM: the ADaM order follows the links
    // even though ADADD is listed first.
    study.write(
        "adam/adadd.yaml",
        "schema_version: '1.0'\ndomain: ADADD\nkeys: [USUBJID]\ninput: {ADSL: {path: ../out/adsl.csv, schema: adsl.yaml}}\ncolumns:\n  - {name: USUBJID, type: str, label: Subject, derivation: ADSL.USUBJID}\noutput: {path: ../out/adadd.csv, columns: [USUBJID]}\n",
    );
    study.write(
        "project.yaml",
        PROJECT.replace("[adam/adsl.yaml]", "[adam/adadd.yaml, adam/adsl.yaml]"),
    );
    let mut run = engine.project_run(&study.project(), None, &token).unwrap();
    assert_eq!(run.names(), ["DM", "AE", "ADSL", "ADADD"]);
    assert_eq!(run.issues().rows(), 0, "{:?}", rows(&run.issues()));
    // No producer artifact existed on disk: ADSL and ADADD read the held
    // artifacts of DM and ADSL, and nothing is written before save.
    assert!(!study.exists("out/dm.csv"));
    let adsl = run.get("ADSL").unwrap();
    assert_eq!(adsl.output().unwrap().rows(), 2);
    let report = run.report();
    let cells = artifacts(&report);
    let mut order = cells.clone();
    order.dedup();
    assert_eq!(
        order,
        [
            "../out/dm.csv",
            "../out/ae.csv",
            "../out/adsl.csv",
            "../out/adadd.csv"
        ]
    );
    assert_eq!(report.rows(), cells.len());
    assert_eq!(
        report.rows(),
        ["DM", "AE", "ADSL", "ADADD"]
            .iter()
            .map(|name| run.get(name).unwrap().report().unwrap().rows())
            .sum::<usize>()
    );
    run.save().unwrap();
    assert_eq!(study.read("out/dm.csv").unwrap(), DM_CSV);
    assert_eq!(study.read("out/adsl.csv").unwrap(), DM_CSV);
    assert_eq!(study.read("out/adadd.csv").unwrap(), "USUBJID\nS1\nS2\n");
    assert!(study
        .read("out/ae.csv")
        .unwrap()
        .starts_with("USUBJID,AESEQ\n"));
    // The held DM artifact is what ADSL's run report counted.
    let adsl_report = study.read("out/adsl-report.csv").unwrap();
    assert!(
        adsl_report.contains(&format!("\"{{\"\"bytes\"\":{},", DM_CSV.len())),
        "{adsl_report}"
    );
    assert!(study.exists("out/dm-report.csv"));
}

#[test]
fn a_producer_outside_the_scope_is_read_from_its_saved_artifact_and_checked() {
    let study = study();
    let engine = engine();
    let token = CancellationToken::new();
    // No saved DM artifact: a clear missing-file failure.
    let adam = engine
        .project_run(&study.project(), Some("adam"), &token)
        .unwrap();
    assert_eq!(adam.names(), ["ADSL"]);
    assert!(adam.get("ADSL").unwrap().output().is_none());
    let issues = rows(&adam.issues());
    assert_eq!(issues[0].condition, "resource_path_missing");
    assert_eq!(issues[0].spec_paths, ["input.DM.path"]);
    assert_eq!(
        issues[0].context["entry"],
        text(&study.path("adam/adsl.yaml"))
    );

    // The saved artifact is read, not rebuilt: the raw DM data is gone.
    study.write("out/dm.csv", DM_CSV);
    std::fs::remove_file(study.0.join("raw/dm.csv")).unwrap();
    let adam = engine
        .project_run(&study.project(), Some("adam"), &token)
        .unwrap();
    assert_eq!(adam.issues().rows(), 0, "{:?}", rows(&adam.issues()));
    assert_eq!(adam.get("ADSL").unwrap().output().unwrap().rows(), 2);

    // A saved artifact that breaks the producer's column contract fails.
    study.write("out/dm.csv", "AGE,USUBJID\n30,S1\n");
    let adam = engine
        .project_run(&study.project(), Some("adam"), &token)
        .unwrap();
    assert!(adam.get("ADSL").unwrap().output().is_none());
    assert_eq!(conditions(&adam.issues()), ["producer_contract_mismatch"]);
}

#[test]
fn an_input_that_reads_a_run_scope_artifact_without_a_link_fails_alone() {
    let study = study();
    study.write(
        "adam/adsl.yaml",
        ADSL.replace("schema: ../sdtm/dm.yaml", "types: {AGE: int}"),
    );
    let engine = engine();
    let token = CancellationToken::new();
    let run = engine.project_run(&study.project(), None, &token).unwrap();
    assert_eq!(run.names(), ["DM", "AE", "ADSL"]);
    assert!(run.get("DM").unwrap().output().is_some());
    let adsl = run.get("ADSL").unwrap();
    assert!(adsl.output().is_none());
    assert!(adsl.report().is_none());
    let issues = rows(&run.issues());
    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0].condition, "unlinked_producer");
    assert_eq!(
        issues[0].context["entry"],
        text(&study.path("adam/adsl.yaml"))
    );
    // Outside a run scope that builds DM, the same input is an ordinary source.
    study.write("out/dm.csv", DM_CSV);
    let adam = engine
        .project_run(&study.project(), Some("adam"), &token)
        .unwrap();
    assert_eq!(adam.issues().rows(), 0, "{:?}", rows(&adam.issues()));
}

#[test]
fn a_failed_specification_stops_only_its_consumers_and_saves_reports_alone() {
    let study = study();
    study.write("raw/dm.csv", "USUBJID,AGE\nS1,thirty\n");
    let engine = engine();
    let token = CancellationToken::new();
    let mut run = engine.project_run(&study.project(), None, &token).unwrap();
    assert_eq!(run.names(), ["DM", "AE", "ADSL"]);
    let dm = run.get("DM").unwrap();
    assert!(dm.output().is_none());
    assert!(dm.report().is_some(), "DM ran and keeps its report");
    assert!(run.get("AE").unwrap().output().is_some());
    let adsl = run.get("ADSL").unwrap();
    assert!(adsl.output().is_none());
    assert!(adsl.report().is_none(), "ADSL did not run");
    let issues = rows(&run.issues());
    assert_eq!(issues.len(), 2, "{issues:?}");
    assert_eq!(
        issues[0].context["entry"],
        text(&study.path("sdtm/dm.yaml"))
    );
    assert_eq!(issues[1].condition, "producer_not_completed");
    assert_eq!(issues[1].requirement.as_deref(), Some("REQ-1345"));
    assert_eq!(issues[1].spec_paths, ["input.DM.schema"]);
    assert_eq!(
        issues[1].context["producer"],
        text(&study.path("sdtm/dm.yaml"))
    );
    assert_eq!(
        issues[1].context["entry"],
        text(&study.path("adam/adsl.yaml"))
    );
    // The stacked report holds the reports of DM and AE, the runs that ran.
    let mut order = artifacts(&run.report());
    order.dedup();
    assert_eq!(order, ["../out/dm.csv", "../out/ae.csv"]);

    let failure = run.save().unwrap_err();
    assert_eq!(rows(&failure.issues).len(), 2);
    // Only the report of the failed specification that ran is written.
    assert!(study
        .read("out/dm-report.csv")
        .unwrap()
        .contains(",failed,"));
    for absent in [
        "out/dm.csv",
        "out/ae.csv",
        "out/adsl.csv",
        "out/adsl-report.csv",
    ] {
        assert!(!study.exists(absent), "{absent}");
    }
}

#[test]
fn every_target_is_validated_before_the_first_file_is_written() {
    let study = study();
    let engine = engine();
    let token = CancellationToken::new();
    // ADSL publishes into a directory that does not exist.
    study.write(
        "adam/adsl.yaml",
        ADSL.replace("../out/adsl.csv", "../missing/adsl.csv"),
    );
    let mut run = engine.project_run(&study.project(), None, &token).unwrap();
    assert_eq!(run.issues().rows(), 0, "{:?}", rows(&run.issues()));
    let failure = run.save().unwrap_err();
    assert!(failure.message.contains("ADSL"), "{}", failure.message);
    assert_eq!(std::fs::read_dir(study.0.join("out")).unwrap().count(), 0);
    // Two run-scope specifications that write one file refuse the run.
    study.write(
        "sdtm/ae.yaml",
        AE.replace("../out/ae.csv", "../out/dm-report.csv"),
    );
    let mut run = engine.project_run(&study.project(), None, &token).unwrap();
    assert!(run.names().is_empty());
    let issues = rows(&run.issues());
    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0].condition, "artifact_path_collision");
    assert_eq!(issues[0].spec_paths, ["output.report", "output.path"]);
    assert!(run.save().is_err());
}

fn define_study() -> Study {
    let study = Study::new();
    let fixture =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../benchmarks/sdtm-dm-metadata");
    for name in ["spec.yaml", "project.yaml"] {
        std::fs::copy(fixture.join(name), study.0.join(name)).unwrap();
    }
    std::fs::create_dir(study.0.join("input")).unwrap();
    std::fs::copy(
        fixture.join("input/dm_raw.csv"),
        study.0.join("input/dm_raw.csv"),
    )
    .unwrap();
    std::fs::copy(fixture.join("expected/dm.csv"), study.0.join("dm.csv")).unwrap();
    study
}

#[test]
fn a_submission_composes_the_selected_section() {
    let study = define_study();
    let fixture =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../benchmarks/sdtm-dm-metadata");
    let token = CancellationToken::new();
    let mut submission = engine()
        .project_submission(
            &study.project(),
            Some("sdtm"),
            Some("2026-09-11T00:00:00"),
            "0.4.0",
            &token,
        )
        .unwrap();
    assert_eq!(
        submission.issues().rows(),
        0,
        "{:?}",
        rows(&submission.issues())
    );
    submission.save().unwrap();
    assert_eq!(
        study.read("define.xml").unwrap(),
        std::fs::read_to_string(fixture.join("expected/define.xml")).unwrap()
    );
    assert!(study.exists("dm.json"));
}

#[test]
fn a_submission_without_a_section_composes_each_section_that_names_define_in_order() {
    let study = define_study();
    let engine = engine();
    let token = CancellationToken::new();
    let project = std::fs::read_to_string(study.0.join("project.yaml")).unwrap();
    // ADaM lists no specification yet and names no define: it is skipped.
    study.write("project.yaml", format!("{project}adam:\n  specs: []\n"));
    let mut submission = engine
        .project_submission(&study.project(), None, None, "0.4.0", &token)
        .unwrap();
    assert_eq!(
        submission.issues().rows(),
        0,
        "{:?}",
        rows(&submission.issues())
    );
    submission.save().unwrap();
    assert!(study.exists("define.xml"));
    std::fs::remove_file(study.0.join("define.xml")).unwrap();

    // ADaM names a define but no standard: SDTM still composes and saves
    // first, and the ADaM failure stops the save after it.
    study.write(
        "project.yaml",
        format!("{project}adam:\n  specs: []\n  define: adam-define.xml\n"),
    );
    let mut submission = engine
        .project_submission(&study.project(), None, None, "0.4.0", &token)
        .unwrap();
    let issues = rows(&submission.issues());
    assert!(!issues.is_empty());
    assert!(issues
        .iter()
        .all(|row| row.condition == "missing_required_field"));
    assert!(issues.iter().any(|row| row.spec_paths == ["adam.standard"]));
    let failure = submission.save().unwrap_err();
    assert!(failure.message.contains("adam"), "{}", failure.message);
    assert!(study.exists("define.xml"));
    assert!(!study.exists("adam-define.xml"));
}
