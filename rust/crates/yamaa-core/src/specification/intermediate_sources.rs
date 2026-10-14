//! Bound retained donor schema copies before any intermediate metadata is cloned.
use super::*;
const METADATA_BYTES: usize = 16_777_216;
pub(super) fn admit<'a>(
    relations: impl Iterator<Item = (&'a TableSchema, &'a str)>,
) -> Result<(), BindError> {
    let mut required = 0usize;
    for (schema, name) in relations {
        required = required
            .checked_add(name.len())
            .and_then(|bytes| bytes.checked_add(core::mem::size_of::<TableSchema>()))
            .and_then(|bytes| bytes.checked_add(core::mem::size_of::<String>()))
            .ok_or_else(overflow)?;
        for column in schema.columns() {
            required = required
                .checked_add(column.name.len())
                .and_then(|bytes| bytes.checked_add(core::mem::size_of::<crate::table::Column>()))
                .ok_or_else(overflow)?;
        }
        if required > METADATA_BYTES {
            return Err(BindError::Catalog(reference_binding::Error::Limit {
                resource: "intermediate_source_metadata",
                limit: METADATA_BYTES,
                required,
            }));
        }
    }
    Ok(())
}
fn overflow() -> BindError {
    BindError::Catalog(reference_binding::Error::SizeOverflow)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn retained_schema_budget_is_aggregate_and_precedes_every_copy() {
        let schema = TableSchema::new(vec![crate::table::Column {
            name: "x".repeat(4096),
            kind: crate::value::ColumnType::Str,
        }])
        .unwrap();
        assert!(admit(core::iter::repeat_n((&schema, "SRC"), 2)).is_ok());
        assert!(
            matches!(admit(core::iter::repeat_n((&schema, "SRC"), 4097)),
            Err(BindError::Catalog(reference_binding::Error::Limit { resource: "intermediate_source_metadata", limit: METADATA_BYTES, required })) if required > METADATA_BYTES)
        );
    }
}
