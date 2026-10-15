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
        ParseResource::Bytes => "bytes",
        ParseResource::Tokens => "tokens",
        ParseResource::Nodes => "nodes",
        ParseResource::Depth => "depth",
    }
}
pub(super) fn regex_resource(resource: crate::regex::Resource) -> &'static str {
    use crate::regex::Resource;
    match resource {
        Resource::PatternBytes => "pattern_bytes",
        Resource::Nodes => "nodes",
        Resource::Groups => "groups",
        Resource::Depth => "depth",
        Resource::Repetition => "repetition",
        Resource::Width => "width",
        Resource::WidthWork => "width_work",
        Resource::WidthCells => "width_cells",
        Resource::SubjectBytes => "subject_bytes",
        Resource::Work => "work",
        Resource::StateCells => "state_cells",
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
            ParseError::UnsupportedRegex { .. } => Self::Unsupported(vec![UnsupportedFeature {
                operation: "predicate_regex".into(),
                path: path.unwrap_or("").into(),
            }]),
            _ => Self::Internal,
        }
    }
    pub(super) fn predicate(error: crate::predicate_compiler::Error, path: Option<&str>) -> Self {
        use crate::{
            predicate::{PlanError, Resource},
            predicate_compiler::Error,
        };
        match error {
            Error::Parse(error) => Self::predicate_parse(error, path),
            Error::Plan(PlanError::Limit(error)) => {
                let resource = match error.resource {
                    Resource::Nodes => "nodes",
                    Resource::Depth => "depth",
                    Resource::Work => "work",
                    Resource::Resolutions => "resolutions",
                    Resource::TextBytes => "text_bytes",
                    Resource::LikeWork => "like_work",
                    Resource::RegexSubjectBytes => "regex_subject_bytes",
                    Resource::RegexWork => "regex_work",
                    Resource::RegexStateCells => "regex_state_cells",
                };
                Self::limit(resource, error.limit, None, path)
            }
            Error::Plan(PlanError::Regex {
                error:
                    crate::regex::CompileError::Limit {
                        resource, limit, ..
                    },
                ..
            }) => Self::limit(regex_resource(resource), limit, None, path),
            _ => Self::Internal,
        }
    }
}

pub(super) fn charge(
    resource: &'static str,
    used: &mut usize,
    amount: usize,
    limit: usize,
    path: Option<&str>,
) -> Result<(), PrepareError> {
    let actual = used.checked_add(amount);
    *used = actual
        .filter(|&n| n <= limit)
        .ok_or_else(|| PrepareError::limit(resource, limit, actual, path))?;
    Ok(())
}
