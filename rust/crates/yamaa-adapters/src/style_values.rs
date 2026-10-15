//! Shared YAML admission for the retained style CLI's edit proof.

use crate::yaml_decode::decode_yaml;

/// Admit both byte documents and prove that the edit preserves typed values.
pub fn same_value(before: &[u8], after: &[u8]) -> bool {
    // Unchanged bytes need no unordered-map proof work, but still need admission.
    if before == after {
        return decode_yaml(before).is_ok();
    }
    match (decode_yaml(before), decode_yaml(after)) {
        (Ok(before), Ok(after)) => before.document.same_style_value(&after.document),
        _ => false,
    }
}
