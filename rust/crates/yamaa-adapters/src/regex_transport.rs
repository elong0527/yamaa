//! Owned regex/1 requests with admission before matching and bounded serialization.
use serde::{Deserialize, Serialize};
use std::{fmt, io, panic::catch_unwind};
use yamaa_core::regex::{
    CompileError, CompileLimits, MatchLimits, Pattern, Resource, CONTRACT_VERSION,
};

pub const MAX_REQUEST_BYTES: usize = 1_048_576;
pub const MAX_RESPONSE_BYTES: usize = 1_048_576;
const PROTOCOL: &str = "regex/1";

/// Transport defects are separate from invalid patterns and resource refusals.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransportError {
    RequestLimit,
    InvalidRequest,
    UnsupportedProtocol,
    Internal,
}
impl fmt::Display for TransportError {
    /// Never return panic payloads or user-controlled source text as host errors.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::RequestLimit => "regex request exceeds byte limit",
            Self::InvalidRequest => "invalid regex request",
            Self::UnsupportedProtocol => "unsupported regex protocol",
            Self::Internal => "internal regex failure",
        })
    }
}
impl std::error::Error for TransportError {}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    protocol: String,
    pattern: String,
    operation: Operation,
}
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum Operation {
    Compile {},
    Search { subject: String },
    FullMatch { subject: String },
}

#[derive(Serialize)]
struct Response<T> {
    protocol: &'static str,
    contract_version: &'static str,
    outcome: T,
}
#[derive(Serialize)]
struct Position {
    byte: usize,
    character: usize,
}
#[derive(Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
enum Outcome<'a> {
    Compiled {
        group_count: usize,
    },
    Matched {
        group_count: usize,
        groups: &'a [Option<&'a str>],
    },
    NoMatch {
        group_count: usize,
    },
    Invalid {
        condition: &'static str,
        requirement: &'static str,
        position: Position,
        reason: &'static str,
    },
    Unsupported {
        position: Position,
        feature: &'static str,
    },
    ResourceLimit {
        phase: &'static str,
        resource: &'static str,
        limit: usize,
    },
}

/// Compile before reading a decoded subject; return owned JSON with no host fallback.
/// Compile-only requests contain no subject. Matching returns all numbered groups,
/// including group zero, preserving empty/unentered groups and no-match separately.
pub fn evaluate_regex(request: &str) -> Result<String, TransportError> {
    if request.len() > MAX_REQUEST_BYTES {
        return Err(TransportError::RequestLimit);
    }
    catch_unwind(|| process(request)).map_err(|_| TransportError::Internal)?
}

/// Decode a closed envelope, then admit the pattern before selecting the operation.
fn process(request: &str) -> Result<String, TransportError> {
    let request: Request =
        serde_json::from_str(request).map_err(|_| TransportError::InvalidRequest)?;
    if request.protocol != PROTOCOL {
        return Err(TransportError::UnsupportedProtocol);
    }
    let position = |byte| Position {
        byte,
        character: request.pattern[..byte].chars().count(),
    };
    let pattern = match Pattern::compile(&request.pattern, CompileLimits::default()) {
        Ok(pattern) => pattern,
        Err(CompileError::Invalid { byte, reason }) => {
            return encode(Outcome::Invalid {
                condition: "invalid_regex",
                requirement: "REQ-0827",
                position: position(byte),
                reason,
            });
        }
        Err(CompileError::Unsupported { byte, feature }) => {
            return encode(Outcome::Unsupported {
                position: position(byte),
                feature,
            });
        }
        Err(CompileError::Limit { resource, limit }) => {
            return encode(refusal("compile", resource, limit));
        }
    };
    let group_count = pattern.group_count();
    let found = match &request.operation {
        Operation::Compile {} => return encode(Outcome::Compiled { group_count }),
        Operation::Search { subject } => pattern.search(subject, MatchLimits::default()),
        Operation::FullMatch { subject } => pattern.full_match(subject, MatchLimits::default()),
    };
    match found {
        Ok(Some(found)) => encode(Outcome::Matched {
            group_count,
            groups: &found.groups,
        }),
        Ok(None) => encode(Outcome::NoMatch { group_count }),
        Err(error) => encode(refusal("match", error.resource, error.limit)),
    }
}

/// Preserve the compiler/matcher resource identity rather than a generic mismatch.
fn refusal(phase: &'static str, resource: Resource, limit: usize) -> Outcome<'static> {
    let resource = match resource {
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
    };
    Outcome::ResourceLimit {
        phase,
        resource,
        limit,
    }
}

struct ResponseBuffer {
    bytes: Vec<u8>,
    exhausted: bool,
}
impl io::Write for ResponseBuffer {
    /// Bound escaped UTF-8 output before each append, without cloning captures.
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > MAX_RESPONSE_BYTES - self.bytes.len() {
            self.exhausted = true;
            return Err(io::Error::other("regex response limit"));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    /// The buffer has no external sink to flush.
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// Serialize borrowed groups directly; discard partial output on response refusal.
fn encode(outcome: Outcome<'_>) -> Result<String, TransportError> {
    let mut buffer = ResponseBuffer {
        bytes: Vec::new(),
        exhausted: false,
    };
    let result = serde_json::to_writer(
        &mut buffer,
        &Response {
            protocol: PROTOCOL,
            contract_version: CONTRACT_VERSION,
            outcome,
        },
    );
    if buffer.exhausted {
        return serde_json::to_string(&Response {
            protocol: PROTOCOL,
            contract_version: CONTRACT_VERSION,
            outcome: Outcome::ResourceLimit {
                phase: "response",
                resource: "response_bytes",
                limit: MAX_RESPONSE_BYTES,
            },
        })
        .map_err(|_| TransportError::Internal);
    }
    result.map_err(|_| TransportError::Internal)?;
    String::from_utf8(buffer.bytes).map_err(|_| TransportError::Internal)
}
