//! Source-option normalization is independent of physical CSV/Parquet encoding.
pub use yamaa_core::source_ordinal::stored_fields;
pub use yamaa_core::source_ordinal::{Error, Limits, Ordinal};
use yamaa_core::{specification::SourceDeclaration, table::TableSchema};
/// Decide generated-field policy once; adapters only encode the admitted positions.
pub fn ordinal(
    source: &SourceDeclaration,
    schema: &TableSchema,
    rows: usize,
    limits: Limits,
) -> Result<Option<Ordinal>, Error> {
    source
        .ordinal
        .as_deref()
        .map(|name| Ordinal::admit(schema, rows, name, limits))
        .transpose()
}
