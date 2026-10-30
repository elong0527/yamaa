//! Printed summaries, error text and issue JSON through the public engine
//! (D28, D37, D39, P-PRINT-1). Every expected line here was written from
//! `docs/design/interface-1.0/api.md` at tag `yamaa-v0.6.2` ("Printed text"
//! and "The command line") and the message templates of
//! `yaml/conditions.yaml` before the engine ran. Each issue line starts with
//! its location (P-ISSUE-1), counted by hand in the study's files below: the
//! studies sit outside the current directory, so a location names its file by
//! its absolute path.
#![cfg(feature = "yaml")]

use std::path::Path;
use std::sync::Arc;

use yamaa::{
    CancellationToken, ContextValue, Engine, EngineOptions, FailureKind, HostLanguage, Status,
    SubmissionOptions, Summary,
};

use crate::support::NoFunctions;

fn engine() -> Engine {
    Engine::new(
        EngineOptions::new(HostLanguage::Python, "printed-text".into(), "0".into())
            .with_functions(Arc::new(NoFunctions)),
    )
}

struct Study(crate::support::Scratch);
impl Study {
    fn new() -> Self {
        let root = crate::support::Scratch::new("printed-text");
        Self(root)
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

const ADSL: &str = "yamaa: \"1.0\"\ntype: spec\ndomain: ADSL\nkeys: [USUBJID]\ninput: {DM: {path: ../raw/dm.csv, types: {AGE: int}}}\ncolumns:\n  - {name: USUBJID, type: str, label: Subject, derivation: DM.USUBJID}\n  - {name: AGE, type: int, label: Age, derivation: DM.AGE}\noutput: {path: ../out/adsl.csv, columns: [USUBJID, AGE]}\n";
/// ASTDY reads TRTSDT, which is declared after it (COLUMN-5).
const ADAE: &str = "yamaa: \"1.0\"\ntype: spec\ndomain: ADAE\nkeys: [USUBJID]\ninput: {AE: {path: ../raw/ae.csv, types: {TRTSDT: int}}}\ncolumns:\n  - {name: USUBJID, type: str, label: Subject, derivation: AE.USUBJID}\n  - {name: ASTDY, type: int, label: Study Day, derivation: TRTSDT + 1}\n  - {name: TRTSDT, type: int, label: Start, derivation: AE.TRTSDT}\noutput: {path: ../out/adae.csv, columns: [USUBJID, ASTDY, TRTSDT]}\n";
/// Reads ADAE through its `schema` link, so it runs only after ADAE.
const ADTTE: &str = "yamaa: \"1.0\"\ntype: spec\ndomain: ADTTE\nkeys: [USUBJID]\ninput: {ADAE: {path: ../out/adae.csv, schema: adae.yaml}}\ncolumns:\n  - {name: USUBJID, type: str, label: Subject, derivation: ADAE.USUBJID}\noutput: {path: ../out/adtte.csv, columns: [USUBJID]}\n";
const PROJECT: &str =
    "yamaa: \"1.0\"\ntype: project\nadam:\n  specs: [adam/adsl.yaml, adam/adae.yaml, adam/adtte.yaml]\n";

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

/// A file of the study as a location names it: absolute, with forward
/// slashes.
fn located(study: &Study, name: &str) -> String {
    study.path(name).replace('\\', "/")
}

/// ADAE's issue, at `columns.ASTDY.derivation`: the `derivation` key of
/// line 8, after `  - {name: ASTDY, type: int, label: Study Day, `
/// (47 characters).
fn forward(study: &Study) -> String {
    format!(
        "{}:8:48: column ASTDY reads TRTSDT, which is declared after it [forward_reference, COLUMN-5]",
        located(study, "adam/adae.yaml")
    )
}

/// ADTTE's issue, at `input.ADAE.schema`: the `schema` key of line 5, after
/// `input: {ADAE: {path: ../out/adae.csv, ` (38 characters).
fn skipped(study: &Study) -> String {
    format!(
        "{}:5:39: this specification did not run because the producer of its input ADAE did not complete [producer_not_completed, PROJECT-11]",
        located(study, "adam/adtte.yaml")
    )
}

#[test]
fn a_run_prints_its_dataset_name_size_and_issue_count() {
    let study = study();
    let token = CancellationToken::new();
    let run = engine()
        .spec(&study.path("adam/adsl.yaml"))
        .cancel_with(&token)
        .run()
        .unwrap();
    assert_eq!(run.status(), Status::Succeeded);
    assert_eq!(
        run.summary().to_string(),
        "yamaa run of ADSL: succeeded, 2 rows x 2 columns, 0 issues"
    );

    let run = engine()
        .spec(&study.path("adam/adae.yaml"))
        .cancel_with(&token)
        .run()
        .unwrap();
    assert_eq!(
        run.summary().to_string(),
        format!(
            "yamaa run of ADAE: failed, no output, 1 issue\nfirst issue: {}",
            forward(&study)
        )
    );
}

#[test]
fn a_run_whose_file_was_not_read_names_the_path_as_given() {
    let study = study();
    let given = study.path("adam/missing.yaml");
    let run = engine().spec(&given).run().unwrap();
    assert_eq!(
        run.summary().to_string(),
        format!(
            "yamaa run of {given}: failed, no output, 1 issue\nfirst issue: {}",
            run.diagnostics()[0].text()
        )
    );
}

#[test]
fn a_check_prints_the_path_as_given_and_no_size() {
    let study = study();
    let token = CancellationToken::new();
    let given = study.path("adam/adae.yaml");
    let check = engine().spec(&given).cancel_with(&token).check().unwrap();
    assert_eq!(check.status(), Status::Failed);
    assert_eq!(
        check.summary().to_string(),
        format!(
            "yamaa check of {given}: failed, 1 issue\nfirst issue: {}",
            forward(&study)
        )
    );
    let check = engine()
        .spec(&study.path("adam/adsl.yaml"))
        .cancel_with(&token)
        .check()
        .unwrap();
    assert_eq!(
        check.summary().to_string(),
        format!(
            "yamaa check of {}: succeeded, 0 issues",
            study.path("adam/adsl.yaml")
        )
    );
    let project = study.path("project.yaml");
    let check = engine()
        .project(&project)
        .cancel_with(&token)
        .check()
        .unwrap();
    assert_eq!(
        check.summary().to_string(),
        format!(
            "yamaa check of {project}: failed, 1 issue\nfirst issue: {}",
            forward(&study)
        )
    );
}

/// The project run example of `api.md`, with this study's names and sizes.
#[test]
fn a_project_run_lists_its_members_between_the_two_lines() {
    let study = study();
    let project = study.path("project.yaml");
    let runs = engine().project(&project).section("adam").run().unwrap();
    assert_eq!(runs.names(), ["ADSL", "ADAE", "ADTTE"]);
    let summary = runs.summary();
    assert_eq!(
        summary.to_string(),
        format!(
            "yamaa project run of {project}, section adam: failed, 3 specifications, 2 issues\n\
             \x20 ADSL   succeeded  2 rows x 2 columns, 0 issues\n\
             \x20 ADAE   failed     no output, 1 issue\n\
             \x20 ADTTE  skipped    no output, 1 issue\n\
             first issue: {}",
            forward(&study)
        )
    );
    assert_eq!(summary.lines().count(), 5);
    // A member prints as a run, under its name.
    let member = runs.member("ADTTE").unwrap();
    assert_eq!(member.status(), Status::Skipped);
    assert_eq!(
        member.summary().to_string(),
        format!(
            "yamaa run of ADTTE: skipped, no output, 1 issue\nfirst issue: {}",
            skipped(&study)
        )
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
    assert_eq!(lines, [forward(&study), skipped(&study)]);
}

/// A listing that could not be read has no name, so the run names its member
/// by the path the project file lists, as written (D130): in its member line,
/// in `members()` and in its own summary. Only a listed name selects it.
#[test]
fn a_member_whose_listing_was_not_read_is_named_by_the_path_listed() {
    let study = study();
    // The second file is missing; the third path has an empty segment, so
    // the listing itself cannot be located.
    study.write(
        "project.yaml",
        "yamaa: \"1.0\"\ntype: project\nadam:\n  specs: [adam/adsl.yaml, adam/missing.yaml, adam//adae.yaml]\n",
    );
    let project = study.path("project.yaml");
    let runs = engine().project(&project).section("adam").run().unwrap();
    assert_eq!(runs.names(), ["ADSL"]);
    let members = runs
        .members()
        .into_iter()
        .map(|(name, run)| (name, run.status(), run.diagnostics().len()))
        .collect::<Vec<_>>();
    assert_eq!(
        members,
        [
            ("ADSL", Status::Succeeded, 0),
            ("adam/missing.yaml", Status::Failed, 1),
            ("adam//adae.yaml", Status::Failed, 1),
        ]
    );
    // A listed name reaches the same run either way.
    assert!(std::ptr::eq(
        runs.members()[0].1,
        runs.member("ADSL").unwrap()
    ));
    assert_eq!(
        runs.summary().to_string(),
        format!(
            "yamaa project run of {project}, section adam: failed, 3 specifications, 2 issues\n\
             \x20 ADSL               succeeded  2 rows x 2 columns, 0 issues\n\
             \x20 adam/missing.yaml  failed     no output, 1 issue\n\
             \x20 adam//adae.yaml    failed     no output, 1 issue\n\
             first issue: {}",
            runs.diagnostics()[0].text()
        )
    );
    let (_, missing) = runs.members()[1];
    assert_eq!(
        missing.summary().to_string(),
        format!(
            "yamaa run of adam/missing.yaml: failed, no output, 1 issue\nfirst issue: {}",
            missing.diagnostics()[0].text()
        )
    );
    let failure = runs.member("adam/missing.yaml").err().unwrap();
    assert_eq!(
        failure.message,
        "cannot select adam/missing.yaml: the project has no specification named adam/missing.yaml [unknown_specification_name, PROJECT-6]"
    );
}

#[test]
fn a_project_run_without_a_section_names_only_the_path() {
    let study = study();
    let project = study.path("project.yaml");
    let runs = engine().project(&project).run().unwrap();
    assert!(runs.summary().to_string().starts_with(&format!(
        "yamaa project run of {project}: failed, 3 specifications"
    )));
}

/// D39: the call that refused, the first issue's line and the count of the
/// others; the `api.md` example, with this study's location.
#[test]
fn error_text_carries_the_first_issue_and_the_count_of_the_others() {
    let study = study();
    let three = ADAE
        .replace(
            "  - {name: TRTSDT, type: int",
            "  - {name: AENDY, type: int, label: End Day, derivation: TRTEDT + 1}\n  - {name: ADURN, type: int, label: Duration, derivation: TRTEDT - 1}\n  - {name: TRTSDT, type: int",
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
    let mut run = engine().spec(&study.path("adam/adae.yaml")).run().unwrap();
    assert_eq!(run.diagnostics().len(), 3, "{:?}", run.diagnostics());
    let failure = run.save(&CancellationToken::new()).unwrap_err();
    assert_eq!(
        failure.message,
        format!("cannot save ADAE: {} (and 2 more issues)", forward(&study))
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
    let failure = engine.project(&project).spec("ADXX").unwrap_err();
    assert_eq!(failure.message, format!("cannot select ADXX: {unknown}"));
    let runs = engine.project(&project).cancel_with(&token).run().unwrap();
    let failure = runs.member("ADXX").err().unwrap();
    assert_eq!(failure.message, format!("cannot select ADXX: {unknown}"));
    // A save with nothing to write: the subject names the section, which
    // lists no specification.
    study.write("project.yaml", format!("{PROJECT}send:\n  specs: []\n"));
    let mut send = engine
        .project(&project)
        .section("send")
        .cancel_with(&token)
        .run()
        .unwrap();
    assert_eq!(
        send.summary().to_string(),
        format!(
            "yamaa project run of {project}, section send: succeeded, 0 specifications, 0 issues"
        )
    );
    let failure = send.save(&CancellationToken::new()).unwrap_err();
    assert_eq!(
        failure.message,
        format!("cannot save {project}, section send: the project run has nothing to save [nothing_to_save, PROJECT-12]")
    );
    let mut submission = engine
        .project(&project)
        .cancel_with(&token)
        .submission(&SubmissionOptions::new("0.4.0"))
        .unwrap();
    assert_eq!(
        submission.summary().to_string(),
        format!("yamaa submission of {project}: succeeded, 0 define.xml and 0 transport files, 0 issues")
    );
    let failure = submission.save(&CancellationToken::new()).unwrap_err();
    assert_eq!(
        failure.message,
        format!("cannot save {project}: the project submission has nothing to save [nothing_to_save, PROJECT-13]")
    );
    // An unknown section refuses the call; a host names the section.
    let Err(failure) = engine
        .project(&project)
        .section("sdtn")
        .cancel_with(&token)
        .run()
    else {
        panic!("an unknown section refuses the run");
    };
    assert!(matches!(failure.kind, FailureKind::Rejected));
    assert_eq!(
        failure.message,
        "cannot select sdtn: the project declares no section sdtn [unknown_section, PROJECT-14]"
    );
    // A refusal with no issue names what failed after the prefix: a check
    // the token already cancelled reads nothing.
    let cancelled = CancellationToken::new();
    cancelled.cancel();
    let Err(failure) = engine.spec("edc/odm.xml").cancel_with(&cancelled).check() else {
        panic!("a cancelled check fails");
    };
    assert!(matches!(failure.kind, FailureKind::Cancelled));
    assert_eq!(
        failure.message,
        "cannot read edc/odm.xml: operation cancelled"
    );
    assert_eq!(failure.issues.rows(), 0);
    // A resolve names the path as given.
    let given = study.path("adam/missing.yaml");
    let failure = engine.spec(&given).resolve().unwrap_err();
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
        .project(&project)
        .section("sdtm")
        .submission(&SubmissionOptions::new("0.4.0").with_created("2026-09-11T00:00:00"))
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
    let run = engine().spec(&study.path("adam/adae.yaml")).run().unwrap();
    let diagnostics = run.diagnostics();
    let value = diagnostics[0].to_value();
    let ContextValue::Map(members) = &value else {
        panic!("an issue is an object");
    };
    assert!(matches!(members["context"], ContextValue::Map(_)));
    assert_eq!(
        value.to_json(),
        format!(
            r#"{{"column":48,"condition":"forward_reference","context":{{"column":"ASTDY","dependency":"TRTSDT"}},"file":{},"line":8,"message":"column ASTDY reads TRTSDT, which is declared after it","operand_route":{},"phase":"validation","rule":"COLUMN-5","severity":"error","source_span":{},"spec_paths":["columns.ASTDY.derivation"]}}"#,
            ContextValue::Text(located(&study, "adam/adae.yaml")).to_json(),
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

// A table's printed preview (D43, P-PRINT-1, #3030 D167). Each preview below
// was written from `api.md`'s `yamaa.Table` form and D167's layout before the
// renderer ran.

/// The output of the specification at `name` in `study`, which builds.
fn output(study: &Study, name: &str) -> yamaa::Table {
    let run = engine().spec(&study.path(name)).run().unwrap();
    assert_eq!(run.status(), Status::Succeeded, "{:?}", run.diagnostics());
    run.output()
        .expect("a run that succeeded has an output")
        .clone()
}

/// `api.md`'s example: three subjects of six columns, the last one's `AGE`
/// and `AGEU` missing.
const DM: &str = "yamaa: \"1.0\"\ntype: spec\ndomain: DM\nkeys: [USUBJID]\ninput:\n  RAW: {path: ../raw/dm.csv, types: {AGE: int}}\noutput: {path: ../out/dm.csv, columns: [STUDYID, DOMAIN, USUBJID, SUBJID, AGE, AGEU]}\ncolumns:\n  - {name: STUDYID, type: str, label: Study Identifier, derivation: RAW.STUDYID}\n  - {name: DOMAIN, type: str, label: Domain Abbreviation, derivation: RAW.DOMAIN}\n  - {name: USUBJID, type: str, label: Unique Subject Identifier, derivation: RAW.USUBJID}\n  - {name: SUBJID, type: str, label: Subject Identifier for the Study, derivation: RAW.SUBJID}\n  - {name: AGE, type: int, label: Age, derivation: RAW.AGE}\n  - {name: AGEU, type: str, label: Age Units, derivation: RAW.AGEU}\n";

#[test]
fn a_table_prints_the_preview_of_api_md() {
    let study = Study::new();
    study.write(
        "raw/dm.csv",
        "STUDYID,DOMAIN,USUBJID,SUBJID,AGE,AGEU\n\
         STUDY01,DM,STUDY01-01,01,34,YEARS\n\
         STUDY01,DM,STUDY01-02,02,51,YEARS\n\
         STUDY01,DM,STUDY01-03,03,,\n",
    );
    study.write("sdtm/dm.yaml", DM);
    assert_eq!(
        output(&study, "sdtm/dm.yaml").preview(),
        "\
yamaa.Table: 3 rows x 6 columns
STUDYID  DOMAIN  USUBJID     SUBJID  AGE   AGEU
str      str     str         str     int   str
STUDY01  DM      STUDY01-01  01      34    YEARS
STUDY01  DM      STUDY01-02  02      51    YEARS
STUDY01  DM      STUDY01-03  03      null  null"
    );
}

/// Twelve subjects of ten columns, every column type among them.
const ADSL_WIDE: &str = "yamaa: \"1.0\"\ntype: spec\ndomain: ADSL\nkeys: [USUBJID]\ninput:\n  DM: {path: ../raw/adsl.csv, types: {AGE: int, HEIGHTCM: float, BRTHDT: date, RFSTDTM: datetime}}\noutput: {path: ../out/adsl.csv, columns: [USUBJID, AGE, HEIGHTCM, BRTHDT, RFSTDTM, DCSREAS, SEX, SAFFL, COUNTRY, SITEID]}\ncolumns:\n  - {name: USUBJID, type: str, label: Unique Subject Identifier, derivation: DM.USUBJID}\n  - {name: AGE, type: int, label: Age, derivation: DM.AGE}\n  - {name: HEIGHTCM, type: float, label: Height in cm, derivation: DM.HEIGHTCM}\n  - {name: BRTHDT, type: date, label: Date of Birth, derivation: DM.BRTHDT}\n  - {name: RFSTDTM, type: datetime, label: Reference Start Datetime, derivation: DM.RFSTDTM}\n  - {name: DCSREAS, type: str, label: Reason for Discontinuation, derivation: DM.DCSREAS}\n  - {name: SEX, type: str, label: Sex, derivation: DM.SEX}\n  - {name: SAFFL, type: str, label: Safety Population Flag, derivation: DM.SAFFL}\n  - {name: COUNTRY, type: str, label: Country, derivation: DM.COUNTRY}\n  - {name: SITEID, type: str, label: Study Site Identifier, derivation: DM.SITEID}\n";

/// More than 10 rows print the first 5, `...` and the last 5; more than 8
/// columns print the first 8, then a `...` column. A float prints VALUE-8's
/// text (`180.0` as `180`), a datetime VALUE-17's (`2013-07-19T10:00` with its
/// seconds), and a cell longer than 20 characters its first 17 and `...`.
#[test]
fn a_long_wide_table_prints_its_first_and_last_rows_of_its_first_columns() {
    let study = Study::new();
    study.write(
        "raw/adsl.csv",
        "USUBJID,AGE,HEIGHTCM,BRTHDT,RFSTDTM,DCSREAS,SEX,SAFFL,COUNTRY,SITEID\n\
         01-701-1015,63,147.3,1950-12-26,2014-01-02T08:30:00,Completed,F,Y,USA,701\n\
         01-701-1023,64,162.6,1949-07-23,2012-08-05T09:15:00,Adverse Event,M,Y,USA,701\n\
         01-701-1028,71,177.8,1942-02-19,2013-07-19T10:00,Completed,M,Y,USA,701\n\
         01-701-1033,74,175.3,1939-04-12,2014-03-18T07:45:00,Withdrawal by Subject,M,Y,USA,701\n\
         01-701-1034,77,154.9,1936-08-04,2014-07-01T08:00:00,Completed,F,Y,USA,701\n\
         01-701-1047,85,148.6,1928-09-02,2013-02-12T13:20:00,Adverse Event,F,Y,USA,701\n\
         01-701-1057,59,166.4,1954-05-07,2013-12-20T09:40:00,Completed,F,Y,USA,701\n\
         01-701-1097,68,168.9,1945-08-21,2014-01-01T08:10:00,Completed,M,Y,USA,701\n\
         01-701-1111,81,158.8,1931-11-23,2012-09-07T11:30:00,Adverse Event,F,Y,USA,701\n\
         01-701-1115,84,180.0,1929-02-05,2012-11-30T09:00:00,Adverse Event,M,Y,USA,701\n\
         01-701-1118,52,,1961-09-29,2014-03-12T08:30:00,,M,N,USA,701\n\
         01-701-1130,84,157.5,1929-12-28,2014-02-13T10:05:00,Completed,F,Y,USA,701\n",
    );
    study.write("adam/adsl.yaml", ADSL_WIDE);
    assert_eq!(
        output(&study, "adam/adsl.yaml").preview(),
        "\
yamaa.Table: 12 rows x 10 columns
USUBJID      AGE  HEIGHTCM  BRTHDT      RFSTDTM              DCSREAS               SEX  SAFFL  ...
str          int  float     date        datetime             str                   str  str    ...
01-701-1015  63   147.3     1950-12-26  2014-01-02T08:30:00  Completed             F    Y      ...
01-701-1023  64   162.6     1949-07-23  2012-08-05T09:15:00  Adverse Event         M    Y      ...
01-701-1028  71   177.8     1942-02-19  2013-07-19T10:00:00  Completed             M    Y      ...
01-701-1033  74   175.3     1939-04-12  2014-03-18T07:45:00  Withdrawal by Sub...  M    Y      ...
01-701-1034  77   154.9     1936-08-04  2014-07-01T08:00:00  Completed             F    Y      ...
...
01-701-1097  68   168.9     1945-08-21  2014-01-01T08:10:00  Completed             M    Y      ...
01-701-1111  81   158.8     1931-11-23  2012-09-07T11:30:00  Adverse Event         F    Y      ...
01-701-1115  84   180       1929-02-05  2012-11-30T09:00:00  Adverse Event         M    Y      ...
01-701-1118  52   null      1961-09-29  2014-03-12T08:30:00  null                  M    N      ...
01-701-1130  84   157.5     1929-12-28  2014-02-13T10:05:00  Completed             F    Y      ..."
    );
}

/// The issues table types its list columns `list<str>` and `list<int>`, and a
/// list prints its items in brackets. `file` holds the issue's absolute path,
/// which the preview cuts at 20 characters: the 17 it keeps, then `...`.
#[test]
fn an_issues_table_prints_its_list_columns() {
    let study = study();
    let token = CancellationToken::new();
    let run = engine()
        .spec(&study.path("adam/adae.yaml"))
        .cancel_with(&token)
        .run()
        .unwrap();
    let path = located(&study, "adam/adae.yaml");
    let file = if path.chars().count() > 20 {
        format!("{}...", path.chars().take(17).collect::<String>())
    } else {
        path
    };
    let width = file.len().max("file".len());
    assert_eq!(
        run.issues().preview(),
        format!(
            "\
yamaa.Table: 1 row x 12 columns
severity  phase       condition          rule      spec_paths            {:<width$}  line  column  ...
str       str         str                str       list<str>             {:<width$}  int   int     ...
error     validation  forward_reference  COLUMN-5  [columns.ASTDY.de...  {file:<width$}  8     48      ...",
            "file", "str"
        )
    );
    // A table without rows prints its names and types.
    let run = engine()
        .spec(&study.path("adam/adsl.yaml"))
        .cancel_with(&token)
        .run()
        .unwrap();
    assert_eq!(
        run.issues().preview(),
        "\
yamaa.Table: 0 rows x 12 columns
severity  phase  condition  rule  spec_paths  file  line  column  ...
str       str    str        str   list<str>   str   int   int     ..."
    );
}

/// One row of one column takes the singular nouns, and a character outside
/// printable ASCII prints as D103's escape.
#[test]
fn a_table_of_one_row_and_column_takes_the_singular() {
    let study = Study::new();
    study.write("raw/sites.csv", "SITENM\nH\u{f4}pital Cochin\n");
    study.write(
        "sdtm/sites.yaml",
        "yamaa: \"1.0\"\ntype: spec\ndomain: SITES\nkeys: [SITENM]\ninput:\n  RAW: ../raw/sites.csv\noutput: {path: ../out/sites.csv, columns: [SITENM]}\ncolumns:\n  - {name: SITENM, type: str, label: Site Name, derivation: RAW.SITENM}\n",
    );
    assert_eq!(
        output(&study, "sdtm/sites.yaml").preview(),
        "yamaa.Table: 1 row x 1 column\nSITENM\nstr\nH\\xf4pital Cochin"
    );
}
