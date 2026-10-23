//! Classified resource failures; filesystem authority remains with the resolver port.

use crate::diagnostic::{ConditionCode, Diagnostic};
use crate::json::Value as JsonValue;

/// A resource adapter reports an observed cause explicitly. An arbitrary host
/// exception, panic or resource limit is never inferred to be one of these causes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ResourceFailure {
    #[error("resource boundary: {0}")]
    Boundary(crate::limits::LimitExceeded),
    #[error("resource missing")]
    Missing,
    #[error("resource not regular file")]
    NotRegularFile,
    #[error("resource uri scheme")]
    UriScheme,
    #[error("resource backslash")]
    Backslash,
    #[error("resource empty segment")]
    EmptySegment,
    #[error("resource symlink")]
    Symlink,
    #[error("resource content changed")]
    ContentChanged,
}
impl ResourceFailure {
    pub fn code(self) -> ConditionCode {
        match self {
            Self::Boundary(_) => ConditionCode::ApplicationRejected,
            Self::Missing => ConditionCode::ResourceMissing,
            Self::NotRegularFile => ConditionCode::ResourceNotRegularFile,
            Self::UriScheme => ConditionCode::ResourceUriScheme,
            Self::Backslash => ConditionCode::ResourceBackslash,
            Self::EmptySegment => ConditionCode::ResourceEmptySegment,
            Self::Symlink => ConditionCode::ResourceSymlink,
            Self::ContentChanged => ConditionCode::ResourceContentChanged,
        }
    }

    /// A planning resource belongs to an admitted declaring field, without a
    /// fabricated dataset identity or a resolved host path in semantic context.
    pub fn field_diagnostic(self, field: &str, written_path: &str) -> Diagnostic {
        if let Self::Boundary(error) = self {
            let mut diagnostic = crate::application_issue::resource_rejected(
                crate::application_issue::Step::Capture,
                crate::application_issue::Fault::ResourceLimit,
                error.resource,
                error.limit,
                error.actual,
                vec![field.into()],
            );
            diagnostic
                .context
                .insert("path".into(), JsonValue::String(written_path.into()));
            return diagnostic;
        }
        Diagnostic {
            code: self.code(),
            spec_paths: vec![field.into()],
            context: [(String::from("path"), JsonValue::String(written_path.into()))].into(),
            source_span: None,
            operand_route: None,
        }
    }

    /// Only authored dataset/path identities enter semantic context. Canonical
    /// filesystem names and host exception messages are deliberately unavailable.
    pub fn diagnostic(self, dataset: &str, written_path: &str) -> Diagnostic {
        if matches!(self, Self::Boundary(_)) {
            let mut diagnostic =
                self.field_diagnostic(&format!("input.{dataset}.path"), written_path);
            diagnostic
                .context
                .insert("dataset".into(), JsonValue::String(dataset.into()));
            return diagnostic;
        }
        Diagnostic {
            code: self.code(),
            spec_paths: vec![format!("input.{dataset}.path")],
            context: [
                (String::from("dataset"), JsonValue::String(dataset.into())),
                (String::from("path"), JsonValue::String(written_path.into())),
            ]
            .into(),
            source_span: None,
            operand_route: None,
        }
    }

    /// A save's refusal of a target whose walk stops here: a directory on its
    /// way that is missing, is not a directory or is a link, or a target that
    /// is a link or another entry that is not a regular file (FILE-7,
    /// FILE-8). It is decided while publishing, so it reports under `output`,
    /// at the field that declares the file, or at none for a path the caller
    /// passes, naming the file as written or given (FILE-11). Any other
    /// failure is no walk's, and has no such finding.
    pub fn target_diagnostic(self, field: Option<&str>, written_path: &str) -> Option<Diagnostic> {
        let code = match self {
            Self::Missing => ConditionCode::PublicationTargetMissing,
            Self::NotRegularFile => ConditionCode::PublicationTargetNotRegularFile,
            Self::Symlink => ConditionCode::PublicationTargetSymlink,
            _ => return None,
        };
        Some(Diagnostic {
            code,
            spec_paths: field.map(String::from).into_iter().collect(),
            context: [(String::from("path"), JsonValue::String(written_path.into()))].into(),
            source_span: None,
            operand_route: None,
        })
    }
}

/// One authored path beside the layer that wrote it and the base its
/// `relative_to` selection resolves against. These grant no filesystem lookup
/// or resource capability.
#[derive(Clone, Copy, Debug)]
pub struct WrittenPath<'a> {
    pub declaring_source: &'a str,
    pub base_source: &'a str,
    pub written: &'a str,
}

/// Written-form admission precedes profiles and filesystem access. Root and
/// drive prefixes are separators, while dot and parent segments remain valid.
pub fn written_path_failure(path: &str) -> Option<ResourceFailure> {
    if has_uri_scheme(path) {
        return Some(ResourceFailure::UriScheme);
    }
    if path.contains('\\') {
        return Some(ResourceFailure::Backslash);
    }
    let segments = if let Some(relative) = path.strip_prefix('/') {
        relative
    } else if path.len() >= 3
        && path.as_bytes()[0].is_ascii_alphabetic()
        && &path.as_bytes()[1..3] == b":/"
    {
        &path[3..]
    } else {
        path
    };
    segments
        .split('/')
        .any(str::is_empty)
        .then_some(ResourceFailure::EmptySegment)
}

/// URI spelling is a static declaration finding. Absolute Windows drive paths
/// remain local paths; the resolver port keeps authority over actual file kinds.
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
