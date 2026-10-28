//! Strict shared YAML source admission into the core's owned document tree.
pub use yamaa_core::yaml_diagnostics::UnicodeIssue;
use yamaa_core::{limits::LimitExceeded, schema::Document};

/// Byte offset and one-based coordinates in the original ASCII snapshot.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub struct SourcePosition {
    pub offset: usize,
    pub line: usize,
    pub column: usize,
}

/// Source errors remain distinct from explicit resource refusal or adapter bugs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DecodeFailure {
    /// This build omits the optional YAML parser.
    Unavailable,
    NonAscii(SourcePosition),
    InvalidYaml {
        position: SourcePosition,
        reason: String,
    },
    InvalidText(Vec<UnicodeIssue>),
    Limit(LimitExceeded),
    Internal,
}

/// Successful decoding retains an original source location for every occurrence.
#[derive(Clone, Debug)]
pub struct DecodedYaml {
    pub document: Document,
    pub locations: Vec<SourcePosition>,
    /// Exclusive scanner end, parallel to `locations` and document nodes.
    pub end_locations: Vec<SourcePosition>,
}

#[cfg(feature = "yaml")]
mod reader;
#[cfg(feature = "yaml")]
pub use reader::decode_yaml;

/// YAML admission remains a typed boundary when the parser is omitted.
#[cfg(not(feature = "yaml"))]
pub fn decode_yaml(_: &[u8]) -> Result<DecodedYaml, DecodeFailure> {
    Err(DecodeFailure::Unavailable)
}
