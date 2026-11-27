//! Strict shared YAML source admission into the core's owned document tree.
pub use yamaa_core::yaml_diagnostics::UnicodeIssue;
use yamaa_core::{limits::LimitExceeded, schema::Document};

/// Byte offset and one-based coordinates in the original snapshot. The
/// column counts characters, which in an ASCII source are its bytes (SPEC-8).
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub struct SourcePosition {
    pub offset: usize,
    pub line: usize,
    pub column: usize,
}

impl SourcePosition {
    /// The position of the byte at `offset`, whose prefix is UTF-8. Lines break
    /// as the scanner breaks them, at a line feed, a carriage return or the two
    /// together, and a column counts characters.
    pub(crate) fn of(source: &[u8], offset: usize) -> Self {
        let mut line = 1;
        let mut column = 1;
        let mut previous_cr = false;
        for &byte in &source[..offset] {
            match byte {
                b'\n' if previous_cr => {}
                b'\r' | b'\n' => {
                    line += 1;
                    column = 1;
                }
                // A UTF-8 continuation byte continues its character.
                _ if byte & 0xc0 == 0x80 => {}
                _ => column += 1,
            }
            previous_cr = byte == b'\r';
        }
        Self {
            offset,
            line,
            column,
        }
    }
}

/// Source errors remain distinct from explicit resource refusal or adapter bugs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DecodeFailure {
    /// This build omits the optional YAML parser.
    Unavailable,
    /// A character the source may not write literally, under the encoding
    /// its header declares (SPEC-8).
    NonAscii(SourcePosition, Encoding),
    /// Bytes that are not UTF-8 in a source that declares UTF-8 (SPEC-8).
    InvalidUtf8(SourcePosition),
    InvalidYaml {
        position: SourcePosition,
        reason: String,
    },
    InvalidText(Vec<UnicodeIssue>),
    Limit(LimitExceeded),
    Internal,
}

/// The text encoding a source's header declares: ASCII unless it writes
/// `encoding: utf-8` (SPEC-8). `Other` is ASCII too, for a header that
/// writes `encoding` with another value, such as `UTF-8`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Encoding {
    Ascii,
    Utf8,
    Other,
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
