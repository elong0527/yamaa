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
//! handler.
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

use std::io::{self, Write};
use std::process::ExitCode;
use std::sync::Arc;

use yamaa::cli::{self, Functions, Neutral, Reporter, Runtime, NOT_RUN};
use yamaa::{EngineOptions, Host};

/// The binary's runtime: the neutral runtime, which runs no project
/// function, except for a `run` that settled on a language, whose
/// functions a [`worker::Worker`] of that language runs.
struct Standalone;

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
        ))))
    }

    fn reporter(&self) -> Reporter {
        Neutral.reporter()
    }
}

fn main() -> ExitCode {
    let arguments = std::env::args_os().skip(1).collect::<Vec<_>>();
    // Only these commands have facade cancellation checkpoints. Other
    // commands retain the operating system's immediate signal handling.
    let cooperative = cli::cooperative(&arguments);
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
    let status = cli::run(
        &arguments,
        &Standalone,
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
