//! Shared YAML admission for the retained style CLI's edit proof.

use crate::yaml_decode::{decode_yaml, DecodeLimits};

/// Admit both byte documents and prove that the edit preserves typed values.
pub fn same_value(before: &[u8], after: &[u8]) -> bool {
    let limits = DecodeLimits::default();
    // Unchanged bytes need no unordered-map proof work, but still need admission.
    if before == after {
        return decode_yaml(before, limits).is_ok();
    }
    match (decode_yaml(before, limits), decode_yaml(after, limits)) {
        (Ok(before), Ok(after)) => before.document.same_style_value(&after.document),
        _ => false,
    }
}
