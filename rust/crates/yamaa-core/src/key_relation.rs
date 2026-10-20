//! Declared key type compatibility and ordered applicable-key inference, without records.
use std::collections::BTreeMap;

use crate::{
    reference_binding::{Catalog, Error, Field},
    result_type::ResultType,
    value::ColumnType,
};

/// One outcome; indices refer to the ordered key names submitted: the
/// output keys [`infer`] selects from, or the key [`declared`] pairs.
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
    /// A declared key column the relation does not store (LOOKUP-5).
    UnknownColumn {
        key: usize,
    },
    /// A declared key column the current row has no column of (LOOKUP-5).
    UnknownValue {
        key: usize,
    },
}

/// What a lookup that declares no `key` of its own matches a relation on: an
/// implicit join, or a qualified aggregate without `key` (LOOKUP-1,
/// LOOKUP-5).
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Implied<'a> {
    /// On the 0.5 surface, the applicable output keys, inferred from the
    /// fields the relation stores ([`infer`]).
    Applicable,
    /// A 1.0 input's declared `key` ([`declared`]), empty when it declares
    /// none (D23, D63).
    Declared(Vec<&'a str>),
}

impl<'a> Implied<'a> {
    /// The key names a lookup tries, in order: the output keys, of which
    /// inference applies those the relation stores, or the declared key, all
    /// of which apply.
    pub fn names<'k>(&'k self, output_keys: &'k [&'a str]) -> &'k [&'a str] {
        match self {
            Self::Applicable => output_keys,
            Self::Declared(names) => names,
        }
    }
    /// [`infer`] or [`declared`], as the surface decides.
    pub fn resolve(
        &self,
        catalog: &Catalog,
        output_keys: &[&str],
        fields: &[Field<'_>],
    ) -> Result<Inference, Error> {
        match self {
            Self::Applicable => infer(catalog, output_keys, fields),
            Self::Declared(names) => declared(catalog, names, fields),
        }
    }
}

/// Pair a 1.0 input's declared `key` (LOOKUP-1, LOOKUP-5): each column, in
/// declared order, with the current row's same-named column. Nothing is
/// selected: every declared column must be one the relation stores and a
/// column of the current row, and the first that is not, or whose pair
/// cannot compare (LOOKUP-9), is reported. An input that declares no key
/// matches on no pair ([`Inference::NoApplicableKeys`]).
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

/// Infer the applicable keys (LOOKUP-5) in output order and report the first key
/// pair whose types cannot compare (LOOKUP-9).
/// Right fields are normalized schema metadata (including SELF/derived fields), not values.
/// Validate every submitted name before semantic early exits; invalid output identity
/// is explicit so hosts can retain their earlier OUTPUT-2 declaration diagnostic.
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
                if !ResultType::from(expected).comparable(actual.into()) {
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
        // The same relation's applicable keys, on the 0.5 surface, are the
        // output keys it stores.
        let applicable = Implied::Applicable;
        assert_eq!(
            applicable.resolve(&catalog, &["D", "A", "B"], &fields),
            Ok(Inference::Keys {
                keys: std::vec![1, 2]
            })
        );
        assert_eq!(applicable.names(&["D", "A"]), ["D", "A"]);
        let declared_key = Implied::Declared(std::vec!["B"]);
        assert_eq!(declared_key.names(&["D", "A"]), ["B"]);
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
