//! The run report's optional file declaration and its static findings,
//! without file authority.
use super::{output_profile, text, PrepareError};
use crate::json::Value as JsonValue;
use crate::{
    diagnostic::{ConditionCode, Diagnostic},
    schema::{Document, DocumentNode},
};
use std::vec;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReportFinding {
    /// A specification declaring a warning verification declares no report file.
    Missing {
        warnings: Vec<String>,
    },
    UnknownProfile {
        path: String,
    },
    /// `output.report` names the same file as `output.path`.
    Collision {
        path: String,
    },
}

/// Read `output.report` and decide the findings that need no file authority:
/// a warning without a report file, an unmapped extension and a written path
/// equal to `output.path`.
pub(super) fn prepare(
    document: &Document,
    output: usize,
    primary: &str,
    warnings: Vec<String>,
) -> Result<(Option<String>, Vec<ReportFinding>), PrepareError> {
    let report = document
        .field(output, "report")
        .filter(|&node| !matches!(document.nodes()[node], DocumentNode::Null))
        .map(|node| text(document, node).map(String::from))
        .transpose()?;
    let mut findings = Vec::new();
    if !warnings.is_empty() && report.is_none() {
        findings.push(ReportFinding::Missing { warnings });
    }
    if let Some(path) = report.as_deref() {
        if output_profile(path).is_none() {
            findings.push(ReportFinding::UnknownProfile { path: path.into() });
        }
        if path == primary {
            findings.push(ReportFinding::Collision { path: path.into() });
        }
    }
    Ok((report, findings))
}

impl ReportFinding {
    pub fn diagnostic(&self) -> Diagnostic {
        let text = |value: &str| JsonValue::String(value.into());
        let (code, spec_paths, context) = match self {
            Self::Missing { warnings } => (
                ConditionCode::MissingReport,
                vec!["output.report".into()],
                [(
                    "warnings".into(),
                    JsonValue::Array(warnings.iter().map(|path| text(path)).collect()),
                )]
                .into(),
            ),
            Self::UnknownProfile { path } => (
                ConditionCode::OutputUnknownProfile,
                vec!["output.report".into()],
                [
                    ("path".into(), text(path)),
                    (
                        "permitted".into(),
                        JsonValue::Array([".csv", ".parquet"].into_iter().map(text).collect()),
                    ),
                ]
                .into(),
            ),
            Self::Collision { path } => (
                ConditionCode::ReportPathCollision,
                vec!["output.path".into(), "output.report".into()],
                [("path".into(), text(path))].into(),
            ),
        };
        Diagnostic {
            code,
            spec_paths,
            context,
            source_span: None,
            operand_route: None,
        }
    }
}
