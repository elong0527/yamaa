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
impl CompilerLimit {
    /// A resource refusal of the expression compiler at `path`, the field
    /// its text is written at; a language finding is no limit.
    pub fn expression(error: &crate::expression::CompileError, path: Option<&str>) -> Option<Self> {
        use crate::expression::{CompileError, Resource};
        let CompileError::Limit {
            resource, limit, ..
        } = error
        else {
            return None;
        };
        let resource = match resource {
            Resource::Depth => "semantic_depth",
            Resource::Regex(resource) => regex_resource(*resource),
        };
        Some(Self {
            resource,
            limit: *limit,
            actual: None,
            spec_paths: path.map(String::from).into_iter().collect(),
        })
    }
}
impl PrepareError {
    /// A resource refusal of the expression compiler; a language finding is
    /// retained for binding instead and never reaches this projection.
    pub(super) fn expression(error: crate::expression::CompileError, path: Option<&str>) -> Self {
        CompilerLimit::expression(&error, path).map_or(Self::Internal, Self::Limit)
    }
}
