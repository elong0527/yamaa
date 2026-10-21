//! The run report's optional file declaration and its static findings,
//! without file authority.
use super::{artifact_profile, unknown_artifact_profile};
use crate::json::Value as JsonValue;
use crate::{
    ast,
    diagnostic::{ConditionCode, Diagnostic},
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
    /// Two output files of one producer graph resolve to one location: `right`
    /// is the later declaration and `left` the earlier one it collides with,
    /// in the same node or another.
    GraphCollision {
        left: &'static str,
        right: &'static str,
        path: String,
    },
}

/// Read `output.report` and decide the findings that need no file authority:
/// a warning without a report file, an unmapped extension and a written path
/// equal to `output.path`.
pub(super) fn prepare(
    output: &ast::Output,
    warnings: Vec<String>,
) -> (Option<String>, Vec<ReportFinding>) {
    let report = output.report.as_ref().map(|report| report.text.clone());
    let mut findings = Vec::new();
    if !warnings.is_empty() && report.is_none() {
        findings.push(ReportFinding::Missing { warnings });
    }
    if let Some(path) = report.as_deref() {
        if artifact_profile(path).is_none() {
            findings.push(ReportFinding::UnknownProfile { path: path.into() });
        }
        if path == output.path.text {
            findings.push(ReportFinding::Collision { path: path.into() });
        }
    }
    (report, findings)
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
            Self::UnknownProfile { path } => {
                return unknown_artifact_profile(path, vec!["output.report".into()])
            }
            Self::Collision { path } => (
                ConditionCode::ReportPathCollision,
                vec!["output.path".into(), "output.report".into()],
                [("path".into(), text(path))].into(),
            ),
            Self::GraphCollision { left, right, path } => (
                ConditionCode::ReportPathCollision,
                vec![(*left).into(), (*right).into()],
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
