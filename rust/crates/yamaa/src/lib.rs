//! The supported Rust API: a small facade over `yamaa-adapters` and
//! `yamaa-core`.
//!
//! An embedder configures one [`Engine`] and calls its use cases: check, run
//! and save, submission and resolve through the request [`Engine::spec`] or
//! [`Engine::project`] gives, and `read_odm` and `file_type`, beside the free
//! [`lint`], [`schema()`], [`conditions()`], [`info`] and [`selftest`]. The
//! facade wraps the existing prepare, bind, execute and publish workflow;
//! there is no second admission or evaluation route. All physical access
//! goes through a resolver; local files are just the default resolver. The
//! use cases carry runnable examples, and `examples/memory.rs` runs one
//! specification entirely from memory.
//!
//! An embedder looks at the crate root, which re-exports what configures an
//! engine or comes back from one: the engine, its options and requests, the
//! results, tables and diagnostics, the [`CancellationToken`], the
//! [`ResourceResolver`] or [`MemoryResolver`] and the [`ProjectFunctions`]
//! of a [`HostLanguage`] that an engine is configured with, the
//! [`InputFrame`] a resolver serves, and, with the `arrow` feature, the
//! Arrow types of a table's Arrow form. A binding author, who implements a
//! port, looks in [`host`] and [`scalar`]: [`scalar`] holds the values
//! project functions exchange, and [`host::functions`] and
//! [`host::resolver`] every other type the two ports take and return.
//!
//! Adapters and core may change in any release without notice; their `pub`
//! items are visibility for the workspace, not a public API. Every public
//! type here is facade-owned, and adapters or core types are converted at the
//! boundary. Core keeps its boundary (no I/O, no `unsafe`, no Arrow).
//!
//! The facade forwards the `yaml`, `parquet`, `readstat` and `odm` format
//! features to `yamaa-adapters`, all on by default, and builds with any
//! combination of them. With no format feature, the CSV route remains. The
//! `arrow` feature, also on by default, is not a format: it gives a table its
//! Arrow form, admits host frames and reads an output as a stream, through
//! the Arrow types this crate re-exports, `Table::batches` and
//! `Table::stream` among them, `InputFrame::import`, and `SpecRequest::stream`
//! with its streamed-run types. A build without it links no Arrow crate unless
//! `parquet` brings Arrow for its own reader.
//! A capability a build omits fails the same way it does in adapters: the
//! stable codes (`yaml_reader_unavailable`, `parquet_reader_unavailable`,
//! `sas_reader_unavailable`, `odm_reader_unavailable`,
//! `parquet_codec_unavailable`) surface as diagnostics or as a
//! [`FailureKind::Unavailable`] failure, and [`Engine::read_odm`] without
//! `odm` fails with `FailureKind::Unavailable { feature: "odm" }`. The facade
//! adds no native dependencies of its own.
//!
//! Malformed Parquet input can make the Parquet or Arrow crate panic. The
//! engine contains each such panic as a finding and keeps it off stderr: its
//! first native decode wraps the process's panic hook, once. The wrapper
//! prints nothing for a panic inside the engine's calls into those crates to
//! decode Parquet, the file's embedded Arrow schema included. It passes every
//! other panic to the hook it wraps: a panic anywhere else, such as in Arrow's
//! import of a host table, and a further panic on the same thread while a
//! contained one unwinds, after which the process aborts.
//!
//! Stable Rust has no atomic hook update (`std::panic::update_hook` is
//! unstable), so the wrapper is installed in two steps: the installed hook is
//! taken, then the wrapper set. A panic on another thread between the two is
//! reported by Rust's default hook, and a hook that another thread sets
//! between them is replaced. An embedder with its own hook should set it
//! before the engine's first Parquet read. A hook set later replaces the
//! wrapper and runs for every panic, contained ones included; when it calls
//! the hook it replaced, only that call stays silent.
#![deny(unsafe_code)]

pub mod api;
pub mod cancel;
pub mod conditions;
pub mod diagnostic;
pub mod engine;
pub mod error;
pub mod file_type;
pub mod host;
pub mod info;
pub mod lint;
pub mod odm;
pub mod progress;
pub mod scalar;
pub mod schema;
pub mod selftest;
#[cfg(feature = "arrow")]
pub mod stream;
pub mod table;
pub mod text;

mod internal;
mod outcome;
mod plan;

pub use api::{
    Check, Plans, ProjectRequest, ProjectRun, Run, SpecRequest, Status, Submission,
    SubmissionOptions, SECTIONS, STATUSES,
};
pub use cancel::CancellationToken;
pub use conditions::{conditions, Condition, ConditionCode, Conditions, CONDITION_COLUMNS};
pub use diagnostic::{
    ContextValue, Diagnostic, DiagnosticContext, Location, Phase, Position, Severity, SourceSpan,
    ISSUE_COLUMNS, PHASES, REPORT_COLUMNS, REPORT_KINDS, REPORT_OUTCOMES, SEVERITIES,
};
pub use engine::{Engine, EngineOptions, Partitions};
pub use error::{Failure, FailureKind, TableError};
pub use file_type::{FileType, FILE_TYPES};
pub use host::functions::{HostLanguage, ProjectFunctions};
pub use host::resolver::{MemoryResolver, ResourceResolver};
pub use info::{info, Host, Info, FEATURES, HOSTS};
pub use lint::{lint, Lint, LintFinding, LINT_COLUMNS, LINT_FINDINGS};
pub use odm::{OdmOptions, ODM_COLUMNS};
pub use progress::{
    Division, ProgressCallback, ProgressEvent, ProgressKind, ProgressPhase, Timing, Timings,
    PROGRESS_KINDS, PROGRESS_PHASES, TIMING_COLUMNS,
};
pub use schema::schema;
pub use selftest::{selftest, SelfTest, SelfTestCase, SELFTEST_COLUMNS};
#[cfg(feature = "arrow")]
pub use stream::{Batches, RunGuard, StreamProgress, Streamed};
#[cfg(feature = "arrow")]
pub use table::{
    Array, DataType, Date32Array, FFI_ArrowArrayStream, Field, Fields, Float64Array, Int64Array,
    LargeStringArray, ListArray, RecordBatch, SchemaRef, StringArray, StringViewArray, TimeUnit,
    TimestampSecondArray,
};
pub use table::{InputFrame, Table};
pub use text::Summary;

/// Version of the shared core compiled into a native installation.
pub const VERSION: &str = yamaa_core::VERSION;

#[cfg(all(test, unix))]
#[allow(dead_code)]
#[path = "../tests/support/scratch.rs"]
mod test_scratch;

#[cfg(test)]
mod tests {
    #[test]
    fn version_matches_the_shared_core() {
        assert_eq!(crate::VERSION, yamaa_core::VERSION);
    }
}
