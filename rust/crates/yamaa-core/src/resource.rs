//! Classified resource failures; filesystem authority remains with the resource port.
use alloc::{format, string::String, vec};

use crate::{
    diagnostic::{ConditionCode, ContextValue, Diagnostic},
    value::Value,
};

/// A resource adapter reports an observed cause explicitly. An arbitrary host
/// exception, panic or resource limit is never inferred to be one of these causes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResourceFailure {
    Missing,
    NotRegularFile,
}
impl ResourceFailure {
    pub fn code(self) -> ConditionCode {
        match self {
            Self::Missing => ConditionCode::ResourceMissing,
            Self::NotRegularFile => ConditionCode::ResourceNotRegularFile,
        }
    }

    /// Only authored dataset/path identities enter semantic context. Canonical
    /// filesystem names and host exception messages are deliberately unavailable.
    pub fn diagnostic(self, dataset: &str, written_path: &str) -> Diagnostic {
        Diagnostic {
            code: self.code(),
            spec_paths: vec![format!("input.{dataset}.path")],
            context: [
                (
                    String::from("dataset"),
                    ContextValue::Scalar(Value::Str(dataset.into())),
                ),
                (
                    String::from("path"),
                    ContextValue::Scalar(Value::Str(written_path.into())),
                ),
            ]
            .into(),
            source_span: None,
            operand_route: None,
        }
    }
}
