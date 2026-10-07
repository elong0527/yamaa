//! Preserve existing diagnostic wire labels while core findings retain decoded kinds.
use yamaa_core::schema::{DocumentKind as K, SchemaContext};

/// Descriptor-value and layer-shape findings have distinct existing wire vocabularies.
pub(super) fn label(context: &SchemaContext) -> Option<&'static str> {
    let (kind, layer) = match context {
        SchemaContext::ValueKind(kind) => (*kind, false),
        SchemaContext::LayerKind(kind) => (*kind, true),
        _ => return None,
    };
    Some(match kind {
        K::Null if layer => "NoneType",
        K::Null => "null",
        K::Boolean => "bool",
        K::Integer => "int",
        K::Float => "float",
        K::Text => "str",
        K::Sequence if layer => "list",
        K::Sequence => "sequence",
        K::Mapping if layer => "dict",
        K::Mapping => "mapping",
    })
}
