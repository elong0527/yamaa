//! The `yamaa` binary as a user runs it: what it prints, on which stream, and
//! its exit status (#3030 section 2.8, D12, D41, D68, D96 to D99, D103, D122,
//! D125).
//!
//! What the command line composes is written out here: the summary and
//! member lines, the order of the lines, the written paths, the JSON keys and
//! the exit statuses. An issue's line and object are the facade's words, so
//! each test takes them from the facade, run from the binary's own directory
//! with the same absolute paths; issue locations (#3262) and message text
//! can then change without an edit here.
//!
//! The command line reads only 1.x files (D122), so the studies here are
//! written as 1.0.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

use yamaa::{CancellationToken, ContextValue, Diagnostic, Engine, EngineOptions, Host};

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

/// The engine the binary configures: the local files and no host language,
/// so no project functions (D110, D123).
fn engine() -> Engine {
    Engine::new(EngineOptions::neutral(
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
const DM: &str = "yamaa: \"1.0\"\ntype: spec\nmodel: SDTM\ndomain: DM\nkeys: [USUBJID]\ninput: {RAW: {path: ../raw/dm.csv, types: {AGE: int}}}\ncolumns:\n  - {name: USUBJID, type: str, label: Subject, derivation: RAW.USUBJID}\n  - {name: AGE, type: int, label: Age, derivation: RAW.AGE}\noutput: {path: ../out/dm.csv, columns: [USUBJID, AGE], report: ../out/dm-report.csv}\n";
const ADAE: &str = "yamaa: \"1.0\"\ntype: spec\nmodel: ADaM\ndomain: ADAE\nkeys: [USUBJID]\ninput: {AE: {path: ../raw/ae.csv, types: {TRTSDT: int}}}\ncolumns:\n  - {name: USUBJID, type: str, label: Subject, derivation: AE.USUBJID}\n  - {name: ASTDY, type: int, label: Study Day, derivation: TRTSDT + 1}\n  - {name: TRTSDT, type: int, label: Start, derivation: AE.TRTSDT}\noutput: {path: ../out/adae.csv, columns: [USUBJID, ASTDY, TRTSDT], report: ../out/adae-report.csv}\n";
const ADTTE: &str = "yamaa: \"1.0\"\ntype: spec\nmodel: ADaM\ndomain: ADTTE\nkeys: [USUBJID]\ninput: {ADAE: {path: ../out/adae.csv, schema: adae.yaml}}\ncolumns:\n  - {name: USUBJID, type: str, label: Subject, derivation: ADAE.USUBJID}\noutput: {path: ../out/adtte.csv, columns: [USUBJID]}\n";
const PROJECT: &str = "yamaa: \"1.0\"\ntype: project\nsdtm:\n  specs: [sdtm/dm.yaml]\nadam:\n  specs: [adam/adae.yaml, adam/adtte.yaml]\n";

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
/// with exit status 2 and writes nothing (D96): `read-odm`, `schema` and
/// `selftest`.
#[test]
fn the_commands_not_implemented_yet_check_their_arguments_then_refuse() {
    let study = Study::new();
    let cases: &[(&[&str], &str)] = &[
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

/// A PATH that declares `type: spec` is checked, run and resolved alone, as
/// `s.check()`, `s.run()` then `save()`, and `s.resolve()` (D122): here DM,
/// which needs nothing from the project that lists it. From the study's own
/// directory, a check names the path as given.
#[test]
fn a_spec_path_is_checked_run_and_resolved_alone() {
    let study = Study::new();
    assert_eq!(
        outcome(&yamaa_in(&study.0, &["check", "sdtm/dm.yaml"])),
        printed(
            0,
            "yamaa check of sdtm/dm.yaml: succeeded, 0 issues\n".to_owned()
        )
    );
    assert_eq!(
        outcome(&yamaa_in(
            &study.0,
            &["--format", "json", "check", "sdtm/dm.yaml"]
        )),
        printed(
            0,
            "{\"command\":\"check\",\"issues\":[],\"path\":\"sdtm/dm.yaml\",\"spec\":null,\
             \"status\":\"succeeded\"}\n"
                .to_owned()
        )
    );
    assert!(study.files("out").is_empty());
    let (report, dm) = (study.path("out/dm-report.csv"), study.path("out/dm.csv"));
    assert_eq!(
        outcome(&yamaa_in(&study.0, &["run", "sdtm/dm.yaml"])),
        printed(
            0,
            format!("yamaa run of DM: succeeded, 2 rows x 2 columns, 0 issues\n{report}\n{dm}\n")
        )
    );
    assert_eq!(study.files("out"), ["dm-report.csv", "dm.csv"]);
    assert_eq!(
        outcome(&yamaa_in(
            &study.0,
            &["run", "sdtm/dm.yaml", "--format", "json"]
        )),
        printed(
            0,
            format!(
                "{{\"command\":\"run\",\"issues\":[],\"members\":[],\"path\":\"sdtm/dm.yaml\",\
                 \"paths\":[\"{report}\",\"{dm}\"],\"section\":null,\"spec\":null,\
                 \"status\":\"succeeded\"}}\n"
            )
        )
    );
    let spec = study.path("sdtm/dm.yaml");
    let resolved = engine().spec_resolve(&spec, None).unwrap().to_json();
    for format in ["text", "json"] {
        assert_eq!(
            outcome(&yamaa(&["resolve", &spec, "--format", format])),
            printed(0, format!("{resolved}\n")),
            "{format}"
        );
    }
}

/// A PATH that declares `type: project` is checked and run whole, as
/// `p.check()` and `p.run()` then `save()` (D122). The check reports ADAE's
/// finding. The run builds every section in order: DM succeeds, ADAE fails
/// and ADTTE, which reads it, is skipped, so the save writes only the
/// reports of the two that ran (PROJECT-12).
#[test]
fn a_project_path_is_checked_and_run_whole() {
    let study = Study::new();
    let project = study.path("project.yaml");
    let engine = engine();
    let token = CancellationToken::new();
    let found = engine
        .project_check(&project, &token)
        .unwrap()
        .diagnostics();
    assert_eq!(found.len(), 1);
    assert_eq!(
        outcome(&yamaa(&["check", &project])),
        printed(
            1,
            format!(
                "yamaa check of {project}: failed, 1 issue\n{}",
                lines(&found)
            )
        )
    );
    assert_eq!(
        outcome(&yamaa(&["check", &project, "--format", "json"])),
        printed(
            1,
            format!(
                "{{\"command\":\"check\",\"issues\":{},\"path\":\"{project}\",\"spec\":null,\
                 \"status\":\"failed\"}}\n",
                json(&found)
            )
        )
    );
    assert!(study.files("out").is_empty());
    let issues = engine
        .project_run(&project, None, &token)
        .unwrap()
        .diagnostics();
    assert_eq!(issues.len(), 2);
    let (dm, adae) = (
        study.path("out/dm-report.csv"),
        study.path("out/adae-report.csv"),
    );
    assert_eq!(
        outcome(&yamaa(&["run", &project])),
        printed(
            1,
            format!(
                "yamaa project run of {project}: failed, 3 specifications, 2 issues\n\
                 \x20 DM     succeeded  2 rows x 2 columns, 0 issues\n\
                 \x20 ADAE   failed     no output, 1 issue\n\
                 \x20 ADTTE  skipped    no output, 1 issue\n\
                 {}{dm}\n{adae}\n",
                lines(&issues)
            )
        )
    );
    assert_eq!(study.files("out"), ["adae-report.csv", "dm-report.csv"]);
    assert_eq!(
        outcome(&yamaa(&["--format", "json", "run", &project])),
        printed(
            1,
            format!(
                "{{\"command\":\"run\",\"issues\":{},\"members\":[\
                 {{\"columns\":2,\"issue_count\":0,\"name\":\"DM\",\"rows\":2,\"status\":\"succeeded\"}},\
                 {{\"columns\":null,\"issue_count\":1,\"name\":\"ADAE\",\"rows\":null,\"status\":\"failed\"}},\
                 {{\"columns\":null,\"issue_count\":1,\"name\":\"ADTTE\",\"rows\":null,\"status\":\"skipped\"}}],\
                 \"path\":\"{project}\",\"paths\":[\"{dm}\",\"{adae}\"],\"section\":null,\"spec\":null,\
                 \"status\":\"failed\"}}\n",
                json(&issues)
            )
        )
    );
}

/// A PATH whose declared type the command does not take is a usage error: it
/// exits 2, prints nothing on standard output in either format and writes
/// nothing (D68; `api.md`, "The command line"). That is a codelists or
/// function file for any command; a spec given `--spec` or `--section`, or
/// given to `submission`; and a project given to `resolve` without
/// `--spec`, since a project has no resolved form of its own. The facade
/// reads only a file's header to type it, so these files need no body.
#[test]
fn a_path_whose_type_the_command_does_not_take_exits_with_status_2() {
    let study = Study::new();
    study.write("ct.yaml", "yamaa: \"1.0\"\ntype: codelists\n");
    study.write("shout.yaml", "yamaa: \"1.0\"\ntype: function\n");
    let cases: &[(&[&str], &str)] = &[
        (
            &["check", "ct.yaml"],
            "check: ct.yaml declares type codelists, which check does not take",
        ),
        (
            &["run", "ct.yaml", "--section", "sdtm"],
            "run: ct.yaml declares type codelists, which run does not take",
        ),
        (
            &["--format", "json", "resolve", "shout.yaml"],
            "resolve: shout.yaml declares type function, which resolve does not take",
        ),
        (
            &["submission", "shout.yaml"],
            "submission: shout.yaml declares type function, which submission does not take",
        ),
        (
            &["check", "sdtm/dm.yaml", "--spec", "DM"],
            "check: sdtm/dm.yaml declares type spec, which check does not take with --spec",
        ),
        (
            &["run", "sdtm/dm.yaml", "--spec", "DM"],
            "run: sdtm/dm.yaml declares type spec, which run does not take with --spec",
        ),
        (
            &[
                "run",
                "sdtm/dm.yaml",
                "--section",
                "sdtm",
                "--format",
                "json",
            ],
            "run: sdtm/dm.yaml declares type spec, which run does not take with --section",
        ),
        (
            &["resolve", "sdtm/dm.yaml", "--spec", "DM"],
            "resolve: sdtm/dm.yaml declares type spec, which resolve does not take with --spec",
        ),
        (
            &["submission", "sdtm/dm.yaml", "--section", "sdtm"],
            "submission: sdtm/dm.yaml declares type spec, which submission does not take",
        ),
        (
            &["resolve", "project.yaml"],
            "resolve: project.yaml declares type project, which resolve does not take \
             without --spec",
        ),
    ];
    for (arguments, message) in cases {
        assert_eq!(
            outcome(&yamaa_in(&study.0, arguments)),
            (Some(2), String::new(), format!("yamaa: {message}\n{HINT}")),
            "{arguments:?}"
        );
    }
    assert!(study.files("out").is_empty());
}

/// The facade's refusal to read the type of the file at `path`, after
/// checking that it names the call as `cannot read <path>` (D39, D125) and
/// holds one issue, `condition`.
fn unread(path: &str, condition: &str) -> yamaa::Failure {
    let refused = engine().file_type(path).unwrap_err();
    assert!(
        refused
            .message
            .starts_with(&format!("cannot read {path}: ")),
        "{}",
        refused.message
    );
    let [issue] = refused.diagnostics.as_slice() else {
        panic!("one issue: {:?}", refused.diagnostics);
    };
    assert_eq!(issue.code.as_str(), condition, "{path}");
    refused
}

/// A file written in language version 0.5, which declares `schema_version`,
/// declares no type: whatever the command and its flags, the call is refused
/// as `unsupported_language_version` at `schema_version`, naming 0.5, and
/// exits 1 as a file problem the facade reports (D68, D122, D125).
#[test]
fn a_file_written_in_language_version_0_5_is_refused_with_exit_status_1() {
    let study = Study::new();
    study.write(
        "old/dm.yaml",
        "schema_version: '1.0'\ndomain: DM\nkeys: [USUBJID]\ninput: {RAW: ../raw/dm.csv}\noutput: {path: ../out/dm.csv, columns: [USUBJID]}\ncolumns:\n  - {name: USUBJID, type: str, label: Subject, derivation: RAW.USUBJID}\n",
    );
    study.write(
        "old/project.yaml",
        "schema_version: '1.0'\nsdtm:\n  specs: [dm.yaml]\n",
    );
    let (spec, project) = (study.path("old/dm.yaml"), study.path("old/project.yaml"));
    let refused = unread(&spec, "unsupported_language_version");
    assert_eq!(
        refused.diagnostics[0].context.get("version"),
        Some(&ContextValue::Text("0.5".to_owned()))
    );
    for arguments in [&["check", &spec][..], &["run", &spec], &["resolve", &spec]] {
        assert_eq!(
            outcome(&yamaa(arguments)),
            printed(1, format!("{}\n", refused.message)),
            "{arguments:?}"
        );
    }
    assert_eq!(
        outcome(&yamaa(&["--format", "json", "run", &spec])),
        printed(
            1,
            format!(
                "{{\"command\":\"run\",\"issues\":{},\"members\":[],\"path\":\"{spec}\",\
                 \"paths\":[],\"section\":null,\"spec\":null,\"status\":\"failed\"}}\n",
                json(&refused.diagnostics)
            )
        )
    );
    // A project's flags change nothing: its type is read first.
    let refused = unread(&project, "unsupported_language_version");
    for arguments in [
        &["check", &project, "--spec", "DM"][..],
        &["run", &project, "--section", "sdtm"],
        &["submission", &project],
        &["resolve", &project, "--spec", "DM"],
    ] {
        assert_eq!(
            outcome(&yamaa(arguments)),
            printed(1, format!("{}\n", refused.message)),
            "{arguments:?}"
        );
    }
    assert_eq!(
        outcome(&yamaa(&[
            "check", &project, "--spec", "DM", "--format", "json"
        ])),
        printed(
            1,
            format!(
                "{{\"command\":\"check\",\"issues\":{},\"path\":\"{project}\",\"spec\":\"DM\",\
                 \"status\":\"failed\"}}\n",
                json(&refused.diagnostics)
            )
        )
    );
    assert!(study.files("out").is_empty());
}

/// A `type` naming none of the four file types is refused as
/// `unexpected_file_type` at `type`, and exits 1 (D125).
#[test]
fn a_type_naming_no_file_type_is_refused_with_exit_status_1() {
    let study = Study::empty();
    study.write(
        "dm.yaml",
        "yamaa: \"1.0\"\ntype: specification\ndomain: DM\n",
    );
    let path = study.path("dm.yaml");
    let refused = unread(&path, "unexpected_file_type");
    for command in ["check", "run", "submission", "resolve"] {
        assert_eq!(
            outcome(&yamaa(&[command, &path])),
            printed(1, format!("{}\n", refused.message)),
            "{command}"
        );
    }
    assert_eq!(
        outcome(&yamaa(&["resolve", &path, "--format", "json"])),
        printed(
            1,
            format!(
                "{{\"command\":\"resolve\",\"issues\":{},\"path\":\"{path}\",\"spec\":null,\
                 \"status\":\"failed\"}}\n",
                json(&refused.diagnostics)
            )
        )
    );
}

/// A PATH that reaches no regular file, or reaches a link, is refused as any
/// read refuses it, naming the path as given, and exits 1 as a file problem
/// the facade reports (FILE-7, FILE-8, FILE-11, D68, D125): here a file that
/// does not exist, a directory and, on Unix, a symbolic link.
#[test]
fn a_path_that_cannot_be_read_is_refused_with_exit_status_1() {
    let study = Study::new();
    let cases = vec![
        (study.path("missing.yaml"), "resource_path_missing"),
        (study.path("sdtm"), "resource_path_not_regular_file"),
    ];
    #[cfg(unix)]
    let cases = {
        std::os::unix::fs::symlink(study.0.join("project.yaml"), study.0.join("link.yaml"))
            .unwrap();
        [
            cases,
            vec![(study.path("link.yaml"), "resource_path_symlink")],
        ]
        .concat()
    };
    for (path, condition) in cases {
        let refused = unread(&path, condition);
        for arguments in [
            &["check", &path][..],
            &["run", &path, "--spec", "DM"],
            &["submission", &path],
            &["resolve", &path],
        ] {
            assert_eq!(
                outcome(&yamaa(arguments)),
                printed(1, format!("{}\n", refused.message)),
                "{arguments:?}"
            );
        }
        assert_eq!(
            outcome(&yamaa(&["--format", "json", "submission", &path])),
            printed(
                1,
                format!(
                    "{{\"command\":\"submission\",\"created\":null,\"issues\":{},\
                     \"path\":\"{path}\",\"paths\":[],\"section\":null,\"status\":\"failed\"}}\n",
                    json(&refused.diagnostics)
                )
            ),
            "{path}"
        );
    }
    assert!(study.files("out").is_empty());
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

/// A listing that cannot be read has no listed name, so its member is named
/// by the listing's path as the project file writes it, in its member line
/// and in the JSON `members` alike (D130). DM still runs, and the failed
/// run saves only DM's report (PROJECT-12).
#[test]
fn a_member_whose_listing_cannot_be_read_is_named_by_its_path() {
    let study = Study::new();
    study.write(
        "project.yaml",
        PROJECT.replace("[sdtm/dm.yaml]", "[sdtm/dm.yaml, sdtm/missing.yaml]"),
    );
    let project = study.path("project.yaml");
    let issues = engine()
        .project_run(&project, Some("sdtm"), &CancellationToken::new())
        .unwrap()
        .diagnostics();
    assert_eq!(issues.len(), 1);
    let report = study.path("out/dm-report.csv");
    assert_eq!(
        outcome(&yamaa(&["run", &project, "--section", "sdtm"])),
        printed(
            1,
            format!(
                "yamaa project run of {project}, section sdtm: failed, 2 specifications, 1 issue\n\
                 \x20 DM                 succeeded  2 rows x 2 columns, 0 issues\n\
                 \x20 sdtm/missing.yaml  failed     no output, 1 issue\n\
                 {}{report}\n",
                lines(&issues)
            )
        )
    );
    assert_eq!(
        outcome(&yamaa(&["--format", "json", "run", &project, "--section", "sdtm"])),
        printed(
            1,
            format!(
                "{{\"command\":\"run\",\"issues\":{},\"members\":[\
                 {{\"columns\":2,\"issue_count\":0,\"name\":\"DM\",\"rows\":2,\"status\":\"succeeded\"}},\
                 {{\"columns\":null,\"issue_count\":1,\"name\":\"sdtm/missing.yaml\",\"rows\":null,\
                 \"status\":\"failed\"}}],\
                 \"path\":\"{project}\",\"paths\":[\"{report}\"],\"section\":\"sdtm\",\"spec\":null,\
                 \"status\":\"failed\"}}\n",
                json(&issues)
            )
        )
    );
    assert_eq!(study.files("out"), ["dm-report.csv"]);
}

/// The status describes the run, not its save (P-STATUS-1): a scope with
/// nothing to save succeeds, and the save's refusal prints its issue and
/// exits 1.
#[test]
fn a_save_with_nothing_to_write_prints_its_issue_and_exits_1() {
    let study = Study::new();
    // The `send` section lists no specification, so its scope is empty.
    study.write("project.yaml", format!("{PROJECT}send:\n  specs: []\n"));
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

/// A save that cannot write a target stops before it writes any file
/// (D99). Here DM's files go to a directory that does not exist, which every
/// platform refuses. The run succeeded, so its status stays `succeeded`;
/// the save's issue joins the run's issues and prints once, with no path,
/// and the command exits 1.
#[test]
fn a_save_that_cannot_write_its_target_prints_why_and_exits_1() {
    let study = Study::new();
    study.write("sdtm/dm.yaml", DM.replace("../out/", "../missing/"));
    let project = study.path("project.yaml");
    let engine = engine();
    let dm = engine.project_spec(&project, "DM").unwrap();
    let mut run = engine
        .spec_run(&dm, Some(&project), &CancellationToken::new())
        .unwrap();
    assert!(run.diagnostics().is_empty());
    let refusal = run.save().unwrap_err();
    assert!(refusal.paths.is_empty());
    let issues = run.diagnostics();
    assert_eq!(issues.len(), 1, "{issues:?}");
    assert_eq!(issues, refusal.diagnostics);
    assert_eq!(
        outcome(&yamaa(&["run", &project, "--spec", "DM"])),
        printed(
            1,
            format!(
                "yamaa run of DM: succeeded, 2 rows x 2 columns, 1 issue\n{}",
                lines(&issues)
            )
        )
    );
    assert_eq!(
        outcome(&yamaa(&[
            "run", &project, "--spec", "DM", "--format", "json"
        ])),
        printed(
            1,
            format!(
                "{{\"command\":\"run\",\"issues\":{},\"members\":[],\"path\":\"{project}\",\
                 \"paths\":[],\"section\":null,\"spec\":\"DM\",\"status\":\"succeeded\"}}\n",
                json(&issues)
            )
        )
    );
    assert!(!study.exists("missing"));
    assert!(study.files("out").is_empty());
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
                "{{\"command\":\"submission\",\"created\":null,\"issues\":{issues},\
                 \"path\":\"{project}\",\"paths\":[],\"section\":\"sdtn\",\"status\":\"failed\"}}"
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

/// A refusal with two issues prints D39's text, which carries the first
/// issue's line and counts the other, then the other issue's line alone, so
/// each issue prints exactly once (D39, D103); its JSON holds both.
#[test]
fn a_refusal_with_two_issues_prints_the_second_after_its_error_text() {
    let study = Study::new();
    // Two fields the project schema does not know give two findings.
    study.write("project.yaml", format!("{PROJECT}bogus: 1\nother: 2\n"));
    let project = study.path("project.yaml");
    let refusal = engine().project_spec(&project, "DM").unwrap_err();
    let [first, second] = refusal.diagnostics.as_slice() else {
        panic!("two issues: {:?}", refusal.diagnostics);
    };
    assert!(refusal.message.starts_with("cannot select DM: "));
    assert!(
        refusal.message.contains(&first.text()),
        "{}",
        refusal.message
    );
    assert!(refusal.message.ends_with(" (and 1 more issue)"));
    assert_eq!(
        outcome(&yamaa(&["check", &project, "--spec", "DM"])),
        printed(1, format!("{}\n{}\n", refusal.message, second.text()))
    );
    assert_eq!(
        outcome(&yamaa(&[
            "check", &project, "--spec", "DM", "--format", "json"
        ])),
        printed(
            1,
            format!(
                "{{\"command\":\"check\",\"issues\":{},\"path\":\"{project}\",\
                 \"spec\":\"DM\",\"status\":\"failed\"}}\n",
                json(&refusal.diagnostics)
            )
        )
    );
}

/// The authored benchmark whose project composes one SDTM define.xml.
fn define_fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../benchmarks/sdtm-dm-metadata")
}

/// That benchmark's specification and project written as 1.0, which the
/// command line reads (D122): the converted example the converter must
/// reproduce (`tools/tests/test_migrate.py`). The benchmark itself is 0.5
/// until the corpus moves to 1.0 (#3030 P2-5 to P2-8).
fn define_example() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../docs/design/interface-1.0/examples/sdtm-dm-metadata")
}

/// A copy of that benchmark's inputs, written as 1.0, with its DM dataset
/// saved, which a submission reads.
fn define_study() -> Study {
    let study = Study::empty();
    for name in ["spec.yaml", "project.yaml"] {
        study.write(name, std::fs::read(define_example().join(name)).unwrap());
    }
    let input = "input/dm_raw.csv";
    study.write(input, std::fs::read(define_fixture().join(input)).unwrap());
    assert_eq!(
        outcome(&yamaa(&[
            "run",
            &study.path("project.yaml"),
            "--spec",
            "DM"
        ]))
        .0,
        Some(0)
    );
    study
}

/// A submission saves each section's transport files, then its define.xml
/// (JSON-14), and prints them in that order; with the creation time given,
/// they hold the bytes the benchmark's authors wrote, and the time is not
/// printed again, though its JSON `created` holds it (D129).
#[test]
fn a_submission_prints_its_summary_and_the_files_in_publication_order() {
    let study = define_study();
    let fixture = define_fixture();
    let project = study.path("project.yaml");
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
                "{{\"command\":\"submission\",\"created\":\"2026-09-11T00:00:00\",\"issues\":[],\
                 \"path\":\"{project}\",\"paths\":[\"{transport}\",\"{define}\"],\
                 \"section\":\"sdtm\",\"status\":\"succeeded\"}}\n"
            )
        )
    );
}

/// SDTM's section metadata copied as an ADaM section that lists no
/// specification and writes its own define target, `adam-define.xml`, as
/// the facade's own test of D129 makes it.
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

/// The one creation time every document of the study's last submission
/// carries: both define.xml files' `CreationDateTime` and the transport
/// file's `datasetJSONCreationDateTime` (D129).
fn carried(study: &Study) -> String {
    let read = |name: &str| std::fs::read_to_string(study.0.join(name)).unwrap();
    let define = read("define.xml");
    let start = define.find("CreationDateTime=\"").unwrap() + "CreationDateTime=\"".len();
    let time = define[start..].split('"').next().unwrap().to_owned();
    for (name, carries) in [
        ("define.xml", format!("CreationDateTime=\"{time}\"")),
        ("adam-define.xml", format!("CreationDateTime=\"{time}\"")),
        (
            "dm.json",
            format!("\"datasetJSONCreationDateTime\": \"{time}\""),
        ),
    ] {
        assert!(read(name).contains(&carries), "{name} carries {time}");
    }
    time
}

/// Without `--section`, a submission composes each section that names its
/// define.xml, and its summary and JSON name no section. Without
/// `--created`, it reads the clock once and gives every section that time
/// (D129): it prints the time after its summary line, its JSON `created`
/// holds it, and every document of both sections carries it.
#[test]
fn a_submission_without_a_section_or_a_time_composes_every_section_at_one_time() {
    let study = define_study();
    let project = std::fs::read_to_string(study.0.join("project.yaml")).unwrap();
    study.write(
        "project.yaml",
        format!("{project}{}", adam_section(&project)),
    );
    let project = study.path("project.yaml");
    let written = format!(
        "{}\n{}\n{}\n",
        study.path("dm.json"),
        study.path("define.xml"),
        study.path("adam-define.xml")
    );
    let (status, stdout, stderr) = outcome(&yamaa(&["submission", &project]));
    let time = carried(&study);
    // A whole-second civil datetime (DEFINE-3).
    let shape = time
        .chars()
        .map(|c| if c.is_ascii_digit() { 'd' } else { c })
        .collect::<String>();
    assert_eq!(shape, "dddd-dd-ddTdd:dd:dd", "{time}");
    assert_eq!(
        (status, stdout, stderr),
        printed(
            0,
            format!(
                "yamaa submission of {project}: succeeded, 2 define.xml and 1 transport file, \
                 0 issues\ncreated: {time}\n{written}"
            )
        )
    );
    let (status, stdout, stderr) = outcome(&yamaa(&["submission", &project, "--format", "json"]));
    let time = carried(&study);
    let paths = written.trim_end().replace('\n', "\",\"");
    assert_eq!(
        (status, stdout, stderr),
        printed(
            0,
            format!(
                "{{\"command\":\"submission\",\"created\":\"{time}\",\"issues\":[],\
                 \"path\":\"{project}\",\"paths\":[\"{paths}\"],\"section\":null,\
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

/// The binary's engine claims no host language (#3030 section 2.10, D123):
/// it reads no project's `environments.<host>` entry, so through a 1.0
/// project whose Python lock does not exist it checks both specifications
/// and runs the one that calls no function. A run of the one that calls a
/// project function is refused before it reads any input, which does not
/// exist either, as `unsupported_operation` with operation `environment`
/// (D110), and writes nothing.
#[test]
fn the_engine_reads_no_host_lock_and_refuses_a_run_that_calls_a_function() {
    let study = Study::empty();
    study.write("raw.csv", "ID,V\n1,a\n");
    study.write(
        "functions/shout.yaml",
        "yamaa: \"1.0\"\ntype: function\nname: shout\ndescription: The value in capitals.\nparams: [{name: x, type: str}]\nreturns: str\nimplementations: {python: pkg.shout}\ntests:\n  - {id: normal, covers: [normal], args: {x: a}, result: A}\n  - {id: empty, covers: [boundary], args: {x: ''}, result: ''}\n  - {id: missing, covers: ['short-circuit-missing:x'], args: {x: null}, result: null}\n",
    );
    study.write(
        "plain.yaml",
        "yamaa: \"1.0\"\ntype: spec\ndomain: PLAIN\nkeys: [ID]\ninput: {RAW: raw.csv}\noutput: {path: plain.csv, columns: [ID, V]}\ncolumns:\n  - {name: ID, type: str, label: Identifier, derivation: RAW.ID}\n  - {name: V, type: str, label: Value, derivation: RAW.V}\n",
    );
    study.write(
        "calls.yaml",
        "yamaa: \"1.0\"\ntype: spec\ndomain: CALLS\nkeys: [ID]\ninput: {RAW: absent.csv}\noutput: {path: calls.csv, columns: [ID, LOUD]}\ncolumns:\n  - {name: ID, type: str, label: Identifier, derivation: RAW.ID}\n  - {name: LOUD, type: str, label: Loud, derivation: {function: {name: shout, args: {x: RAW.V}}}}\n",
    );
    study.write(
        "project.yaml",
        "yamaa: \"1.0\"\ntype: project\nfunctions: [functions/shout.yaml]\nenvironments:\n  python: {lock: missing/uv.lock}\nadam: {specs: [plain.yaml, calls.yaml]}\n",
    );
    let project = study.path("project.yaml");
    let engine = engine();
    for name in ["PLAIN", "CALLS"] {
        let spec = engine.project_spec(&project, name).unwrap();
        assert_eq!(
            outcome(&yamaa(&["check", &project, "--spec", name])),
            printed(0, format!("yamaa check of {spec}: succeeded, 0 issues\n")),
            "{name}"
        );
    }
    assert_eq!(
        outcome(&yamaa(&["run", &project, "--spec", "PLAIN"])),
        printed(
            0,
            format!(
                "yamaa run of PLAIN: succeeded, 1 row x 2 columns, 0 issues\n{}\n",
                study.path("plain.csv")
            )
        )
    );
    let calls = engine.project_spec(&project, "CALLS").unwrap();
    let issues = engine
        .spec_run(&calls, Some(&project), &CancellationToken::new())
        .unwrap()
        .diagnostics();
    let [refused] = issues.as_slice() else {
        panic!("one issue: {issues:?}");
    };
    assert_eq!(
        (refused.code.as_str(), refused.context.get("operation")),
        (
            "unsupported_operation",
            Some(&ContextValue::Text("environment".to_owned()))
        )
    );
    assert_eq!(
        outcome(&yamaa(&["run", &project, "--spec", "CALLS"])),
        printed(
            1,
            format!(
                "yamaa run of CALLS: failed, no output, 1 issue\n{}",
                lines(&issues)
            )
        )
    );
    assert_eq!(
        outcome(&yamaa(&[
            "--format", "json", "run", &project, "--spec", "CALLS"
        ])),
        printed(
            1,
            format!(
                "{{\"command\":\"run\",\"issues\":{},\"members\":[],\"path\":\"{project}\",\
                 \"paths\":[],\"section\":null,\"spec\":\"CALLS\",\"status\":\"failed\"}}\n",
                json(&issues)
            )
        )
    );
    assert!(!study.exists("calls.csv"));
}

/// The facade takes paths and names as UTF-8 text, so a PATH or a value
/// that is not is an argument the command line cannot pass on (D68), before
/// it reads the PATH's type.
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
            format!("yamaa: check: PATH is not UTF-8: project-\u{fffd}.yaml\n{HINT}")
        )
    );
}
