//! The `yamaa` binary as a user runs it: what it prints, on which stream, and
//! its exit status (#3030 section 2.8, D12, D41, D68, D96 to D99, D103).
//!
//! What the command line composes is written out here: the summary and
//! member lines, the order of the lines, the written paths, the JSON keys and
//! the exit statuses. An issue's line and object are the facade's words, so
//! each test takes them from the facade, run from the binary's own directory
//! with the same absolute paths; issue locations (#3262) and message text
//! can then change without an edit here.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

use yamaa::{CancellationToken, Diagnostic, Engine, EngineOptions, Host, HostLanguage};

const HINT: &str = "run 'yamaa --help' for usage\n";

/// Run the binary with `arguments` in `directory`.
fn yamaa_in(directory: &Path, arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_yamaa"))
        .args(arguments)
        .current_dir(directory)
        .output()
        .expect("the yamaa binary starts")
}

/// Run the binary with `arguments` in this test's own directory, where the
/// facade calls of the test run too.
fn yamaa(arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_yamaa"))
        .args(arguments)
        .output()
        .expect("the yamaa binary starts")
}

/// The exit status, standard output and standard error of one run.
fn outcome(output: &Output) -> (Option<i32>, String, String) {
    let text = |bytes: &[u8]| String::from_utf8(bytes.to_vec()).expect("UTF-8 output");
    (
        output.status.code(),
        text(&output.stdout),
        text(&output.stderr),
    )
}

/// A run that succeeded or found something prints on standard output alone.
fn printed(status: i32, stdout: String) -> (Option<i32>, String, String) {
    (Some(status), stdout, String::new())
}

/// The engine the binary configures: the local files and no project
/// functions (D110).
fn engine() -> Engine {
    Engine::new(EngineOptions::new(
        HostLanguage::default(),
        Host::Cli.as_str().to_owned(),
        String::new(),
    ))
}

/// Each issue's line, as the facade prints it from this directory.
fn lines(issues: &[Diagnostic]) -> String {
    issues.iter().map(|issue| issue.text() + "\n").collect()
}

/// The issues as a JSON array of the facade's issue objects.
fn json(issues: &[Diagnostic]) -> String {
    let objects = issues
        .iter()
        .map(|issue| issue.to_value().to_json())
        .collect::<Vec<_>>();
    format!("[{}]", objects.join(","))
}

static NEXT: AtomicUsize = AtomicUsize::new(0);

/// A fresh directory of this test's own, canonical and without a Windows
/// verbatim prefix, so its files are spelled as the engine spells the files
/// it writes (D38).
struct Study(PathBuf);

/// DM builds; ADAE fails its own check: ASTDY reads TRTSDT, which is
/// declared after it (COLUMN-5); ADTTE reads ADAE through its `schema` link,
/// so it is skipped when ADAE fails (PROJECT-11).
const DM: &str = "schema_version: '1.0'\ndomain: DM\nkeys: [USUBJID]\ninput: {RAW: {path: ../raw/dm.csv, types: {AGE: int}}}\ncolumns:\n  - {name: USUBJID, type: str, label: Subject, derivation: RAW.USUBJID}\n  - {name: AGE, type: int, label: Age, derivation: RAW.AGE}\noutput: {path: ../out/dm.csv, columns: [USUBJID, AGE], report: ../out/dm-report.csv}\n";
const ADAE: &str = "schema_version: '1.0'\ndomain: ADAE\nkeys: [USUBJID]\ninput: {AE: {path: ../raw/ae.csv, types: {TRTSDT: int}}}\ncolumns:\n  - {name: USUBJID, type: str, label: Subject, derivation: AE.USUBJID}\n  - {name: ASTDY, type: int, label: Study Day, derivation: {compute: {expr: TRTSDT + 1}}}\n  - {name: TRTSDT, type: int, label: Start, derivation: AE.TRTSDT}\noutput: {path: ../out/adae.csv, columns: [USUBJID, ASTDY, TRTSDT], report: ../out/adae-report.csv}\n";
const ADTTE: &str = "schema_version: '1.0'\ndomain: ADTTE\nkeys: [USUBJID]\ninput: {ADAE: {path: ../out/adae.csv, schema: adae.yaml}}\ncolumns:\n  - {name: USUBJID, type: str, label: Subject, derivation: ADAE.USUBJID}\noutput: {path: ../out/adtte.csv, columns: [USUBJID]}\n";
const PROJECT: &str =
    "schema_version: '1.0'\nsdtm:\n  specs: [sdtm/dm.yaml]\nadam:\n  specs: [adam/adae.yaml, adam/adtte.yaml]\n";

impl Study {
    fn empty() -> Self {
        let directory = std::env::temp_dir().join(format!(
            "yamaa-cli-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).unwrap();
        let canonical = std::fs::canonicalize(&directory).unwrap();
        let text = canonical.to_str().unwrap();
        Self(PathBuf::from(text.strip_prefix(r"\\?\").unwrap_or(text)))
    }

    fn new() -> Self {
        let study = Self::empty();
        study.write("raw/dm.csv", "USUBJID,AGE\nS1,30\nS2,41\n");
        study.write("raw/ae.csv", "USUBJID,TRTSDT\nS1,3\nS2,4\n");
        study.write("sdtm/dm.yaml", DM);
        study.write("adam/adae.yaml", ADAE);
        study.write("adam/adtte.yaml", ADTTE);
        study.write("project.yaml", PROJECT);
        std::fs::create_dir(study.0.join("out")).unwrap();
        study
    }

    fn write(&self, name: &str, contents: impl AsRef<[u8]>) {
        let path = self.0.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, contents).unwrap();
    }

    /// A file of the study as the engine spells it: absolute, with forward
    /// slashes.
    fn path(&self, name: &str) -> String {
        format!("{}/{name}", self.0.to_str().unwrap().replace('\\', "/"))
    }

    fn exists(&self, name: &str) -> bool {
        self.0.join(name).exists()
    }

    /// The names of the files in `directory`, sorted.
    fn files(&self, directory: &str) -> Vec<String> {
        let mut names = std::fs::read_dir(self.0.join(directory))
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
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

#[test]
fn version_prints_the_engine_information_and_succeeds() {
    let expected = format!(
        "yamaa {}\nlanguage versions: 1.0\nfeatures: odm, parquet, readstat, yaml\n\
         host: cli\nplatform: {}-{}\n",
        yamaa::VERSION,
        std::env::consts::OS,
        std::env::consts::ARCH
    );
    for arguments in [
        &["--version"][..],
        &["--format", "text", "--version"],
        &["--version", "--format", "text"],
    ] {
        assert_eq!(
            outcome(&yamaa(arguments)),
            printed(0, expected.clone()),
            "{arguments:?}"
        );
    }
    assert_eq!(
        outcome(&yamaa(&["--version", "--format", "json"])),
        printed(
            0,
            format!(
                "{{\"features\":[\"odm\",\"parquet\",\"readstat\",\"yaml\"],\"host\":\"cli\",\
                 \"host_version\":null,\"language_versions\":[\"1.0\"],\"platform\":\"{}-{}\",\
                 \"version\":\"{}\"}}\n",
                std::env::consts::OS,
                std::env::consts::ARCH,
                yamaa::VERSION
            )
        )
    );
}

#[test]
fn help_prints_every_command_and_wins_over_other_arguments() {
    let output = yamaa(&["--help"]);
    let (status, stdout, stderr) = outcome(&output);
    assert_eq!((status, stderr.as_str()), (Some(0), ""));
    assert!(stdout.starts_with(
        "usage: yamaa [--format text|json] COMMAND [ARGUMENTS]\n       \
         yamaa --version [--format text|json]\n       yamaa --help\n"
    ));
    for synopsis in [
        "  check PATH [--spec NAME]\n",
        "  run PATH [--spec NAME | --section SECTION]\n",
        "  submission PATH [--section SECTION] [--created TIME]\n",
        "  resolve PATH [--spec NAME]\n",
        "  read-odm SOURCE OUT [--archive-member NAME] [--max-archive-members N] \
         [--max-expanded-bytes N] [--max-xml-depth N]\n",
        "  lint [--fix] PATH...\n",
        "  schema TYPE\n",
        "  selftest\n",
    ] {
        assert!(stdout.contains(synopsis), "{synopsis}");
    }
    // Before `--`, `--help` prints the usage whatever else is given.
    for arguments in [
        &["check", "--help"][..],
        &["--bogus", "--help"],
        &["run", "--spec", "--help"],
    ] {
        assert_eq!(yamaa(arguments).stdout, output.stdout, "{arguments:?}");
    }
}

#[test]
fn a_usage_error_exits_with_status_2_and_writes_only_to_standard_error() {
    let cases: &[(&[&str], &str)] = &[
        (&[], "no command given"),
        (&["frobnicate"], "unknown command frobnicate"),
        (&["-h"], "unknown option -h"),
        (&["--bogus"], "unknown option --bogus"),
        (
            &["--spec", "DM", "check", "project.yaml"],
            "--spec must follow its command",
        ),
        (&["--format"], "--format needs a value: text or json"),
        (
            &["--format", "yaml", "--version"],
            "--format takes text or json, not yaml",
        ),
        (
            &["--format", "text", "check", "a.yaml", "--format", "json"],
            "--format is given twice",
        ),
        (&["--version", "--version"], "--version is given twice"),
        (&["--version", "selftest"], "--version takes no command"),
        (&["check"], "check: missing PATH"),
        (
            &["check", "a.yaml", "b.yaml"],
            "check: unexpected argument b.yaml",
        ),
        (
            &["check", "project.yaml", "--section", "sdtm"],
            "check: unknown option --section",
        ),
        (&["check", "project.yaml", "-x"], "check: unknown option -x"),
        (
            &["check", "project.yaml", "--spec"],
            "check: --spec needs a value",
        ),
        (
            &["run", "project.yaml", "--spec", "DM", "--section", "sdtm"],
            "run: give --spec or --section, not both",
        ),
        (
            &["run", "project.yaml", "--spec", "DM", "--spec", "AE"],
            "run: --spec is given twice",
        ),
        (
            &["submission", "project.yaml", "--spec", "DM"],
            "submission: unknown option --spec",
        ),
        (
            &["resolve", "project.yaml", "--fix"],
            "resolve: unknown option --fix",
        ),
        (&["read-odm", "study.xml"], "read-odm: missing OUT"),
        (
            &[
                "read-odm",
                "study.xml",
                "items.csv",
                "--max-xml-depth",
                "deep",
            ],
            "read-odm: --max-xml-depth takes a whole number, not deep",
        ),
        (
            &[
                "read-odm",
                "study.xml",
                "items.csv",
                "--max-archive-members",
                "-1",
            ],
            "read-odm: --max-archive-members takes a whole number, not -1",
        ),
        (
            &[
                "read-odm",
                "study.xml",
                "items.csv",
                "--max-expanded-bytes",
                "18446744073709551616",
            ],
            "read-odm: --max-expanded-bytes takes a whole number, not 18446744073709551616",
        ),
        (&["lint"], "lint: missing PATH"),
        (&["lint", "--fix"], "lint: missing PATH"),
        (
            &["lint", "--fix", "--fix", "a.yaml"],
            "lint: --fix is given twice",
        ),
        (&["schema"], "schema: missing TYPE"),
        (
            &["schema", "spec", "project"],
            "schema: unexpected argument project",
        ),
        (&["selftest", "now"], "selftest: unexpected argument now"),
    ];
    for (arguments, message) in cases {
        assert_eq!(
            outcome(&yamaa(arguments)),
            (Some(2), String::new(), format!("yamaa: {message}\n{HINT}")),
            "{arguments:?}"
        );
    }
}

/// What the command line cannot run yet checks its arguments, then refuses
/// with exit status 2 and writes nothing (D96): a PATH given neither
/// `--spec` nor `--section`, whose `type:` header the facade does not read
/// yet, and `read-odm`, `schema` and `selftest`.
#[test]
fn the_forms_not_implemented_yet_check_their_arguments_then_refuse() {
    let study = Study::new();
    let cases: &[(&[&str], &str)] = &[
        (&["check", "sdtm/dm.yaml"], "check without --spec"),
        (&["check", "project.yaml"], "check without --spec"),
        (
            &["--format", "json", "check", "project.yaml"],
            "check without --spec",
        ),
        (&["run", "sdtm/dm.yaml"], "run without --spec or --section"),
        (
            &["run", "project.yaml", "--format", "json"],
            "run without --spec or --section",
        ),
        (&["resolve", "sdtm/dm.yaml"], "resolve without --spec"),
        (&["read-odm", "study.xml", "items.parquet"], "read-odm"),
        (
            &[
                "read-odm",
                "study.tar.gz",
                "items.csv",
                "--archive-member",
                "study.xml",
                "--max-archive-members",
                "10",
                "--max-expanded-bytes",
                "1024",
                "--max-xml-depth",
                "64",
            ],
            "read-odm",
        ),
        (&["schema", "spec"], "schema"),
        (&["--format", "json", "schema", "project"], "schema"),
        (&["selftest"], "selftest"),
        (&["selftest", "--format", "json"], "selftest"),
    ];
    for (arguments, what) in cases {
        assert_eq!(
            outcome(&yamaa_in(&study.0, arguments)),
            (
                Some(2),
                String::new(),
                format!("yamaa: {what} is not implemented yet\n")
            ),
            "{arguments:?}"
        );
    }
    // Nothing ran, so nothing was written, OUT included.
    assert!(study.files("out").is_empty());
    assert!(!study.exists("items.parquet") && !study.exists("items.csv"));
}

/// From the study's own directory, with the PATH as a user types it: the
/// summary line, then each file the save wrote, the report before the
/// dataset (OUTPUT-10).
#[test]
fn run_of_a_listed_specification_prints_its_summary_and_the_files_it_saved() {
    let study = Study::new();
    let (report, dm) = (study.path("out/dm-report.csv"), study.path("out/dm.csv"));
    assert_eq!(
        outcome(&yamaa_in(
            &study.0,
            &["run", "project.yaml", "--spec", "DM"]
        )),
        printed(
            0,
            format!("yamaa run of DM: succeeded, 2 rows x 2 columns, 0 issues\n{report}\n{dm}\n")
        )
    );
    assert_eq!(study.files("out"), ["dm-report.csv", "dm.csv"]);
    assert_eq!(
        outcome(&yamaa_in(
            &study.0,
            &["--format", "json", "run", "project.yaml", "--spec", "DM"]
        )),
        printed(
            0,
            format!(
                "{{\"command\":\"run\",\"issues\":[],\"members\":[],\"path\":\"project.yaml\",\
                 \"paths\":[\"{report}\",\"{dm}\"],\"section\":null,\"spec\":\"DM\",\
                 \"status\":\"succeeded\"}}\n"
            )
        )
    );
}

/// A failed run saves only its report, then refuses (PROJECT-12): the
/// command prints the run's issue and the report it wrote, and exits 1.
#[test]
fn a_failed_run_prints_its_issues_and_the_report_it_saved() {
    let study = Study::new();
    let project = study.path("project.yaml");
    let engine = engine();
    let adae = engine.project_spec(&project, "ADAE").unwrap();
    let issues = engine
        .spec_run(&adae, Some(&project), &CancellationToken::new())
        .unwrap()
        .diagnostics();
    assert_eq!(issues.len(), 1);
    let report = study.path("out/adae-report.csv");
    assert_eq!(
        outcome(&yamaa(&["run", &project, "--spec", "ADAE"])),
        printed(
            1,
            format!(
                "yamaa run of ADAE: failed, no output, 1 issue\n{}{report}\n",
                lines(&issues)
            )
        )
    );
    assert_eq!(study.files("out"), ["adae-report.csv"]);
    assert_eq!(
        outcome(&yamaa(&[
            "run", &project, "--spec", "ADAE", "--format", "json"
        ])),
        printed(
            1,
            format!(
                "{{\"command\":\"run\",\"issues\":{},\"members\":[],\"path\":\"{project}\",\
                 \"paths\":[\"{report}\"],\"section\":null,\"spec\":\"ADAE\",\
                 \"status\":\"failed\"}}\n",
                json(&issues)
            )
        )
    );
}

/// A section run prints its members between the summary line and the
/// issues (P-PRINT-1 rule 4), then every report it saved.
#[test]
fn a_section_run_prints_its_members_every_issue_and_the_files_it_saved() {
    let study = Study::new();
    let project = study.path("project.yaml");
    let issues = engine()
        .project_run(&project, Some("adam"), &CancellationToken::new())
        .unwrap()
        .diagnostics();
    assert_eq!(issues.len(), 2);
    let report = study.path("out/adae-report.csv");
    assert_eq!(
        outcome(&yamaa(&["run", &project, "--section", "adam"])),
        printed(
            1,
            format!(
                "yamaa project run of {project}, section adam: failed, 2 specifications, 2 issues\n\
                 \x20 ADAE   failed     no output, 1 issue\n\
                 \x20 ADTTE  skipped    no output, 1 issue\n\
                 {}{report}\n",
                lines(&issues)
            )
        )
    );
    assert_eq!(
        outcome(&yamaa(&["--format", "json", "run", &project, "--section", "adam"])),
        printed(
            1,
            format!(
                "{{\"command\":\"run\",\"issues\":{},\"members\":[\
                 {{\"columns\":null,\"issue_count\":1,\"name\":\"ADAE\",\"rows\":null,\"status\":\"failed\"}},\
                 {{\"columns\":null,\"issue_count\":1,\"name\":\"ADTTE\",\"rows\":null,\"status\":\"skipped\"}}],\
                 \"path\":\"{project}\",\"paths\":[\"{report}\"],\"section\":\"adam\",\"spec\":null,\
                 \"status\":\"failed\"}}\n",
                json(&issues)
            )
        )
    );
    let (report, dm) = (study.path("out/dm-report.csv"), study.path("out/dm.csv"));
    assert_eq!(
        outcome(&yamaa(&["run", &project, "--section", "sdtm"])),
        printed(
            0,
            format!(
                "yamaa project run of {project}, section sdtm: succeeded, 1 specification, 0 issues\n\
                 \x20 DM  succeeded  2 rows x 2 columns, 0 issues\n{report}\n{dm}\n"
            )
        )
    );
    assert_eq!(
        outcome(&yamaa(&["run", &project, "--section", "sdtm", "--format", "json"])),
        printed(
            0,
            format!(
                "{{\"command\":\"run\",\"issues\":[],\"members\":[\
                 {{\"columns\":2,\"issue_count\":0,\"name\":\"DM\",\"rows\":2,\"status\":\"succeeded\"}}],\
                 \"path\":\"{project}\",\"paths\":[\"{report}\",\"{dm}\"],\"section\":\"sdtm\",\
                 \"spec\":null,\"status\":\"succeeded\"}}\n"
            )
        )
    );
}

/// The status describes the run, not its save (P-STATUS-1): a scope with
/// nothing to save succeeds, and the save's refusal prints its issue and
/// exits 1.
#[test]
fn a_save_with_nothing_to_write_prints_its_issue_and_exits_1() {
    let study = Study::new();
    let project = study.path("project.yaml");
    let refusal = engine()
        .project_run(&project, Some("send"), &CancellationToken::new())
        .unwrap()
        .save()
        .unwrap_err();
    assert_eq!(refusal.diagnostics.len(), 1);
    assert_eq!(
        outcome(&yamaa(&["run", &project, "--section", "send"])),
        printed(
            1,
            format!(
                "yamaa project run of {project}, section send: succeeded, 0 specifications, \
                 0 issues\n{}",
                lines(&refusal.diagnostics)
            )
        )
    );
    assert_eq!(
        outcome(&yamaa(&[
            "run",
            &project,
            "--section",
            "send",
            "--format",
            "json"
        ])),
        printed(
            1,
            format!(
                "{{\"command\":\"run\",\"issues\":{},\"members\":[],\"path\":\"{project}\",\
                 \"paths\":[],\"section\":\"send\",\"spec\":null,\"status\":\"succeeded\"}}\n",
                json(&refusal.diagnostics)
            )
        )
    );
}

#[test]
fn check_of_a_listed_specification_prints_its_summary_and_every_issue() {
    let study = Study::new();
    let project = study.path("project.yaml");
    let engine = engine();
    let dm = engine.project_spec(&project, "DM").unwrap();
    assert_eq!(
        outcome(&yamaa(&["check", &project, "--spec", "DM"])),
        printed(0, format!("yamaa check of {dm}: succeeded, 0 issues\n"))
    );
    assert_eq!(
        outcome(&yamaa(&[
            "--format", "json", "check", &project, "--spec", "DM"
        ])),
        printed(
            0,
            format!(
                "{{\"command\":\"check\",\"issues\":[],\"path\":\"{project}\",\"spec\":\"DM\",\
                 \"status\":\"succeeded\"}}\n"
            )
        )
    );
    let adae = engine.project_spec(&project, "ADAE").unwrap();
    let issues = engine
        .spec_check(&adae, Some(&project), &CancellationToken::new())
        .unwrap()
        .diagnostics();
    assert_eq!(issues.len(), 1);
    assert_eq!(
        outcome(&yamaa(&["check", &project, "--spec", "ADAE"])),
        printed(
            1,
            format!("yamaa check of {adae}: failed, 1 issue\n{}", lines(&issues))
        )
    );
    assert_eq!(
        outcome(&yamaa(&[
            "check", &project, "--spec", "ADAE", "--format", "json"
        ])),
        printed(
            1,
            format!(
                "{{\"command\":\"check\",\"issues\":{},\"path\":\"{project}\",\"spec\":\"ADAE\",\
                 \"status\":\"failed\"}}\n",
                json(&issues)
            )
        )
    );
    // A check reads no data and writes nothing.
    assert!(study.files("out").is_empty());
}

/// `resolve` prints the resolved specification as canonical JSON in both
/// formats.
#[test]
fn resolve_prints_the_resolved_specification_as_canonical_json() {
    let study = Study::new();
    let project = study.path("project.yaml");
    let engine = engine();
    let dm = engine.project_spec(&project, "DM").unwrap();
    let resolved = engine.spec_resolve(&dm, Some(&project)).unwrap().to_json();
    assert!(
        resolved.starts_with("{\"artifact_version\":1,"),
        "{resolved}"
    );
    for format in ["text", "json"] {
        assert_eq!(
            outcome(&yamaa(&[
                "resolve", &project, "--spec", "DM", "--format", format
            ])),
            printed(0, format!("{resolved}\n")),
            "{format}"
        );
    }
}

/// A refused call prints D39's error text, which carries its first issue,
/// and exits 1 (D39, D98): an unknown name of `--spec`, and an unknown
/// section, which the hosts report as a refusal too.
#[test]
fn a_refused_call_prints_its_error_text_and_exits_1() {
    let study = Study::new();
    let project = study.path("project.yaml");
    let engine = engine();
    let unknown = engine.project_spec(&project, "ADXX").unwrap_err();
    assert_eq!(unknown.diagnostics.len(), 1);
    assert!(unknown.message.starts_with("cannot select ADXX: "));
    let issues = json(&unknown.diagnostics);
    for (command, document) in [
        (
            "check",
            format!(
                "{{\"command\":\"check\",\"issues\":{issues},\"path\":\"{project}\",\
                 \"spec\":\"ADXX\",\"status\":\"failed\"}}"
            ),
        ),
        (
            "run",
            format!(
                "{{\"command\":\"run\",\"issues\":{issues},\"members\":[],\"path\":\"{project}\",\
                 \"paths\":[],\"section\":null,\"spec\":\"ADXX\",\"status\":\"failed\"}}"
            ),
        ),
        (
            "resolve",
            format!(
                "{{\"command\":\"resolve\",\"issues\":{issues},\"path\":\"{project}\",\
                 \"spec\":\"ADXX\",\"status\":\"failed\"}}"
            ),
        ),
    ] {
        assert_eq!(
            outcome(&yamaa(&[command, &project, "--spec", "ADXX"])),
            printed(1, format!("{}\n", unknown.message)),
            "{command}"
        );
        assert_eq!(
            outcome(&yamaa(&[
                command, &project, "--spec", "ADXX", "--format", "json"
            ])),
            printed(1, format!("{document}\n")),
            "{command}"
        );
    }
    let Err(error) = engine.project_run(&project, Some("sdtn"), &CancellationToken::new()) else {
        panic!("an unknown section refuses the run");
    };
    let section = yamaa::Failure::from_error(yamaa::Action::Select("sdtn"), &error);
    assert!(section.message.starts_with("cannot select sdtn: "));
    let others = lines(section.diagnostics.get(1..).unwrap_or_default());
    let issues = json(&section.diagnostics);
    for (command, document) in [
        (
            "run",
            format!(
                "{{\"command\":\"run\",\"issues\":{issues},\"members\":[],\"path\":\"{project}\",\
                 \"paths\":[],\"section\":\"sdtn\",\"spec\":null,\"status\":\"failed\"}}"
            ),
        ),
        (
            "submission",
            format!(
                "{{\"command\":\"submission\",\"issues\":{issues},\"path\":\"{project}\",\
                 \"paths\":[],\"section\":\"sdtn\",\"status\":\"failed\"}}"
            ),
        ),
    ] {
        assert_eq!(
            outcome(&yamaa(&[command, &project, "--section", "sdtn"])),
            printed(1, format!("{}\n{others}", section.message)),
            "{command}"
        );
        assert_eq!(
            outcome(&yamaa(&[
                command,
                &project,
                "--section",
                "sdtn",
                "--format",
                "json"
            ])),
            printed(1, format!("{document}\n")),
            "{command}"
        );
    }
    assert!(study.files("out").is_empty());
}

/// A submission saves each section's transport files, then its define.xml
/// (JSON-14), and prints them in that order; with the creation time given,
/// they hold the bytes the benchmark's authors wrote.
#[test]
fn a_submission_prints_its_summary_and_the_files_in_publication_order() {
    let study = Study::empty();
    let fixture =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../benchmarks/sdtm-dm-metadata");
    for name in ["spec.yaml", "project.yaml", "input/dm_raw.csv"] {
        study.write(name, std::fs::read(fixture.join(name)).unwrap());
    }
    let project = study.path("project.yaml");
    // The submission reads the saved DM dataset, so DM runs first.
    assert_eq!(
        outcome(&yamaa(&["run", &project, "--spec", "DM"])).0,
        Some(0)
    );
    let (transport, define) = (study.path("dm.json"), study.path("define.xml"));
    let arguments = [
        "submission",
        &project,
        "--section",
        "sdtm",
        "--created",
        "2026-09-11T00:00:00",
    ];
    assert_eq!(
        outcome(&yamaa(&arguments)),
        printed(
            0,
            format!(
                "yamaa submission of {project}, section sdtm: succeeded, 1 define.xml and \
                 1 transport file, 0 issues\n{transport}\n{define}\n"
            )
        )
    );
    for name in ["dm.json", "define.xml"] {
        assert_eq!(
            std::fs::read(study.0.join(name)).unwrap(),
            std::fs::read(fixture.join("expected").join(name)).unwrap(),
            "{name}"
        );
    }
    assert_eq!(
        outcome(&yamaa(&[&arguments[..], &["--format", "json"]].concat())),
        printed(
            0,
            format!(
                "{{\"command\":\"submission\",\"issues\":[],\"path\":\"{project}\",\
                 \"paths\":[\"{transport}\",\"{define}\"],\"section\":\"sdtm\",\
                 \"status\":\"succeeded\"}}\n"
            )
        )
    );
}

/// `lint` prints each finding's line, then its summary line (D103), and
/// exits 1 while a finding remains; `--fix` rewrites the file and names it in
/// the JSON `paths`.
#[test]
fn lint_prints_each_finding_then_its_summary() {
    const SPEC: &str = "schema_version: '1.0'\ndomain: DM\noutput:\n  path: dm.csv\n";
    let study = Study::empty();
    study.write("dm.yaml", SPEC);
    let file = study.path("dm.yaml");
    let linted = yamaa::lint(std::slice::from_ref(&file), false).unwrap();
    let [finding] = linted.findings() else {
        panic!("one finding: {:?}", linted.findings());
    };
    assert_eq!(
        outcome(&yamaa(&["lint", &file])),
        printed(
            1,
            format!("{}\n1 lint finding in 1 of 1 file\n", finding.text())
        )
    );
    assert_eq!(
        outcome(&yamaa(&["--format", "json", "lint", &file])),
        printed(
            1,
            format!(
                "{{\"command\":\"lint\",\"findings\":[{}],\"paths\":[],\"status\":\"failed\"}}\n",
                finding.to_value().to_json()
            )
        )
    );
    assert_eq!(
        outcome(&yamaa(&["lint", "--fix", &file])),
        printed(
            0,
            "0 lint findings in 0 of 1 file; fixed 1 file\n".to_owned()
        )
    );
    assert_eq!(
        outcome(&yamaa(&["lint", &file])),
        printed(0, "0 lint findings in 0 of 1 file\n".to_owned())
    );
    study.write("dm.yaml", SPEC);
    assert_eq!(
        outcome(&yamaa(&["lint", &file, "--fix", "--format", "json"])),
        printed(
            0,
            format!(
                "{{\"command\":\"lint\",\"findings\":[],\"paths\":[\"{file}\"],\
                 \"status\":\"succeeded\"}}\n"
            )
        )
    );
    // A file that cannot be read refuses the whole call with the issue that
    // says why.
    let missing = study.path("missing.yaml");
    let refusal = yamaa::lint(std::slice::from_ref(&missing), false).unwrap_err();
    assert_eq!(refusal.diagnostics.len(), 1);
    assert_eq!(
        outcome(&yamaa(&["lint", &file, &missing])),
        printed(1, format!("{}\n", refusal.message))
    );
    assert_eq!(
        outcome(&yamaa(&["lint", &missing, "--format", "json"])),
        printed(
            1,
            format!(
                "{{\"command\":\"lint\",\"findings\":[],\"issues\":{},\"paths\":[],\
                 \"status\":\"failed\"}}\n",
                json(&refusal.diagnostics)
            )
        )
    );
}

/// The facade takes paths and names as UTF-8 text, so a PATH or a value
/// that is not is an argument the command line cannot pass on (D68). A form
/// not implemented yet reads no PATH, so it still refuses as not implemented.
#[cfg(unix)]
#[test]
fn a_value_that_is_not_utf8_exits_with_status_2() {
    use std::os::unix::ffi::OsStrExt;

    let odd = std::ffi::OsStr::from_bytes(b"project-\xff.yaml");
    let run = |arguments: &[&std::ffi::OsStr]| {
        outcome(
            &Command::new(env!("CARGO_BIN_EXE_yamaa"))
                .args(arguments)
                .output()
                .expect("the yamaa binary starts"),
        )
    };
    let text = std::ffi::OsStr::new;
    assert_eq!(
        run(&[text("run"), odd, text("--spec"), text("DM")]),
        (
            Some(2),
            String::new(),
            format!("yamaa: run: PATH is not UTF-8: project-\u{fffd}.yaml\n{HINT}")
        )
    );
    assert_eq!(
        run(&[text("check"), text("project.yaml"), text("--spec"), odd]),
        (
            Some(2),
            String::new(),
            format!("yamaa: check: --spec is not UTF-8: project-\u{fffd}.yaml\n{HINT}")
        )
    );
    assert_eq!(
        run(&[text("lint"), text("dm.yaml"), odd]),
        (
            Some(2),
            String::new(),
            format!("yamaa: lint: PATH is not UTF-8: project-\u{fffd}.yaml\n{HINT}")
        )
    );
    assert_eq!(
        run(&[text("check"), odd]),
        (
            Some(2),
            String::new(),
            "yamaa: check without --spec is not implemented yet\n".to_owned()
        )
    );
}
