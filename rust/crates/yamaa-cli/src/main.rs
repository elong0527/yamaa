//! The `yamaa` command line (#3030 D12, section 2.8): one binary over the
//! `yamaa` facade alone (section 2.10).
//!
//! Standard output carries what a command prints, as text or, with
//! `--format json`, as one canonical JSON document; standard error carries
//! only the refusals of exit status 2, with two exceptions. A panic inside a
//! `selftest` case fails that case, so the command exits 1, and Rust's
//! default panic hook writes the panic message, which shows where the engine
//! panicked, to standard error (D142). And `check`, `run` and `submission`
//! with `--verbose` write each progress event there as it happens, then,
//! after what they print, the timings table of their result (#3762), so
//! standard output is the same with or without it. The exit status is:
//!
//! - 0 when the command completed and succeeded, or `--help` or `--version`
//!   printed its text;
//! - 1 when the command completed with findings: a `failed` or `skipped`
//!   status, remaining lint findings, a failed selftest case, or a refusal,
//!   such as an unknown section (D98) or `schema` TYPE (D149), or a PATH
//!   whose type cannot be read (D125), which the facade explains with its
//!   issues;
//! - 2 when the command could not run as asked (D68): a usage error, an
//!   argument the facade cannot take, a PATH whose type the command does not
//!   take, or a failed write to standard output.
//!
//! No configuration file, environment variable or colour changes what it
//! does. Ctrl+C stops `check`, `run` and `submission` at the facade's next
//! cancellation checkpoint, so they print nothing and a save writes every
//! file it planned or none (D46, D74); it stops the other commands at once,
//! so an interrupted `lint --fix`, which replaces each file atomically, can
//! leave some files fixed and others not. The process then ends the native
//! way, with none of the exit statuses above (D203): on POSIX it re-raises
//! SIGINT, which shells report as status 130, and on Windows it exits with
//! `STATUS_CONTROL_C_EXIT`.
//!
//! What each command prints, and how it dispatches on its PATH's `type:`
//! header (D122), is in [`commands`].
#![deny(unsafe_code)]

mod arguments;
mod commands;
// The process's signal/console boundary is the only code that calls OS APIs.
#[allow(unsafe_code)]
mod interrupt;

use std::io::{self, Write};
use std::process::ExitCode;

use arguments::{Format, Invocation, Usage, COMMANDS};
use commands::Execution;

/// Exit status 0: the command completed and succeeded.
const SUCCEEDED: u8 = 0;
/// Exit status 1: the command completed with findings.
const FINDINGS: u8 = 1;
/// Exit status 2: the command could not run as asked.
const NOT_RUN: u8 = 2;

fn main() -> ExitCode {
    let arguments = std::env::args_os().skip(1).collect::<Vec<_>>();
    let invocation = arguments::parse(&arguments);
    // Only these commands have facade cancellation checkpoints. Other
    // commands retain the operating system's immediate signal handling.
    let cooperative = matches!(
        &invocation,
        Ok(Invocation::Command(call, _)) if matches!(call.name, "check" | "run" | "submission")
    );
    let token = match if cooperative {
        interrupt::install()
    } else {
        Ok(yamaa::CancellationToken::new())
    } {
        Ok(token) => token,
        Err(error) => {
            let _ = writeln!(
                io::stderr(),
                "yamaa: cannot install Ctrl+C handler: {error}"
            );
            return ExitCode::from(NOT_RUN);
        }
    };
    let status = run(
        invocation,
        &mut io::stdout().lock(),
        &mut io::stderr().lock(),
        &token,
    );
    // All engine-owned state and staged files have unwound before the
    // original interrupt terminates the process (D46, D74).
    if cooperative {
        interrupt::finish();
    }
    ExitCode::from(status)
}

/// Carry out `invocation`, writing to `stdout` and `stderr`, and
/// return the exit status.
fn run(
    invocation: Result<Invocation, Usage>,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
    token: &yamaa::CancellationToken,
) -> u8 {
    match invocation {
        Ok(Invocation::Help) => print(stdout, stderr, &help()),
        Ok(Invocation::Version(format)) => print(stdout, stderr, &version(format)),
        Ok(Invocation::Command(call, format)) => {
            let execution = commands::execute(&call, format, token);
            // An interrupt has no result or diagnostic to print (D46).
            if token.is_cancelled() {
                return SUCCEEDED;
            }
            match execution {
                Execution::Ran {
                    stdout: text,
                    stderr: timings,
                    status,
                } => match print(stdout, stderr, &text) {
                    SUCCEEDED => {
                        // Standard error is the last stream to report on, so a
                        // failed write to it is not reported.
                        let _ = stderr.write_all(timings.as_bytes());
                        status
                    }
                    failed => failed,
                },
                Execution::Refused(usage) => refuse(stderr, &usage),
            }
        }
        Err(usage) => refuse(stderr, &usage),
    }
}

/// Refuse a usage error with exit status 2.
fn refuse(stderr: &mut dyn Write, Usage(message): &Usage) -> u8 {
    // Standard error is the last stream to report on, so a failed write to
    // it is not reported.
    let _ = writeln!(stderr, "yamaa: {message}\nrun 'yamaa --help' for usage");
    NOT_RUN
}

/// Write `text` to standard output, returning status 0; a failed write
/// exits with status 2.
fn print(stdout: &mut dyn Write, stderr: &mut dyn Write, text: &str) -> u8 {
    match stdout
        .write_all(text.as_bytes())
        .and_then(|()| stdout.flush())
    {
        Ok(()) => SUCCEEDED,
        Err(error) => {
            let _ = writeln!(stderr, "yamaa: cannot write to standard output: {error}");
            NOT_RUN
        }
    }
}

/// `yamaa --version`: the engine information (D41, P-INFO-1). As text, one
/// field per line; the command line runs on no interpreter, so it has no
/// host version line. As JSON, one object with `cli.json.info_keys`, whose
/// `host_version` is null.
fn version(format: Format) -> String {
    let info = yamaa::info(yamaa::Host::Cli, None);
    match format {
        Format::Text => format!(
            "yamaa {}\nlanguage versions: {}\nfeatures: {}\nhost: {}\nplatform: {}\n",
            info.version,
            info.language_versions.join(", "),
            info.features.join(", "),
            info.host.as_str(),
            info.platform,
        ),
        Format::Json => format!("{}\n", commands::info(&info).to_json()),
    }
}

/// `yamaa --help`: the synopses, the commands and the exit statuses.
fn help() -> String {
    let mut text = String::from(
        "usage: yamaa [--format text|json] COMMAND [ARGUMENTS]\n       \
         yamaa --version [--format text|json]\n       \
         yamaa --help\n\ncommands:\n",
    );
    for command in &COMMANDS {
        text.push_str("  ");
        text.push_str(command.synopsis);
        text.push('\n');
    }
    text.push_str(
        "\nPATH names a spec or a project, as its type: header declares; --spec,\n\
         --section and submission take a project. read-odm saves the clinical\n\
         items of SOURCE, an ODM XML file or a TAR archive that holds one, to\n\
         OUT, a .csv or .parquet file. schema prints the JSON Schema of the file\n\
         type TYPE: spec, project, codelists or function. conditions prints the\n\
         catalogue of the conditions the engine may report and those a release\n\
         retired. selftest runs the cases the binary carries and compares each\n\
         with its expected files.\n\n\
         exit status: 0 succeeded, 1 completed with findings, 2 could not run as asked\n",
    );
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;

    /// A standard output that refuses every write, as a closed pipe does.
    struct Refusing;

    impl Write for Refusing {
        fn write(&mut self, _: &[u8]) -> io::Result<usize> {
            Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "the reader is gone",
            ))
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    /// A standard output that takes every write and then fails to flush, as
    /// a full disk can.
    struct Full;

    impl Write for Full {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            Ok(bytes.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Err(io::Error::other("the disk is full"))
        }
    }

    fn words(words: &[&str]) -> Vec<OsString> {
        words.iter().map(OsString::from).collect()
    }

    /// D46, D74: every command that takes a token uses the caller's token,
    /// including every run route and the saves those routes compose.
    #[test]
    fn cancelled_commands_publish_and_print_nothing() {
        struct Study(std::path::PathBuf);
        impl Drop for Study {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
        let study =
            Study(std::env::temp_dir().join(format!("yamaa-cli-cancelled-{}", std::process::id())));
        std::fs::create_dir_all(study.0.join("out")).unwrap();
        std::fs::write(study.0.join("raw.csv"), "USUBJID\nS1\n").unwrap();
        std::fs::write(
            study.0.join("dm.yaml"),
            "yamaa: '1.0'\ntype: spec\nmodel: SDTM\ndomain: DM\nkeys: [USUBJID]\n\
             input: {RAW: {path: raw.csv}}\ncolumns:\n\
             - {name: USUBJID, type: str, label: Subject, derivation: RAW.USUBJID}\n\
             output: {path: out/dm.csv, columns: [USUBJID], report: out/report.csv}\n",
        )
        .unwrap();
        std::fs::write(
            study.0.join("project.yaml"),
            "yamaa: '1.0'\ntype: project\nsdtm: {specs: [dm.yaml]}\n",
        )
        .unwrap();
        let spec = study.0.join("dm.yaml").to_str().unwrap().to_owned();
        let project = study.0.join("project.yaml").to_str().unwrap().to_owned();
        let token = yamaa::CancellationToken::new();
        token.cancel();
        for arguments in [
            vec!["check", &spec],
            vec!["check", &project],
            vec!["check", &project, "--spec", "DM"],
            vec!["run", &spec],
            vec!["run", &project],
            vec!["run", &project, "--spec", "DM"],
            vec!["run", &project, "--section", "sdtm"],
            vec!["submission", &project],
        ] {
            let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
            run(
                arguments::parse(&words(&arguments)),
                &mut stdout,
                &mut stderr,
                &token,
            );
            let files = std::fs::read_dir(study.0.join("out"))
                .unwrap()
                .map(|entry| entry.unwrap().file_name())
                .collect::<Vec<_>>();
            assert_eq!(
                (stdout, stderr, files),
                (Vec::<u8>::new(), Vec::<u8>::new(), Vec::new()),
                "{arguments:?}"
            );
        }
    }

    /// A failed write exits 2 whatever the command found: here a lint whose
    /// finding would otherwise exit 1.
    #[test]
    fn a_failed_write_to_standard_output_exits_with_status_2() {
        let spec =
            std::env::temp_dir().join(format!("yamaa-cli-write-{}.yaml", std::process::id()));
        std::fs::write(
            &spec,
            "yamaa: \"1.0\"\ntype: spec\ndomain: DM\noutput:\n  path: dm.csv\n",
        )
        .unwrap();
        let spec = spec.to_str().unwrap().to_owned();
        for arguments in [
            &["--version"][..],
            &["--version", "--format", "json"],
            &["--help"],
            &["lint", &spec],
            &["lint", &spec, "--format", "json"],
        ] {
            let mut stderr = Vec::new();
            assert_eq!(
                run(
                    arguments::parse(&words(arguments)),
                    &mut Refusing,
                    &mut stderr,
                    &yamaa::CancellationToken::new()
                ),
                2
            );
            assert_eq!(
                String::from_utf8(stderr).unwrap(),
                "yamaa: cannot write to standard output: the reader is gone\n"
            );
            let mut stderr = Vec::new();
            assert_eq!(
                run(
                    arguments::parse(&words(arguments)),
                    &mut Full,
                    &mut stderr,
                    &yamaa::CancellationToken::new()
                ),
                2
            );
            assert_eq!(
                String::from_utf8(stderr).unwrap(),
                "yamaa: cannot write to standard output: the disk is full\n"
            );
        }
        std::fs::remove_file(spec).unwrap();
    }

    #[test]
    fn version_and_help_write_only_to_standard_output() {
        for (arguments, expected) in [
            (&["--version"][..], version(Format::Text)),
            (&["--version", "--format", "json"], version(Format::Json)),
            (&["--help"], help()),
        ] {
            let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
            assert_eq!(
                run(
                    arguments::parse(&words(arguments)),
                    &mut stdout,
                    &mut stderr,
                    &yamaa::CancellationToken::new()
                ),
                0
            );
            assert_eq!(String::from_utf8(stdout).unwrap(), expected);
            assert!(stderr.is_empty());
        }
    }

    /// `--format json` prints `info()` as one object, with `host_version`
    /// null for the command line.
    #[test]
    fn version_as_json_holds_the_six_fields_of_info() {
        let info = yamaa::info(yamaa::Host::Cli, None);
        assert_eq!(
            version(Format::Json),
            format!(
                "{{\"features\":[{}],\"host\":\"cli\",\"host_version\":null,\
                 \"language_versions\":[\"1.0\"],\"platform\":\"{}\",\"version\":\"{}\"}}\n",
                info.features
                    .iter()
                    .map(|feature| format!("\"{feature}\""))
                    .collect::<Vec<_>>()
                    .join(","),
                info.platform,
                yamaa::VERSION
            )
        );
    }

    #[test]
    fn help_lists_every_command_once_in_order() {
        let text = help();
        let listed = text
            .lines()
            .skip_while(|line| *line != "commands:")
            .skip(1)
            .take_while(|line| !line.is_empty())
            .map(|line| line.trim_start().split(' ').next().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(
            listed,
            COMMANDS
                .iter()
                .map(|command| command.name)
                .collect::<Vec<_>>()
        );
    }
}
