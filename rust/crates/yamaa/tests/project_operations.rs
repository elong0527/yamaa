//! Project operations through the public engine: names, project checks, run
//! scope and order, saved producers, failures, saves and submissions.
#![cfg(feature = "yaml")]

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use arrow_array::{Array, ListArray, StringArray};
use yamaa::{
    Argument, CallError, CancellationToken, Engine, EngineOptions, FunctionError, FunctionHandle,
    FunctionIdentity, HostLanguage, LockContext, ProjectFunctions, Scalar, Signature, Status,
    Table,
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
    phase: String,
    condition: String,
    rule: Option<String>,
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
        let (phase, condition, rule, context) = (
            text("phase"),
            text("condition"),
            text("rule"),
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
            rows.push(Row {
                phase: phase.value(row).to_owned(),
                condition: condition.value(row).to_owned(),
                rule: (!rule.is_null(row)).then(|| rule.value(row).to_owned()),
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
    assert_eq!(issues[0].rule.as_deref(), Some("PROJECT-6"));
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
    assert_eq!(issues[0].rule.as_deref(), Some("PROJECT-5"));
    assert_eq!(issues[0].spec_paths, ["sdtm.specs[0]", "adam.specs[1]"]);
    assert_eq!(issues[0].context["name"], text("DM"));
    let check = conditions(
        &engine
            .project_check(&study.project(), &token)
            .unwrap()
            .issues(),
    );
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
    let check = engine.project_check(&project, &token).unwrap().issues();
    assert_eq!(check.rows(), 0, "{:?}", rows(&check));
    let run = engine.project_run(&project, None, &token).unwrap();
    assert_eq!(run.names(), ["DM", "AE", "ADSL"]);
    assert_eq!(run.issues().rows(), 0, "{:?}", rows(&run.issues()));
    // A selected specification is checked and run as the caller named it.
    let path = engine.project_spec(&project, "DM").unwrap();
    assert_eq!(path, study.path("sdtm/dm.yaml"));
    let check = engine
        .spec_check(&path, Some(&project), &token)
        .unwrap()
        .issues();
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

/// Counts its calls and interrupts every one, as a user's Ctrl-C would.
struct Interrupting(AtomicUsize);
impl ProjectFunctions for Interrupting {
    fn verify_lock(
        &self,
        _: &LockContext<'_>,
        _: &[FunctionIdentity],
    ) -> Result<(), FunctionError> {
        Ok(())
    }
    fn bind(&self, _: &FunctionIdentity, _: &Signature) -> Result<FunctionHandle, FunctionError> {
        Ok(FunctionHandle::new(()))
    }
    fn invoke(&self, _: &FunctionHandle, _: &[Argument<'_>]) -> Result<Scalar, CallError> {
        self.0.fetch_add(1, Ordering::Relaxed);
        Err(CallError::Raised(FunctionError::Interrupted {
            payload: None,
        }))
    }
}

#[test]
fn a_user_interrupt_stops_the_project_run() {
    let study = Study::new();
    study.write("raw.csv", "ID,V\n1,2\n");
    study.write(
        "environment.yaml",
        "schema_version: '1.0'\nfunctions:\n  double:\n    function: pkg.double\n    description: Double the value.\n    params: [{name: x, type: int}]\n    returns: int\n    tests:\n      - {id: normal, covers: [normal], args: {x: 1}, result: 2}\n      - {id: zero, covers: [boundary], args: {x: 0}, result: 0}\n      - {id: missing, covers: ['short-circuit-missing:x'], args: {x: null}, result: null}\n",
    );
    let spec = |domain: &str| {
        format!(
            "schema_version: '1.0'\ndomain: {domain}\nkeys: [ID]\ninput: {{RAW: {{path: raw.csv, types: {{ID: int, V: int}}}}}}\ncolumns:\n  - {{name: ID, type: int, label: Identifier, derivation: RAW.ID}}\n  - {{name: W, type: int, label: Doubled, derivation: {{function: {{name: double, args: {{x: RAW.V}}}}}}}}\noutput: {{path: {}.csv, columns: [ID, W]}}\n",
            domain.to_lowercase()
        )
    };
    study.write("one.yaml", spec("ONE"));
    study.write("two.yaml", spec("TWO"));
    study.write(
        "project.yaml",
        "schema_version: '1.0'\nenvironments: {python: environment.yaml}\nadam: {specs: [one.yaml, two.yaml]}\n",
    );
    let functions = Arc::new(Interrupting(AtomicUsize::new(0)));
    let engine = Engine::new(
        EngineOptions::new(HostLanguage::Python, "project-test".into(), "0".into())
            .with_functions(functions.clone()),
    );
    let run = engine
        .project_run(&study.project(), None, &CancellationToken::new())
        .unwrap();
    // TWO never builds, so no more project code runs after the interrupt.
    assert_eq!(run.names(), ["ONE"]);
    assert!(run.get("ONE").unwrap().build().interrupt().is_some());
    assert_eq!(functions.0.load(Ordering::Relaxed), 1);
}

/// The one issue of a save that had nothing to write, under `rule`.
fn assert_nothing_to_save(issues: &Table, rule: &str) {
    let issues = rows(issues);
    assert_eq!(issues.len(), 1, "{issues:?}");
    assert_eq!(issues[0].phase, "output");
    assert_eq!(issues[0].condition, "nothing_to_save");
    assert_eq!(issues[0].rule.as_deref(), Some(rule));
    assert!(issues[0].spec_paths.is_empty());
    assert_eq!(
        issues[0].context.as_object().map(|context| context.len()),
        Some(0)
    );
}

#[test]
fn a_save_that_would_write_nothing_fails() {
    let study = study();
    let engine = engine();
    let token = CancellationToken::new();
    // The project declares no `send` section: the run builds nothing.
    let mut send = engine
        .project_run(&study.project(), Some("send"), &token)
        .unwrap();
    assert!(send.names().is_empty());
    assert_eq!(send.issues().rows(), 0);
    // An empty scope succeeds; its save refuses.
    assert_eq!(send.status(), Status::Succeeded);
    let failure = send.save().unwrap_err();
    assert_eq!(
        failure.message,
        "the project run built no specification, so nothing was saved"
    );
    assert_nothing_to_save(&failure.issues, "PROJECT-12");
    // No section names `define`: the submission composes nothing.
    let mut submission = engine
        .project_submission(&study.project(), None, None, "0.4.0", &token)
        .unwrap();
    assert_eq!(submission.issues().rows(), 0);
    assert_eq!(submission.status(), Status::Succeeded);
    let failure = submission.save().unwrap_err();
    assert_eq!(
        failure.message,
        "the submission composed no section, so nothing was saved"
    );
    assert_nothing_to_save(&failure.issues, "PROJECT-13");
    assert_eq!(std::fs::read_dir(study.0.join("out")).unwrap().count(), 0);
}

#[test]
fn a_project_run_refuses_a_name_it_did_not_build() {
    let study = study();
    let engine = engine();
    let token = CancellationToken::new();
    // The SDTM run builds DM and AE. ADSL is listed too, in another section.
    let sdtm = engine
        .project_run(&study.project(), Some("sdtm"), &token)
        .unwrap();
    assert_eq!(sdtm.names(), ["DM", "AE"]);
    for name in ["ADSL", "ADXX"] {
        let failure = sdtm.member(name).err().unwrap();
        assert_eq!(
            failure.message,
            format!("the project run has no specification named {name:?}; it has DM, AE")
        );
        let issues = rows(&failure.issues);
        assert_eq!(issues.len(), 1, "{issues:?}");
        assert_eq!(issues[0].phase, "validation");
        assert_eq!(issues[0].condition, "unknown_specification_name");
        assert_eq!(issues[0].rule.as_deref(), Some("PROJECT-6"));
        assert!(issues[0].spec_paths.is_empty());
        assert_eq!(issues[0].context["name"], text(name));
        // Every name the run has, sorted.
        assert_eq!(
            issues[0].context["available"],
            Value::Array(vec![text("AE"), text("DM")])
        );
    }
    // A run that built nothing has no name to offer.
    let send = engine
        .project_run(&study.project(), Some("send"), &token)
        .unwrap();
    let failure = send.member("DM").err().unwrap();
    assert_eq!(
        failure.message,
        "the project run has no specification named \"DM\"; it built none"
    );
    let issues = rows(&failure.issues);
    assert_eq!(issues.len(), 1, "{issues:?}");
    assert_eq!(issues[0].condition, "unknown_specification_name");
    assert_eq!(issues[0].context["name"], text("DM"));
    assert_eq!(issues[0].context["available"], Value::Array(Vec::new()));
}

/// A cancelled call returns `Error::Cancelled`, with no result and no issue:
/// a check, a run and a submission alike.
#[test]
fn a_cancelled_call_returns_no_result_and_no_issue() {
    let study = study();
    let engine = engine();
    let token = CancellationToken::new();
    token.cancel();
    let project = study.project();
    let path = study.path("sdtm/dm.yaml");
    assert!(matches!(
        engine.spec_check(&path, Some(&project), &token),
        Err(yamaa::Error::Cancelled)
    ));
    assert!(matches!(
        engine.project_check(&project, &token),
        Err(yamaa::Error::Cancelled)
    ));
    assert!(matches!(
        engine.spec_run(&path, Some(&project), &token),
        Err(yamaa::Error::Cancelled)
    ));
    assert!(matches!(
        engine.project_run(&project, None, &token),
        Err(yamaa::Error::Cancelled)
    ));
    assert!(matches!(
        engine.project_submission(&project, None, None, "0.4.0", &token),
        Err(yamaa::Error::Cancelled)
    ));
    assert_eq!(std::fs::read_dir(study.0.join("out")).unwrap().count(), 0);
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
    let table = engine()
        .project_check(&study.project(), &CancellationToken::new())
        .unwrap()
        .issues();
    let issues = rows(&table);
    let ae = study.path("sdtm/ae.yaml");
    let adae = study.path("adam/adae.yaml");
    let unlinked = issues
        .iter()
        .find(|row| row.condition == "unlinked_producer")
        .expect("unlinked producer finding");
    assert_eq!(unlinked.rule.as_deref(), Some("PROJECT-10"));
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
    // The save returns every path it wrote, in run order, each report before
    // its primary artifact (D38, OUTPUT-10).
    assert_eq!(
        run.save().unwrap(),
        [
            "out/dm-report.csv",
            "out/dm.csv",
            "out/ae.csv",
            "out/adsl-report.csv",
            "out/adsl.csv",
            "out/adadd.csv"
        ]
        .map(|name| study.path(name))
    );
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
fn a_specification_may_not_write_the_saved_artifact_it_reads() {
    let study = study();
    study.write("out/dm.csv", DM_CSV);
    let engine = engine();
    let token = CancellationToken::new();
    for (field, path) in [
        ("output.path", "../out/dm.csv"),
        ("output.report", "../out/dm-report.csv"),
    ] {
        let target = if field == "output.path" {
            "../out/adsl.csv"
        } else {
            "../out/adsl-report.csv"
        };
        study.write("adam/adsl.yaml", ADSL.replace(target, path));
        let mut adam = engine
            .project_run(&study.project(), Some("adam"), &token)
            .unwrap();
        assert!(adam.get("ADSL").unwrap().output().is_none());
        let issues = rows(&adam.issues());
        assert_eq!(issues.len(), 1, "{issues:?}");
        assert_eq!(issues[0].condition, "artifact_path_collision");
        assert_eq!(issues[0].spec_paths, [field, field]);
        assert!(adam.save().is_err());
        assert_eq!(study.read("out/dm.csv").unwrap(), DM_CSV);
    }
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
    // Its own finding stopped it, so it failed rather than skipped.
    assert_eq!(adsl.status(), Status::Failed);
    assert_eq!(run.status(), Status::Failed);
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
    // DM failed, AE succeeded, and ADSL, whose producer did not complete,
    // was skipped; the run failed.
    assert_eq!(dm.status(), Status::Failed);
    assert_eq!(run.get("AE").unwrap().status(), Status::Succeeded);
    assert_eq!(adsl.status(), Status::Skipped);
    assert_eq!(run.status(), Status::Failed);
    let issues = rows(&run.issues());
    assert_eq!(issues.len(), 2, "{issues:?}");
    assert_eq!(
        issues[0].context["entry"],
        text(&study.path("sdtm/dm.yaml"))
    );
    assert_eq!(issues[1].condition, "producer_not_completed");
    assert_eq!(issues[1].rule.as_deref(), Some("PROJECT-11"));
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
    // Only the report of the failed specification that ran is written, and
    // the refusal names it (P-API-1).
    assert_eq!(failure.paths, [study.path("out/dm-report.csv")]);
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
    // DM run alone fails the same way: its save writes the report, then
    // refuses with the run's issues and names what it wrote.
    std::fs::remove_file(study.0.join("out/dm-report.csv")).unwrap();
    let mut alone = engine
        .spec_run(&study.path("sdtm/dm.yaml"), Some(&study.project()), &token)
        .unwrap();
    let failure = alone.save().unwrap_err();
    assert_eq!(failure.message, "cannot save a failed run; see issues");
    assert_eq!(rows(&failure.issues).len(), 1);
    assert_eq!(failure.paths, [study.path("out/dm-report.csv")]);
    assert!(study.exists("out/dm-report.csv"));
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
    assert!(failure.paths.is_empty());
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

/// One save stages every file of every specification before it replaces any
/// target (D74): a file that cannot be staged stops the save with nothing
/// written, though earlier specifications' files staged cleanly.
#[cfg(unix)]
#[test]
fn a_file_that_cannot_be_staged_stops_the_save_before_any_target_is_replaced() {
    use std::os::unix::fs::PermissionsExt;
    let study = study();
    // ADSL publishes into a directory that exists but admits no new entry.
    study.write(
        "adam/adsl.yaml",
        ADSL.replace("../out/adsl", "../locked/adsl"),
    );
    let locked = study.0.join("locked");
    std::fs::create_dir(&locked).unwrap();
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o500)).unwrap();
    if std::fs::write(locked.join("probe"), b"").is_ok() {
        // A privileged user is not refused; nothing to observe here.
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o700)).unwrap();
        return;
    }
    let token = CancellationToken::new();
    let mut run = engine()
        .project_run(&study.project(), None, &token)
        .unwrap();
    assert_eq!(run.status(), Status::Succeeded);
    let failure = run.save().unwrap_err();
    assert!(failure.message.contains("ADSL"), "{}", failure.message);
    assert!(failure.paths.is_empty());
    let issues = rows(&failure.issues);
    assert_eq!(issues.len(), 1, "{issues:?}");
    assert_eq!(issues[0].condition, "engine_rejected");
    assert_eq!(issues[0].spec_paths, ["output.report"]);
    assert_eq!(issues[0].context["entered"], Value::from(1));
    assert_eq!(issues[0].context["completed"], Value::from(0));
    // Neither DM's nor AE's staged files became visible, and none is left.
    assert_eq!(std::fs::read_dir(study.0.join("out")).unwrap().count(), 0);
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o700)).unwrap();
    assert_eq!(std::fs::read_dir(&locked).unwrap().count(), 0);
    // Once the directory admits it, the same run saves every file.
    assert_eq!(run.save().unwrap().len(), 5);
    assert_eq!(run.get("ADSL").unwrap().issues().rows(), 0);
    assert_eq!(study.read("locked/adsl.csv").unwrap(), DM_CSV);
}

/// One specification runs on the same route alone or as a member of a
/// project run, and every view computes its status from its issues and its
/// build (P-STATUS-1).
#[test]
fn a_specification_runs_alike_alone_and_in_a_project_run_with_computed_statuses() {
    let study = study();
    let engine = engine();
    let token = CancellationToken::new();
    let project = study.project();
    let dm = study.path("sdtm/dm.yaml");
    let check = engine.spec_check(&dm, Some(&project), &token).unwrap();
    assert_eq!(check.status(), Status::Succeeded);
    assert_eq!(
        engine.project_check(&project, &token).unwrap().status(),
        Status::Succeeded
    );
    let alone = engine.spec_run(&dm, Some(&project), &token).unwrap();
    assert_eq!(alone.status(), Status::Succeeded);
    let runs = engine.project_run(&project, None, &token).unwrap();
    assert_eq!(runs.status(), Status::Succeeded);
    for name in runs.names() {
        assert_eq!(
            runs.get(&name).unwrap().status(),
            Status::Succeeded,
            "{name}"
        );
    }
    // The member and the specification run alone hold the same output,
    // report and evidence.
    let member = runs.get("DM").unwrap();
    assert_eq!(
        member.output().unwrap().ipc().unwrap(),
        alone.output().unwrap().ipc().unwrap()
    );
    assert_eq!(
        member.report().unwrap().ipc().unwrap(),
        alone.report().unwrap().ipc().unwrap()
    );
    assert_eq!(
        member.build().observations(false),
        alone.build().observations(false)
    );
    // A failed input fails the run but not the check, which reads no data.
    study.write("raw/dm.csv", "USUBJID,AGE\nS1,thirty\n");
    assert_eq!(
        engine
            .spec_run(&dm, Some(&project), &token)
            .unwrap()
            .status(),
        Status::Failed
    );
    assert_eq!(
        engine
            .spec_check(&dm, Some(&project), &token)
            .unwrap()
            .status(),
        Status::Succeeded
    );
    // A finding of the specification itself fails its check too.
    study.write("sdtm/dm.yaml", DM.replace("RAW.AGE", "RAW.AGE +"));
    let check = engine.spec_check(&dm, Some(&project), &token).unwrap();
    assert_ne!(check.issues().rows(), 0);
    assert_eq!(check.status(), Status::Failed);
    assert_eq!(
        engine.project_check(&project, &token).unwrap().status(),
        Status::Failed
    );
    // A save's refusal joins the issues but describes the save, not the run.
    study.write(
        "sdtm/ae.yaml",
        AE.replace("../out/ae.csv", "../missing/ae.csv"),
    );
    let mut ae = engine
        .spec_run(&study.path("sdtm/ae.yaml"), Some(&project), &token)
        .unwrap();
    assert_eq!(ae.issues().rows(), 0);
    assert_eq!(ae.status(), Status::Succeeded);
    assert!(ae.save().is_err());
    assert_ne!(ae.issues().rows(), 0);
    assert_eq!(ae.status(), Status::Succeeded);
    assert_eq!(
        [Status::Succeeded, Status::Failed, Status::Skipped].map(Status::as_str),
        ["succeeded", "failed", "skipped"]
    );
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
    assert_eq!(submission.status(), Status::Succeeded);
    // Transport files come before the document (JSON-14).
    assert_eq!(
        submission.save().unwrap(),
        [study.path("dm.json"), study.path("define.xml")]
    );
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
    assert_eq!(submission.status(), Status::Failed);
    assert!(issues
        .iter()
        .all(|row| row.condition == "missing_required_field"));
    assert!(issues.iter().any(|row| row.spec_paths == ["adam.standard"]));
    let failure = submission.save().unwrap_err();
    assert!(failure.message.contains("adam"), "{}", failure.message);
    assert_eq!(
        failure.paths,
        [study.path("dm.json"), study.path("define.xml")]
    );
    assert!(study.exists("define.xml"));
    assert!(!study.exists("adam-define.xml"));
}

#[test]
fn two_sections_that_write_one_file_save_neither() {
    let study = define_study();
    let engine = engine();
    let token = CancellationToken::new();
    let project = std::fs::read_to_string(study.0.join("project.yaml")).unwrap();
    // ADaM copies SDTM's section metadata, its define target included.
    let adam = project[project.find("sdtm:\n").unwrap()..]
        .replacen("sdtm:", "adam:", 1)
        .replace(
            "name: SDTMIG\n    version: '3.4'",
            "name: ADaMIG\n    version: '1.3'",
        )
        .replace("  specs:\n  - spec.yaml\n", "  specs: []\n")
        .replace("  datasets: json\n", "");
    study.write("project.yaml", format!("{project}{adam}"));
    let mut submission = engine
        .project_submission(
            &study.project(),
            None,
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
    let failure = submission.save().unwrap_err();
    assert!(failure.message.contains("adam"), "{}", failure.message);
    assert!(failure.paths.is_empty());
    assert!(!study.exists("define.xml"));
    assert!(!study.exists("dm.json"));
}

/// One save admits every section's targets before it writes any file
/// (PROJECT-13): a later section whose target can no longer be written stops
/// the save with nothing written, and that section's issues say why.
#[test]
fn a_later_section_whose_target_cannot_be_written_saves_nothing() {
    let study = define_study();
    let engine = engine();
    let token = CancellationToken::new();
    let project = std::fs::read_to_string(study.0.join("project.yaml")).unwrap();
    // ADaM copies SDTM's section metadata with a define target of its own.
    let adam = project[project.find("sdtm:\n").unwrap()..]
        .replacen("sdtm:", "adam:", 1)
        .replace(
            "name: SDTMIG\n    version: '3.4'",
            "name: ADaMIG\n    version: '1.3'",
        )
        .replace("  specs:\n  - spec.yaml\n", "  specs: []\n")
        .replace("  datasets: json\n", "")
        .replace("define: define.xml", "define: adam-define.xml");
    study.write("project.yaml", format!("{project}{adam}"));
    let mut submission = engine
        .project_submission(
            &study.project(),
            None,
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
    std::fs::create_dir(study.0.join("adam-define.xml")).unwrap();
    let failure = submission.save().unwrap_err();
    assert!(failure.message.contains("adam"), "{}", failure.message);
    assert!(failure.paths.is_empty());
    let issues = rows(&failure.issues);
    assert_eq!(issues.len(), 1, "{issues:?}");
    assert_eq!(issues[0].condition, "engine_rejected");
    assert_eq!(
        issues[0].context["stage"],
        text("publication_target_admission")
    );
    assert_eq!(issues[0].context["code"], text("invalid_target"));
    assert!(!study.exists("define.xml"));
    assert!(!study.exists("dm.json"));
    // The submission keeps the cause; its status describes the composition.
    assert_eq!(conditions(&submission.issues()), ["engine_rejected"]);
    assert_eq!(submission.status(), Status::Succeeded);
}
