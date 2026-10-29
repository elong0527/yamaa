//! The one error of every facade operation, [`Failure`], and the
//! facade-owned failures it can carry.
//!
//! Every fallible facade operation returns `Result<_, Failure>` (#3765): a
//! check, run, stream, resolve or submission, a project's names and listed
//! specifications, a save, `read_odm`, `file_type`, `save_table`, `lint` and
//! `schema`, and a streamed run's result and reader. A failure's
//! [`FailureKind`] says what refused the operation, and its message is the
//! hosts' error text (D39). Internal error enums (per-module failures, decode
//! failures, resource errors) never appear in a facade signature; they
//! convert into these types at the boundary.
//!
//! The ports keep their own errors, which a `Failure` never carries, in
//! [`crate::host`] (#3766): [`FunctionError`] and [`CallError`] are the
//! project-function port's, with [`LockSyntaxError`] for its lock helper,
//! and [`ResolverFailure`] the resolver's.
//! [`InputFrame::import`](crate::InputFrame::import), a value constructor
//! with no action to name, keeps [`TableError`]. Each carries a stable code
//! and a message.
//!
//! [`FunctionError`]: crate::host::functions::FunctionError
//! [`CallError`]: crate::host::functions::CallError
//! [`LockSyntaxError`]: crate::host::functions::LockSyntaxError
//! [`ResolverFailure`]: crate::host::resolver::ResolverFailure

use crate::diagnostic::{Diagnostic, Report};
use crate::table::Table;
use crate::text::{self, Action};

/// A refused operation, the one error of every facade operation (#3765):
/// what refused it, D39's error text, the issues that explain it, and the
/// files it wrote before it was refused (P-API-1).
#[non_exhaustive]
#[derive(Clone, Debug)]
pub struct Failure {
    /// What refused the operation. Only a [`FailureKind::Rejected`] failure
    /// carries issues.
    pub kind: FailureKind,
    /// D39's error text: the call that refused, then the first issue's line
    /// and the count of the other issues, such as `cannot save ADAE: column
    /// ASTDY reads TRTSDT, which is declared after it [forward_reference,
    /// COLUMN-5] (and 2 more issues)`, or, for a failure no issue explains,
    /// what failed, such as `cannot read adae.yaml: operation cancelled`.
    /// Only ASCII; see [`crate::text`].
    pub message: String,
    /// The issues table both hosts attach to their error (D28).
    pub issues: Table,
    /// The same issues as typed records, first issue first, for the command
    /// line's printed lines and JSON.
    pub diagnostics: Vec<Diagnostic>,
    /// The files a refused save wrote before it stopped, such as a failed
    /// run's report, in publication order and spelled as `save` returns
    /// them; empty for any other refusal.
    pub paths: Vec<String>,
    /// The original progress callback exception, when the host supplied one.
    /// Bindings chain this opaque payload onto the refusal without rerendering it.
    pub cause: Option<crate::host::functions::HostPayload>,
}

/// What refused a facade operation, the kind of a [`Failure`]: its issues,
/// or one of the failures that no issue explains, each carrying what names
/// it. A table's failure is boxed, so every `Result<_, Failure>` stays
/// small.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FailureKind {
    /// The operation was refused with issues: the failure's `diagnostics`
    /// and `issues`.
    Rejected,
    /// The operation was cancelled: its token was cancelled, or a project
    /// function raised the host's interrupt, as
    /// [`ProjectFunctions::is_interrupt`](crate::ProjectFunctions::is_interrupt)
    /// classifies it (D46). The operation returns no result and no issue.
    Cancelled,
    /// A table could not be built or imported.
    Table(Box<TableError>),
    /// A format feature is not compiled in.
    Unavailable { feature: &'static str },
    /// A stable internal code with no internal payload.
    Internal { code: &'static str },
    /// A streamed run refused a reader or its result (#3697), with the
    /// stable `code` that names why: `host_thread`, a run that can call its
    /// host's project functions would be built off the thread that started
    /// it; `stream_taken`, its output stream was already started or read;
    /// `stream_open`, its result was asked for while a reader's stream has
    /// not ended; and `busy`, another call, such as a project function its
    /// build called, uses it. See [`crate::stream`].
    Stream { code: &'static str },
}

impl FailureKind {
    /// What a failure of this kind says after its action when no issue
    /// explains it; none for a refusal with issues, whose first issue
    /// speaks.
    fn detail(&self) -> Option<String> {
        let detail = match self {
            Self::Rejected => return None,
            Self::Cancelled => "operation cancelled".to_owned(),
            Self::Table(failure) => {
                format!("table failure ({}): {}", failure.code(), failure.message())
            }
            Self::Unavailable { feature } => format!("format feature not compiled in: {feature}"),
            Self::Internal { code } => format!("internal failure: {code}"),
            Self::Stream { code } => {
                let reason = match *code {
                    "host_thread" => {
                        "a run that can call project functions is built only on the thread that started it"
                    }
                    "stream_taken" => "the run's output stream has already been read",
                    "stream_open" => "the run's output stream has not been read to its end",
                    "busy" => "the streamed run is in use",
                    _ => "the streamed run refused",
                };
                format!("{reason} [{code}]")
            }
        };
        Some(detail)
    }
}

impl Failure {
    /// The refusal of `action` that `report`'s issues explain.
    pub(crate) fn rejected(action: Action<'_>, report: &Report) -> Self {
        Self {
            kind: FailureKind::Rejected,
            message: text::error_text(action, &report.diagnostics, None),
            issues: report.table(),
            diagnostics: report.diagnostics.clone(),
            paths: Vec::new(),
            cause: None,
        }
    }

    /// The refusal of `action` for a failure of `kind` that no issue
    /// explains: the kind's own detail follows the action.
    pub(crate) fn of(action: Action<'_>, kind: FailureKind) -> Self {
        let detail = kind.detail();
        Self {
            kind,
            message: text::error_text(action, &[], detail.as_deref()),
            ..Self::rejected(action, &Report::default())
        }
    }

    /// The cancellation of `action`.
    pub(crate) fn cancelled(action: Action<'_>) -> Self {
        Self::of(action, FailureKind::Cancelled)
    }

    /// The same refusal, after a save wrote `paths`.
    pub(crate) fn with_paths(mut self, paths: Vec<String>) -> Self {
        self.paths = paths;
        self
    }

    pub(crate) fn with_cause(mut self, cause: Option<crate::host::functions::HostPayload>) -> Self {
        if cause.is_some() {
            self.cause = cause;
        }
        self
    }

    /// The refusal of `action` that `issues`, which no file locates,
    /// explain.
    pub(crate) fn from_issues(
        action: Action<'_>,
        issues: &[yamaa_core::diagnostic::Diagnostic],
    ) -> Self {
        Self::rejected(action, &Report::from_issues(issues))
    }

    /// A refusal that one registered finding explains.
    pub(crate) fn from_finding(
        action: Action<'_>,
        finding: yamaa_core::diagnostic::Diagnostic,
    ) -> Self {
        Self::from_issues(action, std::slice::from_ref(&finding))
    }
}

impl std::fmt::Display for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for Failure {}

/// A table that could not be built or imported, carrying its stable code and
/// message.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TableError {
    code: &'static str,
    message: String,
}

impl TableError {
    /// Attach a frame import failure to the specification operation and its
    /// input alias. Both hosts use this diagnostic projection: a validation
    /// `value_not_permitted` at `inputs.<alias>`, retaining the stable table
    /// code and complete import message, with no files written.
    pub fn at_input(self, specification: &str, alias: &str) -> Failure {
        Failure::from_issues(
            Action::Read(specification),
            &[yamaa_core::diagnostic::input_frame(
                alias,
                self.code,
                &self.message,
            )],
        )
    }

    /// Return the stable table failure code.
    pub fn code(&self) -> &'static str {
        self.code
    }

    /// Return the table failure message.
    pub fn message(&self) -> &str {
        &self.message
    }

    pub(crate) fn from_public(error: yamaa_adapters::public_table::TableError) -> Self {
        use yamaa_adapters::public_table::TableError as Public;
        let code = match error {
            Public::Limit { .. } => "limit",
            Public::Representation { .. } => "representation",
            Public::InvalidTable => "invalid_table",
            Public::Internal => "internal",
        };
        Self {
            code,
            message: error.to_string(),
        }
    }

    #[cfg(feature = "arrow")]
    pub(crate) fn from_table(error: yamaa_adapters::column_table::TableError) -> Self {
        use yamaa_adapters::column_table::TableError as Table;
        let code = match error {
            Table::Limit { .. } => "limit",
            Table::SchemaMismatch { .. } => "schema_mismatch",
            Table::ArrayType { .. } => "array_type",
            Table::TemporalRange { .. } => "temporal",
            Table::Representation { .. } => "representation",
            Table::Arrow(_) => "arrow",
        };
        Self {
            code,
            message: format!("{error:?}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_errors_carry_stable_codes() {
        use yamaa_adapters::public_table::TableError as Public;
        for (error, code) in [
            (
                Public::Limit {
                    resource: "table_rows",
                    limit: 1,
                    required: None,
                },
                "limit",
            ),
            (
                Public::Representation {
                    representation: "text",
                },
                "representation",
            ),
            (Public::InvalidTable, "invalid_table"),
            (Public::Internal, "internal"),
        ] {
            let converted = TableError::from_public(error);
            assert_eq!(converted.code(), code);
            assert!(!converted.message().is_empty());
        }
    }

    #[test]
    #[cfg(feature = "arrow")]
    fn table_import_errors_carry_stable_codes() {
        use yamaa_adapters::column_table::TableError as Table;
        for (error, code) in [
            (
                Table::Limit {
                    resource: "table_rows",
                    limit: 1,
                    required: None,
                },
                "limit",
            ),
            (Table::SchemaMismatch { batch: 0 }, "schema_mismatch"),
            (
                Table::ArrayType {
                    batch: 0,
                    column: 1,
                },
                "array_type",
            ),
            (
                Table::TemporalRange {
                    batch: 0,
                    column: 0,
                    row: 0,
                },
                "temporal",
            ),
            (
                Table::Representation {
                    representation: "text",
                },
                "representation",
            ),
        ] {
            let converted = TableError::from_table(error);
            assert_eq!(converted.code(), code);
            assert!(!converted.message().is_empty());
        }
    }

    /// D39's error text of one failure of each kind (#3765): the action,
    /// then the first issue's line and the count of the others for a refusal
    /// with issues, or else the kind's own detail. Only the refusal with
    /// issues carries them, and no failure but a save's carries paths.
    #[test]
    fn a_failure_of_each_kind_names_its_action_then_what_refused_it() {
        use yamaa_core::application_issue::{Fault, Step};
        // Two engine faults; the first is the one the message prints, its
        // text the registry's engine fault message for the resolve step.
        let fault = yamaa_core::diagnostic::ConditionCode::ApplicationRejected.definition();
        let issues = [Step::Resolve, Step::Execute]
            .into_iter()
            .flat_map(|step| {
                yamaa_application::file_application::rejected(step, Fault::ArtifactRepresentation)
            })
            .collect::<Vec<_>>();
        let read = |subject| Action::Read(subject);
        let cases = [
            (
                "rejected",
                Failure::rejected(Action::Save("ADAE"), &Report::from_issues(&issues)),
                format!(
                    "cannot save ADAE: the engine refused the operation in its resolve step: \
                     artifact_representation [{}] (and 1 more issue)",
                    fault.condition
                ),
                2,
            ),
            (
                "cancelled",
                Failure::of(read("adsl.yaml"), FailureKind::Cancelled),
                "cannot read adsl.yaml: operation cancelled".to_owned(),
                0,
            ),
            (
                "table",
                Failure::of(
                    read("edc/odm.xml"),
                    FailureKind::Table(Box::new(TableError::from_public(
                        yamaa_adapters::public_table::TableError::InvalidTable,
                    ))),
                ),
                "cannot read edc/odm.xml: table failure (invalid_table): invalid table schema \
                 or value"
                    .to_owned(),
                0,
            ),
            (
                "unavailable",
                Failure::of(
                    read("edc/odm.xml"),
                    FailureKind::Unavailable { feature: "odm" },
                ),
                "cannot read edc/odm.xml: format feature not compiled in: odm".to_owned(),
                0,
            ),
            (
                "internal",
                Failure::of(
                    read("TEST"),
                    FailureKind::Internal {
                        code: "stream_panic",
                    },
                ),
                "cannot read TEST: internal failure: stream_panic".to_owned(),
                0,
            ),
            (
                "stream",
                Failure::of(
                    read("TEST"),
                    FailureKind::Stream {
                        code: "stream_taken",
                    },
                ),
                "cannot read TEST: the run's output stream has already been read [stream_taken]"
                    .to_owned(),
                0,
            ),
        ];
        for (kind, failure, message, issues) in cases {
            // Exhaustive, so a new kind fails to compile here until it is
            // named; give it a case in the table above too.
            let named = match failure.kind {
                FailureKind::Rejected => "rejected",
                FailureKind::Cancelled => "cancelled",
                FailureKind::Table(_) => "table",
                FailureKind::Unavailable { .. } => "unavailable",
                FailureKind::Internal { .. } => "internal",
                FailureKind::Stream { .. } => "stream",
            };
            assert_eq!(named, kind);
            assert_eq!(
                (
                    failure.message.as_str(),
                    failure.to_string(),
                    failure.diagnostics.len(),
                    failure.issues.rows(),
                    failure.paths.len(),
                ),
                (message.as_str(), message.clone(), issues, issues, 0),
                "{kind}"
            );
        }
    }
}
