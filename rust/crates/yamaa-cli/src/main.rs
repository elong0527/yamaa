//! The `yamaa` command line (#3030 D12, section 2.8): one binary over the
//! `yamaa` facade alone (section 2.10).
//!
//! Standard output carries what a command prints; standard error carries only
//! the refusals of exit status 2, never anything else. The exit status is:
//!
//! - 0 when the command completed and succeeded, or `--help` or `--version`
//!   printed its text;
//! - 1 when the command completed with findings; no command reports any yet;
//! - 2 when the command could not run as asked (D68): a usage error or a
//!   failed write to standard output, and, for now, a command not
//!   implemented yet.
//!
//! No configuration file, environment variable or colour changes what it
//! does. Ctrl+C ends the process by the signal and publishes nothing (D46).
//!
//! Only `--help` and `--version` work so far. Every command checks its
//! arguments and then refuses as not implemented yet, and so does
//! `--format json`: the commands print the summaries and JSON of #3248 and
//! save through #3247, which the facade does not offer yet. This refusal is
//! temporary; each command drops it when it lands.
#![forbid(unsafe_code)]

mod arguments;

use std::ffi::OsString;
use std::io::{self, Write};
use std::process::ExitCode;

use arguments::{Format, Invocation, Usage, COMMANDS};

/// Exit status 0: the command completed and succeeded.
const SUCCEEDED: u8 = 0;
/// Exit status 2: the command could not run as asked.
const NOT_RUN: u8 = 2;

fn main() -> ExitCode {
    let arguments = std::env::args_os().skip(1).collect::<Vec<_>>();
    let status = run(
        &arguments,
        &mut io::stdout().lock(),
        &mut io::stderr().lock(),
    );
    ExitCode::from(status)
}

/// Carry out what `arguments` ask for, writing to `stdout` and `stderr`, and
/// return the exit status.
fn run(arguments: &[OsString], stdout: &mut dyn Write, stderr: &mut dyn Write) -> u8 {
    match arguments::parse(arguments) {
        Ok(Invocation::Help) => print(stdout, stderr, &help()),
        Ok(Invocation::Version(Format::Text)) => print(stdout, stderr, &version()),
        Ok(Invocation::Version(Format::Json) | Invocation::Command(_, Format::Json)) => {
            not_implemented(stderr, "--format json")
        }
        Ok(Invocation::Command(name, Format::Text)) => not_implemented(stderr, name),
        Err(Usage(message)) => {
            // Standard error is the last stream to report on, so a failed
            // write to it is not reported.
            let _ = writeln!(stderr, "yamaa: {message}\nrun 'yamaa --help' for usage");
            NOT_RUN
        }
    }
}

/// Write `text` to standard output; a failed write exits with status 2.
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

/// Refuse what the command line does not do yet, naming it. Temporary: see
/// the crate documentation.
fn not_implemented(stderr: &mut dyn Write, what: &str) -> u8 {
    let _ = writeln!(stderr, "yamaa: {what} is not implemented yet");
    NOT_RUN
}

/// `yamaa --version`: the engine information (D41, P-INFO-1), one field per
/// line. The command line runs on no interpreter, so it has no host version
/// line.
fn version() -> String {
    let info = yamaa::info(yamaa::Host::Cli, None);
    format!(
        "yamaa {}\nlanguage versions: {}\nfeatures: {}\nhost: {}\nplatform: {}\n",
        info.version,
        info.language_versions.join(", "),
        info.features.join(", "),
        info.host.as_str(),
        info.platform,
    )
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
        "\nThe commands and --format json are not implemented yet: each checks its\n\
         arguments, then exits with status 2.\n\n\
         exit status: 0 succeeded, 1 completed with findings, 2 could not run as asked\n",
    );
    text
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn a_failed_write_to_standard_output_exits_with_status_2() {
        for arguments in [&["--version"][..], &["--help"]] {
            let mut stderr = Vec::new();
            assert_eq!(run(&words(arguments), &mut Refusing, &mut stderr), 2);
            assert_eq!(
                String::from_utf8(stderr).unwrap(),
                "yamaa: cannot write to standard output: the reader is gone\n"
            );
            let mut stderr = Vec::new();
            assert_eq!(run(&words(arguments), &mut Full, &mut stderr), 2);
            assert_eq!(
                String::from_utf8(stderr).unwrap(),
                "yamaa: cannot write to standard output: the disk is full\n"
            );
        }
    }

    #[test]
    fn version_and_help_write_only_to_standard_output() {
        for (arguments, expected) in [(&["--version"][..], version()), (&["--help"], help())] {
            let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
            assert_eq!(run(&words(arguments), &mut stdout, &mut stderr), 0);
            assert_eq!(String::from_utf8(stdout).unwrap(), expected);
            assert!(stderr.is_empty());
        }
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
