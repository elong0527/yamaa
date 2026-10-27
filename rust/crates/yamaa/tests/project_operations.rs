//! Project operations through the public engine: names, project checks, run
//! scope and order, saved producers, failures, saves and submissions.
#![cfg(feature = "yaml")]

use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use arrow_array::{Array, ListArray, StringArray};
use yamaa::host::functions::{
    Argument, CallError, FunctionError, FunctionHandle, FunctionIdentity, LockContext, Signature,
};
use yamaa::scalar::Scalar;
use yamaa::{
    CancellationToken, Engine, EngineOptions, HostLanguage, ProjectFunctions, Status,
    SubmissionOptions, Table,
};
use yamaa_core::json::Value;

use crate::support::NoFunctions;

fn engine() -> Engine {
    Engine::new(
        EngineOptions::new(HostLanguage::Python, "project-test".into(), "0".into())
            .with_functions(Arc::new(NoFunctions)),
    )
}

struct Study(crate::support::Scratch);
impl Study {
    fn new() -> Self {
        let root = crate::support::Scratch::new("project-operations");
        // The engine's spelling: no Windows verbatim prefix.
        Self(root)
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

const DM: &str = "yamaa: \"1.0\"\ntype: spec\ndomain: DM\nkeys: [USUBJID]\ninput: {RAW: {path: ../raw/dm.csv, types: {AGE: int}}}\ncolumns:\n  - {name: USUBJID, type: str, label: Subject, derivation: RAW.USUBJID}\n  - {name: AGE, type: int, label: Age, derivation: RAW.AGE}\noutput: {path: ../out/dm.csv, columns: [USUBJID, AGE], report: ../out/dm-report.csv}\n";
const AE: &str = "yamaa: \"1.0\"\ntype: spec\ndomain: AE\nkeys: [USUBJID, AESEQ]\ninput: {RAW: {path: ../raw/ae.csv, types: {AESEQ: int}}}\ncolumns:\n  - {name: USUBJID, type: str, label: Subject, derivation: RAW.USUBJID}\n  - {name: AESEQ, type: int, label: Sequence, derivation: RAW.AESEQ}\noutput: {path: ../out/ae.csv, columns: [USUBJID, AESEQ]}\n";
const ADSL: &str = "yamaa: \"1.0\"\ntype: spec\ndomain: ADSL\nkeys: [USUBJID]\ninput: {DM: {path: ../out/dm.csv, schema: ../sdtm/dm.yaml}}\ncolumns:\n  - {name: USUBJID, type: str, label: Subject, derivation: DM.USUBJID}\n  - {name: AGE, type: int, label: Age, derivation: DM.AGE}\noutput: {path: ../out/adsl.csv, columns: [USUBJID, AGE], report: ../out/adsl-report.csv}\n";
const PROJECT: &str =
    "yamaa: \"1.0\"\ntype: project\nsdtm:\n  specs: [sdtm/dm.yaml, sdtm/ae.yaml]\nadam:\n  specs: [adam/adsl.yaml]\n";
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

#[derive(Debug, PartialEq)]
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

/// D39's error text for a save of `subject` that `failure` refused: the
/// call, the first issue's printed line and the count of the others.
fn save_refusal(subject: &str, failure: &yamaa::Failure) -> String {
    let count = match failure.diagnostics.len() - 1 {
        0 => String::new(),
        1 => " (and 1 more issue)".to_owned(),
        others => format!(" (and {others} more issues)"),
    };
    format!(
        "cannot save {subject}: {}{count}",
        failure.diagnostics[0].text()
    )
}

/// D45: a project's names in run order, producer first, as a run of every
/// section orders them, read without data. A listing that cannot be read has
/// no name, as in a run; a run that a collision refuses names nothing, while
/// the names stay its listings' (D180); a project that cannot be read, or
/// that lists a name twice, refuses as `cannot read <project>` (D39).
#[test]
fn project_names_list_the_run_order_and_refuse_as_a_read() {
    let study = study();
    let engine = engine();
    // ADSL, listed first, reads DM, so DM runs before it.
    study.write(
        "project.yaml",
        "yamaa: \"1.0\"\ntype: project\nsdtm:\n  specs: [adam/adsl.yaml, sdtm/dm.yaml, sdtm/ae.yaml]\n",
    );
    let names = engine.project(&study.project()).names().unwrap();
    assert_eq!(names, ["DM", "ADSL", "AE"]);
    let runs = engine.project(&study.project()).run().unwrap();
    assert_eq!(runs.names(), names);

    // AE writes DM's file, so the run refuses before any specification
    // builds and names nothing, yet each name still selects its listing.
    study.write("project.yaml", PROJECT);
    study.write("sdtm/ae.yaml", AE.replace("../out/ae.csv", "../out/dm.csv"));
    let names = engine.project(&study.project()).names().unwrap();
    assert_eq!(names, ["DM", "AE", "ADSL"]);
    for (name, listing) in names
        .iter()
        .zip(["sdtm/dm.yaml", "sdtm/ae.yaml", "adam/adsl.yaml"])
    {
        assert_eq!(
            Path::new(engine.project(&study.project()).spec(name).unwrap().path()),
            study.0.join(listing),
            "{name}"
        );
    }
    let runs = engine.project(&study.project()).run().unwrap();
    assert!(runs.names().is_empty());
    assert_eq!(conditions(&runs.issues()), ["artifact_path_collision"]);
    study.write("sdtm/ae.yaml", AE);

    study.write(
        "project.yaml",
        PROJECT.replace("sdtm/ae.yaml", "sdtm/absent.yaml"),
    );
    assert_eq!(
        engine.project(&study.project()).names().unwrap(),
        ["DM", "ADSL"]
    );

    study.write("adam/dm.yaml", DM.replace("../out/dm", "../out/dm-copy"));
    study.write(
        "project.yaml",
        PROJECT.replace("[adam/adsl.yaml]", "[adam/adsl.yaml, adam/dm.yaml]"),
    );
    let duplicate = engine.project(&study.project()).names().unwrap_err();
    // Listing and selecting report the same located duplicate-name finding.
    let selected = engine.project(&study.project()).spec("DM").unwrap_err();
    assert_eq!(duplicate.diagnostics, selected.diagnostics);
    assert!(
        duplicate.message.starts_with(&format!("cannot read {}: ", study.project()))
            && duplicate
                .message
                .ends_with("the project lists two specifications named DM [duplicate_specification_name, PROJECT-5]"),
        "{}",
        duplicate.message
    );
    assert_eq!(
        conditions(&duplicate.issues),
        ["duplicate_specification_name"]
    );
    assert!(duplicate.paths.is_empty());

    let absent = study.path("absent.yaml");
    let missing = engine.project(&absent).names().unwrap_err();
    assert!(
        missing
            .message
            .starts_with(&format!("cannot read {absent}: ")),
        "{}",
        missing.message
    );
    assert_eq!(rows(&missing.issues).len(), 1);
}

/// A request's section scopes its run alone among these operations: its
/// names, the specifications it selects and its check are the whole
/// project's, so a field AE does not declare, listed in SDTM, is a finding of
/// the check of a request scoped to ADaM (`unknown_field`, SCHEMA-13).
#[test]
fn a_project_requests_section_leaves_its_names_selection_and_check_whole() {
    let study = study();
    let engine = engine();
    let project = study.project();
    let adam = engine.project(&project).section("adam");
    assert_eq!(adam.names().unwrap(), ["DM", "AE", "ADSL"]);
    assert_eq!(adam.spec("AE").unwrap().path(), study.path("sdtm/ae.yaml"));
    assert_eq!(adam.run().unwrap().names(), ["ADSL"]);
    study.write("sdtm/ae.yaml", format!("{AE}bogus: 1\n"));
    assert_eq!(
        conditions(&adam.check().unwrap().issues()),
        ["unknown_field"]
    );
}

#[test]
fn names_select_listed_specifications_and_refuse_unknown_or_shared_names() {
    let study = study();
    let engine = engine();
    let project = study.project();
    let adsl = engine.project(&project).spec("ADSL").unwrap();
    assert_eq!(Path::new(adsl.path()), study.0.join("adam/adsl.yaml"));
    let unknown = engine.project(&study.project()).spec("ADXX").unwrap_err();
    // D39: the call, then the issue, which lists the names in its context.
    assert_eq!(
        unknown.message,
        "cannot select ADXX: the project has no specification named ADXX \
         [unknown_specification_name, PROJECT-6]"
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
    let duplicate = engine.project(&study.project()).spec("AE").unwrap_err();
    let issues = rows(&duplicate.issues);
    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0].condition, "duplicate_specification_name");
    assert_eq!(issues[0].rule.as_deref(), Some("PROJECT-5"));
    assert_eq!(issues[0].spec_paths, ["sdtm.specs[0]", "adam.specs[1]"]);
    assert_eq!(issues[0].context["name"], text("DM"));
    let check = conditions(
        &engine
            .project(&study.project())
            .cancel_with(&token)
            .check()
            .unwrap()
            .issues(),
    );
    assert!(check.iter().any(|c| c == "duplicate_specification_name"));
    let mut run = engine
        .project(&study.project())
        .cancel_with(&token)
        .run()
        .unwrap();
    assert!(run.names().is_empty());
    assert_eq!(conditions(&run.issues()), ["duplicate_specification_name"]);
    assert!(run.save(&CancellationToken::new()).is_err());
    let mut submission = engine
        .project(&study.project())
        .cancel_with(&token)
        .submission(&SubmissionOptions::new("0.4.0"))
        .unwrap();
    assert_eq!(
        conditions(&submission.issues()),
        ["duplicate_specification_name"]
    );
    assert!(submission.save(&CancellationToken::new()).is_err());
    assert!(!study.exists("out/dm.csv"));
}

/// A listed specification that fails validation keeps the `domain` its file
/// writes as its name (PROJECT-5), and selecting it checks it exactly as
/// naming its file with the project file does (PROJECT-6): here AE restates
/// its input's path shorthand (`redundant_long_form`, SCHEMA-15).
#[test]
fn a_listed_specification_with_a_validation_finding_keeps_its_name_and_selects() {
    let study = study();
    let engine = engine();
    let project = study.project();
    study.write("sdtm/ae.yaml", AE.replace(", types: {AESEQ: int}}", "}"));
    assert_eq!(
        engine.project(&project).names().unwrap(),
        ["DM", "AE", "ADSL"]
    );
    let selected = engine.project(&project).spec("AE").unwrap();
    assert_eq!(selected.path(), study.path("sdtm/ae.yaml"));
    let selected = rows(&selected.check().unwrap().issues());
    let named = rows(
        &engine
            .spec(&study.path("sdtm/ae.yaml"))
            .project(&project)
            .check()
            .unwrap()
            .issues(),
    );
    assert_eq!(selected, named);
    assert_eq!(
        selected
            .iter()
            .map(|row| (
                row.phase.as_str(),
                row.condition.as_str(),
                row.rule.as_deref(),
                row.spec_paths.clone()
            ))
            .collect::<Vec<_>>(),
        [(
            "validation",
            "redundant_long_form",
            Some("SCHEMA-15"),
            vec!["input.RAW".to_owned()]
        )]
    );
}

/// The caller's spelling of its project file keeps the authority of the
/// directory it names, as an entry path's does, though the engine names each
/// listed specification by its canonical identity. Here the spelling passes
/// through a linked directory, as every macOS temporary path passes through
/// `/var`, a link to `/private/var`.
#[cfg(unix)]
#[test]
fn a_project_spelled_through_a_linked_directory_keeps_its_authority() {
    struct Link(std::path::PathBuf);
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
    let check = engine
        .project(&project)
        .cancel_with(&token)
        .check()
        .unwrap()
        .issues();
    assert_eq!(check.rows(), 0, "{:?}", rows(&check));
    let run = engine.project(&project).cancel_with(&token).run().unwrap();
    assert_eq!(run.names(), ["DM", "AE", "ADSL"]);
    assert_eq!(run.issues().rows(), 0, "{:?}", rows(&run.issues()));
    // A selected specification is checked and run as the caller named it.
    let dm = engine
        .project(&project)
        .cancel_with(&token)
        .spec("DM")
        .unwrap();
    assert_eq!(dm.path(), study.path("sdtm/dm.yaml"));
    let check = dm.check().unwrap().issues();
    assert_eq!(check.rows(), 0, "{:?}", rows(&check));
    let run = dm.run().unwrap();
    assert_eq!(run.issues().rows(), 0, "{:?}", rows(&run.issues()));
    assert_eq!(run.output().unwrap().rows(), 2);
    // The authority is the caller's own: a specification that declares the
    // linked spelling still walks it from the filesystem root and refuses it.
    let linked = link.0.join("raw/dm.csv").to_str().unwrap().to_owned();
    study.write("sdtm/dm.yaml", DM.replace("../raw/dm.csv", &linked));
    let run = engine.spec(dm.path()).cancel_with(&token).run().unwrap();
    assert!(run.output().is_none());
    assert_eq!(conditions(&run.issues()), ["resource_path_symlink"]);
}

/// The issue of a call refused for selecting `section`, which the project
/// does not declare; `declared` are the sections it does, sorted.
fn assert_unknown_section(failure: Option<yamaa::Failure>, section: &str, declared: &[&str]) {
    let failure = failure.unwrap_or_else(|| panic!("selecting {section} refuses the call"));
    assert!(
        matches!(failure.kind, yamaa::FailureKind::Rejected),
        "{failure:?}"
    );
    assert_eq!(
        failure.message,
        format!(
            "cannot select {section}: the project declares no section {section} \
             [unknown_section, PROJECT-14]"
        )
    );
    let issues = rows(&failure.issues);
    assert_eq!(issues.len(), 1, "{issues:?}");
    assert_eq!(issues[0].phase.as_str(), "validation");
    assert_eq!(issues[0].condition, "unknown_section");
    assert_eq!(issues[0].rule.as_deref(), Some("PROJECT-14"));
    assert!(issues[0].spec_paths.is_empty());
    assert_eq!(issues[0].context["section"], text(section));
    assert_eq!(
        issues[0].context["sections"],
        Value::Array(declared.iter().map(|section| text(section)).collect())
    );
}

/// A project run or submission accepts only a section the project declares:
/// a name outside `sdtm`, `adam` and `send`, and one of them the project does
/// not declare, both refuse the call before anything builds (PROJECT-14,
/// #3030 D73 and D98).
#[test]
fn a_section_the_project_does_not_declare_refuses_the_call() {
    let study = study();
    let engine = engine();
    let token = CancellationToken::new();
    let project = study.project();
    for section in ["send", "adm", "SDTM"] {
        assert_unknown_section(
            engine
                .project(&project)
                .section(section)
                .cancel_with(&token)
                .run()
                .err(),
            section,
            &["adam", "sdtm"],
        );
        assert_unknown_section(
            engine
                .project(&project)
                .section(section)
                .cancel_with(&token)
                .submission(&SubmissionOptions::new("0.4.0"))
                .err(),
            section,
            &["adam", "sdtm"],
        );
    }
    assert_eq!(std::fs::read_dir(study.0.join("out")).unwrap().count(), 0);
    // A 1.0 project file declares its sections the same way; one whose
    // `specs` is empty is declared, and selecting it builds nothing.
    study.write(
        "project.yaml",
        "yamaa: \"1.0\"\ntype: project\nsend: {specs: []}\nadam: {specs: []}\n",
    );
    assert_unknown_section(
        engine
            .project(&project)
            .section("sdtm")
            .cancel_with(&token)
            .run()
            .err(),
        "sdtm",
        &["adam", "send"],
    );
    let send = engine
        .project(&project)
        .section("send")
        .cancel_with(&token)
        .run()
        .unwrap();
    assert!(send.names().is_empty());
    assert_eq!(send.issues().rows(), 0);
    // The project file is read first: one that cannot be read reports its
    // own findings as the result's issues, and no section is checked.
    let absent = study.path("absent.yaml");
    let run = engine
        .project(&absent)
        .section("adm")
        .cancel_with(&token)
        .run()
        .unwrap();
    assert_eq!(conditions(&run.issues()), ["resource_path_missing"]);
    let submission = engine
        .project(&absent)
        .section("adm")
        .cancel_with(&token)
        .submission(&SubmissionOptions::new("0.4.0"))
        .unwrap();
    assert_eq!(conditions(&submission.issues()), ["resource_path_missing"]);
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

/// A user interrupt in project code cancels the project run (D46): the call
/// returns `Error::Cancelled`, and no later specification builds or calls
/// project code.
#[test]
fn a_user_interrupt_cancels_the_project_run() {
    let study = Study::new();
    study.write("raw.csv", "ID,V\n1,2\n");
    study.write(
        "functions/double.yaml",
        "yamaa: \"1.0\"\ntype: function\nname: double\ndescription: Double the value.\nparams: [{name: x, type: int}]\nreturns: int\nimplementations: {python: pkg.double}\ntests:\n  - {id: normal, covers: [normal], args: {x: 1}, result: 2}\n  - {id: zero, covers: [boundary], args: {x: 0}, result: 0}\n  - {id: missing, covers: ['short-circuit-missing:x'], args: {x: null}, result: null}\n",
    );
    let spec = |domain: &str| {
        format!(
            "yamaa: \"1.0\"\ntype: spec\ndomain: {domain}\nkeys: [ID]\ninput: {{RAW: {{path: raw.csv, types: {{ID: int, V: int}}}}}}\ncolumns:\n  - {{name: ID, type: int, label: Identifier, derivation: RAW.ID}}\n  - {{name: W, type: int, label: Doubled, derivation: {{function: {{name: double, args: {{x: RAW.V}}}}}}}}\noutput: {{path: {}.csv, columns: [ID, W]}}\n",
            domain.to_lowercase()
        )
    };
    study.write("one.yaml", spec("ONE"));
    study.write("two.yaml", spec("TWO"));
    study.write(
        "project.yaml",
        "yamaa: \"1.0\"\ntype: project\nfunctions: [functions/double.yaml]\nadam: {specs: [one.yaml, two.yaml]}\n",
    );
    let functions = Arc::new(Interrupting(AtomicUsize::new(0)));
    let engine = Engine::new(
        EngineOptions::new(HostLanguage::Python, "project-test".into(), "0".into())
            .with_functions(functions.clone()),
    );
    let run = engine.project(&study.project()).run();
    assert!(matches!(
        run,
        Err(yamaa::Failure {
            kind: yamaa::FailureKind::Cancelled,
            ..
        })
    ));
    // TWO never builds, so no more project code runs after the interrupt.
    assert_eq!(functions.0.load(Ordering::Relaxed), 1);
}

/// The one issue of a save that had nothing to write, under `rule`.
/// The one issue of a save with nothing to write, which names the operation
/// whose result it is: `run` under PROJECT-12, `submission` under PROJECT-13.
fn assert_nothing_to_save(issues: &Table, rule: &str) {
    let issues = rows(issues);
    assert_eq!(issues.len(), 1, "{issues:?}");
    assert_eq!(issues[0].phase.as_str(), "output");
    assert_eq!(issues[0].condition, "nothing_to_save");
    assert_eq!(issues[0].rule.as_deref(), Some(rule));
    assert!(issues[0].spec_paths.is_empty());
    let operation = if rule == "PROJECT-12" {
        "run"
    } else {
        "submission"
    };
    assert_eq!(
        issues[0].context,
        yamaa_adapters::json_wire::decode(&format!("{{\"operation\":\"{operation}\"}}")).unwrap()
    );
}

#[test]
fn a_save_that_would_write_nothing_fails() {
    let study = study();
    // The `send` section lists no specification: the run builds nothing.
    study.write("project.yaml", format!("{PROJECT}send:\n  specs: []\n"));
    let engine = engine();
    let token = CancellationToken::new();
    let mut send = engine
        .project(&study.project())
        .section("send")
        .cancel_with(&token)
        .run()
        .unwrap();
    assert!(send.names().is_empty());
    assert_eq!(send.issues().rows(), 0);
    // An empty scope succeeds; its save refuses.
    assert_eq!(send.status(), Status::Succeeded);
    let failure = send.save(&CancellationToken::new()).unwrap_err();
    assert_eq!(
        failure.message,
        format!(
            "cannot save {}, section send: the project run has nothing to save \
             [nothing_to_save, PROJECT-12]",
            study.project()
        )
    );
    assert_nothing_to_save(&failure.issues, "PROJECT-12");
    // No section names `define`: the submission composes nothing.
    let mut submission = engine
        .project(&study.project())
        .cancel_with(&token)
        .submission(&SubmissionOptions::new("0.4.0"))
        .unwrap();
    assert_eq!(submission.issues().rows(), 0);
    assert_eq!(submission.status(), Status::Succeeded);
    let failure = submission.save(&CancellationToken::new()).unwrap_err();
    assert_eq!(
        failure.message,
        format!(
            "cannot save {}: the project submission has nothing to save \
             [nothing_to_save, PROJECT-13]",
            study.project()
        )
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
        .project(&study.project())
        .section("sdtm")
        .cancel_with(&token)
        .run()
        .unwrap();
    assert_eq!(sdtm.names(), ["DM", "AE"]);
    for name in ["ADSL", "ADXX"] {
        let failure = sdtm.member(name).err().unwrap();
        assert_eq!(
            failure.message,
            format!(
                "cannot select {name}: the project has no specification named {name} \
                 [unknown_specification_name, PROJECT-6]"
            )
        );
        let issues = rows(&failure.issues);
        assert_eq!(issues.len(), 1, "{issues:?}");
        assert_eq!(issues[0].phase.as_str(), "validation");
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
    study.write("project.yaml", format!("{PROJECT}send:\n  specs: []\n"));
    let send = engine
        .project(&study.project())
        .section("send")
        .cancel_with(&token)
        .run()
        .unwrap();
    let failure = send.member("DM").err().unwrap();
    assert_eq!(
        failure.message,
        "cannot select DM: the project has no specification named DM \
         [unknown_specification_name, PROJECT-6]"
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
        engine
            .spec(&path)
            .project(&project)
            .cancel_with(&token)
            .check(),
        Err(yamaa::Failure {
            kind: yamaa::FailureKind::Cancelled,
            ..
        })
    ));
    assert!(matches!(
        engine.project(&project).cancel_with(&token).check(),
        Err(yamaa::Failure {
            kind: yamaa::FailureKind::Cancelled,
            ..
        })
    ));
    assert!(matches!(
        engine
            .spec(&path)
            .project(&project)
            .cancel_with(&token)
            .run(),
        Err(yamaa::Failure {
            kind: yamaa::FailureKind::Cancelled,
            ..
        })
    ));
    assert!(matches!(
        engine.project(&project).cancel_with(&token).run(),
        Err(yamaa::Failure {
            kind: yamaa::FailureKind::Cancelled,
            ..
        })
    ));
    assert!(matches!(
        engine
            .project(&project)
            .cancel_with(&token)
            .submission(&SubmissionOptions::new("0.4.0")),
        Err(yamaa::Failure {
            kind: yamaa::FailureKind::Cancelled,
            ..
        })
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
        "yamaa: \"1.0\"\ntype: spec\ndomain: ADAE\nkeys: [USUBJID]\ninput: {DM: {path: ../out/dm.csv, types: {AGE: int}}}\ncolumns:\n  - {name: USUBJID, type: str, label: Subject, derivation: DM.USUBJID}\noutput: {path: ../out/adae.csv, columns: [USUBJID]}\n",
    );
    study.write(
        "project.yaml",
        PROJECT.replace("[adam/adsl.yaml]", "[adam/adsl.yaml, adam/adae.yaml]"),
    );
    let table = engine().project(&study.project()).check().unwrap().issues();
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
        .project(&study.project())
        .section("sdtm")
        .cancel_with(&token)
        .run()
        .unwrap();
    assert_eq!(sdtm.names(), ["DM", "AE"]);
    assert!(sdtm.get("ADSL").is_none());
    assert!(sdtm.get("DM").unwrap().output().is_some());
    assert_eq!(sdtm.issues().rows(), 0);

    // ADADD reads ADSL, which reads DM: the ADaM order follows the links
    // even though ADADD is listed first.
    study.write(
        "adam/adadd.yaml",
        "yamaa: \"1.0\"\ntype: spec\ndomain: ADADD\nkeys: [USUBJID]\ninput: {ADSL: {path: ../out/adsl.csv, schema: adsl.yaml}}\ncolumns:\n  - {name: USUBJID, type: str, label: Subject, derivation: ADSL.USUBJID}\noutput: {path: ../out/adadd.csv, columns: [USUBJID]}\n",
    );
    study.write(
        "project.yaml",
        PROJECT.replace("[adam/adsl.yaml]", "[adam/adadd.yaml, adam/adsl.yaml]"),
    );
    let mut run = engine
        .project(&study.project())
        .cancel_with(&token)
        .run()
        .unwrap();
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
        run.save(&CancellationToken::new()).unwrap(),
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
        .project(&study.project())
        .section("adam")
        .cancel_with(&token)
        .run()
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
        .project(&study.project())
        .section("adam")
        .cancel_with(&token)
        .run()
        .unwrap();
    assert_eq!(adam.issues().rows(), 0, "{:?}", rows(&adam.issues()));
    assert_eq!(adam.get("ADSL").unwrap().output().unwrap().rows(), 2);

    // A saved artifact that breaks the producer's column contract fails.
    study.write("out/dm.csv", "AGE,USUBJID\n30,S1\n");
    let adam = engine
        .project(&study.project())
        .section("adam")
        .cancel_with(&token)
        .run()
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
            .project(&study.project())
            .section("adam")
            .cancel_with(&token)
            .run()
            .unwrap();
        assert!(adam.get("ADSL").unwrap().output().is_none());
        let issues = rows(&adam.issues());
        assert_eq!(issues.len(), 1, "{issues:?}");
        assert_eq!(issues[0].condition, "artifact_path_collision");
        assert_eq!(issues[0].spec_paths, [field, field]);
        assert!(adam.save(&CancellationToken::new()).is_err());
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
    let run = engine
        .project(&study.project())
        .cancel_with(&token)
        .run()
        .unwrap();
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
        .project(&study.project())
        .section("adam")
        .cancel_with(&token)
        .run()
        .unwrap();
    assert_eq!(adam.issues().rows(), 0, "{:?}", rows(&adam.issues()));
}

#[test]
fn a_failed_specification_stops_only_its_consumers_and_saves_reports_alone() {
    let study = study();
    study.write("raw/dm.csv", "USUBJID,AGE\nS1,thirty\n");
    let engine = engine();
    let token = CancellationToken::new();
    let mut run = engine
        .project(&study.project())
        .cancel_with(&token)
        .run()
        .unwrap();
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

    let failure = run.save(&CancellationToken::new()).unwrap_err();
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
        .spec(&study.path("sdtm/dm.yaml"))
        .project(&study.project())
        .cancel_with(&token)
        .run()
        .unwrap();
    let failure = alone.save(&CancellationToken::new()).unwrap_err();
    // D39: the run names its dataset, read before the run failed.
    assert_eq!(failure.message, save_refusal("DM", &failure));
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
    let mut run = engine
        .project(&study.project())
        .cancel_with(&token)
        .run()
        .unwrap();
    assert_eq!(run.issues().rows(), 0, "{:?}", rows(&run.issues()));
    let failure = run.save(&CancellationToken::new()).unwrap_err();
    // D39: the call names the project run; the issue names ADSL's
    // specification as its entry.
    assert_eq!(failure.message, save_refusal(&study.project(), &failure));
    assert_eq!(
        rows(&failure.issues)[0].context["entry"],
        text(&study.path("adam/adsl.yaml"))
    );
    assert!(failure.paths.is_empty());
    assert_eq!(std::fs::read_dir(study.0.join("out")).unwrap().count(), 0);
    // Two run-scope specifications that write one file refuse the run.
    study.write(
        "sdtm/ae.yaml",
        AE.replace("../out/ae.csv", "../out/dm-report.csv"),
    );
    let mut run = engine
        .project(&study.project())
        .cancel_with(&token)
        .run()
        .unwrap();
    assert!(run.names().is_empty());
    let issues = rows(&run.issues());
    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0].condition, "artifact_path_collision");
    assert_eq!(issues[0].spec_paths, ["output.report", "output.path"]);
    assert!(run.save(&CancellationToken::new()).is_err());
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
        .project(&study.project())
        .cancel_with(&token)
        .run()
        .unwrap();
    assert_eq!(run.status(), Status::Succeeded);
    let failure = run.save(&CancellationToken::new()).unwrap_err();
    assert_eq!(failure.message, save_refusal(&study.project(), &failure));
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
    assert_eq!(run.save(&CancellationToken::new()).unwrap().len(), 5);
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
    let check = engine
        .spec(&dm)
        .project(&project)
        .cancel_with(&token)
        .check()
        .unwrap();
    assert_eq!(check.status(), Status::Succeeded);
    assert_eq!(
        engine
            .project(&project)
            .cancel_with(&token)
            .check()
            .unwrap()
            .status(),
        Status::Succeeded
    );
    let alone = engine
        .spec(&dm)
        .project(&project)
        .cancel_with(&token)
        .run()
        .unwrap();
    assert_eq!(alone.status(), Status::Succeeded);
    let runs = engine.project(&project).cancel_with(&token).run().unwrap();
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
        member.output().unwrap().batches(),
        alone.output().unwrap().batches()
    );
    assert_eq!(
        member.report().unwrap().batches(),
        alone.report().unwrap().batches()
    );
    assert_eq!(member.observations(), alone.observations());
    // A failed input fails the run but not the check, which reads no data.
    study.write("raw/dm.csv", "USUBJID,AGE\nS1,thirty\n");
    assert_eq!(
        engine
            .spec(&dm)
            .project(&project)
            .cancel_with(&token)
            .run()
            .unwrap()
            .status(),
        Status::Failed
    );
    assert_eq!(
        engine
            .spec(&dm)
            .project(&project)
            .cancel_with(&token)
            .check()
            .unwrap()
            .status(),
        Status::Succeeded
    );
    // A finding of the specification itself fails its check too.
    study.write("sdtm/dm.yaml", DM.replace("RAW.AGE", "RAW.AGE +"));
    let check = engine
        .spec(&dm)
        .project(&project)
        .cancel_with(&token)
        .check()
        .unwrap();
    assert_ne!(check.issues().rows(), 0);
    assert_eq!(check.status(), Status::Failed);
    assert_eq!(
        engine
            .project(&project)
            .cancel_with(&token)
            .check()
            .unwrap()
            .status(),
        Status::Failed
    );
    // A save's refusal joins the issues but describes the save, not the run.
    study.write(
        "sdtm/ae.yaml",
        AE.replace("../out/ae.csv", "../missing/ae.csv"),
    );
    let mut ae = engine
        .spec(&study.path("sdtm/ae.yaml"))
        .project(&project)
        .cancel_with(&token)
        .run()
        .unwrap();
    assert_eq!(ae.issues().rows(), 0);
    assert_eq!(ae.status(), Status::Succeeded);
    assert!(ae.save(&CancellationToken::new()).is_err());
    assert_ne!(ae.issues().rows(), 0);
    assert_eq!(ae.status(), Status::Succeeded);
    assert_eq!(
        [Status::Succeeded, Status::Failed, Status::Skipped].map(Status::as_str),
        ["succeeded", "failed", "skipped"]
    );
}

/// Every member runs through the engine's one host-functions port. An engine
/// configured without project functions, as the command line is, runs every
/// build that calls no project function through an empty port, with or
/// without a project, exactly as a host builds it; a build that calls one is
/// refused before it reads data.
#[test]
fn an_engine_without_functions_runs_every_build_that_calls_no_project_function() {
    let study = study();
    let bare = Engine::new(EngineOptions::new(
        HostLanguage::Python,
        "project-test".into(),
        "0".into(),
    ));
    let token = CancellationToken::new();
    let project = study.project();
    let dm = study.path("sdtm/dm.yaml");
    let alone = bare.spec(&dm).cancel_with(&token).run().unwrap();
    let member = bare
        .spec(&dm)
        .project(&project)
        .cancel_with(&token)
        .run()
        .unwrap();
    let hosted = engine()
        .spec(&dm)
        .project(&project)
        .cancel_with(&token)
        .run()
        .unwrap();
    for run in [&alone, &member, &hosted] {
        assert_eq!(run.issues().rows(), 0);
        assert_eq!(run.status(), Status::Succeeded);
    }
    let output = |run: &yamaa::Run| run.output().unwrap().batches().to_vec();
    assert_eq!(output(&member), output(&alone));
    assert_eq!(output(&member), output(&hosted));
    assert_eq!(
        member.report().unwrap().batches(),
        hosted.report().unwrap().batches()
    );
    let runs = bare.project(&project).cancel_with(&token).run().unwrap();
    assert_eq!(runs.status(), Status::Succeeded);
    assert_eq!(runs.names(), ["DM", "AE", "ADSL"]);

    // A project whose function file defines the function a listed
    // specification calls.
    let study = Study::new();
    study.write("raw.csv", "ID,V\n1,2\n");
    study.write(
        "functions/double.yaml",
        "yamaa: \"1.0\"\ntype: function\nname: double\ndescription: Double the value.\nparams: [{name: x, type: int}]\nreturns: int\nimplementations: {python: pkg.double}\ntests:\n  - {id: normal, covers: [normal], args: {x: 1}, result: 2}\n  - {id: zero, covers: [boundary], args: {x: 0}, result: 0}\n  - {id: missing, covers: ['short-circuit-missing:x'], args: {x: null}, result: null}\n",
    );
    study.write(
        "one.yaml",
        "yamaa: \"1.0\"\ntype: spec\ndomain: ONE\nkeys: [ID]\ninput: {RAW: {path: raw.csv, types: {ID: int, V: int}}}\ncolumns:\n  - {name: ID, type: int, label: Identifier, derivation: RAW.ID}\n  - {name: W, type: int, label: Doubled, derivation: {function: {name: double, args: {x: RAW.V}}}}\noutput: {path: one.csv, columns: [ID, W]}\n",
    );
    study.write(
        "project.yaml",
        "yamaa: \"1.0\"\ntype: project\nfunctions: [functions/double.yaml]\nadam: {specs: [one.yaml]}\n",
    );
    let calls = bare
        .spec(&study.path("one.yaml"))
        .project(&study.project())
        .cancel_with(&token)
        .run()
        .unwrap();
    assert_eq!(calls.status(), Status::Failed);
    assert!(calls.output().is_none());
    assert!(calls.report().is_none());
    // An engine that claims Python but was given no functions runs through
    // the empty port, so the refusal names the first derivation that calls
    // one (D221), located at its `derivation` key: line 8, column 42, after
    // `  - {name: W, type: int, label: Doubled, ` (P-ISSUE-1).
    let found = rows(&calls.issues());
    let [row] = found.as_slice() else {
        panic!("one issue: {found:?}")
    };
    assert_eq!(
        (
            row.phase.as_str(),
            row.condition.as_str(),
            row.rule.as_deref(),
            row.spec_paths.as_slice(),
            &row.context,
        ),
        (
            "planning",
            "unsupported_operation",
            None,
            ["columns.W.derivation".to_owned()].as_slice(),
            &yamaa_adapters::json!({"operation": "environment"}),
        ),
        "the refused run of an engine without functions"
    );
    let location = calls.diagnostics()[0].location.clone().unwrap();
    assert_eq!(
        (
            location.file,
            location.position.map(|at| (at.line, at.column))
        ),
        (study.path("one.yaml"), Some((8, 42))),
        "the refusal's location"
    );
}

/// An engine that claims no host language, as the command line's does,
/// reads no project's `environments.<host>` entry, so it verifies no lock
/// and activates nothing (#3030 section 2.10). Through a 1.0 project whose
/// Python lock does not exist, it runs the specification that calls no
/// function. It checks and resolves the one that calls the project's
/// function, and refuses to run it before reading any input, as
/// `unsupported_operation` (D110). An engine that claims Python reads the same
/// project's lock, and fails where the lock is missing.
#[test]
fn an_engine_without_a_host_language_reads_no_lock_and_refuses_each_function_call() {
    let study = Study::new();
    study.write("raw.csv", "ID,V\n1,a\n");
    study.write(
        "functions/shout.yaml",
        "yamaa: \"1.0\"\ntype: function\nname: shout\ndescription: The value in capitals.\nparams: [{name: x, type: str}]\nreturns: str\nimplementations: {python: pkg.shout}\ntests:\n  - {id: normal, covers: [normal], args: {x: a}, result: A}\n  - {id: empty, covers: [boundary], args: {x: ''}, result: ''}\n  - {id: missing, covers: ['short-circuit-missing:x'], args: {x: null}, result: null}\n",
    );
    study.write(
        "plain.yaml",
        "yamaa: \"1.0\"\ntype: spec\ndomain: PLAIN\nkeys: [ID]\ninput: {RAW: raw.csv}\noutput: {path: plain.csv, columns: [ID, V]}\ncolumns:\n  - {name: ID, type: str, label: Identifier, derivation: RAW.ID}\n  - {name: V, type: str, label: Value, derivation: RAW.V}\n",
    );
    // The input this one names does not exist, so a run that read it would
    // fail there.
    study.write(
        "calls.yaml",
        "yamaa: \"1.0\"\ntype: spec\ndomain: CALLS\nkeys: [ID]\ninput: {RAW: absent.csv}\noutput: {path: calls.csv, columns: [ID, LOUD]}\ncolumns:\n  - {name: ID, type: str, label: Identifier, derivation: RAW.ID}\n  - {name: LOUD, type: str, label: Loud, derivation: {function: {name: shout, args: {x: RAW.V}}}}\n",
    );
    study.write(
        "project.yaml",
        "yamaa: \"1.0\"\ntype: project\nfunctions: [functions/shout.yaml]\nenvironments:\n  python: {lock: missing/uv.lock}\nadam: {specs: [plain.yaml, calls.yaml]}\n",
    );
    let neutral = Engine::new(EngineOptions::neutral("cli".into(), String::new()));
    let token = CancellationToken::new();
    let project = study.project();
    let (plain, calls) = (study.path("plain.yaml"), study.path("calls.yaml"));

    let run = neutral
        .spec(&plain)
        .project(&project)
        .cancel_with(&token)
        .run()
        .unwrap();
    assert_eq!(conditions(&run.issues()), Vec::<String>::new());
    assert_eq!(run.status(), Status::Succeeded);
    assert_eq!(run.output().unwrap().rows(), 1);
    let check = neutral
        .spec(&calls)
        .project(&project)
        .cancel_with(&token)
        .check()
        .unwrap();
    assert_eq!(conditions(&check.issues()), Vec::<String>::new());
    assert!(neutral.spec(&calls).project(&project).resolve().is_ok());
    let refused = neutral
        .spec(&calls)
        .project(&project)
        .cancel_with(&token)
        .run()
        .unwrap();
    assert_eq!(refused.status(), Status::Failed);
    assert!(refused.output().is_none() && refused.report().is_none());
    // D110's refusal names the first derivation that calls a project
    // function (D221), located at its `derivation` key: line 9, column 42,
    // after `  - {name: LOUD, type: str, label: Loud, ` (P-ISSUE-1).
    let found = rows(&refused.issues());
    let [row] = found.as_slice() else {
        panic!("one issue: {found:?}")
    };
    assert_eq!(
        (
            row.phase.as_str(),
            row.condition.as_str(),
            row.rule.as_deref(),
            row.spec_paths.as_slice(),
            &row.context,
        ),
        (
            "planning",
            "unsupported_operation",
            None,
            ["columns.LOUD.derivation".to_owned()].as_slice(),
            &yamaa_adapters::json!({"operation": "environment"}),
        ),
        "the refused run"
    );
    let location = refused.diagnostics()[0].location.clone().unwrap();
    assert_eq!(
        (
            location.file,
            location.position.map(|at| (at.line, at.column))
        ),
        (calls.clone(), Some((9, 42))),
        "the refusal's location"
    );
    let runs = neutral.project(&project).cancel_with(&token).run().unwrap();
    assert_eq!(runs.names(), ["PLAIN", "CALLS"]);
    assert_eq!(runs.get("PLAIN").unwrap().status(), Status::Succeeded);
    assert_eq!(
        conditions(&runs.get("CALLS").unwrap().issues()),
        ["unsupported_operation"]
    );

    // An engine that claims Python reads its own lock, which does not exist.
    let python = Engine::new(EngineOptions::new(
        HostLanguage::Python,
        "project-test".into(),
        "0".into(),
    ));
    for spec in [&plain, &calls] {
        let check = python
            .spec(spec)
            .project(&project)
            .cancel_with(&token)
            .check()
            .unwrap();
        let found = rows(&check.issues());
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].condition, "resource_path_missing");
        assert_eq!(found[0].spec_paths, ["environments.python.lock"]);
        assert_eq!(found[0].context.get("path"), Some(&text("missing/uv.lock")));
    }
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
        .project(&study.project())
        .section("sdtm")
        .cancel_with(&token)
        .submission(&SubmissionOptions::new("0.4.0").with_created("2026-09-11T00:00:00"))
        .unwrap();
    assert_eq!(
        submission.issues().rows(),
        0,
        "{:?}",
        rows(&submission.issues())
    );
    assert_eq!(submission.status(), Status::Succeeded);
    assert_eq!(submission.created(), "2026-09-11T00:00:00");
    // Transport files come before the document (JSON-14).
    assert_eq!(
        submission.save(&CancellationToken::new()).unwrap(),
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
        .project(&study.project())
        .cancel_with(&token)
        .submission(&SubmissionOptions::new("0.4.0"))
        .unwrap();
    assert_eq!(
        submission.issues().rows(),
        0,
        "{:?}",
        rows(&submission.issues())
    );
    submission.save(&CancellationToken::new()).unwrap();
    assert!(study.exists("define.xml"));
    std::fs::remove_file(study.0.join("define.xml")).unwrap();

    // ADaM names a define but no standards: SDTM still composes and saves
    // first, and the ADaM failure stops the save after it.
    study.write(
        "project.yaml",
        format!("{project}adam:\n  specs: []\n  define: adam-define.xml\n"),
    );
    let mut submission = engine
        .project(&study.project())
        .cancel_with(&token)
        .submission(&SubmissionOptions::new("0.4.0"))
        .unwrap();
    let issues = rows(&submission.issues());
    assert!(!issues.is_empty());
    assert_eq!(submission.status(), Status::Failed);
    assert!(issues
        .iter()
        .all(|row| row.condition == "missing_required_field"));
    assert!(issues
        .iter()
        .any(|row| row.spec_paths == ["adam.standards"]));
    let failure = submission.save(&CancellationToken::new()).unwrap_err();
    // D39: the call names the submission; ADaM's issue says what it lacks.
    assert_eq!(failure.message, save_refusal(&study.project(), &failure));
    assert!(
        failure.message.contains(
            ": adam.standards lacks the field standards, which it requires \
                 [missing_required_field, "
        ),
        "{}",
        failure.message
    );
    assert_eq!(
        failure.paths,
        [study.path("dm.json"), study.path("define.xml")]
    );
    assert!(study.exists("define.xml"));
    assert!(!study.exists("adam-define.xml"));
}

/// SDTM's section metadata copied as an ADaM section that lists no
/// specification and writes its own define target, `adam-define.xml`.
fn adam_section(project: &str) -> String {
    project[project.find("sdtm:\n").unwrap()..]
        .replacen("sdtm:", "adam:", 1)
        .replace(
            "name: SDTMIG\n    version: '3.4'",
            "name: ADaMIG\n    version: '1.3'",
        )
        .replace("  specs:\n  - spec.yaml\n", "  specs: []\n")
        .replace("  transport: dataset_json\n", "")
        .replace("define: define.xml", "define: adam-define.xml")
}

/// Without `created`, a submission reads the clock once and gives every
/// section that time, which `created()` reports (D69, D129): each section's
/// define.xml and transport file carry it.
#[test]
fn a_submission_gives_every_section_one_creation_time_and_records_it() {
    let study = define_study();
    let project = std::fs::read_to_string(study.0.join("project.yaml")).unwrap();
    study.write(
        "project.yaml",
        format!("{project}{}", adam_section(&project)),
    );
    let mut submission = engine()
        .project(&study.project())
        .submission(&SubmissionOptions::new("0.4.0"))
        .unwrap();
    assert_eq!(
        submission.issues().rows(),
        0,
        "{:?}",
        rows(&submission.issues())
    );
    let created = submission.created().to_owned();
    // A whole-second civil datetime (DEFINE-3).
    let shape = created
        .chars()
        .map(|c| if c.is_ascii_digit() { 'd' } else { c })
        .collect::<String>();
    assert_eq!(shape, "dddd-dd-ddTdd:dd:dd", "{created}");
    assert_eq!(
        submission.save(&CancellationToken::new()).unwrap(),
        [
            study.path("dm.json"),
            study.path("define.xml"),
            study.path("adam-define.xml")
        ]
    );
    for define in ["define.xml", "adam-define.xml"] {
        let text = study.read(define).unwrap();
        assert!(
            text.contains(&format!("CreationDateTime=\"{created}\"")),
            "{define} carries {created}"
        );
    }
    assert!(study
        .read("dm.json")
        .unwrap()
        .contains(&format!("\"datasetJSONCreationDateTime\": \"{created}\"")));
}

/// `created()` records the time a submission used even when nothing
/// composed: the argument as given, text DEFINE-3 refuses included, or the
/// time read for a project that cannot be read.
#[test]
fn a_submission_records_its_creation_time_whatever_it_composed() {
    let study = define_study();
    let engine = engine();
    let token = CancellationToken::new();
    let refused = engine
        .project(&study.project())
        .section("sdtm")
        .cancel_with(&token)
        .submission(&SubmissionOptions::new("0.4.0").with_created("2026-09-11"))
        .unwrap();
    assert_eq!(refused.created(), "2026-09-11");
    let issues = rows(&refused.issues());
    assert_eq!(issues.len(), 1, "{issues:?}");
    assert_eq!(issues[0].condition, "value_not_permitted");
    assert_eq!(issues[0].rule.as_deref(), Some("DEFINE-3"));
    assert_eq!(issues[0].spec_paths, ["created"]);

    let missing = study.path("missing.yaml");
    let unread = engine
        .project(&missing)
        .cancel_with(&token)
        .submission(&SubmissionOptions::new("0.4.0").with_created("2026-09-11T00:00:00"))
        .unwrap();
    assert_eq!(unread.status(), Status::Failed);
    assert_eq!(unread.created(), "2026-09-11T00:00:00");
    let unread = engine
        .project(&missing)
        .cancel_with(&token)
        .submission(&SubmissionOptions::new("0.4.0"))
        .unwrap();
    assert_eq!(unread.created().len(), "2026-09-11T00:00:00".len());
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
        .replace("  transport: dataset_json\n", "");
    study.write("project.yaml", format!("{project}{adam}"));
    let mut submission = engine
        .project(&study.project())
        .cancel_with(&token)
        .submission(&SubmissionOptions::new("0.4.0").with_created("2026-09-11T00:00:00"))
        .unwrap();
    assert_eq!(
        submission.issues().rows(),
        0,
        "{:?}",
        rows(&submission.issues())
    );
    let failure = submission.save(&CancellationToken::new()).unwrap_err();
    assert_eq!(failure.message, save_refusal(&study.project(), &failure));
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
    study.write(
        "project.yaml",
        format!("{project}{}", adam_section(&project)),
    );
    let mut submission = engine
        .project(&study.project())
        .cancel_with(&token)
        .submission(&SubmissionOptions::new("0.4.0").with_created("2026-09-11T00:00:00"))
        .unwrap();
    assert_eq!(
        submission.issues().rows(),
        0,
        "{:?}",
        rows(&submission.issues())
    );
    std::fs::create_dir(study.0.join("adam-define.xml")).unwrap();
    let failure = submission.save(&CancellationToken::new()).unwrap_err();
    assert_eq!(failure.message, save_refusal(&study.project(), &failure));
    assert!(failure.paths.is_empty());
    // A directory where the file belongs is refused at the section's
    // `define`, naming the file as written (FILE-7, #3030 D189).
    let issues = rows(&failure.issues);
    assert_eq!(issues.len(), 1, "{issues:?}");
    assert_eq!(issues[0].phase.as_str(), "output");
    assert_eq!(issues[0].condition, "resource_path_not_regular_file");
    assert_eq!(issues[0].spec_paths, ["adam.define"]);
    assert_eq!(issues[0].context["path"], text("adam-define.xml"));
    assert!(!study.exists("define.xml"));
    assert!(!study.exists("dm.json"));
    // The submission keeps the cause; its status describes the composition.
    assert_eq!(
        conditions(&submission.issues()),
        ["resource_path_not_regular_file"]
    );
    assert_eq!(submission.status(), Status::Succeeded);
}

const GUARDED_PROJECT: &str = r#"yamaa: "1.0"
type: project
study:
  id: CATH
  name: CATH
  description: Example study submitting its demographics and subject analysis
  protocol_name: CATH-01
sdtm:
  standards:
    - name: SDTMIG
      version: "3.4"
      status: Final
  specs:
    - dm.yaml
  define: define.xml
adam:
  standards:
    - name: ADaMIG
      version: "1.3"
      status: Final
  specs:
    - adsl.yaml
  define: adam-define.xml
"#;

const GUARDED_DM: &str = r#"yamaa: "1.0"
type: spec
model: SDTM
domain: DM
keys: [STUDYID, USUBJID]
input:
  RAW: raw.csv
output:
  path: dm.csv
  columns: [DOMAIN, STUDYID, USUBJID]
  report: dm-report.csv
columns:
  - name: DOMAIN
    type: str
    label: Domain Abbreviation
    derivation: "'DM'"
    submission:
      core: Req
      role: Identifier
      length: 2
      origins:
        - type: Assigned
          source: Sponsor
  - name: STUDYID
    type: str
    label: Study Identifier
    derivation: RAW.STUDYID
    submission:
      core: Req
      role: Identifier
      length: 4
      origins:
        - type: Protocol
          source: Sponsor
  - name: USUBJID
    type: str
    label: Unique Subject Identifier
    derivation: RAW.USUBJID
    submission:
      core: Req
      role: Identifier
      length: 9
      origins:
        - type: Assigned
          source: Sponsor
submission:
  label: Demographics
  class: SPECIAL PURPOSE
  structure: One record per subject
  repeating: false
"#;

/// ADSL over DM's artifact through its `schema` link.
const GUARDED_ADSL: &str = r#"yamaa: "1.0"
type: spec
model: ADaM
domain: ADSL
keys: [STUDYID, USUBJID]
input:
  DM:
    path: dm.csv
    schema: dm.yaml
output:
  path: adsl.csv
  columns: [STUDYID, USUBJID]
  report: adsl-report.csv
columns:
  - name: STUDYID
    type: str
    label: Study Identifier
    derivation: DM.STUDYID
    submission:
      core: Req
      mandatory: true
      length: 4
      origins:
        - type: Predecessor
          description: DM.STUDYID
  - name: USUBJID
    type: str
    label: Unique Subject Identifier
    derivation: DM.USUBJID
    submission:
      core: Req
      mandatory: true
      length: 9
      origins:
        - type: Predecessor
          description: DM.USUBJID
submission:
  label: Subject-Level Analysis Dataset
  class: SUBJECT LEVEL ANALYSIS DATASET
  structure: One record per subject
  repeating: false
"#;

/// A specification that declares a run report composes only with the
/// artifact and the inputs that report records (PROJECT-15): reports the
/// engine saved compose, ADSL's included, which counted DM's held artifact;
/// a changed raw input makes DM stale, and DM rebuilt alone makes ADSL stale.
#[test]
fn a_submission_composes_the_artifacts_their_run_reports_record_and_refuses_stale_ones() {
    let study = Study::new();
    study.write("project.yaml", GUARDED_PROJECT);
    study.write("dm.yaml", GUARDED_DM);
    study.write("adsl.yaml", GUARDED_ADSL);
    let raw = "STUDYID,USUBJID\nCATH,CATH-0001\nCATH,CATH-0002\n";
    study.write("raw.csv", raw);
    let engine = engine();
    let token = CancellationToken::new();
    let build = |section: Option<&str>| {
        let project = study.project();
        let request = engine.project(&project).cancel_with(&token);
        let mut run = match section {
            Some(section) => request.section(section),
            None => request,
        }
        .run()
        .unwrap();
        assert_eq!(run.issues().rows(), 0, "{:?}", rows(&run.issues()));
        run.save(&CancellationToken::new()).unwrap();
    };
    let submit = || {
        let submission = engine
            .project(&study.project())
            .cancel_with(&token)
            .submission(&SubmissionOptions::new("0.4.0").with_created("2026-09-11T00:00:00"))
            .unwrap();
        rows(&submission.issues())
    };
    let assert_stale = |issues: &[Row], path: &str, dataset: &str, input: &str, bytes| {
        assert_eq!(issues.len(), 1, "{issues:?}");
        let issue = &issues[0];
        assert_eq!(
            (issue.phase.as_str(), issue.condition.as_str()),
            ("validation", "stale_artifact")
        );
        assert_eq!(issue.rule.as_deref(), Some("PROJECT-15"));
        assert_eq!(issue.spec_paths, [format!("{path}.output.report")]);
        let report = format!("{}-report.csv", dataset.to_ascii_lowercase());
        assert_eq!(issue.context["dataset"], text(dataset));
        assert_eq!(issue.context["report"], text(&report));
        assert_eq!(issue.context["reason"], text("input_bytes"));
        assert_eq!(issue.context["input"], text(input));
        assert_eq!(issue.context["actual"], Value::from(bytes));
    };

    build(None);
    assert_eq!(submit().len(), 0, "{:?}", submit());

    let grown = format!("{raw}CATH,CATH-0003\n");
    study.write("raw.csv", &grown);
    assert_stale(&submit(), "sdtm.specs[0]", "DM", "RAW", grown.len());

    build(Some("sdtm"));
    let dm = study.read("dm.csv").unwrap();
    assert_eq!(dm.lines().count(), 4);
    assert_stale(&submit(), "adam.specs[0]", "ADSL", "DM", dm.len());

    build(Some("adam"));
    assert_eq!(submit().len(), 0, "{:?}", submit());
}
