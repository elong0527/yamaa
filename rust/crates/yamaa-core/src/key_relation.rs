//! Declared key type compatibility and ordered applicable-key inference, without records.
use alloc::{collections::BTreeMap, vec::Vec};

use crate::{
    reference_binding::{Catalog, Error, Field},
    value::ColumnType,
};

/// Bound all submitted names, including duplicates and fields never selected as keys.
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub entries: usize,
    pub context_bytes: usize,
}

impl Default for Limits {
    /// Keep temporary relation metadata within the reference service's trusted policy.
    fn default() -> Self {
        Self {
            entries: 65_536,
            context_bytes: 1_048_576,
        }
    }
}

/// One inference outcome; indices refer to the original ordered output key names.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Inference {
    /// Output identity is invalid independently of the right relation's fields.
    UndeclaredOutput {
        key: usize,
    },
    NoApplicableKeys,
    Incompatible {
        key: usize,
        expected: ColumnType,
        actual: ColumnType,
    },
    Keys {
        keys: Vec<usize>,
    },
}

/// REQ-0005 permits exact types or the int/float family, never date/datetime coercion.
pub fn comparable(left: ColumnType, right: ColumnType) -> bool {
    use ColumnType::{Float, Int};
    left == right || matches!((left, right), (Int, Float) | (Float, Int))
}

/// Infer REQ-0150 keys in output order and report the first REQ-0151 type mismatch.
/// Right fields are normalized schema metadata (including SELF/derived fields), not values.
/// Admit every submitted entry before semantic early exits; invalid output identity
/// is explicit so hosts can retain their earlier REQ-0220 declaration diagnostic.
pub fn infer(
    catalog: &Catalog,
    keys: &[&str],
    fields: &[Field<'_>],
    limits: Limits,
) -> Result<Inference, Error> {
    let entries = keys
        .len()
        .checked_add(fields.len())
        .ok_or(Error::SizeOverflow)?;
    if entries > limits.entries {
        return Err(Error::Limit {
            resource: "key_entries",
            limit: limits.entries,
            required: entries,
        });
    }
    let names = keys
        .iter()
        .copied()
        .chain(fields.iter().map(|field| field.name));
    let bytes = names.clone().try_fold(0usize, |total, name| {
        total.checked_add(name.len()).ok_or(Error::SizeOverflow)
    })?;
    if bytes > limits.context_bytes {
        return Err(Error::Limit {
            resource: "key_context_bytes",
            limit: limits.context_bytes,
            required: bytes,
        });
    }
    if names.clone().any(str::is_empty) {
        return Err(Error::EmptyName);
    }
    let mut right = BTreeMap::new();
    for field in fields {
        if right.insert(field.name, field.column_type).is_some() {
            return Err(Error::DuplicateField);
        }
    }
    // Identity validity precedes join selection, including when no key is shared.
    let mut output_types = Vec::with_capacity(keys.len());
    for (key, name) in keys.iter().enumerate() {
        if let Some(kind) = catalog.output_type(name) {
            output_types.push(kind);
        } else {
            return Ok(Inference::UndeclaredOutput { key });
        }
    }
    let mut selected = Vec::new();
    for (key, (name, expected)) in keys.iter().zip(output_types).enumerate() {
        if let Some(&actual) = right.get(name) {
            if !comparable(expected, actual) {
                return Ok(Inference::Incompatible {
                    key,
                    expected,
                    actual,
                });
            }
            selected.push(key);
        }
    }
    Ok(if selected.is_empty() {
        Inference::NoApplicableKeys
    } else {
        Inference::Keys { keys: selected }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reference_binding;
    use ColumnType::{Date, DateTime, Float, Int, Str};

    #[test]
    fn all_declared_type_pairs_have_independent_truth() {
        let kinds = [Str, Int, Float, Date, DateTime];
        let expected = [
            [true, false, false, false, false],
            [false, true, true, false, false],
            [false, true, true, false, false],
            [false, false, false, true, false],
            [false, false, false, false, true],
        ];
        for (i, left) in kinds.iter().enumerate() {
            for (j, right) in kinds.iter().enumerate() {
                assert_eq!(comparable(*left, *right), expected[i][j]);
            }
        }
    }

    fn catalog() -> Catalog {
        Catalog::compile(
            &[
                Field {
                    name: "A",
                    column_type: Int,
                },
                Field {
                    name: "B",
                    column_type: Str,
                },
                Field {
                    name: "D",
                    column_type: Date,
                },
            ],
            &[],
            reference_binding::Limits::default(),
        )
        .unwrap()
    }

    #[test]
    fn selection_and_mismatch_follow_authored_key_order() {
        let catalog = catalog();
        let fields = [
            Field {
                name: "B",
                column_type: Str,
            },
            Field {
                name: "A",
                column_type: Float,
            },
        ];
        assert_eq!(
            infer(&catalog, &["D", "A", "B"], &fields, Limits::default()),
            Ok(Inference::Keys {
                keys: alloc::vec![1, 2]
            })
        );
        let fields = [
            Field {
                name: "D",
                column_type: DateTime,
            },
            Field {
                name: "B",
                column_type: Int,
            },
        ];
        assert_eq!(
            infer(&catalog, &["B", "D"], &fields, Limits::default()),
            Ok(Inference::Incompatible {
                key: 0,
                expected: Str,
                actual: Int
            })
        );
        assert_eq!(
            infer(&catalog, &["D", "B"], &fields, Limits::default()),
            Ok(Inference::Incompatible {
                key: 0,
                expected: Date,
                actual: DateTime
            })
        );
        assert_eq!(
            infer(&catalog, &["A"], &fields, Limits::default()),
            Ok(Inference::NoApplicableKeys)
        );
        assert_eq!(
            infer(&catalog, &[], &fields, Limits::default()),
            Ok(Inference::NoApplicableKeys)
        );
        assert_eq!(
            infer(&catalog, &["ABSENT", "B"], &fields, Limits::default()),
            Ok(Inference::UndeclaredOutput { key: 0 })
        );
    }

    #[test]
    fn admission_counts_skipped_fields_duplicate_keys_and_utf8_bytes() {
        let catalog = catalog();
        let fields = [Field {
            name: "é",
            column_type: Int,
        }];
        let limits = Limits {
            entries: 2,
            context_bytes: 2,
        };
        assert_eq!(
            infer(&catalog, &["A", "A"], &fields, limits),
            Err(Error::Limit {
                resource: "key_entries",
                limit: 2,
                required: 3
            })
        );
        assert_eq!(
            infer(&catalog, &["A"], &fields, limits),
            Err(Error::Limit {
                resource: "key_context_bytes",
                limit: 2,
                required: 3
            })
        );
        assert_eq!(
            infer(&catalog, &["A"], &[fields[0], fields[0]], Limits::default()),
            Err(Error::DuplicateField)
        );
        assert_eq!(
            infer(
                &catalog,
                &["ABSENT"],
                &[Field {
                    name: "",
                    column_type: Str
                }],
                Limits::default()
            ),
            Err(Error::EmptyName)
        );
        let fields = [Field {
            name: "A",
            column_type: Int,
        }];
        assert_eq!(
            infer(&catalog, &["A", "A"], &fields, Limits::default()),
            Ok(Inference::Keys {
                keys: alloc::vec![0, 1]
            })
        );
    }
}
