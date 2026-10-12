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
    UriScheme,
    Symlink,
}
impl ResourceFailure {
    pub fn code(self) -> ConditionCode {
        match self {
            Self::Missing => ConditionCode::ResourceMissing,
            Self::NotRegularFile => ConditionCode::ResourceNotRegularFile,
            Self::UriScheme => ConditionCode::ResourceUriScheme,
            Self::Symlink => ConditionCode::ResourceSymlink,
        }
    }

    /// A planning resource belongs to an admitted declaring field, without a
    /// fabricated dataset identity or a resolved host path in semantic context.
    pub fn field_diagnostic(self, field: &str, written_path: &str) -> Diagnostic {
        Diagnostic {
            code: self.code(),
            spec_paths: vec![field.into()],
            context: [(
                String::from("path"),
                ContextValue::Scalar(Value::Str(written_path.into())),
            )]
            .into(),
            source_span: None,
            operand_route: None,
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

/// URI spelling is a static declaration finding. Absolute Windows drive paths
/// remain local paths; resource ports retain authority over actual file kinds.
pub fn has_uri_scheme(path: &str) -> bool {
    let Some((scheme, rest)) = path.split_once(':') else {
        return false;
    };
    if scheme.len() == 1
        && scheme.as_bytes()[0].is_ascii_alphabetic()
        && (rest.starts_with('/') || rest.starts_with('\\'))
    {
        return false;
    }
    let mut bytes = scheme.bytes();
    bytes
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic())
        && bytes.all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'-' | b'.'))
}
