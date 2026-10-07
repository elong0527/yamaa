//! One byte-oriented YAML service for both installed hosts. Tree envelopes are
//! identical to schema/1: integers are decimal strings and floats are exact bits.

use std::{fmt, io, panic::catch_unwind};

use serde_json::{json, Value};

use crate::{
    schema_transport::wire::Tree,
    yaml_decode::{decode_yaml, DecodeFailure, DecodeLimits},
};

pub const MAX_SOURCE_BYTES: usize = 8_388_608;
pub const MAX_RESPONSE_BYTES: usize = 16_777_216;

/// Host transport failures; authored YAML failures are returned as data.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransportError {
    ResponseLimit,
    Internal,
}
impl fmt::Display for TransportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::ResponseLimit => "YAML response exceeds byte limit",
            Self::Internal => "internal YAML decoder failure",
        })
    }
}
impl std::error::Error for TransportError {}

struct Response {
    bytes: Vec<u8>,
    limit: usize,
}
impl io::Write for Response {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self.bytes.len().saturating_add(bytes.len()) > self.limit {
            return Err(io::Error::other("response limit"));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn encode(outcome: Value, limit: usize) -> Result<String, TransportError> {
    encode_outcome("yaml/1", outcome, limit)
}
/// Bound the complete encoded envelope without publishing a partial prefix.
pub(crate) fn encode_outcome(
    protocol: &str,
    outcome: Value,
    limit: usize,
) -> Result<String, TransportError> {
    let mut response = Response {
        bytes: Vec::new(),
        limit,
    };
    serde_json::to_writer(
        &mut response,
        &json!({"protocol":protocol,"outcome":outcome}),
    )
    .map_err(|error| {
        if error.is_io() {
            TransportError::ResponseLimit
        } else {
            TransportError::Internal
        }
    })?;
    String::from_utf8(response.bytes).map_err(|_| TransportError::Internal)
}

/// Semantic source findings are shared by byte probes and document preparation.
/// Resource refusals and internal defects are never fabricated language findings.
pub(crate) fn semantic_diagnostics(error: &DecodeFailure) -> Option<Vec<Value>> {
    Some(match error {
        DecodeFailure::NonAscii(position) => vec![json!({
            "condition":"non_ascii_source", "spec_paths":["$"],
            "context":{"line":position.line,"column":position.column},
        })],
        DecodeFailure::InvalidYaml { position, reason } => vec![json!({
            "condition":"invalid_yaml", "spec_paths":["$"],
            "context":{"line":position.line,"column":position.column,"reason":reason},
        })],
        DecodeFailure::InvalidText(issues) => {
            issues.iter().map(|issue| json!({
                "condition":"invalid_text", "spec_paths":[issue.path],
                "context":{"code_point":format!("U+{:04X}",issue.code_point),"offset":issue.offset},
            })).collect::<Vec<_>>()
        }
        DecodeFailure::Limit(_) | DecodeFailure::Internal => return None,
    })
}

fn outcome(source: &[u8]) -> Result<Value, TransportError> {
    let limits = DecodeLimits::default();
    debug_assert_eq!(limits.scan.source_bytes, MAX_SOURCE_BYTES);
    Ok(match decode_yaml(source, limits) {
        Ok(decoded) => json!({
            "status":"decoded", "document":Tree::from_core(&decoded.document),
            "locations":decoded.locations,
        }),
        Err(error) => {
            if let Some(diagnostics) = semantic_diagnostics(&error) {
                return Ok(json!({"status":"invalid","diagnostics":diagnostics}));
            }
            match error {
                DecodeFailure::Limit(resource) => json!({
                    "status":"resource_limit", "phase":"yaml_source", "resource":resource,
                    "limit": match resource {
                        "source_bytes" => limits.scan.source_bytes,
                        "events" => limits.scan.events,
                        "depth" => limits.scan.depth.min(limits.document.depth).min(64),
                        "decoded_bytes" => limits.scan.decoded_bytes,
                        "parse_bytes" => limits.scan.parse_bytes,
                        "nodes" => limits.document.nodes,
                        "text_bytes" => limits.document.text_bytes,
                        "edges" => limits.document.edges,
                        "numeric_digits" => limits.numeric_digits,
                        "diagnostic_bytes" => limits.diagnostic_bytes,
                        _ => return Err(TransportError::Internal),
                    },
                }),
                _ => return Err(TransportError::Internal),
            }
        }
    })
}

/// Decode retained bytes without host interpretation, filesystem IO or execution
/// authority. All JSON text is valid Unicode; invalid source text is diagnostic
/// data, and no partial tree is returned on failure.
pub fn decode_yaml_bytes(source: &[u8]) -> Result<String, TransportError> {
    catch_unwind(|| encode(outcome(source)?, MAX_RESPONSE_BYTES))
        .map_err(|_| TransportError::Internal)?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn installed_hosts_share_independent_full_wire_truth() {
        for line in include_str!("../tests/fixtures/yaml_transport.tsv")
            .lines()
            .skip(1)
        {
            let fields: Vec<_> = line.split('\t').collect();
            assert_eq!(fields.len(), 3);
            let source: Vec<_> = fields[1]
                .as_bytes()
                .chunks_exact(2)
                .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
                .collect();
            assert_eq!(
                decode_yaml_bytes(&source).unwrap(),
                fields[2],
                "{}",
                fields[0]
            );
        }
    }

    #[test]
    fn byte_boundary_has_lossless_schema_compatible_values() {
        let response: Value =
            serde_json::from_str(&decode_yaml_bytes(b"[9007199254740993, -0.0]").unwrap()).unwrap();
        assert_eq!(
            response,
            json!({"protocol":"yaml/1","outcome":{
                "status":"decoded", "document":{"root":2,"nodes":[
                    {"kind":"integer","value":"9007199254740993"},
                    {"kind":"float","bits":"8000000000000000"},
                    {"kind":"sequence","items":[0,1]},
                ]}, "locations":[
                    {"offset":1,"line":1,"column":2},
                    {"offset":19,"line":1,"column":20},
                    {"offset":0,"line":1,"column":1},
                ],
            }})
        );
    }

    #[test]
    fn invalid_bytes_and_surrogates_are_explicit_outcomes_without_partial_trees() {
        for (source, expected) in [
            (
                b"x: \xff".as_slice(),
                json!({"condition":"non_ascii_source","spec_paths":["$"],"context":{"line":1,"column":4}}),
            ),
            (
                br#"x: "a\uD800""#.as_slice(),
                json!({"condition":"invalid_text","spec_paths":["$.x"],"context":{"code_point":"U+D800","offset":1}}),
            ),
        ] {
            let response: Value =
                serde_json::from_str(&decode_yaml_bytes(source).unwrap()).unwrap();
            assert_eq!(
                response,
                json!({"protocol":"yaml/1","outcome":{"status":"invalid","diagnostics":[expected]}})
            );
        }
    }

    #[test]
    fn source_and_response_limits_do_not_publish_prefixes() {
        let source = vec![b' '; MAX_SOURCE_BYTES + 1];
        let response: Value = serde_json::from_str(&decode_yaml_bytes(&source).unwrap()).unwrap();
        assert_eq!(
            response["outcome"],
            json!({"status":"resource_limit","phase":"yaml_source","resource":"source_bytes","limit":8388608})
        );
        assert_eq!(
            encode(json!({"status":"decoded"}), 1),
            Err(TransportError::ResponseLimit)
        );
    }
}
