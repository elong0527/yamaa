//! Typed value preservation proof for layout-only document edits.

use yamaa_core::schema::Document;

/// Prove that admitted documents keep scalar kinds and sequence values.
pub fn same_value(before: &Document, after: &Document, maximum: usize) -> bool {
    before.same_style_value(after, maximum)
}
