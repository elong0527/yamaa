//! Diagnostic facts from the already bounded, retained YAML source admission.
//! Parsing, physical capture and resource refusal remain on the source boundary.
use crate::{
    diagnostic::ConditionCode,
    project_diagnostics::{Context, Diagnostic, Fact},
};
use alloc::{format, string::String, vec, vec::Vec};

/// Original path and first invalid scalar offset from the retained decoder.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnicodeIssue {
    pub path: String,
    pub code_point: u32,
    pub offset: usize,
}
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
/// adapters retain the original exact final-envelope serialization admission.
pub fn diagnostics<'a>(failure: Failure<'a>, identity: Option<&'a str>) -> Vec<Diagnostic<'a>> {
    match failure {
        Failure::NonAscii { line, column } => {
            let mut context: Context<'a> = [
                ("line", Fact::Unsigned(line)),
                ("column", Fact::Unsigned(column)),
            ]
            .into();
            if let Some(identity) = identity {
                context.insert("path", Fact::Text(identity));
            }
            vec![Diagnostic {
                code: ConditionCode::SourceNonAscii,
                spec_path: "$".into(),
                context,
            }]
        }
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
                ]
                .into(),
            })
            .collect(),
    }
}
