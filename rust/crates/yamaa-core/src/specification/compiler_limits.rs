//! Classified compiler policy refusals retain the capacity that was exhausted.
use super::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompilerLimit {
    pub resource: &'static str,
    pub limit: usize,
    /// Unknown when admission stopped before counting the complete declaration.
    pub actual: Option<usize>,
    pub spec_paths: Vec<String>,
}
impl PrepareError {
    pub(super) fn limit(
        resource: &'static str,
        limit: usize,
        actual: Option<usize>,
        path: Option<&str>,
    ) -> Self {
        Self::Limit(CompilerLimit {
            resource,
            limit,
            actual,
            spec_paths: path.map(String::from).into_iter().collect(),
        })
    }
}

pub(super) fn regex_resource(resource: crate::portable_regex::Resource) -> &'static str {
    use crate::portable_regex::Resource;
    match resource {
        Resource::Depth => "semantic_depth",
        Resource::Size => "regex_size",
    }
}
impl PrepareError {
    /// A resource refusal of the expression compiler; a language finding is
    /// retained for binding instead and never reaches this projection.
    pub(super) fn expression(error: crate::expression::CompileError, path: Option<&str>) -> Self {
        use crate::expression::{CompileError, Resource};
        match error {
            CompileError::Limit {
                resource: Resource::Depth,
                limit,
                ..
            } => Self::limit("semantic_depth", limit, None, path),
            CompileError::Limit {
                resource: Resource::Regex(resource),
                limit,
                ..
            } => Self::limit(regex_resource(resource), limit, None, path),
            CompileError::Invalid { .. } => Self::Internal,
        }
    }
}
