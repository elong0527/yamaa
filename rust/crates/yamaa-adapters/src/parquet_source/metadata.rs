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

    // FlatBuffers permits many vector entries to share one table and string.
    // Verification must account for each traversal, not only the stored bytes.
    fn shared_metadata(repetitions: usize, text_bytes: usize) -> Vec<KeyValue> {
        let mut builder = flatbuffers::FlatBufferBuilder::new();
        let key = builder.create_string("shared");
        let value = builder.create_string(&"x".repeat(text_bytes));
        let item = arrow_ipc::KeyValue::create(
            &mut builder,
            &arrow_ipc::KeyValueArgs {
                key: Some(key),
                value: Some(value),
            },
        );
        let metadata = builder.create_vector(&vec![item; repetitions]);
        let fields = builder.create_vector::<flatbuffers::WIPOffset<arrow_ipc::Field<'_>>>(&[]);
        let schema = arrow_ipc::Schema::create(
            &mut builder,
            &arrow_ipc::SchemaArgs {
                fields: Some(fields),
                ..Default::default()
            },
        );
        let message = arrow_ipc::Message::create(
            &mut builder,
            &arrow_ipc::MessageArgs {
                header_type: arrow_ipc::MessageHeader::Schema,
                header: Some(schema.as_union_value()),
                custom_metadata: Some(metadata),
                ..Default::default()
            },
        );
        builder.finish(message, None);
        vec![KeyValue {
            key: "ARROW:schema".into(),
            value: Some(BASE64_STANDARD.encode(builder.finished_data())),
        }]
    }

    #[test]
    fn shared_tables_and_strings_retain_flatbuffer_verifier_limits() {
        assert!(admit(Some(&shared_metadata(4, 8))).unwrap().is_some());
        let defaults = flatbuffers::VerifierOptions::default();
        // A four-MiB vector exceeds the default table-visit limit without a
        // deep tree or a correspondingly large collection of stored tables.
        assert_eq!(
            admit(Some(&shared_metadata(defaults.max_tables, 8))),
            Err(Error::Malformed)
        );
        // The same stored string can also multiply apparent traversal bytes.
        let text_bytes = 1024 * 1024;
        assert_eq!(
            admit(Some(&shared_metadata(
                defaults.max_apparent_size / text_bytes + 1,
                text_bytes,
            ))),
            Err(Error::Malformed)
        );
    }

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
