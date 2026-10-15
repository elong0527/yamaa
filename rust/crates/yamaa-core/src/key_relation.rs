//! Declared key type compatibility and ordered applicable-key inference, without records.
use std::collections::BTreeMap;

use crate::{
    reference_binding::{Catalog, Error, Field},
    value::ColumnType,
};

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

/// A known expression type can be boolean even though declared columns cannot.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ComparableType {
    Column(ColumnType),
    Boolean,
}

impl From<ColumnType> for ComparableType {
    /// Keep the closed declared-column vocabulary separate from expression booleans.
    fn from(value: ColumnType) -> Self {
        Self::Column(value)
    }
}

/// REQ-0005 permits exact types or the int/float family, never boolean/numeric coercion.
pub fn comparable(left: ComparableType, right: ComparableType) -> bool {
    use ColumnType::{Float, Int};
    use ComparableType::Column;
    left == right
        || matches!(
            (left, right),
            (Column(Int), Column(Float)) | (Column(Float), Column(Int))
        )
}

/// Infer REQ-0150 keys in output order and report the first REQ-0151 type mismatch.
/// Right fields are normalized schema metadata (including SELF/derived fields), not values.
/// Validate every submitted name before semantic early exits; invalid output identity
/// is explicit so hosts can retain their earlier REQ-0220 declaration diagnostic.
pub fn infer(catalog: &Catalog, keys: &[&str], fields: &[Field<'_>]) -> Result<Inference, Error> {
    let fields = fields
        .iter()
        .map(|field| (field.name, Some(field.column_type)))
        .collect::<Vec<_>>();
    infer_partial(catalog, keys, &fields)
}

/// Raw donor fields preserve applicability even when no one kind is known.
pub(crate) fn infer_partial(
    catalog: &Catalog,
    keys: &[&str],
    fields: &[(&str, Option<ColumnType>)],
) -> Result<Inference, Error> {
    let names = keys
        .iter()
        .copied()
        .chain(fields.iter().map(|field| field.0));
    if names.clone().any(str::is_empty) {
        return Err(Error::EmptyName);
    }
    let mut right = BTreeMap::new();
    for &(name, kind) in fields {
        if right.insert(name, kind).is_some() {
            return Err(Error::DuplicateField);
        }
    }
    // Identity validity precedes join selection, including when no key is shared.
    let mut output_types = Vec::with_capacity(keys.len());
    for (key, name) in keys.iter().enumerate() {
        if catalog.output_type(name).is_some() {
            output_types.push(catalog.type_of(name));
        } else {
            return Ok(Inference::UndeclaredOutput { key });
        }
    }
    let mut selected = Vec::new();
    for (key, (name, expected)) in keys.iter().zip(output_types).enumerate() {
        if let Some(&actual) = right.get(name) {
            if let (Some(expected), Some(actual)) = (expected, actual) {
                if !comparable(expected.into(), actual.into()) {
                    return Ok(Inference::Incompatible {
                        key,
                        expected,
                        actual,
                    });
                }
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
                assert_eq!(comparable((*left).into(), (*right).into()), expected[i][j]);
            }
        }
        for kind in kinds {
            assert!(!comparable(ComparableType::Boolean, kind.into()));
            assert!(!comparable(kind.into(), ComparableType::Boolean));
        }
        assert!(comparable(ComparableType::Boolean, ComparableType::Boolean));
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
            infer(&catalog, &["D", "A", "B"], &fields),
            Ok(Inference::Keys {
                keys: std::vec![1, 2]
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
            infer(&catalog, &["B", "D"], &fields),
            Ok(Inference::Incompatible {
                key: 0,
                expected: Str,
                actual: Int
            })
        );
        assert_eq!(
            infer(&catalog, &["D", "B"], &fields),
            Ok(Inference::Incompatible {
                key: 0,
                expected: Date,
                actual: DateTime
            })
        );
        assert_eq!(
            infer(&catalog, &["A"], &fields),
            Ok(Inference::NoApplicableKeys)
        );
        assert_eq!(
            infer(&catalog, &[], &fields),
            Ok(Inference::NoApplicableKeys)
        );
        assert_eq!(
            infer(&catalog, &["ABSENT", "B"], &fields),
            Ok(Inference::UndeclaredOutput { key: 0 })
        );
    }

    #[test]
    fn invalid_names_duplicate_fields_and_repeated_keys_keep_distinct_outcomes() {
        let catalog = catalog();
        let fields = [Field {
            name: "é",
            column_type: Int,
        }];
        assert_eq!(
            infer(&catalog, &["A"], &[fields[0], fields[0]]),
            Err(Error::DuplicateField)
        );
        assert_eq!(
            infer(
                &catalog,
                &["ABSENT"],
                &[Field {
                    name: "",
                    column_type: Str
                }]
            ),
            Err(Error::EmptyName)
        );
        let fields = [Field {
            name: "A",
            column_type: Int,
        }];
        assert_eq!(
            infer(&catalog, &["A", "A"], &fields),
            Ok(Inference::Keys {
                keys: std::vec![0, 1]
            })
        );
    }
}
