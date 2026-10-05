//! Bounded transport for the shared analysis of already-bound column dependency rules.
use serde::{Deserialize, Serialize};
use std::{fmt, panic::catch_unwind};
use yamaa_core::column_dependencies::{self, Analysis, Diagnostic};
use yamaa_core::dependency_analysis::{self, Limits};

/// Trusted byte admission precedes JSON allocation; callers cannot raise graph limits.
pub const MAX_REQUEST_BYTES: usize = 1_048_576;
const PROTOCOL: &str = "column-dependencies/1";

/// Transport failures are distinct from cycles and explicit resource outcomes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TransportError {
    RequestLimit,
    InvalidEnvelope,
    UnsupportedProtocol,
    InvalidGraph,
    InvalidKeys,
    Internal,
}

impl fmt::Display for TransportError {
    /// Keep host errors stable without echoing request contents or panic payloads.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::RequestLimit => "column dependency analysis request exceeds byte limit",
            Self::InvalidEnvelope => "invalid column dependency analysis envelope",
            Self::UnsupportedProtocol => "unsupported column dependency analysis protocol",
            Self::InvalidGraph => "dependency index outside declared graph",
            Self::InvalidKeys => "invalid column dependency key indices",
            Self::Internal => "internal column dependency analysis failure",
        })
    }
}
impl std::error::Error for TransportError {}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    protocol: String,
    dependencies: Vec<Option<Vec<usize>>>,
    keys: Vec<usize>,
    has_rows: bool,
}

#[derive(Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
enum Outcome {
    Complete {
        order: Vec<usize>,
        diagnostics: Vec<PortableDiagnostic>,
    },
    Limit {
        resource: &'static str,
        limit: String,
        required: String,
    },
}

#[derive(Serialize)]
struct PortableDiagnostic {
    condition: &'static str,
    requirement: &'static str,
    location: &'static str,
    columns: Vec<usize>,
}

impl From<Diagnostic> for PortableDiagnostic {
    /// Carry core-selected conditions and paths without repeating language rules.
    fn from(diagnostic: Diagnostic) -> Self {
        Self {
            condition: diagnostic.condition(),
            requirement: diagnostic.requirement(),
            location: diagnostic.location(),
            columns: diagnostic.columns(),
        }
    }
}

/// Analyze once without names, data access, callbacks, host recursion or semantic fallback.
pub fn analyze_column_dependencies(request: &str) -> Result<String, TransportError> {
    if request.len() > MAX_REQUEST_BYTES {
        return Err(TransportError::RequestLimit);
    }
    catch_unwind(|| {
        let request: Request =
            serde_json::from_str(request).map_err(|_| TransportError::InvalidEnvelope)?;
        if request.protocol != PROTOCOL {
            return Err(TransportError::UnsupportedProtocol);
        }
        let outcome = match column_dependencies::analyze(
            &request.dependencies,
            &request.keys,
            request.has_rows,
            Limits::default(),
        ) {
            Ok(Analysis { order, diagnostics }) => Outcome::Complete {
                order,
                diagnostics: diagnostics.into_iter().map(Into::into).collect(),
            },
            Err(column_dependencies::Error::Graph(dependency_analysis::Error::NodeLimit {
                limit,
                required,
            })) => Outcome::Limit {
                resource: "nodes",
                limit: limit.to_string(),
                required: required.to_string(),
            },
            Err(column_dependencies::Error::Graph(dependency_analysis::Error::EdgeLimit {
                limit,
                required,
            })) => Outcome::Limit {
                resource: "edges",
                limit: limit.to_string(),
                required: required.to_string(),
            },
            Err(column_dependencies::Error::Graph(
                dependency_analysis::Error::InvalidDependency { .. },
            )) => {
                return Err(TransportError::InvalidGraph);
            }
            Err(
                column_dependencies::Error::InvalidKey { .. }
                | column_dependencies::Error::DuplicateKey { .. },
            ) => return Err(TransportError::InvalidKeys),
            Err(column_dependencies::Error::Graph(dependency_analysis::Error::SizeOverflow)) => {
                return Err(TransportError::Internal)
            }
        };
        #[derive(Serialize)]
        struct Response {
            protocol: &'static str,
            outcome: Outcome,
        }
        serde_json::to_string(&Response {
            protocol: PROTOCOL,
            outcome,
        })
        .map_err(|_| TransportError::Internal)
    })
    .map_err(|_| TransportError::Internal)?
}
