//! Bounded transport for the shared analysis of already-bound dependency graphs.
use serde::{Deserialize, Serialize};
use std::{fmt, panic::catch_unwind};
use yamaa_core::dependency_analysis::{self, Analysis, Limits};

/// Trusted byte admission precedes JSON allocation; callers cannot raise graph limits.
pub const MAX_REQUEST_BYTES: usize = 1_048_576;
const PROTOCOL: &str = "dependency-analysis/1";

/// Transport failures are distinct from cycles and explicit resource outcomes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TransportError {
    RequestLimit,
    InvalidEnvelope,
    UnsupportedProtocol,
    InvalidGraph,
    Internal,
}

impl fmt::Display for TransportError {
    /// Keep host errors stable without echoing request contents or panic payloads.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::RequestLimit => "dependency analysis request exceeds byte limit",
            Self::InvalidEnvelope => "invalid dependency analysis envelope",
            Self::UnsupportedProtocol => "unsupported dependency analysis protocol",
            Self::InvalidGraph => "dependency index outside declared graph",
            Self::Internal => "internal dependency analysis failure",
        })
    }
}
impl std::error::Error for TransportError {}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    protocol: String,
    dependencies: Vec<Vec<usize>>,
}

#[derive(Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
enum Outcome {
    Complete {
        cycle: Option<Vec<usize>>,
        order: Vec<usize>,
    },
    Limit {
        resource: &'static str,
        limit: String,
        required: String,
    },
}

/// Analyze once without names, data access, callbacks, host recursion or semantic fallback.
pub fn analyze_dependencies(request: &str) -> Result<String, TransportError> {
    if request.len() > MAX_REQUEST_BYTES {
        return Err(TransportError::RequestLimit);
    }
    catch_unwind(|| {
        let request: Request =
            serde_json::from_str(request).map_err(|_| TransportError::InvalidEnvelope)?;
        if request.protocol != PROTOCOL {
            return Err(TransportError::UnsupportedProtocol);
        }
        let outcome = match dependency_analysis::analyze(&request.dependencies, Limits::default()) {
            Ok(Analysis { cycle, order }) => Outcome::Complete { cycle, order },
            Err(dependency_analysis::Error::NodeLimit { limit, required }) => Outcome::Limit {
                resource: "nodes",
                limit: limit.to_string(),
                required: required.to_string(),
            },
            Err(dependency_analysis::Error::EdgeLimit { limit, required }) => Outcome::Limit {
                resource: "edges",
                limit: limit.to_string(),
                required: required.to_string(),
            },
            Err(dependency_analysis::Error::InvalidDependency { .. }) => {
                return Err(TransportError::InvalidGraph);
            }
            Err(dependency_analysis::Error::SizeOverflow) => return Err(TransportError::Internal),
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
