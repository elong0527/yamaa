//! A key bucket's records ordered by a `between` bound (#4888), so a row
//! finds the records whose bounds hold its range value by a search instead
//! of comparing every record of the bucket (LOOKUP-9).
//!
//! The order is built once per bucket and range columns, the first time a
//! row probes the bucket. It holds the records whose stated bounds are all
//! present, the only ones a range can keep. It answers a row only when no
//! record can refuse the comparison: every stated bound holds one type, and
//! the range value's type compares with each. That check is one per bucket;
//! any other row, and any bucket that mixes types, is left to the
//! per-record comparison, which reports the refusal where it meets it.
use super::eval::owned;
use super::Fail;
use crate::dataset::{RelationRange, RelationRangeTypes};
use crate::table::ValueRef;
use crate::value::{comparable_types, compare_present_refs, Value, ValueType};
use std::cmp::Ordering;

/// Buckets smaller than this are compared record by record: an order would
/// cost more to build than it saves.
pub(super) const ORDERED: usize = 16;

/// One bucket's records with every stated bound present, by their first
/// bound: the lower bound when one is stated, otherwise the upper.
pub(super) struct Bounded {
    records: Vec<Bounds>,
    /// Per position, the greatest upper bound at or before it, when both
    /// bounds are stated.
    reach: Vec<Value>,
    /// The type every lower and every upper bound holds; none when no
    /// record holds that bound.
    lower: Option<ValueType>,
    upper: Option<ValueType>,
}

/// One record's bounds: the first it is ordered by, then the upper bound
/// when both are stated.
struct Bounds {
    row: usize,
    first: Value,
    second: Option<Value>,
}

impl Bounded {
    /// Order `rows` by their bounds, reading each record's lower bound, then
    /// its upper bound unless the lower one is missing, as the per-record
    /// comparison reads them. None when a bound holds values of types that
    /// do not compare.
    pub(super) fn build<'t, E>(
        rows: &[usize],
        range: &RelationRange,
        mut read: impl FnMut(usize, usize) -> Result<ValueRef<'t>, Fail<E>>,
    ) -> Result<Option<Self>, Fail<E>> {
        let mut bounded = Self {
            records: Vec::new(),
            reach: Vec::new(),
            lower: None,
            upper: None,
        };
        super::reserved(bounded.records.try_reserve_exact(rows.len()))?;
        for &row in rows {
            let mut values = [None, None];
            for (slot, (bound, kind)) in [
                (range.lower(), &mut bounded.lower),
                (range.upper(), &mut bounded.upper),
            ]
            .into_iter()
            .enumerate()
            {
                let Some(bound) = bound else {
                    continue;
                };
                let value = read(row, bound.column)?;
                let Some(actual) = value.value_type() else {
                    break;
                };
                match *kind {
                    Some(held) if !comparable_types(held, actual) => return Ok(None),
                    Some(_) => (),
                    None => *kind = Some(actual),
                }
                values[slot] = Some(owned(value));
            }
            let present = |slot: usize, stated: bool| values[slot].is_some() || !stated;
            if !(present(0, range.lower().is_some()) && present(1, range.upper().is_some())) {
                continue;
            }
            let [lower, upper] = values;
            let (first, second) = match lower {
                Some(lower) => (lower, upper),
                None => (upper.expect("a stated bound"), None),
            };
            bounded.records.push(Bounds { row, first, second });
        }
        // A stable sort: records with equal first bounds keep stored order.
        bounded.records.sort_by(|left, right| {
            compare_present_refs((&left.first).into(), (&right.first).into())
                .unwrap_or(Ordering::Equal)
        });
        if range.lower().is_some() && range.upper().is_some() {
            super::reserved(bounded.reach.try_reserve_exact(bounded.records.len()))?;
            for record in &bounded.records {
                let upper = record.second.as_ref().expect("both bounds stated");
                let greatest = match bounded.reach.last() {
                    Some(reach)
                        if compare_present_refs(reach.into(), upper.into())
                            == Ok(Ordering::Greater) =>
                    {
                        reach.clone()
                    }
                    _ => upper.clone(),
                };
                bounded.reach.push(greatest);
            }
        }
        Ok(Some(bounded))
    }

    /// The records whose bounds hold `value`, in stored order; none when a
    /// record could refuse to compare with it, which the per-record
    /// comparison then reports.
    pub(super) fn within(&self, range: &RelationRange, value: ValueRef<'_>) -> Option<Vec<usize>> {
        let actual = value.value_type()?;
        if RelationRangeTypes::incomparable(actual, self.lower, self.upper).is_some() {
            return None;
        }
        // The types compare, so no comparison below can fail.
        let lower = |bound: &Value| range.lower_admits(bound.into(), value).unwrap_or(false);
        let upper = |bound: &Value| range.upper_admits(value, bound.into()).unwrap_or(false);
        let mut rows = Vec::new();
        if range.lower().is_some() {
            // Lower bounds ascend, so those that admit the value lead.
            let admitted = self.records.partition_point(|record| lower(&record.first));
            if range.upper().is_some() {
                // Walk back until no earlier upper bound reaches the value.
                for position in (0..admitted).rev() {
                    if !upper(&self.reach[position]) {
                        break;
                    }
                    let record = &self.records[position];
                    if upper(record.second.as_ref().expect("both bounds stated")) {
                        rows.push(record.row);
                    }
                }
            } else {
                rows.extend(self.records[..admitted].iter().map(|record| record.row));
            }
        } else {
            // Upper bounds ascend, so those that admit the value trail.
            let refused = self.records.partition_point(|record| !upper(&record.first));
            rows.extend(self.records[refused..].iter().map(|record| record.row));
        }
        rows.sort_unstable();
        Some(rows)
    }
}

#[cfg(test)]
mod tests {
    // The seam: a bucket's order must keep exactly the records, in stored
    // order, that LOOKUP-9's per-record comparison keeps. No benchmark holds
    // a key bucket large enough for the order to answer it.
    use super::*;
    use crate::dataset::{CurrentField, CurrentRead, RelationRangeBound};
    use crate::expression_table::Closed;
    use std::convert::Infallible;

    fn bound(column: usize) -> RelationRangeBound {
        RelationRangeBound {
            column,
            identifier: format!("B{column}"),
            kind: Some(ValueType::Int),
        }
    }

    fn range(lower: bool, upper: bool, closed: Closed) -> RelationRange {
        let current = CurrentField {
            read: CurrentRead::Output(0),
            identifier: "ADY".into(),
        };
        RelationRange::new(
            "between".into(),
            current,
            Some(ValueType::Int),
            lower.then(|| bound(0)),
            upper.then(|| bound(1)),
            closed,
        )
        .unwrap()
    }

    /// The records' lower and upper bounds, column 0 and column 1.
    fn order(records: &[[Value; 2]], range: &RelationRange) -> Option<Bounded> {
        let rows: Vec<usize> = (0..records.len()).collect();
        Bounded::build(&rows, range, |row, column| {
            Ok::<_, Fail<Infallible>>(ValueRef::from(&records[row][column]))
        })
        .unwrap()
    }

    /// Windows `[r, r + 5]` for r = 19 down to 0 in stored order, then one
    /// window `[0, 100]` and one missing its upper bound.
    fn windows() -> Vec<[Value; 2]> {
        let mut records: Vec<[Value; 2]> = (0..20)
            .rev()
            .map(|r| [Value::Int(r), Value::Int(r + 5)])
            .collect();
        records.push([Value::Int(0), Value::Int(100)]);
        records.push([Value::Int(7), Value::Missing]);
        records
    }

    #[test]
    fn the_order_keeps_the_records_whose_bounds_hold_the_value_in_stored_order() {
        // Row i holds the window starting at 19 - i; row 20 spans [0, 100];
        // row 21 misses its upper bound and holds for no value.
        let cases = [
            (Closed::Both, 7, vec![12, 13, 14, 15, 16, 17, 20]),
            (Closed::Left, 7, vec![12, 13, 14, 15, 16, 20]),
            (Closed::Right, 7, vec![13, 14, 15, 16, 17, 20]),
            (Closed::Neither, 7, vec![13, 14, 15, 16, 20]),
            (Closed::Both, 50, vec![20]),
            (Closed::Both, 101, vec![]),
            (Closed::Both, -1, vec![]),
        ];
        let records = windows();
        for (closed, value, expected) in cases {
            let range = range(true, true, closed);
            let found = order(&records, &range)
                .unwrap()
                .within(&range, ValueRef::Int(value));
            assert_eq!(found, Some(expected), "{closed:?} at {value}");
        }
    }

    #[test]
    fn a_single_bound_keeps_every_record_on_its_side() {
        let records: Vec<[Value; 2]> = (0..20)
            .rev()
            .map(|r| [Value::Int(r), Value::Int(r)])
            .collect();
        // Row i holds 19 - i in both columns.
        let cases = [
            (true, false, Closed::Both, (15..20).collect::<Vec<usize>>()),
            (true, false, Closed::Neither, (16..20).collect()),
            (false, true, Closed::Both, (0..16).collect()),
            (false, true, Closed::Neither, (0..15).collect()),
        ];
        for (lower, upper, closed, expected) in cases {
            let range = range(lower, upper, closed);
            let found = order(&records, &range)
                .unwrap()
                .within(&range, ValueRef::Int(4));
            assert_eq!(
                found,
                Some(expected),
                "lower {lower}, upper {upper}, {closed:?}"
            );
        }
    }

    #[test]
    fn a_comparison_that_could_fail_is_left_to_the_records() {
        let range = range(true, true, Closed::Both);
        // A value whose type no bound compares with.
        let found = order(&windows(), &range)
            .unwrap()
            .within(&range, ValueRef::Str("7"));
        assert_eq!(found, None, "a text value against integer bounds");
        // A bound column that mixes types builds no order.
        let mut mixed = windows();
        mixed[3][0] = Value::Str("16".into());
        assert!(
            order(&mixed, &range).is_none(),
            "a lower bound mixing types"
        );
    }
}
