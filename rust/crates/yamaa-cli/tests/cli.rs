//! The `yamaa` binary as a user runs it: what it prints, on which stream, and
//! its exit status (#3030 section 2.8, D12, D41, D68).

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const HINT: &str = "run 'yamaa --help' for usage\n";

/// Run the binary with `arguments` in `directory`.
fn yamaa_in(directory: &Path, arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_yamaa"))
        .args(arguments)
        .current_dir(directory)
        .output()
        .expect("the yamaa binary starts")
}

fn yamaa(arguments: &[&str]) -> Output {
    yamaa_in(Path::new(env!("CARGO_MANIFEST_DIR")), arguments)
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

/// A fresh, empty directory of this test's own.
fn scratch(name: &str) -> PathBuf {
    let directory = std::env::temp_dir().join(format!("yamaa-cli-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir(&directory).unwrap();
    directory
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
            (Some(0), expected.clone(), String::new()),
            "{arguments:?}"
        );
    }
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

/// Every command reads its arguments and then refuses, until it prints the
/// summaries and JSON of #3248 and saves through #3247. The refusal names
/// what is missing, so the conformance runner can tell it from a usage
/// error.
#[test]
fn each_command_checks_its_arguments_then_refuses_as_not_implemented() {
    let directory = scratch("not-implemented");
    let cases: &[(&[&str], &str)] = &[
        (&["check", "spec.yaml"], "check"),
        (&["check", "project.yaml", "--spec", "DM"], "check"),
        (&["run", "spec.yaml"], "run"),
        (&["run", "project.yaml", "--section", "adam"], "run"),
        (&["run", "--spec", "DM", "project.yaml"], "run"),
        (&["submission", "project.yaml"], "submission"),
        (
            &[
                "submission",
                "project.yaml",
                "--created",
                "2026-10-20T09:00:00",
                "--section",
                "sdtm",
            ],
            "submission",
        ),
        (&["resolve", "spec.yaml"], "resolve"),
        (&["resolve", "project.yaml", "--spec", "DM"], "resolve"),
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
        (&["lint", "spec.yaml"], "lint"),
        (
            &["lint", "--fix", "spec.yaml", "project.yaml", "specs"],
            "lint",
        ),
        (&["lint", "--", "-odd.yaml"], "lint"),
        (&["schema", "spec"], "schema"),
        (&["selftest"], "selftest"),
        (&["--format", "text", "check", "spec.yaml"], "check"),
        (&["check", "spec.yaml", "--format", "text"], "check"),
        (&["--format", "json", "--version"], "--format json"),
        (&["--version", "--format", "json"], "--format json"),
        (&["--format", "json", "check", "spec.yaml"], "--format json"),
        (&["lint", "spec.yaml", "--format", "json"], "--format json"),
    ];
    for (arguments, what) in cases {
        assert_eq!(
            outcome(&yamaa_in(&directory, arguments)),
            (
                Some(2),
                String::new(),
                format!("yamaa: {what} is not implemented yet\n")
            ),
            "{arguments:?}"
        );
    }
    // No refused command wrote anything, its OUT included.
    assert_eq!(std::fs::read_dir(&directory).unwrap().count(), 0);
    std::fs::remove_dir(&directory).unwrap();
}

/// A PATH keeps its native spelling (D42), even when it is not UTF-8.
#[cfg(unix)]
#[test]
fn a_path_need_not_be_utf8() {
    use std::os::unix::ffi::OsStrExt;

    let path = std::ffi::OsStr::from_bytes(b"spec-\xff.yaml");
    let output = Command::new(env!("CARGO_BIN_EXE_yamaa"))
        .arg("check")
        .arg(path)
        .output()
        .expect("the yamaa binary starts");
    assert_eq!(
        outcome(&output),
        (
            Some(2),
            String::new(),
            "yamaa: check is not implemented yet\n".to_owned()
        )
    );
}
