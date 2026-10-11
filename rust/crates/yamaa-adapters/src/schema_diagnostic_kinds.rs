//! Represent the core's qualified schema diagnostic kind vocabulary.
use yamaa_core::schema::{schema_diagnostic_kind_label, SchemaContext};
pub(super) fn label(context: &SchemaContext) -> Option<&'static str> {
    schema_diagnostic_kind_label(context)
}
