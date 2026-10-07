//! Bound the embedded Arrow schema before the generic Parquet schema conversion.
use super::compact::Error;
use base64::{prelude::BASE64_STANDARD, Engine};
use parquet::file::metadata::KeyValue;

#[derive(Clone, Copy, Debug)]
pub(super) struct Limits {
    pub bytes: usize,
    pub tables: usize,
    pub depth: usize,
}

pub(super) fn admit(metadata: Option<&Vec<KeyValue>>, limits: Limits) -> Result<(), Error> {
    // The general reader uses the last non-null value for each metadata key.
    let value = metadata.into_iter().flatten().rev().find_map(|item| {
        if item.key == "ARROW:schema" {
            item.value.as_deref()
        } else {
            None
        }
    });
    let Some(value) = value else {
        return Ok(());
    };
    if value.len() > limits.bytes {
        return Err(Error::Limit);
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
            flatbuffers::InvalidFlatbuffer::TooManyTables
            | flatbuffers::InvalidFlatbuffer::ApparentSizeTooLarge
            | flatbuffers::InvalidFlatbuffer::DepthLimitReached => Error::Limit,
            _ => Error::Malformed,
        })?;
    message.header_as_schema().ok_or(Error::Malformed)?;
    Ok(())
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
            Err(Error::Limit)
        );
        let data = vec![KeyValue {
            key: "ARROW:schema".into(),
            value: Some("AAAA".into()),
        }];
        assert_eq!(admit(Some(&data), limits()), Err(Error::Malformed));
    }
}
