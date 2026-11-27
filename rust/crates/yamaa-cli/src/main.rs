//! The `yamaa` command line (#3030 D12, section 2.8): one binary over the
//! `yamaa` facade alone (section 2.10).
//!
//! The commands, what they print on which stream and their exit statuses
//! are the facade's `yamaa::cli`, which the Python and R packages run too,
//! over their own engines. This binary runs them over [`Standalone`]: an
//! engine that claims no host language (D110, D123) until a `run` of a
//! project settles the language of the project functions it calls, with
//! `--host` or from the project file, which an interpreter's worker then
//! runs ([`worker`]); on the process's standard streams, with its Ctrl+C
//! handler. Every line it writes to standard error, its refusals, each
//! `--verbose` progress line and each failure of a worker, goes through the
//! one writer it gives `cli::run`, a [`Stderr`].
//!
//! Ctrl+C stops `check`, `run` and `submission` at the facade's next
//! cancellation checkpoint, so they print nothing and a save writes every
//! file it planned or none (D46, D74); it stops the other commands at once,
//! so an interrupted `lint --fix`, which replaces each file atomically, can
//! leave some files fixed and others not. The process then ends the native
//! way, with none of the exit statuses (D203): on POSIX it re-raises
//! SIGINT, which shells report as status 130, and on Windows it exits with
//! `STATUS_CONTROL_C_EXIT`.
#![deny(unsafe_code)]

// The process's signal/console boundary is the only code that calls OS APIs.
#[allow(unsafe_code)]
mod interrupt;
mod worker;

use std::fmt;
use std::io::{self, Write};
use std::process::ExitCode;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use yamaa::cli::{self, Functions, Neutral, Reporter, Runtime, NOT_RUN};
use yamaa::{EngineOptions, Host};

/// The binary's standard error, which every line it writes there shares
/// (#5111): what `cli::run` writes, each `--verbose` progress line, which
/// the engine reports as it happens, and each failure of a worker, which a
/// thread of the command may meet while the calling thread waits for it.
/// Each write takes the stream for itself alone and gives it back, so no
/// thread holds it across the command's work (#5074), and no line is split
/// by another's.
#[derive(Clone)]
pub(crate) struct Stderr(Arc<Mutex<dyn Write + Send>>);

impl Stderr {
    pub(crate) fn new(stream: impl Write + Send + 'static) -> Self {
        Self(Arc::new(Mutex::new(stream)))
    }

    /// Write `line` and its line feed at once. A line that cannot be
    /// written is lost, and the command goes on.
    pub(crate) fn line(&self, line: &str) {
        let _ = self.stream().write_all(format!("{line}\n").as_bytes());
    }

    /// The stream, taken for one write; a thread that panicked while it
    /// wrote leaves it to the others.
    fn stream(&self) -> MutexGuard<'_, dyn Write + Send + 'static> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl Write for Stderr {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.stream().write(bytes)
    }

    fn write_all(&mut self, bytes: &[u8]) -> io::Result<()> {
        self.stream().write_all(bytes)
    }

    fn write_fmt(&mut self, arguments: fmt::Arguments<'_>) -> io::Result<()> {
        self.stream().write_fmt(arguments)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.stream().flush()
    }
}

/// The binary's runtime: the neutral runtime, which runs no project
/// function, except for a `run` that settled on a language, whose
/// functions a [`worker::Worker`] of that language runs; its progress lines
/// and its worker's failures go to its standard error.
struct Standalone {
    stderr: Stderr,
}

impl Runtime for Standalone {
    fn host(&self) -> (Host, Option<String>) {
        Neutral.host()
    }

    fn settles_functions(&self) -> bool {
        true
    }

    fn options(&self, functions: Option<&Functions<'_>>) -> Result<EngineOptions, String> {
        let Some(functions) = functions else {
            return Neutral.options(None);
        };
        Ok(EngineOptions::new(
            functions.language,
            Host::Cli.as_str().to_owned(),
            String::new(),
        )
        .with_functions(Arc::new(worker::Worker::new(
            functions.language,
            functions.interpreter.map(str::to_owned),
            self.stderr.clone(),
        ))))
    }

    fn reporter(&self) -> Reporter {
        let stderr = self.stderr.clone();
        Arc::new(move |line: &str| stderr.line(line))
    }
}

fn main() -> ExitCode {
    let arguments = std::env::args_os().skip(1).collect::<Vec<_>>();
    let mut stderr = Stderr::new(io::stderr());
    // Only these commands have facade cancellation checkpoints. Other
    // commands retain the operating system's immediate signal handling.
    let cooperative = yamaa::host_internal::cooperative(&arguments);
    let token = match if cooperative {
        interrupt::install()
    } else {
        Ok(yamaa::CancellationToken::new())
    } {
        Ok(token) => token,
        Err(error) => {
            stderr.line(&format!("yamaa: cannot install Ctrl+C handler: {error}"));
            return ExitCode::from(NOT_RUN);
        }
    };
    // The streams stay unlocked between writes: a thread the command starts
    // may write to standard error, as a worker's failure does, while this
    // one waits for it (#5074).
    let runtime = Standalone {
        stderr: stderr.clone(),
    };
    let status = cli::run(&arguments, &runtime, &mut io::stdout(), &mut stderr, &token);
    // All engine-owned state and staged files have unwound before the
    // original interrupt terminates the process (D46, D74).
    if cooperative {
        interrupt::finish();
    }
    ExitCode::from(status)
}

#[cfg(test)]
mod tests {
    // The seam: `cli::run` given a caller's standard error. The binary's
    // integration tests read the process's stream, which every writer
    // reaches, so only a caller in the process sees that a progress line
    // and a worker's failure go through the writer it gave.
    use super::*;

    /// A standard error the test reads back once the command has run.
    #[derive(Clone, Default)]
    struct Captured(Arc<Mutex<Vec<u8>>>);

    impl Write for Captured {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.0.lock().unwrap().write(bytes)
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    /// A scratch study, removed when the test ends.
    struct Study(std::path::PathBuf);

    impl Drop for Study {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// A `--verbose` run of a project whose one specification, CALLS, calls
    /// a Python function, with an interpreter that does not exist: each
    /// progress event in order, the run's `preparation`, then `activation`,
    /// which starts the worker, whose failure fails it, with the failure's
    /// line as it happens; then `output`, since the save of a failed run
    /// still publishes its report (OUTPUT-10), here none; no division, since
    /// the run never reached its input; then the timings table of the three
    /// phases (docs/articles/api.md, "Progress and timings" and the command
    /// line's "Output"). Spawning
    /// the interpreter fails with the operating system's "not found", code 2
    /// on every platform. The milliseconds are facts of the call, so each is
    /// checked as a number and shown as `ms`.
    #[test]
    fn progress_lines_and_worker_failures_go_through_the_callers_standard_error() {
        let study =
            Study(std::env::temp_dir().join(format!("yamaa-cli-stderr-{}", std::process::id())));
        std::fs::create_dir_all(study.0.join("functions")).unwrap();
        for (name, text) in [
            ("raw.csv", "ID,V\n1,a\n"),
            (
                "functions/shout.yaml",
                "yamaa: \"1.0\"\ntype: function\nname: shout\ndescription: The value in capitals.\n\
                 params: [{name: x, type: str}]\nreturns: str\nimplementations: {python: pkg.shout}\n\
                 tests:\n  - {id: normal, covers: [normal], args: {x: a}, result: A}\n  \
                 - {id: empty, covers: [boundary], args: {x: ''}, result: ''}\n  \
                 - {id: missing, covers: ['short-circuit-missing:x'], args: {x: null}, result: null}\n",
            ),
            (
                "calls.yaml",
                "yamaa: \"1.0\"\ntype: spec\ndomain: CALLS\nkeys: [ID]\ninput: {RAW: raw.csv}\n\
                 output: {path: calls.csv, columns: [ID, LOUD]}\ncolumns:\n  \
                 - {name: ID, type: str, label: Identifier, derivation: RAW.ID}\n  \
                 - {name: LOUD, type: str, label: Loud, derivation: {function: {name: shout, args: {x: RAW.V}}}}\n",
            ),
            (
                "project.yaml",
                "yamaa: \"1.0\"\ntype: project\nfunctions: [functions/shout.yaml]\nadam: {specs: [calls.yaml]}\n",
            ),
        ] {
            std::fs::write(study.0.join(name), text).unwrap();
        }
        let project = study.0.join("project.yaml");
        let arguments = [
            "run".into(),
            project.into_os_string(),
            "--interpreter".into(),
            "yamaa-no-such-interpreter".into(),
            "--verbose".into(),
        ];
        let captured = Captured::default();
        let mut stderr = Stderr::new(captured.clone());
        let runtime = Standalone {
            stderr: stderr.clone(),
        };
        let mut stdout = Vec::new();
        let status = cli::run(
            &arguments,
            &runtime,
            &mut stdout,
            &mut stderr,
            &yamaa::CancellationToken::new(),
        );
        let written = String::from_utf8(captured.0.lock().unwrap().clone()).unwrap();
        let shown = written
            .lines()
            .map(|line| {
                let mut cells = line.split('\t').collect::<Vec<_>>();
                if cells.len() == 5 && cells[0] != "name" {
                    for cell in &mut cells[2..4] {
                        assert!(
                            cell.parse::<f64>().is_ok_and(|value| value >= 0.0),
                            "{line}"
                        );
                        *cell = "ms";
                    }
                }
                cells.join("\t")
            })
            .collect::<Vec<_>>();
        let not_found = io::Error::from_raw_os_error(2);
        assert_eq!(
            (status, shown),
            (
                cli::FINDINGS,
                vec![
                    "CALLS: preparation start".to_owned(),
                    "CALLS: preparation end: succeeded".to_owned(),
                    "CALLS: activation start".to_owned(),
                    format!(
                        "yamaa: cannot start the python worker: yamaa-no-such-interpreter: {not_found}"
                    ),
                    "CALLS: activation end: failed".to_owned(),
                    "CALLS: output start".to_owned(),
                    "CALLS: output end: succeeded".to_owned(),
                    "name\tphase\tstarted_ms\telapsed_ms\toutcome".to_owned(),
                    "CALLS\tpreparation\tms\tms\tsucceeded".to_owned(),
                    "CALLS\tactivation\tms\tms\tfailed".to_owned(),
                    "CALLS\toutput\tms\tms\tsucceeded".to_owned(),
                ]
            ),
            "the run's standard error: {written}"
        );
    }
}
