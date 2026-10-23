//! Declared key type compatibility, without records.
use std::collections::BTreeMap;

use crate::{
    reference_binding::{Catalog, Error, Field},
    result_type::ResultType,
    value::ColumnType,
};

/// One outcome; indices refer to the ordered key names [`declared`] pairs.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Inference {
    /// The input declares no `key`, so the lookup matches on no pair.
    NoApplicableKeys,
    Incompatible {
        key: usize,
        expected: ColumnType,
        actual: ColumnType,
    },
    Keys {
        keys: Vec<usize>,
    },
    /// A declared key column the relation does not store (LOOKUP-5).
    UnknownColumn {
        key: usize,
    },
    /// A declared key column the current row has no column of (LOOKUP-5).
    UnknownValue {
        key: usize,
    },
}

/// Pair an input's declared `key`, which an implicit join or a qualified
/// aggregate without `key` matches on (LOOKUP-1, LOOKUP-5): each column, in
/// declared order, with the current row's same-named column. Nothing is
/// selected: every declared column must be one the relation stores and a
/// column of the current row, and the first that is not, or whose pair
/// cannot compare (LOOKUP-9), is reported. An input that declares no key
/// matches on no pair ([`Inference::NoApplicableKeys`]; D23, D63).
pub fn declared(
    catalog: &Catalog,
    names: &[&str],
    fields: &[Field<'_>],
) -> Result<Inference, Error> {
    if names
        .iter()
        .copied()
        .chain(fields.iter().map(|field| field.name))
        .any(str::is_empty)
    {
        return Err(Error::EmptyName);
    }
    let mut right = BTreeMap::new();
    for field in fields {
        if right.insert(field.name, field.column_type).is_some() {
            return Err(Error::DuplicateField);
        }
    }
    if names.is_empty() {
        return Ok(Inference::NoApplicableKeys);
    }
    for (key, name) in names.iter().enumerate() {
        let Some(&actual) = right.get(name) else {
            return Ok(Inference::UnknownColumn { key });
        };
        if catalog.output_type(name).is_none() {
            return Ok(Inference::UnknownValue { key });
        }
        if let Some(expected) = catalog.type_of(name) {
            if !ResultType::from(expected).comparable(actual.into()) {
                return Ok(Inference::Incompatible {
                    key,
                    expected,
                    actual,
                });
            }
        }
    }
    Ok(Inference::Keys {
        keys: (0..names.len()).collect(),
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
        let comparable =
            |left: ResultType, right: ResultType| left.comparable(right) && right.comparable(left);
        for (i, left) in kinds.iter().enumerate() {
            for (j, right) in kinds.iter().enumerate() {
                assert_eq!(comparable((*left).into(), (*right).into()), expected[i][j]);
            }
        }
        let boolean = ResultType::of(crate::value::ValueType::Bool);
        for kind in kinds {
            assert!(!comparable(boolean, kind.into()));
            assert!(!comparable(kind.into(), boolean));
        }
        assert!(comparable(boolean, boolean));
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

    /// A declared key pairs every column it names, in declared order, and
    /// selects nothing: a column the relation does not store, one the
    /// current row has no column of, and an incomparable pair each fail at
    /// the first such column; no key matches on no pair.
    #[test]
    fn a_declared_key_pairs_every_column_it_names() {
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
            Field {
                name: "E",
                column_type: Str,
            },
        ];
        assert_eq!(
            declared(&catalog, &["B", "A"], &fields),
            Ok(Inference::Keys {
                keys: std::vec![0, 1]
            })
        );
        assert_eq!(
            declared(&catalog, &["A", "D"], &fields),
            Ok(Inference::UnknownColumn { key: 1 })
        );
        assert_eq!(
            declared(&catalog, &["E"], &fields),
            Ok(Inference::UnknownValue { key: 0 })
        );
        let mismatched = [Field {
            name: "D",
            column_type: Str,
        }];
        assert_eq!(
            declared(&catalog, &["D"], &mismatched),
            Ok(Inference::Incompatible {
                key: 0,
                expected: Date,
                actual: Str
            })
        );
        assert_eq!(
            declared(&catalog, &[], &fields),
            Ok(Inference::NoApplicableKeys)
        );
    }

    #[test]
    fn invalid_names_and_duplicate_fields_keep_distinct_outcomes() {
        let catalog = catalog();
        let field = Field {
            name: "A",
            column_type: Int,
        };
        assert_eq!(
            declared(&catalog, &["A"], &[field, field]),
            Err(Error::DuplicateField)
        );
        assert_eq!(
            declared(
                &catalog,
                &["A"],
                &[Field {
                    name: "",
                    column_type: Str
                }]
            ),
            Err(Error::EmptyName)
        );
    }
}
