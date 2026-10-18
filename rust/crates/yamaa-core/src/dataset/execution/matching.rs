//! One lookup model (LOOKUP-25), shared by implicit joins, named intermediates
//! and aggregates: match the records whose keys equal the current row's match
//! values and whose bounds hold its range value (LOOKUP-9), narrow the matched
//! records by the filter (LOOKUP-8), and select one of several candidates
//! (LOOKUP-10). Candidates keep stored record order. Each kind answers
//! absence (LOOKUP-24) and reduces its candidates on its own.
use super::*;
use crate::dataset::{RelationRange, RelationRangeFailure, RelationRangeTypes};
use std::cmp::Ordering;

/// The current row's side of one match, resolved before any record is read:
/// one value per key pair, the donor column's known type for each pair, and
/// the range value.
pub(super) struct Probe<'a> {
    pub keys: &'a [(usize, ValueRef<'a>)],
    pub kinds: &'a [Option<ValueType>],
    pub range: Option<(&'a RelationRange, ValueRef<'a>)>,
}

impl Probe<'_> {
    /// A missing match value or range value seeks no record (LOOKUP-9).
    pub(super) fn incomplete(&self) -> bool {
        self.keys
            .iter()
            .any(|(_, value)| matches!(value, ValueRef::Missing))
            || self
                .range
                .is_some_and(|(_, value)| matches!(value, ValueRef::Missing))
    }
}

/// Why a match stopped before its candidates were complete.
pub(super) enum Refusal<E> {
    Failed(Box<ExecutionError<E>>),
    /// Key pair `index` holds a record value whose type cannot compare with
    /// the current row's match value.
    Key {
        index: usize,
        current: Option<ValueType>,
        donor: Option<ValueType>,
    },
    /// A stated bound cannot compare with the range value.
    Range(RelationRangeTypes),
}

impl<E> From<Box<ExecutionError<E>>> for Refusal<E> {
    fn from(error: Box<ExecutionError<E>>) -> Self {
        Self::Failed(error)
    }
}

/// The candidates in the order the records were offered, and the first record
/// whose keys matched before the range and the filter were applied.
pub(super) struct Candidates {
    pub rows: Vec<usize>,
    pub first_key_match: Option<usize>,
}

/// Exact key equality under VALUE-19, with no operand converted. A missing
/// record value never matches; values whose types cannot compare refuse.
fn key_equal(donor: ValueRef<'_>, current: ValueRef<'_>) -> Result<bool, ()> {
    if let (ValueRef::Bool(left), ValueRef::Bool(right)) = (donor, current) {
        return Ok(left == right);
    }
    match crate::value::compare_present_refs(donor, current) {
        Ok(Ordering::Equal) => Ok(true),
        Err(crate::value::ComparisonError::IncompatibleTypes) => Err(()),
        Ok(_) | Err(crate::value::ComparisonError::MissingOperand) => Ok(false),
    }
}

/// Whether a key pair's types are known to compare before its record value is
/// read: such a pair needs no read once the record has failed to match.
fn known_comparable(current: ValueRef<'_>, donor: Option<ValueType>) -> bool {
    current
        .value_type()
        .zip(donor)
        .is_some_and(|(current, donor)| crate::value::comparable_types(current, donor))
}

/// Steps (a) and (b) for one current row. Every key pair is compared on each
/// offered record, so a pair whose values cannot compare refuses whatever the
/// other pairs hold; the range is compared on the records whose keys match,
/// and `narrow` is asked only about matched records.
pub(super) fn candidates<'t, E>(
    rows: impl IntoIterator<Item = usize>,
    capacity: usize,
    probe: &Probe<'_>,
    mut read: impl FnMut(usize, usize) -> Result<ValueRef<'t>, Box<ExecutionError<E>>>,
    mut narrow: impl FnMut(usize) -> Result<bool, Box<ExecutionError<E>>>,
) -> Result<Candidates, Refusal<E>> {
    let mut found = Candidates {
        rows: Vec::new(),
        first_key_match: None,
    };
    found
        .rows
        .try_reserve_exact(capacity)
        .map_err(|_| Box::new(ExecutionError::Allocation))?;
    for row in rows {
        let mut matches = true;
        for (index, &(column, current)) in probe.keys.iter().enumerate() {
            if !matches && known_comparable(current, probe.kinds.get(index).copied().flatten()) {
                continue;
            }
            let donor = read(row, column)?;
            match key_equal(donor, current) {
                Ok(true) => (),
                Ok(false) => matches = false,
                Err(()) => {
                    return Err(Refusal::Key {
                        index,
                        current: current.value_type(),
                        donor: donor.value_type(),
                    })
                }
            }
        }
        if !matches {
            continue;
        }
        found.first_key_match.get_or_insert(row);
        if let Some((range, value)) = probe.range {
            let allowed = range
                .allows(value, |column| read(row, column))
                .map_err(|failure| match failure {
                    RelationRangeFailure::Access(error) => Refusal::Failed(error),
                    RelationRangeFailure::Incomparable(types) => Refusal::Range(types),
                })?;
            if !allowed {
                continue;
            }
        }
        if narrow(row)? {
            found.rows.push(row);
        }
    }
    Ok(found)
}

/// Step (c): the first or last of several candidates by `order_by`, the
/// earlier stored record first or the later one last on a tie (LOOKUP-10).
pub(super) fn select<'t, E>(
    candidates: &[usize],
    selection: &SourceSelection,
    mut read: impl FnMut(usize, usize) -> Result<ValueRef<'t>, Box<ExecutionError<E>>>,
) -> Result<usize, Box<ExecutionError<E>>> {
    let mut best = candidates[0];
    for &row in &candidates[1..] {
        let mut order = Ordering::Equal;
        for term in &selection.order_by {
            order = windows::compare(read(row, term.column)?, read(best, term.column)?, term);
            if order != Ordering::Equal {
                break;
            }
        }
        order = order.then_with(|| row.cmp(&best));
        if (selection.keep == Keep::First && order == Ordering::Less)
            || (selection.keep == Keep::Last && order == Ordering::Greater)
        {
            best = row;
        }
    }
    Ok(best)
}

/// The key pair's authored source in a key-type finding: the match variable,
/// or `key[FIELD]` for a match value written as an expression.
pub(super) fn key_source(key: &crate::dataset::IntermediateKey, field: &str) -> String {
    if key.value.is_expression() {
        std::format!("key[{field}]")
    } else {
        key.value
            .read()
            .map_or_else(|| field.into(), |read| read.identifier().into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dataset::{CurrentField, CurrentRead, OrderTerm, RelationRangeBound};

    type Error = Box<ExecutionError<std::convert::Infallible>>;

    fn read<'t>(
        table: &'t [Vec<Value>],
    ) -> impl FnMut(usize, usize) -> Result<ValueRef<'t>, Error> {
        move |row, column| Ok(ValueRef::from(&table[row][column]))
    }

    #[test]
    fn candidates_match_keys_then_range_then_filter_in_stored_order() {
        let table = vec![
            vec![Value::Int(1), Value::Int(1), Value::Int(9)],
            vec![Value::Int(2), Value::Int(1), Value::Int(9)],
            vec![Value::Int(1), Value::Int(5), Value::Int(9)],
            vec![Value::Int(1), Value::Int(2), Value::Int(3)],
            vec![Value::Int(1), Value::Missing, Value::Int(9)],
            vec![Value::Int(1), Value::Int(0), Value::Int(9)],
        ];
        let range = RelationRange::new(
            "between".into(),
            CurrentField {
                read: CurrentRead::Output(0),
                identifier: "DAY".into(),
            },
            Some(ValueType::Int),
            Some(RelationRangeBound {
                column: 1,
                identifier: "LO".into(),
                kind: Some(ValueType::Int),
            }),
            Some(RelationRangeBound {
                column: 2,
                identifier: "HI".into(),
                kind: Some(ValueType::Int),
            }),
        )
        .unwrap();
        let one = Value::float(1.0);
        let keys = [(0, ValueRef::from(&one))];
        let probe = Probe {
            keys: &keys,
            kinds: &[Some(ValueType::Int)],
            range: Some((&range, ValueRef::Int(4))),
        };
        let mut asked = Vec::new();
        let Ok(found) = candidates(0..table.len(), table.len(), &probe, read(&table), |row| {
            asked.push(row);
            Ok(row != 0)
        }) else {
            panic!("comparable keys and bounds");
        };
        // Record 1 fails its key, 2 and 3 their range and 4 a missing bound;
        // the filter is asked about the matched records 0 and 5 only.
        assert_eq!(asked, [0, 5]);
        assert_eq!(found.rows, [5]);
        assert_eq!(found.first_key_match, Some(0));
    }

    #[test]
    fn an_incomparable_key_value_refuses_even_after_another_pair_differs() {
        let table = vec![vec![Value::Int(2), Value::Str("A".into())]];
        let keys = [(0, ValueRef::Int(1)), (1, ValueRef::Int(7))];
        let probe = Probe {
            keys: &keys,
            kinds: &[Some(ValueType::Int), None],
            range: None,
        };
        let Err(Refusal::Key {
            index,
            current,
            donor,
        }) = candidates(0..1, 1, &probe, read(&table), |_| Ok(true))
        else {
            panic!("a text record value against an int match value refuses");
        };
        assert_eq!(
            (index, current, donor),
            (1, Some(ValueType::Int), Some(ValueType::Str))
        );
        // A pair known to compare is not read once the record has differed.
        let probe = Probe {
            keys: &keys,
            kinds: &[Some(ValueType::Int), Some(ValueType::Int)],
            range: None,
        };
        let mut reads = 0;
        let found: Result<_, Refusal<std::convert::Infallible>> = candidates(
            0..1,
            1,
            &probe,
            |row, column| {
                reads += 1;
                Ok(ValueRef::from(&table[row][column]))
            },
            |_| Ok(true),
        );
        assert!(matches!(found, Ok(Candidates { ref rows, .. }) if rows.is_empty()));
        assert_eq!(reads, 1);
    }

    #[test]
    fn booleans_match_by_equality_and_missing_values_seek_nothing() {
        let table = vec![vec![Value::Bool(true)], vec![Value::Bool(false)]];
        let keys = [(0, ValueRef::Bool(false))];
        let probe = Probe {
            keys: &keys,
            kinds: &[None],
            range: None,
        };
        let Ok(found) = candidates(0..2, 2, &probe, read(&table), |_| Ok(true)) else {
            panic!("booleans compare by equality");
        };
        assert_eq!(found.rows, [1]);
        let missing = [(0, ValueRef::Missing)];
        assert!(Probe {
            keys: &missing,
            kinds: &[None],
            range: None,
        }
        .incomplete());
    }

    #[test]
    fn selection_orders_by_terms_and_breaks_ties_by_stored_order() {
        let table = vec![
            vec![Value::Int(2)],
            vec![Value::Int(1)],
            vec![Value::Int(2)],
            vec![Value::Missing],
        ];
        let order = |descending| SourceSelection {
            order_by: vec![OrderTerm {
                column: 0,
                descending,
                nulls_first: true,
            }],
            keep: Keep::Last,
        };
        assert_eq!(
            select(&[0, 1, 2, 3], &order(false), read(&table)).ok(),
            Some(2)
        );
        let first = SourceSelection {
            keep: Keep::First,
            ..order(false)
        };
        assert_eq!(select(&[0, 1, 2, 3], &first, read(&table)).ok(), Some(3));
        assert_eq!(select(&[0, 2], &order(true), read(&table)).ok(), Some(2));
    }
}
