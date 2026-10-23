//! Diagnostic facts from the already bounded, retained YAML source admission.
//! Parsing, physical capture and resource refusal remain on the source boundary.
use crate::{
    diagnostic::ConditionCode,
    project_diagnostics::{Diagnostic, Fact},
};
use std::vec;

/// Original path and first invalid scalar offset from the retained decoder.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnicodeIssue {
    pub path: String,
    pub code_point: u32,
    pub offset: usize,
}
#[derive(Debug)]
pub enum Failure<'a> {
    NonAscii {
        line: usize,
        column: usize,
    },
    InvalidYaml {
        line: usize,
        column: usize,
        reason: &'a str,
    },
    InvalidText(&'a [UnicodeIssue]),
}
/// Source admission already bounds the complete ordered decoder findings and
/// authored paths. This projection adds no stricter quota or parser policy;
/// the edge that renders a finding owns its serialization. `identity` is the
/// file as the caller names it, which a finding about its text names as
/// `path`.
pub fn diagnostics<'a>(failure: Failure<'a>, identity: &'a str) -> Vec<Diagnostic<'a>> {
    match failure {
        Failure::NonAscii { line, column } => vec![Diagnostic {
            code: ConditionCode::SourceNonAscii,
            spec_path: "$".into(),
            context: [
                ("line", Fact::Unsigned(line)),
                ("column", Fact::Unsigned(column)),
                ("path", Fact::Text(identity)),
            ]
            .into(),
        }],
        Failure::InvalidYaml {
            line,
            column,
            reason,
        } => vec![Diagnostic {
            code: ConditionCode::SourceInvalidYaml,
            spec_path: "$".into(),
            context: [
                ("line", Fact::Unsigned(line)),
                ("column", Fact::Unsigned(column)),
                ("reason", Fact::Text(reason)),
            ]
            .into(),
        }],
        Failure::InvalidText(issues) => issues
            .iter()
            .map(|issue| Diagnostic {
                code: ConditionCode::SourceInvalidText,
                spec_path: issue.path.clone(),
                context: [
                    (
                        "code_point",
                        Fact::OwnedText(format!("U+{:04X}", issue.code_point)),
                    ),
                    ("offset", Fact::Unsigned(issue.offset)),
                    ("path", Fact::Text(identity)),
                ]
                .into(),
            })
            .collect(),
    }
}

impl<'a> std::fmt::Display for Failure<'a> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "yaml diagnostics: {self:?}")
    }
}
impl<'a> std::error::Error for Failure<'a> {}
