//! Bound the embedded Arrow schema before the generic Parquet schema conversion.
use super::compact::Error;
use base64::{prelude::BASE64_STANDARD, Engine};
use parquet::file::metadata::KeyValue;

pub(super) fn admit(
    metadata: Option<&Vec<KeyValue>>,
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

    let bytes = BASE64_STANDARD
        .decode(value)
        .map_err(|_| Error::Malformed)?;
    let message = if bytes.len() > 8 && bytes[..4] == [255; 4] {
        &bytes[8..]
    } else {
        &bytes
    };
    let options = flatbuffers::VerifierOptions {
        max_depth: yamaa_core::limits::Limits::SEMANTIC_DEPTH,
        max_tables: usize::MAX,
        max_apparent_size: usize::MAX,
        ..Default::default()
    };
    let message =
        arrow_ipc::root_as_message_with_opts(&options, message).map_err(|_| Error::Malformed)?;
    let schema = message.header_as_schema().ok_or(Error::Malformed)?;
    crate::storage_api::ipc_schema(schema)
        .map(Some)
        .map_err(|_| Error::Malformed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_schema_hints_and_encoding_are_not_ignored() {
        admit(None).unwrap();
        let data = vec![KeyValue {
            key: "ARROW:schema".into(),
            value: Some("!".into()),
        }];
        assert_eq!(admit(Some(&data)), Err(Error::Malformed));
        let data = vec![KeyValue {
            key: "ARROW:schema".into(),
            value: Some("AAAA".into()),
        }];
        assert_eq!(admit(Some(&data)), Err(Error::Malformed));
    }
}
