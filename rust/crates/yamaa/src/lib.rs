//! The supported Rust API: a small facade over `yamaa-adapters` and
//! `yamaa-core`.
//!
//! An embedder configures one [`Engine`] and calls operations on it. The
//! facade wraps the existing prepare, bind, execute and publish workflow;
//! there is no second admission or evaluation route. All physical access goes
//! through a resolver; local files are just the default resolver. Every
//! operation on [`Engine`] carries a runnable example, and
//! `examples/memory.rs` runs one specification entirely from memory.
//!
//! Adapters and core may change in any release without notice; their `pub`
//! items are visibility for the workspace, not a public API. Every public
//! type here is facade-owned, and adapters or core types are converted at the
//! boundary. Core keeps its boundary (no I/O, no `unsafe`, no Arrow).
//!
//! The facade forwards the `yaml`, `parquet`, `readstat` and `odm` format
//! features to `yamaa-adapters`, all on by default, and builds with any
//! combination of them. With no format feature, CSV and Arrow routes remain.
//! A capability a build omits fails the same way it does in adapters: the
//! stable codes (`yaml_reader_unavailable`, `parquet_reader_unavailable`,
//! `sas_reader_unavailable`, `parquet_codec_unavailable`) surface as
//! diagnostics or as [`Error::Unavailable`], and [`read_odm`] without `odm`
//! returns `Error::Unavailable { feature: "odm" }`. The facade adds no native
//! dependencies of its own.
#![deny(unsafe_code)]

pub mod api;
pub mod cancel;
pub mod diagnostic;
pub mod engine;
pub mod error;
pub mod functions;
pub mod observe;
pub mod odm;
pub mod plan;
pub mod resolver;
pub mod scalar;
pub mod table;

mod internal;

pub use api::{Failure, ProjectRun, Run, Submission};
pub use cancel::CancellationToken;
pub use diagnostic::{ContextValue, Diagnostic, DiagnosticContext, Report, Severity, SourceSpan};
pub use engine::{
    AttemptFailure, Build, DefineArtifact, DefineBuild, DefineRequest, Engine, EngineOptions,
    Request, ResolvedSpecification, SaveOutcome, SourceBytes,
};
pub use error::{
    Error, FunctionError, HostPayload, LockFinding, LockReason, LockSyntaxError, OdmError,
    ResolverFailure, TableError,
};
pub use functions::{
    decode_renv_lock, Argument, BatchArgument, CallError, ColumnType, FunctionHandle,
    FunctionIdentity, HostLanguage, LockContext, LockKind, Parameter, ProjectFunctions, RenvLock,
    Signature,
};
pub use observe::{CallStage, DatasetPhase, Event, Outcome, Phase, Progress, Refusal};
pub use odm::{read_odm, OdmOptions, ODM_COLUMNS};
pub use plan::{Plan, RunRequest};
pub use resolver::{
    MemoryResolver, ResolverKind, ResolverOperation, ResolverReply, ResolverRequest,
    ResourceResolver,
};
pub use scalar::{Date, DateTime, EncodedScalar, Scalar, ScalarDecodeError, ScalarRef};
pub use table::{
    Array, DataType, Date32Array, FFI_ArrowArrayStream, Field, Fields, Float64Array, InputFrame,
    Int64Array, LargeStringArray, ListArray, RecordBatch, SchemaRef, StringArray, StringViewArray,
    Table, TimeUnit, TimestampSecondArray,
};

/// Version of the shared core compiled into a native installation.
pub const VERSION: &str = yamaa_core::VERSION;

/// Compare two style documents for value equality through shared YAML
/// admission, without exposing any parser result.
pub fn style_same_value(before: &[u8], after: &[u8]) -> bool {
    yamaa_adapters::style_values::same_value(before, after)
}

#[cfg(test)]
mod tests {
    #[test]
    fn version_matches_the_shared_core() {
        assert_eq!(crate::VERSION, yamaa_core::VERSION);
    }

    #[cfg(feature = "yaml")]
    #[test]
    fn style_comparison_uses_shared_admission() {
        assert!(crate::style_same_value(b"a: 1\n", b"a: 1\n"));
        assert!(!crate::style_same_value(b"a: 1\n", b"a: 2\n"));
        assert!(!crate::style_same_value(b"a: [\n", b"a: [\n"));
    }
}
