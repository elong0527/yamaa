//! The command line as a facade use case (#3030 D12, section 2.8): the
//! commands of the `yamaa` binary, which any client runs over its own
//! engine.
//!
//! The `yamaa` binary of `rust/crates/yamaa-cli` runs them over an engine
//! that claims no host language (D110, D123), so it runs no project
//! function. The Python and R packages run the same commands over their own
//! engine, with their installed project functions, through `python -m yamaa`
//! and `yamaa_cli()`. What differs between those clients is a [`Runtime`]:
//! the engine options of every command, the host that `--version` and
//! `selftest` name, and where `--verbose` writes a progress line as it
//! happens. Everything else, the arguments, what each command prints and on
//! which stream, and its exit status, is one implementation here, so the
//! three entries print the same words.
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
//! - [`SUCCEEDED`], 0, when the command completed and succeeded, or
//!   `--help` or `--version` printed its text;
//! - [`FINDINGS`], 1, when the command completed with findings: a `failed`
//!   or `skipped` status, remaining lint findings, a failed selftest case,
//!   or a refusal, such as an unknown section (D98) or `schema` TYPE
//!   (D149), or a PATH whose type cannot be read (D125), which the facade
//!   explains with its issues;
//! - [`NOT_RUN`], 2, when the command could not run as asked (D68): a usage
//!   error, an argument the facade cannot take, a PATH whose type the
//!   command does not take, or a failed write to standard output.
//!
//! No configuration file, environment variable or colour changes what a
//! command does. A cancelled `check`, `run` or `submission` prints nothing
//! and a save writes every file it planned or none (D46, D74): the client
//! owns the token and what happens to the process after it.
//!
//! What each command prints, and how it dispatches on its PATH's `type:`
//! header (D122), is in `commands`; the grammar of the arguments is in
//! `arguments`.

mod arguments;
mod commands;

use std::ffi::OsString;
use std::io::Write;
use std::sync::Arc;

use crate::{CancellationToken, EngineOptions, Host, HostLanguage};

use arguments::{Format, Invocation, Usage, COMMANDS};
use commands::Execution;

/// Exit status 0: the command completed and succeeded.
pub const SUCCEEDED: u8 = 0;
/// Exit status 1: the command completed with findings.
pub const FINDINGS: u8 = 1;
/// Exit status 2: the command could not run as asked.
pub const NOT_RUN: u8 = 2;

/// Where a command's progress lines go under `--verbose`, as they happen.
pub type Reporter = Arc<dyn Fn(&str) + Send + Sync>;

/// The project functions a `run` of a project asks a runtime that settles
/// them for ([`Runtime::settles_functions`]): the host language the command
/// settled on, from `--host` or the project file
/// ([`crate::ProjectRequest::host_languages`]), and the interpreter the user
/// named with `--interpreter`, if any.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Functions<'a> {
    pub language: HostLanguage,
    pub interpreter: Option<&'a str>,
}

/// What differs between the clients that run the commands: the binary, and
/// each host package's entry.
pub trait Runtime: Send + Sync {
    /// The host that `--version` and `selftest` name, and its version: the
    /// command line's `cli` with none, or a host language with the
    /// interpreter's.
    fn host(&self) -> (Host, Option<String>);

    /// Whether a `run` of a project settles the language of the project
    /// functions it may call and asks the runtime for them: true for a
    /// runtime whose engine claims no host language of its own, the
    /// standalone binary's, which then takes `--host` and `--interpreter`;
    /// false, the default, for a host package, which runs its own functions
    /// in its own language and refuses both flags as a usage error.
    fn settles_functions(&self) -> bool {
        false
    }

    /// The options of every engine the commands build: the host language,
    /// the runtime identity and the project functions. The commands add the
    /// partitions, `--threads` and the `--verbose` progress callback. A
    /// `run` of a project gives a runtime that settles functions the ones it
    /// settled on, or none when the project names no one language; the
    /// other commands, and every command of a host package, give none. A
    /// runtime that cannot run the functions asked of it says why: a usage
    /// error, which exits 2.
    fn options(&self, functions: Option<&Functions<'_>>) -> Result<EngineOptions, String>;

    /// Where `--verbose` writes each progress line as it happens.
    fn reporter(&self) -> Reporter;
}

/// The neutral runtime: an engine that claims no host language (#3030
/// section 2.10, D123), so it reads no project's `environments.<host>`
/// entry, verifies no lock and activates nothing, and a build that calls a
/// project function is refused before it reads data (D110). It settles the
/// functions of a `run`, as the binary's runtime does, but runs none: the
/// binary's own runtime runs them through an interpreter's worker, and
/// answers for a project that settles on a language where this refuses. Its
/// progress lines go to the process's standard error.
pub struct Neutral;

impl Runtime for Neutral {
    fn host(&self) -> (Host, Option<String>) {
        (Host::Cli, None)
    }

    fn settles_functions(&self) -> bool {
        true
    }

    fn options(&self, functions: Option<&Functions<'_>>) -> Result<EngineOptions, String> {
        match functions {
            Some(functions) => Err(format!(
                "this command line runs no {} project function",
                functions.language.as_str()
            )),
            None => Ok(EngineOptions::neutral(
                Host::Cli.as_str().to_owned(),
                String::new(),
            )),
        }
    }

    fn reporter(&self) -> Reporter {
        Arc::new(|line: &str| {
            // A line that cannot be written is lost, and the command goes on.
            let _ = writeln!(std::io::stderr(), "{line}");
        })
    }
}

/// Whether `arguments` name a command with cancellation checkpoints,
/// `check`, `run` or `submission`, whose token the client polls; the other
/// commands run to their end.
pub fn cooperative(arguments: &[OsString]) -> bool {
    matches!(
        arguments::parse(arguments),
        Ok(Invocation::Command(call, _)) if matches!(call.name, "check" | "run" | "submission")
    )
}

/// Carry out the command line `arguments`, the words after the program
/// name, over `runtime`, writing to `stdout` and `stderr`, and return the
/// exit status. A command the `token` cancelled prints nothing and returns
/// [`SUCCEEDED`]: the client reports the cancellation its own way.
pub fn run(
    arguments: &[OsString],
    runtime: &dyn Runtime,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
    token: &CancellationToken,
) -> u8 {
    match arguments::parse(arguments) {
        Ok(Invocation::Help) => print(stdout, stderr, &help()),
        Ok(Invocation::Version(format)) => print(stdout, stderr, &version(format, runtime)),
        Ok(Invocation::Command(call, format)) => {
            let execution = commands::execute(&call, format, token, runtime);
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
/// field per line, with a `host version:` line only for a host that runs on
/// an interpreter: the command line runs on none, so it has no such line. As
/// JSON, one object with `cli.json.info_keys`, whose `host_version` is null
/// for the command line.
fn version(format: Format, runtime: &dyn Runtime) -> String {
    let (host, host_version) = runtime.host();
    let info = crate::info(host, host_version.as_deref());
    match format {
        Format::Text => {
            let mut text = format!(
                "yamaa {}\nlanguage versions: {}\nfeatures: {}\nhost: {}\n",
                info.version,
                info.language_versions.join(", "),
                info.features.join(", "),
                info.host.as_str(),
            );
            if let Some(host_version) = &info.host_version {
                text.push_str(&format!("host version: {host_version}\n"));
            }
            text.push_str(&format!("platform: {}\n", info.platform));
            text
        }
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
    use std::io;

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

    /// A host's runtime: the Python host on an interpreter, over a neutral
    /// engine, which drops its progress lines.
    struct Hosted;

    impl Runtime for Hosted {
        fn host(&self) -> (Host, Option<String>) {
            (Host::Python, Some("3.12.4".to_owned()))
        }

        fn options(&self, functions: Option<&Functions<'_>>) -> Result<EngineOptions, String> {
            assert!(functions.is_none(), "a host runtime settles no functions");
            Ok(EngineOptions::neutral(
                "python".to_owned(),
                "3.12.4".to_owned(),
            ))
        }

        fn reporter(&self) -> Reporter {
            Arc::new(|_: &str| {})
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
        let token = CancellationToken::new();
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
            assert!(cooperative(&words(&arguments)), "{arguments:?}");
            let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
            run(
                &words(&arguments),
                &Neutral,
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
        for arguments in [
            vec!["lint", &spec],
            vec!["schema", "spec"],
            vec!["--version"],
            vec!["conditions"],
        ] {
            assert!(!cooperative(&words(&arguments)), "{arguments:?}");
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
                    &words(arguments),
                    &Neutral,
                    &mut Refusing,
                    &mut stderr,
                    &CancellationToken::new()
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
                    &words(arguments),
                    &Neutral,
                    &mut Full,
                    &mut stderr,
                    &CancellationToken::new()
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
            (&["--version"][..], version(Format::Text, &Neutral)),
            (
                &["--version", "--format", "json"],
                version(Format::Json, &Neutral),
            ),
            (&["--help"], help()),
        ] {
            let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
            assert_eq!(
                run(
                    &words(arguments),
                    &Neutral,
                    &mut stdout,
                    &mut stderr,
                    &CancellationToken::new()
                ),
                0
            );
            assert_eq!(String::from_utf8(stdout).unwrap(), expected);
            assert!(stderr.is_empty());
        }
    }

    /// `--format json` prints `info()` as one object, with `host_version`
    /// null for the command line; a host's runtime names its host and its
    /// version, as text on a line of its own.
    #[test]
    fn version_names_the_runtimes_host_in_both_formats() {
        let info = crate::info(Host::Cli, None);
        let features = info
            .features
            .iter()
            .map(|feature| format!("\"{feature}\""))
            .collect::<Vec<_>>()
            .join(",");
        assert_eq!(
            version(Format::Json, &Neutral),
            format!(
                "{{\"features\":[{features}],\"host\":\"cli\",\"host_version\":null,\
                 \"language_versions\":[\"1.0\"],\"platform\":\"{}\",\"version\":\"{}\"}}\n",
                info.platform,
                crate::VERSION
            )
        );
        assert_eq!(
            version(Format::Text, &Neutral),
            format!(
                "yamaa {}\nlanguage versions: 1.0\nfeatures: {}\nhost: cli\nplatform: {}\n",
                crate::VERSION,
                info.features.join(", "),
                info.platform
            )
        );
        let hosted = Hosted;
        assert_eq!(
            version(Format::Json, &hosted),
            format!(
                "{{\"features\":[{features}],\"host\":\"python\",\"host_version\":\"3.12.4\",\
                 \"language_versions\":[\"1.0\"],\"platform\":\"{}\",\"version\":\"{}\"}}\n",
                info.platform,
                crate::VERSION
            )
        );
        assert_eq!(
            version(Format::Text, &hosted),
            format!(
                "yamaa {}\nlanguage versions: 1.0\nfeatures: {}\nhost: python\n\
                 host version: 3.12.4\nplatform: {}\n",
                crate::VERSION,
                info.features.join(", "),
                info.platform
            )
        );
    }

    /// A host's runtime runs `selftest` as that host, so its report names it.
    #[test]
    fn selftest_reports_the_runtimes_host() {
        let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
        let status = run(
            &words(&["--format", "json", "selftest"]),
            &Hosted,
            &mut stdout,
            &mut stderr,
            &CancellationToken::new(),
        );
        let printed = String::from_utf8(stdout).unwrap();
        assert!(
            printed.contains("\"host\":\"python\",\"host_version\":\"3.12.4\""),
            "{printed}"
        );
        assert!(stderr.is_empty());
        let expected = match crate::selftest(Host::Python, Some("3.12.4")).status() {
            crate::Status::Succeeded => SUCCEEDED,
            _ => FINDINGS,
        };
        assert_eq!(status, expected);
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
