//! Closed values crossing application ports. No parser or compiler API is exposed.
pub use yamaa_core::function_signature::{LogicalSignature, ProjectFunctionIdentity};
pub use yamaa_core::project_environment::{LockKind, LockReference};
pub use yamaa_core::project_function::Language;
pub use yamaa_core::resource::ResourceFailure;
pub use yamaa_core::table::ValueRef;
pub use yamaa_core::temporal::{Date, DatePrecision, DateTime, DateTimePrecision};
pub use yamaa_core::value::Value;

pub const VERSION: &str = yamaa_core::VERSION;

pub fn unavailable_file_transport() -> yamaa_core::diagnostic::Diagnostic {
    yamaa_core::application_issue::unsupported("native_file_transport", None)
}
