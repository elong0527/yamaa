//! The yamaa file lint: departures from the written form the style guide
//! (`docs/articles/style.md`) fixes, and the fixes proved safe.
//!
//! A lint finding names how a file is written, never what it means: it has no
//! phase and no condition, and a file runs as its canonical form does. Each
//! file is read once through the local file resolver, admitted by the shared
//! YAML document admission, and judged from the shared scanner's located
//! events with the positions a reader sees; a 1.x file is also read through
//! its file type's schema, as validation reads it. A 1.x specification that
//! declares `parent` or `include` is compared with the layers it inherits,
//! read through the same resolver, for `partial_restatement` alone. A
//! specification that declares either, or that a specification of the call
//! includes, itself or through the parent chain it inherits, is a layer: a
//! value it writes may replace one it inherits.
//!
//! [`lint`] returns each file's findings in file order, then by line, column
//! and finding. With `fix`, a file is rewritten only when the edited text
//! reads, under that same admission, as exactly the values of the original,
//! or for a canonical rewrite as exactly those values with that rewrite
//! applied, with the same comment lines, and does not change when it is fixed
//! again; a fixed 1.x file gives no validation finding the original did not.
//! Each rewritten file is replaced atomically. Every file is read and admitted
//! before any is written, so a file that cannot be read or admitted refuses
//! the whole call and writes nothing.

#[cfg(feature = "yaml")]
mod authoring;
#[cfg(feature = "yaml")]
mod canonical;
#[cfg(feature = "yaml")]
mod checks;
#[cfg(feature = "yaml")]
mod document;
#[cfg(feature = "yaml")]
mod fields;
#[cfg(feature = "yaml")]
mod files;
#[cfg(feature = "yaml")]
mod fix;
#[cfg(feature = "yaml")]
mod inheritance;

#[cfg(all(test, feature = "yaml"))]
mod tests;

use crate::yaml_decode::DecodeFailure;
use yamaa_core::application_issue::{Fault, Step};
use yamaa_core::diagnostic::Diagnostic;

/// Every finding the lint reports, in the order the style guide names them.
pub const FINDINGS: [&str; 15] = [
    "field_order",
    "root_spacing",
    "entry_spacing",
    "blank_lines",
    "line_width",
    "invalid_suppression",
    "repeated_row_derivation",
    "redundant_intermediate",
    "missing_label",
    "redundant_long_form",
    "restated_default",
    "scalar_as_text",
    "unused_input_key",
    "equality_filter",
    "partial_restatement",
];

/// One departure, at the one-based line and column where it starts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Finding {
    pub line: usize,
    pub column: usize,
    /// One of [`FINDINGS`].
    pub finding: &'static str,
    /// The spec path of the departure, `$` for the root.
    pub spec_path: String,
    pub message: String,
}

/// The findings that remain in one file, and whether a fix rewrote it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Linted {
    /// The file's absolute path with forward slashes.
    pub file: String,
    pub findings: Vec<Finding>,
    pub fixed: bool,
}

/// Why a lint call refused, naming the file concerned.
#[derive(Debug)]
pub enum Error {
    /// This build omits the optional YAML parser.
    Unavailable,
    /// The file could not be read as a regular file through the resolver.
    Read {
        file: String,
        error: crate::file_resources::Error,
    },
    /// The file's text is not an admitted YAML document.
    Source {
        file: String,
        failure: DecodeFailure,
    },
    /// The file declares `schema_version`: it is written in language version
    /// 0.5, which the engine does not read (SCHEMA-2).
    Language { file: String },
    /// A fixed file could not be replaced.
    Write {
        file: String,
        error: crate::file_publication::Error,
    },
}

impl Error {
    /// The file the refusal concerns, when it concerns one.
    pub fn file(&self) -> Option<&str> {
        match self {
            Self::Unavailable => None,
            Self::Read { file, .. }
            | Self::Source { file, .. }
            | Self::Language { file }
            | Self::Write { file, .. } => Some(file),
        }
    }

    /// The registered findings that say why: a resource refusal for a file
    /// that cannot be read, the source finding for text that cannot be
    /// admitted, `unsupported_language_version` at `schema_version`, naming
    /// 0.5, for a file written for 0.5, and the publication refusal for a
    /// file that cannot be replaced.
    pub fn diagnostics(&self) -> Vec<Diagnostic> {
        use yamaa_core::application_issue::{rejected, resource_rejected};
        match self {
            Self::Unavailable => vec![rejected(Step::Prepare, Fault::YamlReaderUnavailable)],
            Self::Read { file, error } => {
                vec![crate::file_resources::caller_read_failure(file, error)]
            }
            Self::Source { file, failure } => source_diagnostics(file, failure),
            Self::Language { .. } => vec![yamaa_core::language::legacy_refusal(
                yamaa_core::language::LANGUAGE_VERSION,
            )],
            Self::Write { error, .. } => {
                if let crate::file_publication::Error::Limit(limit) = error {
                    return vec![resource_rejected(
                        Step::Output,
                        Fault::PublicationLimit,
                        limit.resource,
                        limit.limit,
                        limit.actual,
                        Vec::new(),
                    )];
                }
                vec![rejected(Step::Output, write_code(error))]
            }
        }
    }
}

/// The findings document admission gives a specification's own text.
fn source_diagnostics(file: &str, failure: &DecodeFailure) -> Vec<Diagnostic> {
    use yamaa_core::application_issue::{rejected, resource_rejected};
    use yamaa_core::diagnostic::ConditionCode;
    use yamaa_core::json::Value;
    let finding = |code, spec_path: String, context: Vec<(&str, Value)>| Diagnostic {
        code,
        spec_paths: vec![spec_path],
        context: context
            .into_iter()
            .map(|(key, value)| (key.to_owned(), value))
            .collect(),
        source_span: None,
        operand_route: None,
    };
    match failure {
        DecodeFailure::NonAscii(position) => vec![finding(
            ConditionCode::SourceNonAscii,
            "$".into(),
            vec![
                ("line", Value::from(position.line)),
                ("column", Value::from(position.column)),
                ("path", Value::String(file.into())),
            ],
        )],
        DecodeFailure::InvalidYaml { position, reason } => vec![finding(
            ConditionCode::SourceInvalidYaml,
            "$".into(),
            vec![
                ("line", Value::from(position.line)),
                ("column", Value::from(position.column)),
                ("reason", Value::String(reason.clone())),
            ],
        )],
        DecodeFailure::InvalidText(issues) => issues
            .iter()
            .map(|issue| {
                finding(
                    ConditionCode::SourceInvalidText,
                    issue.path.clone(),
                    vec![
                        (
                            "code_point",
                            Value::String(format!("U+{:04X}", issue.code_point)),
                        ),
                        ("offset", Value::from(issue.offset)),
                        ("path", Value::String(file.into())),
                    ],
                )
            })
            .collect(),
        DecodeFailure::Limit(limit) => vec![resource_rejected(
            Step::Prepare,
            Fault::ResourceLimit,
            limit.resource,
            limit.limit,
            limit.actual,
            Vec::new(),
        )],
        DecodeFailure::Unavailable => vec![rejected(Step::Prepare, Fault::YamlReaderUnavailable)],
        DecodeFailure::Internal => vec![rejected(Step::Prepare, Fault::YamlAdmission)],
    }
}

fn write_code(error: &crate::file_publication::Error) -> Fault {
    use crate::file_publication::Error as E;
    match error {
        E::Host(_) => Fault::HostPublicationFailure,
        E::InvalidTarget | E::Unreachable(_) => Fault::InvalidTarget,
        E::PathMismatch => Fault::PathMismatch,
        E::Limit(_) => Fault::PublicationLimit,
        E::StagingCollision => Fault::StagingCollision,
        E::Changed => Fault::TargetChanged,
        E::Io(_) => Fault::OutputPublicationFailed,
        E::Cleanup { .. } => Fault::CleanupIncomplete,
    }
}

/// Lint every file `paths` names, in argument order; a directory names the
/// yamaa files beneath it in sorted path order, leaving out hidden
/// directories and any `.yaml` file whose root does not declare the 1.x
/// header with a `type` naming a file type, and refuses when it cannot be
/// listed or such a file cannot be read. A named file that declares
/// `schema_version`, written for 0.5, is refused as
/// `unsupported_language_version`, naming 0.5 (SCHEMA-2). With `fix`, apply
/// the proved fixes first.
pub fn lint(paths: &[String], fix: bool) -> Result<Vec<Linted>, Error> {
    #[cfg(feature = "yaml")]
    {
        files::lint(paths, fix)
    }
    #[cfg(not(feature = "yaml"))]
    {
        let _ = (paths, fix);
        Err(Error::Unavailable)
    }
}

/// The findings in `text`, a specification file's complete contents. Text
/// has no location to read a `parent` or a fragment from, so it gives no
/// `partial_restatement`, and it is never a fragment another file includes.
pub fn check_text(text: &[u8]) -> Result<Vec<Finding>, DecodeFailure> {
    #[cfg(feature = "yaml")]
    {
        files::findings(text, None, false)
    }
    #[cfg(not(feature = "yaml"))]
    {
        let _ = text;
        Err(DecodeFailure::Unavailable)
    }
}

/// `text` with every proved layout fix applied; unchanged when no fix holds.
pub fn fix_text(text: &[u8]) -> Result<Vec<u8>, DecodeFailure> {
    #[cfg(feature = "yaml")]
    {
        let document = document::Document::parse(text)?;
        Ok(fix::fix_text(&document.text, false).into_bytes())
    }
    #[cfg(not(feature = "yaml"))]
    {
        let _ = text;
        Err(DecodeFailure::Unavailable)
    }
}
