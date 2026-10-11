//! Governed sidecar declarations and static findings, without file authority.
use super::{output_profile, text, PrepareError};
use crate::{
    diagnostic::{ConditionCode, ContextValue, Diagnostic},
    schema::{Document, DocumentNode},
    value::Value,
};
use alloc::{string::String, vec, vec::Vec};

#[derive(Clone, Debug, Default)]
pub struct OutputLogs {
    warning: Option<String>,
    verification: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LogFinding {
    MissingWarning {
        warnings: Vec<String>,
    },
    UnknownProfile {
        field: &'static str,
        path: String,
    },
    Collision {
        left: &'static str,
        right: &'static str,
        path: String,
    },
}
impl OutputLogs {
    pub fn warning(&self) -> Option<&str> {
        self.warning.as_deref()
    }
    pub fn verification(&self) -> Option<&str> {
        self.verification.as_deref()
    }
    pub fn declared(&self) -> bool {
        self.warning.is_some() || self.verification.is_some()
    }
    pub(super) fn prepare(
        document: &Document,
        output: usize,
        primary: &str,
        warnings: Vec<String>,
    ) -> Result<(Self, Vec<LogFinding>), PrepareError> {
        let path = |name| {
            document
                .field(output, name)
                .filter(|&node| !matches!(document.nodes()[node], DocumentNode::Null))
                .map(|node| text(document, node).map(String::from))
                .transpose()
        };
        let result = Self {
            warning: path("warning_log")?,
            verification: path("verification_log")?,
        };
        let mut findings = Vec::new();
        if !warnings.is_empty() && result.warning.is_none() {
            findings.push(LogFinding::MissingWarning { warnings });
        }
        for (field, path) in [
            ("output.warning_log", result.warning()),
            ("output.verification_log", result.verification()),
        ] {
            if let Some(path) = path.filter(|path| output_profile(path).is_none()) {
                findings.push(LogFinding::UnknownProfile {
                    field,
                    path: path.into(),
                });
            }
        }
        for (left, left_path, right, right_path) in [
            (
                "output.path",
                Some(primary),
                "output.warning_log",
                result.warning(),
            ),
            (
                "output.path",
                Some(primary),
                "output.verification_log",
                result.verification(),
            ),
            (
                "output.warning_log",
                result.warning(),
                "output.verification_log",
                result.verification(),
            ),
        ] {
            if let Some(path) = left_path.filter(|path| Some(*path) == right_path) {
                findings.push(LogFinding::Collision {
                    left,
                    right,
                    path: path.into(),
                });
            }
        }
        Ok((result, findings))
    }
}
impl LogFinding {
    pub fn visit_text(&self, mut visit: impl FnMut(&str)) {
        match self {
            Self::MissingWarning { warnings } => warnings.iter().for_each(|path| visit(path)),
            Self::UnknownProfile { field, path } => {
                visit(field);
                visit(path);
            }
            Self::Collision { left, right, path } => {
                visit(left);
                visit(right);
                visit(path);
            }
        }
    }
    pub fn diagnostic(&self) -> Diagnostic {
        let text = |value: &str| ContextValue::Scalar(Value::Str(value.into()));
        let (code, spec_paths, context) = match self {
            Self::MissingWarning { warnings } => (
                ConditionCode::MissingWarningLog,
                vec!["output.warning_log".into()],
                [(
                    "warnings".into(),
                    ContextValue::Sequence(warnings.iter().map(|path| text(path)).collect()),
                )]
                .into(),
            ),
            Self::UnknownProfile { field, path } => (
                ConditionCode::OutputUnknownProfile,
                vec![(*field).into()],
                [
                    ("path".into(), text(path)),
                    (
                        "permitted".into(),
                        ContextValue::Sequence(
                            [".csv", ".parquet"].into_iter().map(text).collect(),
                        ),
                    ),
                ]
                .into(),
            ),
            Self::Collision { left, right, path } => (
                if *right == "output.warning_log" {
                    ConditionCode::WarningLogPathCollision
                } else {
                    ConditionCode::VerificationLogPathCollision
                },
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
