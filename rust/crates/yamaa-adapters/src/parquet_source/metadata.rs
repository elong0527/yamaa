//! Bound the embedded Arrow schema before the generic Parquet schema conversion.
use super::compact::Error;
use base64::{prelude::BASE64_STANDARD, Engine};
use parquet::file::metadata::KeyValue;

#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub bytes: usize,
    pub tables: usize,
    pub depth: usize,
}

pub(super) fn admit(
    metadata: Option<&Vec<KeyValue>>,
    limits: Limits,
) -> Result<Option<arrow_schema::Schema>, Error> {
    // The general reader uses the last non-null value for each metadata key.
    let value = metadata.into_iter().flatten().rev().find_map(|item| {
        if item.key == "ARROW:schema" {
            item.value.as_deref()
        } else {
            None
        }
    });
    let Some(value) = value else {
        return Ok(None);
    };
    if value.len() > limits.bytes {
        return Err(Error::ResourceLimit {
            resource: "arrow_schema_bytes",
            limit: limits.bytes,
            actual: Some(value.len()),
        });
    }
    let bytes = BASE64_STANDARD
        .decode(value)
        .map_err(|_| Error::Malformed)?;
    let message = if bytes.len() > 8 && bytes[..4] == [255; 4] {
        &bytes[8..]
    } else {
        &bytes
    };
    let options = flatbuffers::VerifierOptions {
        max_depth: limits.depth,
        max_tables: limits.tables,
        max_apparent_size: limits.bytes,
        ..Default::default()
    };
    let message =
        arrow_ipc::root_as_message_with_opts(&options, message).map_err(|error| match error {
            flatbuffers::InvalidFlatbuffer::TooManyTables => Error::ResourceLimit {
                resource: "arrow_schema_tables",
                limit: limits.tables,
                actual: None,
            },
            flatbuffers::InvalidFlatbuffer::ApparentSizeTooLarge => Error::ResourceLimit {
                resource: "arrow_schema_bytes",
                limit: limits.bytes,
                actual: None,
            },
            flatbuffers::InvalidFlatbuffer::DepthLimitReached => Error::ResourceLimit {
                resource: "arrow_schema_depth",
                limit: limits.depth,
                actual: None,
            },
            _ => Error::Malformed,
        })?;
    let schema = message.header_as_schema().ok_or(Error::Malformed)?;
    arrow_ipc::convert::try_fb_to_schema(schema)
        .map(Some)
        .map_err(|_| Error::Malformed)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn limits() -> Limits {
        Limits {
            bytes: 1024,
            tables: 128,
            depth: 32,
        }
    }
    #[test]
    fn metadata_size_and_encoding_are_checked_without_ignoring_invalid_schema_hints() {
        admit(None, limits()).unwrap();
        let data = vec![KeyValue {
            key: "ARROW:schema".into(),
            value: Some("!".into()),
        }];
        assert_eq!(admit(Some(&data), limits()), Err(Error::Malformed));
        assert_eq!(
            admit(
                Some(&data),
                Limits {
                    bytes: 0,
                    ..limits()
                }
            ),
            Err(Error::ResourceLimit {
                resource: "arrow_schema_bytes",
                limit: 0,
                actual: Some(1),
            })
        );
        let data = vec![KeyValue {
            key: "ARROW:schema".into(),
            value: Some("AAAA".into()),
        }];
        assert_eq!(admit(Some(&data), limits()), Err(Error::Malformed));
    }
}
