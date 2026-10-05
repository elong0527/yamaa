//! Wire translation for shared key inference; hosts retain authored diagnostic paths.
use super::{core, field, Field, Kind};
use serde::Serialize;
use yamaa_core::key_relation;

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(super) enum Inference {
    UndeclaredOutput {
        key: usize,
    },
    NoApplicableKeys,
    Incompatible {
        key: usize,
        expected: Kind,
        actual: Kind,
    },
    Keys {
        keys: Vec<usize>,
    },
}

/// Borrow decoded metadata and preserve core-selected indices and exact declared types.
pub(super) fn infer(
    catalog: &core::Catalog,
    keys: &[String],
    fields: &[Field],
) -> Result<Inference, core::Error> {
    let keys: Vec<_> = keys.iter().map(String::as_str).collect();
    let fields: Vec<_> = fields.iter().map(field).collect();
    Ok(
        match key_relation::infer(catalog, &keys, &fields, key_relation::Limits::default())? {
            key_relation::Inference::UndeclaredOutput { key } => {
                Inference::UndeclaredOutput { key }
            }
            key_relation::Inference::NoApplicableKeys => Inference::NoApplicableKeys,
            key_relation::Inference::Incompatible {
                key,
                expected,
                actual,
            } => Inference::Incompatible {
                key,
                expected: expected.into(),
                actual: actual.into(),
            },
            key_relation::Inference::Keys { keys } => Inference::Keys { keys },
        },
    )
}
