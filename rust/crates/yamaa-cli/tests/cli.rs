//! The `yamaa` binary as a user runs it: what it prints, on which stream, and
//! its exit status (#3030 section 2.8, D12, D41, D68, D97 to D99, D103, D119,
//! D122, D125, D134, D149, D169).
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

use yamaa::{CancellationToken, ContextValue, Diagnostic, Engine, EngineOptions, Host, OdmOptions};

use crate::support::{yamaa_in, Scratch};

const HINT: &str = "run 'yamaa --help' for usage\n";

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

/// A scratch directory of this test's own, which `new` fills with the study
/// below.
struct Study(Scratch);

/// DM builds; ADAE fails its own check: ASTDY reads TRTSDT, which is
/// declared after it (COLUMN-5); ADTTE reads ADAE through its `schema` link,
/// so it is skipped when ADAE fails (PROJECT-11).
const DM: &str = "yamaa: \"1.0\"\ntype: spec\nmodel: SDTM\ndomain: DM\nkeys: [USUBJID]\ninput: {RAW: {path: ../raw/dm.csv, types: {AGE: int}}}\ncolumns:\n  - {name: USUBJID, type: str, label: Subject, derivation: RAW.USUBJID}\n  - {name: AGE, type: int, label: Age, derivation: RAW.AGE}\noutput: {path: ../out/dm.csv, columns: [USUBJID, AGE], report: ../out/dm-report.csv}\n";
const ADAE: &str = "yamaa: \"1.0\"\ntype: spec\nmodel: ADaM\ndomain: ADAE\nkeys: [USUBJID]\ninput: {AE: {path: ../raw/ae.csv, types: {TRTSDT: int}}}\ncolumns:\n  - {name: USUBJID, type: str, label: Subject, derivation: AE.USUBJID}\n  - {name: ASTDY, type: int, label: Study Day, derivation: TRTSDT + 1}\n  - {name: TRTSDT, type: int, label: Start, derivation: AE.TRTSDT}\noutput: {path: ../out/adae.csv, columns: [USUBJID, ASTDY, TRTSDT], report: ../out/adae-report.csv}\n";
const ADTTE: &str = "yamaa: \"1.0\"\ntype: spec\nmodel: ADaM\ndomain: ADTTE\nkeys: [USUBJID]\ninput: {ADAE: {path: ../out/adae.csv, schema: adae.yaml}}\ncolumns:\n  - {name: USUBJID, type: str, label: Subject, derivation: ADAE.USUBJID}\noutput: {path: ../out/adtte.csv, columns: [USUBJID]}\n";
const PROJECT: &str = "yamaa: \"1.0\"\ntype: project\nsdtm:\n  specs: [sdtm/dm.yaml]\nadam:\n  specs: [adam/adae.yaml, adam/adtte.yaml]\n";

impl Study {
    fn empty() -> Self {
        Self(Scratch::new("cli"))
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
        self.0.write(name, contents);
    }

    /// A file of the study as the engine spells it: absolute, with forward
    /// slashes.
    fn path(&self, name: &str) -> String {
        format!("{}/{name}", self.0.spelled(""))
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
        "  check PATH [--spec NAME | --parent PATH] [--threads N] [--verbose]\n",
        "  run PATH [--spec NAME | --section SECTION] [--threads N] [--verbose]\n",
        "  submission PATH [--section SECTION] [--created TIME] [--threads N] [--verbose]\n",
        "  resolve PATH [--spec NAME]\n",
        "  read-odm SOURCE OUT [--archive-member NAME] [--max-archive-members N] \
         [--max-expanded-bytes N] [--max-xml-depth N]\n",
        "  lint [--fix] PATH...\n",
        "  schema TYPE\n",
        "  conditions\n",
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
            &[
                "check",
                "project.yaml",
                "--spec",
                "DM",
                "--parent",
                "t.yaml",
            ],
            "check: give --spec or --parent, not both",
        ),
        (
            &["check", "a.yaml", "--parent"],
            "check: --parent needs a value",
        ),
        // Only a check takes a parent (#3694).
        (
            &["run", "a.yaml", "--parent", "t.yaml"],
            "run: unknown option --parent",
        ),
        (
            &["resolve", "a.yaml", "--parent", "t.yaml"],
            "resolve: unknown option --parent",
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
        // #4193: check, run and submission take a positive whole number of
        // threads, and no other command takes one.
        (
            &["run", "a.yaml", "--threads", "two"],
            "run: --threads takes a whole number, not two",
        ),
        (
            &["run", "a.yaml", "--threads", "0"],
            "run: --threads takes a positive whole number, not 0",
        ),
        (
            &["check", "a.yaml", "--threads", "0"],
            "check: --threads takes a positive whole number, not 0",
        ),
        (
            &["submission", "project.yaml", "--threads", "00"],
            "submission: --threads takes a positive whole number, not 00",
        ),
        (
            &["run", "a.yaml", "--threads"],
            "run: --threads needs a value",
        ),
        (
            &["resolve", "a.yaml", "--threads", "2"],
            "resolve: unknown option --threads",
        ),
        (&["read-odm"], "read-odm: missing SOURCE"),
        (&["read-odm", "study.xml"], "read-odm: missing OUT"),
        (
            &["read-odm", "study.xml", "items.csv", "items.parquet"],
            "read-odm: unexpected argument items.parquet",
        ),
        (
            &["read-odm", "study.xml", "items.csv", "--archive-member"],
            "read-odm: --archive-member needs a value",
        ),
        (
            &["read-odm", "study.xml", "items.csv", "--spec", "DM"],
            "read-odm: unknown option --spec",
        ),
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
        (
            &["conditions", "now"],
            "conditions: unexpected argument now",
        ),
        (&["conditions", "--fix"], "conditions: unknown option --fix"),
        (&["selftest", "now"], "selftest: unexpected argument now"),
        (&["selftest", "--fix"], "selftest: unknown option --fix"),
    ];
    for (arguments, message) in cases {
        assert_eq!(
            outcome(&yamaa(arguments)),
            (Some(2), String::new(), format!("yamaa: {message}\n{HINT}")),
            "{arguments:?}"
        );
    }
}

/// `yamaa schema TYPE` prints the JSON Schema of each file type as the
/// facade gives it, canonical JSON text on one line, then one line feed, in
/// both formats, and exits 0 (D149). It writes no file.
#[test]
fn schema_prints_the_json_schema_of_each_file_type() {
    let study = Study::empty();
    for file_type in ["spec", "project", "codelists", "function"] {
        let schema = yamaa::schema(file_type).unwrap();
        assert!(!schema.contains('\n'), "{file_type}");
        for arguments in [
            &["schema", file_type][..],
            &["--format", "json", "schema", file_type],
            &["schema", file_type, "--format", "json"],
        ] {
            assert_eq!(
                outcome(&yamaa_in(&study.0, arguments)),
                printed(0, format!("{schema}\n")),
                "{arguments:?}"
            );
        }
    }
    assert!(study.files(".").is_empty());
}

/// TYPE goes to the facade as given, which compares it exactly with the four
/// file types, so any other text is its refusal, `unexpected_file_type`
/// (SCHEMA-2): D39's error text and exit status 1, as the hosts raise it and
/// as an unknown section is refused (D98, D149). Its JSON holds the command,
/// the status `failed` and the issue, and echoes TYPE as given as `type`, as
/// a refused `resolve` echoes its `path` and `spec` (D169).
#[test]
fn schema_of_a_type_naming_no_file_type_is_refused_with_exit_status_1() {
    for file_type in ["specs", "Spec", ""] {
        let refused = yamaa::schema(file_type).unwrap_err();
        assert!(
            refused
                .message
                .starts_with(&format!("cannot give the schema of {file_type}: ")),
            "{}",
            refused.message
        );
        let [issue] = refused.diagnostics.as_slice() else {
            panic!("one issue: {:?}", refused.diagnostics);
        };
        assert_eq!(
            (issue.code.as_str(), issue.rule.as_deref()),
            ("unexpected_file_type", Some("SCHEMA-2")),
            "{file_type:?}"
        );
        assert_eq!(
            outcome(&yamaa(&["schema", file_type])),
            printed(1, format!("{}\n", refused.message)),
            "{file_type:?}"
        );
        assert_eq!(
            outcome(&yamaa(&["--format", "json", "schema", file_type])),
            printed(
                1,
                format!(
                    "{{\"command\":\"schema\",\"issues\":{},\"status\":\"failed\",\
                     \"type\":\"{file_type}\"}}\n",
                    json(&refused.diagnostics)
                )
            ),
            "{file_type:?}"
        );
    }
}

/// `yamaa selftest` runs the cases the binary carries and prints the facade's
/// report, the hosts' words with the host `cli` (D119). A shipped build
/// compiles every feature, so every case passes, and a case covers every
/// adapter, so no `not covered:` line is printed. With `--format json` it
/// prints the status, each case in list order and the engine information as
/// `--version` prints it; the JSON has no key for uncovered adapters, which
/// only the printed report names. No case failed, so it exits 0; it writes
/// no file (D108).
#[test]
fn selftest_prints_the_report_and_exits_0_when_no_case_failed() {
    let study = Study::empty();
    let platform = format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH);
    let report = format!(
        "yamaa selftest: succeeded, 5 of 5 cases passed\n\
         engine {}, language 1.0, cli, {platform}\n",
        yamaa::VERSION
    );
    for arguments in [&["selftest"][..], &["--format", "text", "selftest"]] {
        assert_eq!(
            outcome(&yamaa_in(&study.0, arguments)),
            printed(0, report.clone()),
            "{arguments:?}"
        );
    }
    let cases = [
        "adam-adae-analysis-time",
        "adam-adsl-xpt",
        "negative-transport-empty-string",
        "schema-source-xpt",
        "sdtm-dm-odm-snapshot",
    ]
    .map(|case| format!("{{\"case\":\"{case}\",\"message\":null,\"status\":\"succeeded\"}}"))
    .join(",");
    let document = format!(
        "{{\"cases\":[{cases}],\"command\":\"selftest\",\
         \"info\":{{\"features\":[\"odm\",\"parquet\",\"readstat\",\"yaml\"],\"host\":\"cli\",\
         \"host_version\":null,\"language_versions\":[\"1.0\"],\"platform\":\"{platform}\",\
         \"version\":\"{}\"}},\"status\":\"succeeded\"}}\n",
        yamaa::VERSION
    );
    for arguments in [
        &["selftest", "--format", "json"][..],
        &["--format", "json", "selftest"],
    ] {
        assert_eq!(
            outcome(&yamaa_in(&study.0, arguments)),
            printed(0, document.clone()),
            "{arguments:?}"
        );
    }
    assert!(study.files(".").is_empty());
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
    let resolved = engine().spec(&spec).resolve().unwrap().to_json();
    for format in ["text", "json"] {
        assert_eq!(
            outcome(&yamaa(&["resolve", &spec, "--format", format])),
            printed(0, format!("{resolved}\n")),
            "{format}"
        );
    }
}

/// `--verbose` (#3762) leaves standard output as it is and writes to
/// standard error each phase of the run as it starts and ends, the phases of
/// a run without project functions in their documented order; then how the
/// run divided its driver input (#3794); then the timings table: its column
/// names, then one row per phase with the run's path as given, its phase,
/// two numbers of milliseconds and its outcome. The command line runs
/// key-bounded over the two leading declared keys, in partitions of at most
/// 624,640 records (D264): DM declares one key and its input holds two
/// records, so the run derives one key-bounded partition of 2 records.
#[test]
fn verbose_writes_each_phase_the_division_and_then_the_timings_to_standard_error() {
    let study = Study::new();
    let plain = outcome(&yamaa_in(&study.0, &["run", "sdtm/dm.yaml"]));
    let (status, stdout, stderr) =
        outcome(&yamaa_in(&study.0, &["run", "sdtm/dm.yaml", "--verbose"]));
    assert_eq!((status, stdout), (plain.0, plain.1), "standard output");
    let phases = [
        "preparation",
        "input",
        "admission",
        "derivation",
        "output_keys",
        "verification",
        "output",
    ];
    let events = phases
        .iter()
        .flat_map(|phase| {
            let partition = (*phase == "derivation")
                .then(|| "sdtm/dm.yaml: derivation partition 1 (2 records)".to_owned());
            std::iter::once(format!("sdtm/dm.yaml: {phase} start"))
                .chain(partition)
                .chain([format!("sdtm/dm.yaml: {phase} end: succeeded")])
        })
        .chain(["sdtm/dm.yaml: partitions key_bounded 1".to_owned()])
        .collect::<Vec<_>>();
    let lines = stderr.lines().collect::<Vec<_>>();
    assert_eq!(
        lines[..events.len()],
        events,
        "the progress events and the division"
    );
    assert_eq!(
        lines[events.len()],
        "name\tphase\tstarted_ms\telapsed_ms\toutcome",
        "the timings header"
    );
    let rows = lines[events.len() + 1..]
        .iter()
        .map(|line| {
            let cells = line.split('\t').collect::<Vec<_>>();
            let milliseconds = cells[2..4]
                .iter()
                .all(|cell| cell.parse::<f64>().is_ok_and(|value| value >= 0.0));
            (cells[0], cells[1], milliseconds, cells[4])
        })
        .collect::<Vec<_>>();
    assert_eq!(
        rows,
        phases
            .iter()
            .map(|phase| ("sdtm/dm.yaml", *phase, true, "succeeded"))
            .collect::<Vec<_>>(),
        "the timings rows"
    );
}

/// `--threads N` (#4193) changes neither what a run prints nor what it
/// reports as it runs: with 1, the serial run, and with 4, the run prints
/// the standard output of a run without the flag, and under `--verbose` the
/// same progress events and division, in the same order.
#[test]
fn threads_leave_what_a_run_prints_and_reports_as_it_is() {
    let study = Study::new();
    let events = |stderr: &str| {
        stderr
            .lines()
            .take_while(|line| !line.starts_with("name\t"))
            .map(str::to_owned)
            .collect::<Vec<_>>()
    };
    let plain = outcome(&yamaa_in(&study.0, &["run", "sdtm/dm.yaml", "--verbose"]));
    for threads in ["1", "4"] {
        let threaded = outcome(&yamaa_in(
            &study.0,
            &["run", "sdtm/dm.yaml", "--threads", threads, "--verbose"],
        ));
        assert_eq!(
            (threaded.0, threaded.1, events(&threaded.2)),
            (plain.0, plain.1.clone(), events(&plain.2)),
            "--threads {threads}: the status, standard output and progress events"
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
        .project(&project)
        .cancel_with(&token)
        .check()
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
        .project(&project)
        .cancel_with(&token)
        .run()
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
/// given to `submission`; a project given to `resolve` without `--spec`,
/// since a project has no resolved form of its own; and a project given to
/// `check` with `--parent`, which checks only a fragment (#3694). The facade
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
        (
            &["check", "project.yaml", "--parent", "sdtm/dm.yaml"],
            "check: project.yaml declares type project, which check does not take with --parent",
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

/// `check PATH --parent PARENT` checks the fragment PATH composed on the
/// specification PARENT as its parent (#3694), and prints the check's summary
/// line in both formats. PARENT is a path given from the current directory,
/// as PATH is, not from the fragment's directory: the fragment in
/// `fragments/` composes on the template in `templates/`.
#[test]
fn check_with_a_parent_checks_the_fragment_composed_on_the_parent_it_names() {
    let directory = Scratch::new("check-parent");
    directory.write(
        "templates/dm.yaml",
        "yamaa: \"1.0\"\ntype: spec\nmodel: SDTM\ndomain: DM\nkeys: [USUBJID]\n\
         input: {RAW: raw.csv}\noutput: {path: dm.csv, columns: [USUBJID, AGE]}\n\
         columns:\n  - {name: USUBJID, type: str, derivation: RAW.USUBJID}\n",
    );
    directory.write("templates/raw.csv", "USUBJID,AGE\nS1,40\n");
    directory.write(
        "fragments/age.yaml",
        "yamaa: \"1.0\"\ntype: spec\ncolumns:\n  - {name: AGE, type: str, derivation: RAW.AGE}\n",
    );
    let arguments = ["fragments/age.yaml", "--parent", "templates/dm.yaml"];
    assert_eq!(
        outcome(&yamaa_in(
            &directory,
            &[&["check"][..], &arguments].concat()
        )),
        printed(
            0,
            "yamaa check of fragments/age.yaml: succeeded, 0 issues\n".into()
        )
    );
    assert_eq!(
        outcome(&yamaa_in(
            &directory,
            &[&["--format", "json", "check"][..], &arguments].concat()
        )),
        printed(
            0,
            "{\"command\":\"check\",\"issues\":[],\"path\":\"fragments/age.yaml\",\
             \"spec\":null,\"status\":\"succeeded\"}\n"
                .into()
        )
    );
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
    let issues = engine
        .project(&project)
        .spec("ADAE")
        .unwrap()
        .run()
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
        .project(&project)
        .section("adam")
        .run()
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
        .project(&project)
        .section("sdtm")
        .run()
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
        .project(&project)
        .section("send")
        .run()
        .unwrap()
        .save(&CancellationToken::new())
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
    let mut run = engine.project(&project).spec("DM").unwrap().run().unwrap();
    assert!(run.diagnostics().is_empty());
    let refusal = run.save(&CancellationToken::new()).unwrap_err();
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
    let dm = engine.project(&project).spec("DM").unwrap();
    assert_eq!(
        outcome(&yamaa(&["check", &project, "--spec", "DM"])),
        printed(
            0,
            format!("yamaa check of {}: succeeded, 0 issues\n", dm.path())
        )
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
    let adae = engine.project(&project).spec("ADAE").unwrap();
    let issues = adae.check().unwrap().diagnostics();
    assert_eq!(issues.len(), 1);
    assert_eq!(
        outcome(&yamaa(&["check", &project, "--spec", "ADAE"])),
        printed(
            1,
            format!(
                "yamaa check of {}: failed, 1 issue\n{}",
                adae.path(),
                lines(&issues)
            )
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
    let dm = engine.project(&project).spec("DM").unwrap();
    let resolved = dm.resolve().unwrap().to_json();
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
    let unknown = engine.project(&project).spec("ADXX").unwrap_err();
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
    let Err(section) = engine.project(&project).section("sdtn").run() else {
        panic!("an unknown section refuses the run");
    };
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
    let refusal = engine().project(&project).spec("DM").unwrap_err();
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
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/declaration-examples/sdtm-dm-metadata")
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
        .replace("  context: Submission\n", "  context: Other\n")
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

/// The adapters' ODM 1.3 document, which the hosts' installed `read_odm`
/// suites read too (`read-odm` in `rust/tests/host-fixtures/cases.json`).
fn odm_document() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../yamaa-adapters/tests/fixtures/odm/odm13.xml")
}

/// A study that holds that document as `odm13.xml`.
fn odm_study() -> Study {
    let study = Study::empty();
    study.write("odm13.xml", std::fs::read(odm_document()).unwrap());
    study
}

/// The document's item table in the csv profile, written by hand from the
/// three rows the hosts' shared truth gives it and the csv cards: a header
/// of the column names (CSV-2), a field quoted exactly when it holds a quote
/// or a comma or is empty (CSV-3), missing as an empty bare field and the
/// empty string as `""` (CSV-4), an integer as its digits (CSV-5), and a line
/// feed after every record (CSV-1).
const ODM_ITEMS_CSV: &str = concat!(
    "ODMVersion,FileOID,StudyOID,MetaDataVersionOID,SubjectKey,StudySubjectID,",
    "SubjectStatus,StudyEventOID,StudyEventRepeatKey,EventName,StartDate,EventStatus,",
    "EventWorkflowStatus,FormOID,FormRepeatKey,FormName,FormLayoutOID,FormStatus,",
    "FormWorkflowStatus,OpenQueries,ItemGroupOID,ItemGroupRepeatKey,ItemGroupName,",
    "TransactionType,ItemOID,ItemName,Value,ValuePresent,IsNull,SourceOrdinal\n",
    "1.3,FILE.13,S.13,M.13,SUBJECT-1,DISPLAY-1,Available,E.13,,Screening,2026-01-02,,,",
    "F.13,,Demographics,,,,,G.13,7,Subject details,Insert,I.DATE,Collection date,",
    "2026-01-02,true,false,1\n",
    "1.3,FILE.13,S.13,M.13,SUBJECT-1,DISPLAY-1,Available,E.13,,Screening,2026-01-02,,,",
    "F.13,,Demographics,,,,,G.13,7,Subject details,Insert,I.EMPTY,Collected empty,",
    "\"\",true,false,2\n",
    "1.3,FILE.13,S.13,M.13,SUBJECT-1,DISPLAY-1,Available,E.13,,Screening,2026-01-02,,,",
    "F.13,,Demographics,,,,,G.13,7,Subject details,Insert,I.NULL,Explicit null,",
    ",false,true,3\n",
);

/// `yamaa read-odm SOURCE OUT` reads SOURCE as the hosts' `read_odm()` reads
/// it and saves its items to OUT, whose extension selects the csv profile
/// whatever its case (OUTPUT-5, D134). It prints only the absolute path it
/// wrote (D38) and exits 0; with `--format json`, its `path` is SOURCE as
/// given and its `rows` counts the items read. The file holds the bytes the
/// csv cards give the hosts' truth, also when every bound is given at the
/// tightest value the document meets.
#[test]
fn read_odm_saves_the_items_of_a_document_and_prints_the_path_it_wrote() {
    let study = odm_study();
    assert_eq!(
        outcome(&yamaa_in(&study.0, &["read-odm", "odm13.xml", "items.csv"])),
        printed(0, format!("{}\n", study.path("items.csv")))
    );
    // The two files differ by more than case, since a case-insensitive file
    // system, as macOS and Windows use by default, holds `x.CSV` and `x.csv`
    // as one entry.
    let upper = study.path("upper.CSV");
    assert_eq!(
        outcome(&yamaa_in(
            &study.0,
            &["--format", "json", "read-odm", "odm13.xml", "upper.CSV"]
        )),
        printed(
            0,
            format!(
                "{{\"command\":\"read-odm\",\"issues\":[],\"path\":\"odm13.xml\",\
                 \"paths\":[\"{upper}\"],\"rows\":3,\"status\":\"succeeded\"}}\n"
            )
        )
    );
    // The document nests 7 elements deep, and a plain XML file holds no
    // archive members.
    assert_eq!(
        outcome(&yamaa_in(
            &study.0,
            &[
                "read-odm",
                "odm13.xml",
                "bounded.csv",
                "--max-archive-members",
                "1",
                "--max-expanded-bytes",
                "65536",
                "--max-xml-depth",
                "8",
            ]
        )),
        printed(0, format!("{}\n", study.path("bounded.csv")))
    );
    for name in ["items.csv", "upper.CSV", "bounded.csv"] {
        assert_eq!(
            std::fs::read_to_string(study.0.join(name)).unwrap(),
            ODM_ITEMS_CSV,
            "{name}"
        );
    }
    assert_eq!(
        study.files("."),
        ["bounded.csv", "items.csv", "odm13.xml", "upper.CSV"]
    );
}

/// A 1.0 specification that copies every item column of `items.parquet` to
/// `output`. The input keeps a stored empty string as the collected empty
/// value the table holds (INPUT-9).
fn copying_spec(output: &str) -> String {
    let mut spec = format!(
        "yamaa: \"1.0\"\ntype: spec\ndomain: ITEMS\nkeys: [SourceOrdinal]\ninput: {{ODM: {{path: items.parquet, empty_string: present}}}}\noutput:\n  path: {output}\n  columns: [{}]\ncolumns:\n",
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

/// OUT may name a Parquet file, which every shipped build writes (D134), and
/// it holds the same table: a run of a specification that copies every column
/// of `items.parquet` saves, under each profile, the bytes `read-odm` saves,
/// so the command saves what the hosts' `read_odm()` and a run save.
#[test]
fn read_odm_saves_parquet_and_both_files_hold_what_a_run_saves() {
    let study = odm_study();
    for out in ["items.parquet", "items.csv"] {
        assert_eq!(
            outcome(&yamaa_in(&study.0, &["read-odm", "odm13.xml", out])),
            printed(0, format!("{}\n", study.path(out))),
            "{out}"
        );
    }
    let parquet = std::fs::read(study.0.join("items.parquet")).unwrap();
    assert!(parquet.starts_with(b"PAR1") && parquet.ends_with(b"PAR1"));
    for extension in ["csv", "parquet"] {
        let spec = format!("copy-{extension}.yaml");
        let ran = format!("run.{extension}");
        study.write(&spec, copying_spec(&ran));
        assert_eq!(
            outcome(&yamaa_in(&study.0, &["run", &spec])),
            printed(
                0,
                format!(
                    "yamaa run of ITEMS: succeeded, 3 rows x 30 columns, 0 issues\n{}\n",
                    study.path(&ran)
                )
            ),
            "{extension}"
        );
        assert_eq!(
            std::fs::read(study.0.join(&ran)).unwrap(),
            std::fs::read(study.0.join(format!("items.{extension}"))).unwrap(),
            "{extension}"
        );
    }
}

/// A refused read names SOURCE as given, and a refused save names OUT as
/// given (D39, D134): either prints D39's error text on standard output,
/// writes nothing and exits 1, as the hosts raise it. The JSON `status`
/// follows the read, as a run's follows the run (D169).
///
/// - The save refuses an extension outside OUTPUT-5's closed mapping, or
///   none, before it writes anything, and a target that cannot be written,
///   as a run's save refuses it (D99, D189): a missing directory or a
///   directory, with FILE-7's conditions naming OUT as given.
///   The items were read, so the JSON `status` is `succeeded`, its `rows`
///   counts them, and its `issues` hold the save's refusal.
/// - The reader refuses a file that does not exist, a bound below its
///   minimum or one the document exceeds, and an archive member of a plain
///   XML file. Each message names the option or its value, so each flag is
///   seen to reach its own option. Nothing was read, so the JSON `status` is
///   `failed` and its `rows` null. Each refusal carries its registered issue
///   (D158), as the hosts' does. The read comes first, so a SOURCE it refuses
///   is the refusal whatever OUT names.
#[test]
fn a_refused_read_or_save_prints_its_error_text_and_exits_1() {
    let study = odm_study();
    std::fs::create_dir(study.0.join("folder.csv")).unwrap();
    let source = study.path("odm13.xml");
    let engine = engine();
    let table = engine.read_odm(&source, &OdmOptions::new()).unwrap();
    for (name, condition, code) in [
        ("items.txt", "unknown_artifact_profile", None::<&str>),
        ("items", "unknown_artifact_profile", None),
        ("absent/items.csv", "resource_path_missing", None),
        ("folder.csv", "resource_path_not_regular_file", None),
    ] {
        let out = study.path(name);
        let refused = engine.save_table(&table, &out).unwrap_err();
        assert!(
            refused.message.starts_with(&format!("cannot save {out}: ")),
            "{}",
            refused.message
        );
        let [issue] = refused.diagnostics.as_slice() else {
            panic!("one issue: {:?}", refused.diagnostics);
        };
        let code = code.map(|code| ContextValue::Text(code.to_owned()));
        assert_eq!(
            (issue.code.as_str(), issue.context.get("code")),
            (condition, code.as_ref()),
            "{name}"
        );
        assert_eq!(
            outcome(&yamaa(&["read-odm", &source, &out])),
            printed(1, format!("{}\n", refused.message)),
            "{name}"
        );
        assert_eq!(
            outcome(&yamaa(&["--format", "json", "read-odm", &source, &out])),
            printed(
                1,
                format!(
                    "{{\"command\":\"read-odm\",\"issues\":{},\"path\":\"{source}\",\
                     \"paths\":[],\"rows\":3,\"status\":\"succeeded\"}}\n",
                    json(&refused.diagnostics)
                )
            ),
            "{name}"
        );
    }
    let absent = study.path("absent.xml");
    let out = study.path("items.csv");
    // The read's refusal, as the hosts' `read_odm()` raises it.
    let refusal = |read: &str, options: &OdmOptions| engine.read_odm(read, options).unwrap_err();
    struct ExpectedIssue<'a> {
        condition: &'a str,
        phase: &'a str,
        rule: &'a str,
        spec_path: Option<&'a str>,
        context: ContextValue,
        message: String,
    }
    let text = |value: &str| ContextValue::Text(value.to_owned());
    let context = |values: &[(&str, ContextValue)]| {
        ContextValue::Map(
            values
                .iter()
                .map(|(key, value)| ((*key).to_owned(), value.clone()))
                .collect(),
        )
    };
    // An option below its minimum names the minimum and the value given.
    let option_issue = |name: &'static str, minimum, value| ExpectedIssue {
        condition: "value_not_permitted",
        phase: "validation",
        rule: "ODM-1",
        spec_path: Some(name),
        context: context(&[
            ("minimum", ContextValue::Int(minimum)),
            ("value", ContextValue::Int(value)),
        ]),
        message: format!("{name} has the value {value}, which is not permitted"),
    };
    let cases: [(&str, &[&str], OdmOptions, ExpectedIssue); 6] = [
        (
            &absent,
            &[],
            OdmOptions::new(),
            ExpectedIssue {
                condition: "resource_path_missing",
                phase: "validation",
                rule: "FILE-7",
                spec_path: None,
                context: context(&[("path", text(&absent))]),
                message: format!("{absent} does not exist"),
            },
        ),
        (
            &source,
            &["--max-expanded-bytes", "0"],
            OdmOptions::new().with_max_expanded_bytes(0),
            option_issue("max_expanded_bytes", 1, 0),
        ),
        (
            &source,
            &["--max-archive-members", "0"],
            OdmOptions::new().with_max_archive_members(0),
            option_issue("max_archive_members", 1, 0),
        ),
        (
            &source,
            &["--max-xml-depth", "7"],
            OdmOptions::new().with_max_xml_depth(7),
            option_issue("max_xml_depth", 8, 7),
        ),
        (
            &source,
            &["--archive-member", "odm13.xml"],
            OdmOptions::new().with_archive_member("odm13.xml"),
            ExpectedIssue {
                condition: "source_odm_archive_member",
                phase: "ingest",
                rule: "ODM-1",
                spec_path: None,
                context: context(&[
                    ("path", text(&source)),
                    ("reason", text("archive_member requires a TAR input")),
                ]),
                message: format!(
                    "no member of {source} can be read: archive_member requires a TAR input"
                ),
            },
        ),
        (
            &source,
            &["--max-expanded-bytes", "100"],
            OdmOptions::new().with_max_expanded_bytes(100),
            ExpectedIssue {
                condition: "source_odm_limit_exceeded",
                phase: "ingest",
                rule: "ODM-1",
                spec_path: None,
                context: context(&[
                    ("path", text(&source)),
                    ("reason", text("ODM XML exceeds 100 bytes")),
                ]),
                message: format!("{source} exceeds a read limit: ODM XML exceeds 100 bytes"),
            },
        ),
    ];
    for (read, flags, options, expected) in cases {
        let refused = refusal(read, &options);
        let ExpectedIssue {
            condition,
            phase,
            rule,
            spec_path,
            context,
            message,
        } = expected;
        let expected_message = format!("cannot read {read}: {message} [{condition}, {rule}]");
        assert_eq!(refused.message, expected_message);
        let expected_issue = ContextValue::Map(
            [
                ("column", ContextValue::Null),
                ("condition", text(condition)),
                ("context", context),
                ("file", ContextValue::Null),
                ("line", ContextValue::Null),
                ("message", text(&message)),
                ("operand_route", ContextValue::Null),
                ("phase", text(phase)),
                ("rule", text(rule)),
                ("severity", text("error")),
                ("source_span", ContextValue::Null),
                (
                    "spec_paths",
                    ContextValue::List(spec_path.into_iter().map(text).collect()),
                ),
            ]
            .into_iter()
            .map(|(key, value)| (key.to_owned(), value))
            .collect(),
        );
        let [issue] = refused.diagnostics.as_slice() else {
            panic!("one issue: {:?}", refused.diagnostics);
        };
        assert_eq!(issue.to_value(), expected_issue);
        let arguments = [&["read-odm", read, &out][..], flags].concat();
        assert_eq!(
            outcome(&yamaa(&arguments)),
            printed(1, format!("{expected_message}\n")),
            "{arguments:?}"
        );
        assert_eq!(
            outcome(&yamaa(&[&["--format", "json"][..], &arguments].concat())),
            printed(
                1,
                format!(
                    "{{\"command\":\"read-odm\",\"issues\":[{}],\"path\":\"{read}\",\"paths\":[],\
                     \"rows\":null,\"status\":\"failed\"}}\n",
                    expected_issue.to_json()
                )
            ),
            "{arguments:?}"
        );
    }
    // The read comes first: an OUT the save would refuse is never examined.
    assert_eq!(
        outcome(&yamaa(&["read-odm", &absent, &study.path("items.txt")])),
        printed(
            1,
            format!("{}\n", refusal(&absent, &OdmOptions::new()).message)
        )
    );
    assert_eq!(study.files("."), ["folder.csv", "odm13.xml"]);
    assert!(study.files("folder.csv").is_empty());
}

/// `lint` prints each finding's line, then its summary line (D103), and
/// exits 1 while a finding remains; `--fix` rewrites the file and names it in
/// the JSON `paths`.
#[test]
fn lint_prints_each_finding_then_its_summary() {
    const SPEC: &str = "yamaa: \"1.0\"\ntype: spec\ndomain: DM\noutput:\n  path: dm.csv\n";
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
        let spec = engine.project(&project).spec(name).unwrap();
        assert_eq!(
            outcome(&yamaa(&["check", &project, "--spec", name])),
            printed(
                0,
                format!("yamaa check of {}: succeeded, 0 issues\n", spec.path())
            ),
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
    let calls = engine.project(&project).spec("CALLS").unwrap();
    let issues = calls.run().unwrap().diagnostics();
    let [refused] = issues.as_slice() else {
        panic!("one issue: {issues:?}");
    };
    // The refusal names the first derivation that calls a project function
    // (D221), located at its `derivation` key: line 9, column 42, after
    // `  - {name: LOUD, type: str, label: Loud, ` (P-ISSUE-1).
    let location = refused.location.clone().expect("a located refusal");
    assert_eq!(
        (
            refused.phase.as_str(),
            refused.code.as_str(),
            refused.rule.as_deref(),
            refused.spec_path.as_slice(),
            location.file,
            location.position.map(|at| (at.line, at.column)),
            refused.context.to_json(),
        ),
        (
            "planning",
            "unsupported_operation",
            None,
            ["columns.LOUD.derivation".to_owned()].as_slice(),
            calls.path().replace('\\', "/"),
            Some((9, 42)),
            r#"{"operation":"environment"}"#,
        ),
        "the refused run"
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
    assert_eq!(
        run(&[text("schema"), odd]),
        (
            Some(2),
            String::new(),
            format!("yamaa: schema: TYPE is not UTF-8: project-\u{fffd}.yaml\n{HINT}")
        )
    );
    // `read-odm` checks SOURCE, OUT and the archive member before it reads
    // SOURCE, which does not exist here either.
    for (arguments, what) in [
        (&[text("read-odm"), odd, text("items.csv")][..], "SOURCE"),
        (&[text("read-odm"), text("odm.xml"), odd], "OUT"),
        (
            &[
                text("read-odm"),
                text("odm.xml"),
                text("items.csv"),
                text("--archive-member"),
                odd,
            ],
            "--archive-member",
        ),
    ] {
        assert_eq!(
            run(arguments),
            (
                Some(2),
                String::new(),
                format!("yamaa: read-odm: {what} is not UTF-8: project-\u{fffd}.yaml\n{HINT}")
            ),
            "{what}"
        );
    }
}
