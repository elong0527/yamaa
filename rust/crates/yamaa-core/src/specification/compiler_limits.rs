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

pub(super) fn parse_resource(resource: crate::numeric_parser::ParseResource) -> &'static str {
    use crate::numeric_parser::ParseResource;
    match resource {
        ParseResource::Depth => "semantic_depth",
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
    pub(super) fn numeric_parse(
        error: crate::numeric_parser::ParseError,
        path: Option<&str>,
    ) -> Self {
        match error {
            crate::numeric_parser::ParseError::Limit {
                resource, limit, ..
            } => Self::limit(parse_resource(resource), limit, None, path),
            _ => Self::Internal,
        }
    }
    pub(super) fn aggregate_parse(
        error: crate::aggregate_parser::ParseError,
        path: Option<&str>,
    ) -> Self {
        match error {
            crate::aggregate_parser::ParseError::Limit {
                resource, limit, ..
            } => Self::limit(parse_resource(resource), limit, None, path),
            _ => Self::Internal,
        }
    }
    pub(super) fn predicate_parse(
        error: crate::predicate_parser::ParseError,
        path: Option<&str>,
    ) -> Self {
        use crate::predicate_parser::ParseError;
        match error {
            ParseError::Limit {
                resource, limit, ..
            } => Self::limit(parse_resource(resource), limit, None, path),
            ParseError::RegexLimit {
                resource, limit, ..
            } => Self::limit(regex_resource(resource), limit, None, path),
            _ => Self::Internal,
        }
    }
    pub(super) fn predicate(error: crate::predicate_compiler::Error, path: Option<&str>) -> Self {
        use crate::{predicate::PlanError, predicate_compiler::Error};
        match error {
            Error::Parse(error) => Self::predicate_parse(error, path),
            Error::Plan(PlanError::Limit(error)) => {
                Self::limit(error.resource, error.limit, error.actual, path)
            }
            Error::Plan(PlanError::Regex {
                error: crate::portable_regex::CompileError::Limit { resource, limit },
                ..
            }) => Self::limit(regex_resource(resource), limit, None, path),
            _ => Self::Internal,
        }
    }
}
