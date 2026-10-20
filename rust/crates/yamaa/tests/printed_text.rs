//! Printed summaries, error text and issue JSON through the public engine
//! (D28, D37, D39, P-PRINT-1). Every expected line here was written from
//! `docs/design/interface-1.0/api.md` ("Printed text" and "The command line")
//! and the message templates of `yaml/conditions.yaml` before the engine ran;
//! issue locations are not recorded yet (#3262), so each issue line has no
//! location.
#![cfg(feature = "yaml")]

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use yamaa::{
    Action, Argument, CallError, CancellationToken, ContextValue, Engine, EngineOptions, Error,
    Failure, FunctionError, FunctionHandle, FunctionIdentity, HostLanguage, LockContext,
    ProjectFunctions, Scalar, Signature, Status, Summary,
};

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
        EngineOptions::new(HostLanguage::Python, "printed-text".into(), "0".into())
            .with_functions(Arc::new(NoFunctions)),
    )
}

static NEXT: AtomicUsize = AtomicUsize::new(0);

struct Study(PathBuf);
impl Study {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "yamaa-printed-text-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&root).unwrap();
        Self(yamaa_adapters::canonical_path::canonicalize(root).unwrap())
    }
    fn write(&self, name: &str, contents: impl AsRef<[u8]>) {
        let path = self.0.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, contents).unwrap();
    }
    fn path(&self, name: &str) -> String {
        self.0.join(name).to_str().unwrap().to_owned()
    }
}
impl Drop for Study {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

const ADSL: &str = "schema_version: '1.0'\ndomain: ADSL\nkeys: [USUBJID]\ninput: {DM: {path: ../raw/dm.csv, types: {AGE: int}}}\ncolumns:\n  - {name: USUBJID, type: str, label: Subject, derivation: DM.USUBJID}\n  - {name: AGE, type: int, label: Age, derivation: DM.AGE}\noutput: {path: ../out/adsl.csv, columns: [USUBJID, AGE]}\n";
/// ASTDY reads TRTSDT, which is declared after it (COLUMN-5).
const ADAE: &str = "schema_version: '1.0'\ndomain: ADAE\nkeys: [USUBJID]\ninput: {AE: {path: ../raw/ae.csv, types: {TRTSDT: int}}}\ncolumns:\n  - {name: USUBJID, type: str, label: Subject, derivation: AE.USUBJID}\n  - {name: ASTDY, type: int, label: Study Day, derivation: {compute: {expr: TRTSDT + 1}}}\n  - {name: TRTSDT, type: int, label: Start, derivation: AE.TRTSDT}\noutput: {path: ../out/adae.csv, columns: [USUBJID, ASTDY, TRTSDT]}\n";
/// Reads ADAE through its `schema` link, so it runs only after ADAE.
const ADTTE: &str = "schema_version: '1.0'\ndomain: ADTTE\nkeys: [USUBJID]\ninput: {ADAE: {path: ../out/adae.csv, schema: adae.yaml}}\ncolumns:\n  - {name: USUBJID, type: str, label: Subject, derivation: ADAE.USUBJID}\noutput: {path: ../out/adtte.csv, columns: [USUBJID]}\n";
const PROJECT: &str =
    "schema_version: '1.0'\nadam:\n  specs: [adam/adsl.yaml, adam/adae.yaml, adam/adtte.yaml]\n";

/// ADSL builds; ADAE fails its own check; ADTTE reads ADAE.
fn study() -> Study {
    let study = Study::new();
    study.write("raw/dm.csv", "USUBJID,AGE\nS1,30\nS2,41\n");
    study.write("raw/ae.csv", "USUBJID,TRTSDT\nS1,3\nS2,4\n");
    study.write("adam/adsl.yaml", ADSL);
    study.write("adam/adae.yaml", ADAE);
    study.write("adam/adtte.yaml", ADTTE);
    study.write("project.yaml", PROJECT);
    std::fs::create_dir_all(study.0.join("out")).unwrap();
    study
}

const FORWARD: &str =
    "column ASTDY reads TRTSDT, which is declared after it [forward_reference, COLUMN-5]";
const SKIPPED: &str = "this specification did not run because the producer of its input ADAE did not complete [producer_not_completed, PROJECT-11]";

#[test]
fn a_run_prints_its_dataset_name_size_and_issue_count() {
    let study = study();
    let token = CancellationToken::new();
    let run = engine()
        .spec_run(&study.path("adam/adsl.yaml"), None, &token)
        .unwrap();
    assert_eq!(run.status(), Status::Succeeded);
    assert_eq!(
        run.summary().to_string(),
        "yamaa run of ADSL: succeeded, 2 rows x 2 columns, 0 issues"
    );
    assert_eq!(run.summary().first_issue, None);

    let run = engine()
        .spec_run(&study.path("adam/adae.yaml"), None, &token)
        .unwrap();
    assert_eq!(
        run.summary().to_string(),
        format!("yamaa run of ADAE: failed, no output, 1 issue\nfirst issue: {FORWARD}")
    );
}

#[test]
fn a_run_whose_file_was_not_read_names_the_path_as_given() {
    let study = study();
    let given = study.path("adam/missing.yaml");
    let run = engine()
        .spec_run(&given, None, &CancellationToken::new())
        .unwrap();
    let summary = run.summary();
    assert_eq!(
        summary.line,
        format!("yamaa run of {given}: failed, no output, 1 issue")
    );
    let first = summary.first_issue.expect("the missing file is an issue");
    assert_eq!(
        first,
        format!("first issue: {}", run.diagnostics()[0].text())
    );
}

#[test]
fn a_check_prints_the_path_as_given_and_no_size() {
    let study = study();
    let token = CancellationToken::new();
    let given = study.path("adam/adae.yaml");
    let check = engine().spec_check(&given, None, &token).unwrap();
    assert_eq!(check.status(), Status::Failed);
    assert_eq!(
        check.summary().to_string(),
        format!("yamaa check of {given}: failed, 1 issue\nfirst issue: {FORWARD}")
    );
    let check = engine()
        .spec_check(&study.path("adam/adsl.yaml"), None, &token)
        .unwrap();
    assert_eq!(
        check.summary().to_string(),
        format!(
            "yamaa check of {}: succeeded, 0 issues",
            study.path("adam/adsl.yaml")
        )
    );
    let project = study.path("project.yaml");
    let check = engine().project_check(&project, &token).unwrap();
    assert_eq!(
        check.summary().to_string(),
        format!("yamaa check of {project}: failed, 1 issue\nfirst issue: {FORWARD}")
    );
}

/// The project run example of `api.md`, with this study's names and sizes.
#[test]
fn a_project_run_lists_its_members_between_the_two_lines() {
    let study = study();
    let project = study.path("project.yaml");
    let runs = engine()
        .project_run(&project, Some("adam"), &CancellationToken::new())
        .unwrap();
    assert_eq!(runs.names(), ["ADSL", "ADAE", "ADTTE"]);
    let summary = runs.summary();
    assert_eq!(
        summary.to_string(),
        format!(
            "yamaa project run of {project}, section adam: failed, 3 specifications, 2 issues\n\
             \x20 ADSL   succeeded  2 rows x 2 columns, 0 issues\n\
             \x20 ADAE   failed     no output, 1 issue\n\
             \x20 ADTTE  skipped    no output, 1 issue\n\
             first issue: {FORWARD}"
        )
    );
    assert_eq!(summary.members.len(), 3);
    assert_eq!(summary.lines().count(), 5);
    // A member prints as a run, under its name.
    let member = runs.member("ADTTE").unwrap();
    assert_eq!(member.status(), Status::Skipped);
    assert_eq!(
        member.summary().to_string(),
        format!("yamaa run of ADTTE: skipped, no output, 1 issue\nfirst issue: {SKIPPED}")
    );
    assert_eq!(
        runs.member("ADSL").unwrap().summary().to_string(),
        "yamaa run of ADSL: succeeded, 2 rows x 2 columns, 0 issues"
    );
    // The command line prints every issue after the member lines.
    let lines = runs
        .diagnostics()
        .iter()
        .map(yamaa::Diagnostic::text)
        .collect::<Vec<_>>();
    assert_eq!(lines, [FORWARD, SKIPPED]);
}

#[test]
fn a_project_run_without_a_section_names_only_the_path() {
    let study = study();
    let project = study.path("project.yaml");
    let runs = engine()
        .project_run(&project, None, &CancellationToken::new())
        .unwrap();
    assert!(runs.summary().line.starts_with(&format!(
        "yamaa project run of {project}: failed, 3 specifications"
    )));
}

/// D39: the call that refused, the first issue's line and the count of the
/// others; the `api.md` example, without its location.
#[test]
fn error_text_carries_the_first_issue_and_the_count_of_the_others() {
    let study = study();
    let three = ADAE
        .replace(
            "  - {name: TRTSDT, type: int",
            "  - {name: AENDY, type: int, label: End Day, derivation: {compute: {expr: TRTEDT + 1}}}\n  - {name: ADURN, type: int, label: Duration, derivation: {compute: {expr: TRTEDT - 1}}}\n  - {name: TRTSDT, type: int",
        )
        .replace(
            "derivation: AE.TRTSDT}\n",
            "derivation: AE.TRTSDT}\n  - {name: TRTEDT, type: int, label: End, derivation: AE.TRTSDT}\n",
        )
        .replace(
            "columns: [USUBJID, ASTDY, TRTSDT]",
            "columns: [USUBJID, ASTDY, AENDY, ADURN, TRTSDT, TRTEDT]",
        );
    study.write("adam/adae.yaml", three);
    let mut run = engine()
        .spec_run(
            &study.path("adam/adae.yaml"),
            None,
            &CancellationToken::new(),
        )
        .unwrap();
    assert_eq!(run.diagnostics().len(), 3, "{:?}", run.diagnostics());
    let failure = run.save().unwrap_err();
    assert_eq!(
        failure.message,
        format!("cannot save ADAE: {FORWARD} (and 2 more issues)")
    );
    assert_eq!(failure.diagnostics.len(), 3);
    assert_eq!(failure.issues.rows(), 3);
    assert_eq!(failure.to_string(), failure.message);
}

#[test]
fn every_refusal_names_its_call() {
    let study = study();
    let engine = engine();
    let token = CancellationToken::new();
    let project = study.path("project.yaml");
    let unknown =
        "the project has no specification named ADXX [unknown_specification_name, PROJECT-6]";
    // An unknown name, of the project and of a project run.
    let failure = engine.project_spec(&project, "ADXX").unwrap_err();
    assert_eq!(failure.message, format!("cannot select ADXX: {unknown}"));
    let runs = engine.project_run(&project, None, &token).unwrap();
    let failure = runs.member("ADXX").err().unwrap();
    assert_eq!(failure.message, format!("cannot select ADXX: {unknown}"));
    // A save with nothing to write: the subject names the section.
    let mut send = engine.project_run(&project, Some("send"), &token).unwrap();
    assert_eq!(
        send.summary().to_string(),
        format!(
            "yamaa project run of {project}, section send: succeeded, 0 specifications, 0 issues"
        )
    );
    let failure = send.save().unwrap_err();
    assert_eq!(
        failure.message,
        format!("cannot save {project}, section send: there is nothing to save [nothing_to_save, PROJECT-12]")
    );
    let mut submission = engine
        .project_submission(&project, None, None, "0.4.0", &token)
        .unwrap();
    assert_eq!(
        submission.summary().to_string(),
        format!("yamaa submission of {project}: succeeded, 0 define.xml and 0 transport files, 0 issues")
    );
    let failure = submission.save().unwrap_err();
    assert_eq!(
        failure.message,
        format!("cannot save {project}: there is nothing to save [nothing_to_save, PROJECT-13]")
    );
    // An unknown section refuses the call; a host names the section.
    let Err(error @ Error::Rejected(_)) = engine.project_run(&project, Some("sdtn"), &token) else {
        panic!("an unknown section refuses the run");
    };
    let failure = Failure::from_error(Action::Select("sdtn"), &error);
    assert!(
        failure.message.starts_with("cannot select sdtn: ")
            && failure.message.ends_with(" [engine_rejected]"),
        "{}",
        failure.message
    );
    // A refusal with no issue keeps the error's own text after the prefix.
    let failure = Failure::from_error(Action::Read("edc/odm.xml"), &Error::Cancelled);
    assert_eq!(
        failure.message,
        "cannot read edc/odm.xml: operation cancelled"
    );
    assert_eq!(failure.issues.rows(), 0);
    // A resolve names the path as given.
    let given = study.path("adam/missing.yaml");
    let failure = engine.spec_resolve(&given, None).unwrap_err();
    assert!(
        failure
            .message
            .starts_with(&format!("cannot resolve {given}: ")),
        "{}",
        failure.message
    );
}

#[test]
fn a_submission_counts_its_define_and_transport_files() {
    let study = Study::new();
    let fixture =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../benchmarks/sdtm-dm-metadata");
    for name in ["spec.yaml", "project.yaml", "input/dm_raw.csv"] {
        study.write(name, std::fs::read(fixture.join(name)).unwrap());
    }
    study.write(
        "dm.csv",
        std::fs::read(fixture.join("expected/dm.csv")).unwrap(),
    );
    let project = study.path("project.yaml");
    let submission = engine()
        .project_submission(
            &project,
            Some("sdtm"),
            Some("2026-09-11T00:00:00"),
            "0.4.0",
            &CancellationToken::new(),
        )
        .unwrap();
    assert_eq!(
        submission.summary().to_string(),
        format!("yamaa submission of {project}, section sdtm: succeeded, 1 define.xml and 1 transport file, 0 issues")
    );
}

/// `--format json` writes an issue as an object whose `context` is an
/// object, with the issues table's twelve keys (`api.md`, "The command
/// line").
#[test]
fn an_issue_as_json_has_the_twelve_keys_and_an_object_context() {
    let study = study();
    let run = engine()
        .spec_run(
            &study.path("adam/adae.yaml"),
            None,
            &CancellationToken::new(),
        )
        .unwrap();
    let diagnostics = run.diagnostics();
    let value = diagnostics[0].to_value();
    let ContextValue::Map(members) = &value else {
        panic!("an issue is an object");
    };
    assert!(matches!(members["context"], ContextValue::Map(_)));
    assert_eq!(
        value.to_json(),
        format!(
            r#"{{"column":null,"condition":"forward_reference","context":{{"column":"ASTDY","dependency":"TRTSDT"}},"file":null,"line":null,"message":"column ASTDY reads TRTSDT, which is declared after it","operand_route":{},"phase":"validation","rule":"COLUMN-5","severity":"error","source_span":{},"spec_paths":["columns.ASTDY.derivation.compute"]}}"#,
            members["operand_route"].to_json(),
            members["source_span"].to_json(),
        )
    );
}

#[test]
fn lazy_objects_print_without_reading() {
    assert_eq!(
        Summary::spec("adam/missing.yaml").to_string(),
        "yamaa spec adam/missing.yaml"
    );
    assert_eq!(
        Summary::project("project.yaml").to_string(),
        "yamaa project project.yaml"
    );
}
