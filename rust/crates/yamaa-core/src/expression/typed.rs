//! The column form's typed loops (#4218). An operator whose operands are
//! literals, computed numbers or columns that lend their typed storage
//! ([`Column::lend`](super::Column::lend)) runs one loop specialised to its
//! operands' layouts, matched once per selection, over the payloads and
//! validity bits, instead of matching each cell's `ValueRef`, and each pair
//! of cells, again.
//!
//! The loops hold no judgement of their own. A pair of present values is
//! ordered by the ordering the row evaluator's comparison uses
//! ([`compare_numeric`], text in byte order, dates and datetimes in time
//! order), the ordering becomes a truth through [`holds`], a missing operand
//! is `UNKNOWN`, and a `LIKE` pattern matches through [`LikePattern`]. A
//! pair the loops do not cover answers None, and the caller keeps the
//! per-cell loop: every pair the row evaluator refuses (text against a
//! number, a date against a datetime), a column of booleans or of mixed
//! values, the missing literal, and a `LIKE` pattern read from a column.

use std::cmp::Ordering;
use std::collections::TryReserveError;

use super::columns::Lent;
use super::evaluate::{holds, LikePattern, Truth};
use super::syntax::Comparison;
use crate::numeric::{float_operation, zero_divisor, BinaryOperator, Number};
use crate::portable_regex::Pattern as Regex;
use crate::table::{ColumnValues, ValueRef};
use crate::temporal::{Date, DateTime};
use crate::value::{compare_numeric, FiniteFloat, Numeric};

/// Where each selected row's value lies in a lent view's payloads.
#[derive(Clone, Copy)]
pub(super) struct At<'c> {
    base: usize,
    rows: Option<&'c [usize]>,
    validity: Option<&'c [u64]>,
}

impl At<'_> {
    /// The payload index of selected row `row`, or None when the bitmap
    /// marks it missing.
    #[inline(always)]
    fn index(&self, row: usize) -> Option<usize> {
        let index = self.base + self.rows.map_or(row, |rows| rows[row]);
        self.validity
            .is_none_or(|words| words[index / 64] >> (index % 64) & 1 == 1)
            .then_some(index)
    }
}

/// One operand's values over the selection, in one layout.
trait Cells: Copy {
    type Item: Copy;
    /// Selected row `row`'s value, or None when it is missing.
    fn cell(&self, row: usize) -> Option<Self::Item>;
}

/// A layout's payloads: the value at a payload index.
trait Payload: Copy {
    type Item: Copy;
    fn read(&self, index: usize) -> Option<Self::Item>;
}

/// A lent view's payloads, read where [`At`] places each selected row.
#[derive(Clone, Copy)]
pub(super) struct Stored<'c, P> {
    at: At<'c>,
    payload: P,
}

impl<'c, P> Stored<'c, P> {
    /// The same rows, read through another view of the payloads.
    fn map<Q>(self, view: impl FnOnce(P) -> Q) -> Stored<'c, Q> {
        Stored {
            at: self.at,
            payload: view(self.payload),
        }
    }
}

impl<P: Payload> Cells for Stored<'_, P> {
    type Item = P::Item;
    #[inline(always)]
    fn cell(&self, row: usize) -> Option<P::Item> {
        self.at
            .index(row)
            .and_then(|index| self.payload.read(index))
    }
}

/// A literal's one value on every row.
#[derive(Clone, Copy)]
pub(super) struct Constant<T>(T);

impl<T: Copy> Cells for Constant<T> {
    type Item = T;
    #[inline(always)]
    fn cell(&self, _: usize) -> Option<T> {
        Some(self.0)
    }
}

/// Arithmetic's numbers, one per selected row.
#[derive(Clone, Copy)]
pub(super) struct Computed<'c>(&'c [Number]);

impl Cells for Computed<'_> {
    type Item = Numeric;
    #[inline(always)]
    fn cell(&self, row: usize) -> Option<Numeric> {
        match self.0[row] {
            Number::Missing => None,
            Number::Int(value) => Some(Numeric::Int(value)),
            Number::Float(value) => Some(Numeric::Float(value.get())),
        }
    }
}

#[derive(Clone, Copy)]
pub(super) struct Ints<'c>(&'c [i64]);

impl Payload for Ints<'_> {
    type Item = Numeric;
    #[inline(always)]
    fn read(&self, index: usize) -> Option<Numeric> {
        Some(Numeric::Int(self.0[index]))
    }
}

#[derive(Clone, Copy)]
pub(super) struct Finite<'c>(&'c [FiniteFloat]);

impl Payload for Finite<'_> {
    type Item = Numeric;
    #[inline(always)]
    fn read(&self, index: usize) -> Option<Numeric> {
        Some(Numeric::Float(self.0[index].get()))
    }
}

/// Binary64 payloads, a nonfinite one reading missing as VALUE-4 admits it,
/// as [`ColumnRef::get`](crate::table::ColumnRef::get) reads it.
#[derive(Clone, Copy)]
pub(super) struct Raw<'c>(&'c [f64]);

impl Payload for Raw<'_> {
    type Item = Numeric;
    #[inline(always)]
    fn read(&self, index: usize) -> Option<Numeric> {
        FiniteFloat::new(self.0[index]).map(|value| Numeric::Float(value.get()))
    }
}

#[derive(Clone, Copy)]
pub(super) struct Spans<'c> {
    spans: &'c [(usize, usize)],
    bytes: &'c str,
}

impl<'c> Payload for Spans<'c> {
    type Item = &'c str;
    #[inline(always)]
    fn read(&self, index: usize) -> Option<&'c str> {
        let (start, end) = self.spans[index];
        Some(&self.bytes[start..end])
    }
}

#[derive(Clone, Copy)]
pub(super) struct Offsets<'c> {
    offsets: &'c [usize],
    bytes: &'c str,
}

impl<'c> Payload for Offsets<'c> {
    type Item = &'c str;
    #[inline(always)]
    fn read(&self, index: usize) -> Option<&'c str> {
        Some(&self.bytes[self.offsets[index]..self.offsets[index + 1]])
    }
}

/// Text read as its bytes, for comparison: text orders and equals exactly as
/// its bytes do, and a byte slice needs no character-boundary check.
#[derive(Clone, Copy)]
pub(super) struct SpanBytes<'c>(Spans<'c>);

impl<'c> Payload for SpanBytes<'c> {
    type Item = &'c [u8];
    #[inline(always)]
    fn read(&self, index: usize) -> Option<&'c [u8]> {
        let (start, end) = self.0.spans[index];
        Some(&self.0.bytes.as_bytes()[start..end])
    }
}

#[derive(Clone, Copy)]
pub(super) struct OffsetBytes<'c>(Offsets<'c>);

impl<'c> Payload for OffsetBytes<'c> {
    type Item = &'c [u8];
    #[inline(always)]
    fn read(&self, index: usize) -> Option<&'c [u8]> {
        let Offsets { offsets, bytes } = self.0;
        Some(&bytes.as_bytes()[offsets[index]..offsets[index + 1]])
    }
}

/// Payloads present where the bitmap says.
#[derive(Clone, Copy)]
pub(super) struct Masked<'c, T>(&'c [T]);

impl<T: Copy> Payload for Masked<'_, T> {
    type Item = T;
    #[inline(always)]
    fn read(&self, index: usize) -> Option<T> {
        Some(self.0[index])
    }
}

/// Payloads that carry their own presence.
#[derive(Clone, Copy)]
pub(super) struct Optional<'c, T>(&'c [Option<T>]);

impl<T: Copy> Payload for Optional<'_, T> {
    type Item = T;
    #[inline(always)]
    fn read(&self, index: usize) -> Option<T> {
        self.0[index]
    }
}

/// A numeric operand's layout.
#[derive(Clone, Copy)]
pub(super) enum NumberLayout<'c> {
    Ints(Stored<'c, Ints<'c>>),
    Finite(Stored<'c, Finite<'c>>),
    Raw(Stored<'c, Raw<'c>>),
    Computed(Computed<'c>),
    Constant(Constant<Numeric>),
}

/// A text operand's layout.
#[derive(Clone, Copy)]
pub(super) enum TextLayout<'c> {
    Spans(Stored<'c, Spans<'c>>),
    Offsets(Stored<'c, Offsets<'c>>),
    Constant(Constant<&'c str>),
}

/// A date or datetime operand's layout.
#[derive(Clone, Copy)]
pub(super) enum TemporalLayout<'c, T> {
    Masked(Stored<'c, Masked<'c, T>>),
    Optional(Stored<'c, Optional<'c, T>>),
    Constant(Constant<T>),
}

/// One operand's values in a layout the typed loops cover.
#[derive(Clone, Copy)]
pub(super) enum Typed<'c> {
    Numbers(NumberLayout<'c>),
    Text(TextLayout<'c>),
    Dates(TemporalLayout<'c, Date>),
    DateTimes(TemporalLayout<'c, DateTime>),
}

impl<'c> Typed<'c> {
    /// A literal's layout; None for the missing literal and a boolean.
    pub(super) fn literal(value: ValueRef<'c>) -> Option<Self> {
        Some(match value {
            ValueRef::Int(value) => {
                Self::Numbers(NumberLayout::Constant(Constant(Numeric::Int(value))))
            }
            ValueRef::Float(value) => Self::Numbers(NumberLayout::Constant(Constant(
                Numeric::Float(value.get()),
            ))),
            ValueRef::Str(value) => Self::Text(TextLayout::Constant(Constant(value))),
            ValueRef::Date(value) => Self::Dates(TemporalLayout::Constant(Constant(value))),
            ValueRef::DateTime(value) => Self::DateTimes(TemporalLayout::Constant(Constant(value))),
            ValueRef::Missing | ValueRef::Bool(_) => return None,
        })
    }

    /// The layout of numbers computed for each selected row.
    pub(super) fn computed(numbers: &'c [Number]) -> Self {
        Self::Numbers(NumberLayout::Computed(Computed(numbers)))
    }

    /// The layout of a column's lent storage.
    pub(super) fn lent(lent: Lent<'c>) -> Self {
        let view = lent.view;
        let at = At {
            base: view.start() + lent.offset,
            rows: lent.rows,
            validity: view.validity(),
        };
        match view.values() {
            ColumnValues::Int(values) => Self::Numbers(NumberLayout::Ints(Stored {
                at,
                payload: Ints(values),
            })),
            ColumnValues::FiniteFloat(values) => Self::Numbers(NumberLayout::Finite(Stored {
                at,
                payload: Finite(values),
            })),
            ColumnValues::Float(values) => Self::Numbers(NumberLayout::Raw(Stored {
                at,
                payload: Raw(values),
            })),
            ColumnValues::TextSpans { spans, bytes } => Self::Text(TextLayout::Spans(Stored {
                at,
                payload: Spans { spans, bytes },
            })),
            ColumnValues::Str { offsets, bytes } => Self::Text(TextLayout::Offsets(Stored {
                at,
                payload: Offsets { offsets, bytes },
            })),
            ColumnValues::MaskedDate(values) => Self::Dates(TemporalLayout::Masked(Stored {
                at,
                payload: Masked(values),
            })),
            ColumnValues::Date(values) => Self::Dates(TemporalLayout::Optional(Stored {
                at,
                payload: Optional(values),
            })),
            ColumnValues::MaskedDateTime(values) => {
                Self::DateTimes(TemporalLayout::Masked(Stored {
                    at,
                    payload: Masked(values),
                }))
            }
            ColumnValues::DateTime(values) => Self::DateTimes(TemporalLayout::Optional(Stored {
                at,
                payload: Optional(values),
            })),
        }
    }
}

/// Finite binary64 numbers over the selection, row `row` present where bit
/// `row` of `valid` is set: a float column read as numbers, and arithmetic
/// over it, missing operands found by word (#4218).
pub(super) struct Floats {
    values: Vec<f64>,
    valid: Vec<u64>,
}

/// The other operand of a binary operator over [`Floats`].
#[derive(Clone, Copy)]
pub(super) enum Other<'f> {
    Floats(&'f Floats),
    Number(f64),
}

impl Floats {
    /// A float operand's values; None for any other layout.
    pub(super) fn read(rows: usize, value: Typed<'_>) -> Option<Result<Self, TryReserveError>> {
        match value {
            Typed::Numbers(NumberLayout::Finite(cells)) => Some(Self::gather(rows, cells)),
            Typed::Numbers(NumberLayout::Raw(cells)) => Some(Self::gather(rows, cells)),
            _ => None,
        }
    }

    fn gather<C: Cells<Item = Numeric>>(rows: usize, cells: C) -> Result<Self, TryReserveError> {
        let mut values = Vec::new();
        values.try_reserve_exact(rows)?;
        let mut valid = Vec::new();
        valid.try_reserve_exact(rows.div_ceil(64))?;
        let mut word = 0;
        for row in 0..rows {
            values.push(match cells.cell(row) {
                Some(Numeric::Float(value)) => {
                    word |= 1 << (row % 64);
                    value
                }
                Some(Numeric::Int(_)) => unreachable!("a float layout holds floats"),
                None => 0.0,
            });
            if row % 64 == 63 || row + 1 == rows {
                valid.push(word);
                word = 0;
            }
        }
        Ok(Self { values, valid })
    }

    /// Row `row`'s number.
    #[inline(always)]
    pub(super) fn number(&self, row: usize) -> Number {
        if self.valid[row / 64] >> (row % 64) & 1 == 1 {
            Number::float(self.values[row])
        } else {
            Number::Missing
        }
    }

    /// One number per row.
    pub(super) fn numbers(&self) -> Result<Vec<Number>, TryReserveError> {
        collect(self.values.len(), |row| self.number(row))
    }

    /// The layout the typed loops read the numbers in.
    pub(super) fn typed(&self) -> Typed<'_> {
        Typed::Numbers(NumberLayout::Raw(Stored {
            at: At {
                base: 0,
                rows: None,
                validity: Some(&self.valid),
            },
            payload: Raw(&self.values),
        }))
    }

    /// `operator` over each row's pair, these numbers the left operand, or
    /// the right one when `swapped`, written over them: a row is present
    /// where both operands are and the result is finite, as
    /// `numeric::binary` decides through the same `zero_divisor` and
    /// `float_operation`. None when a row whose operands are both present
    /// has a divisor the operator refuses, so that the row evaluator
    /// reports it.
    pub(super) fn binary(
        mut self,
        operator: BinaryOperator,
        other: Other<'_>,
        swapped: bool,
    ) -> Option<Self> {
        let refused = match other {
            Other::Floats(other) => self.apply(
                operator,
                swapped,
                |row| other.values[row],
                |word| other.valid[word],
            ),
            Other::Number(number) => self.apply(operator, swapped, |_| number, |_| u64::MAX),
        };
        (!refused).then_some(self)
    }

    /// The loop of [`Floats::binary`], `other(row)` and `other_valid(word)`
    /// reading the other operand; true when a divisor is refused.
    #[inline(always)]
    fn apply(
        &mut self,
        operator: BinaryOperator,
        swapped: bool,
        other: impl Fn(usize) -> f64,
        other_valid: impl Fn(usize) -> u64,
    ) -> bool {
        let Self { values, valid } = self;
        for (index, word) in valid.iter_mut().enumerate() {
            let both = *word & other_valid(index);
            let start = index * 64;
            let mut finite = 0;
            for (bit, value) in values[start..].iter_mut().take(64).enumerate() {
                let (a, b) = if swapped {
                    (other(start + bit), *value)
                } else {
                    (*value, other(start + bit))
                };
                if both >> bit & 1 == 1 && zero_divisor(operator, b) {
                    return true;
                }
                *value = float_operation(operator, a, b);
                finite |= u64::from(value.is_finite()) << bit;
            }
            *word = both & finite;
        }
        false
    }
}

/// `$body` with `$cells` bound to a numeric layout's cells, one copy of
/// `$body` per layout.
macro_rules! numbers {
    ($layout:expr, $cells:ident => $body:expr) => {
        match $layout {
            NumberLayout::Ints($cells) => $body,
            NumberLayout::Finite($cells) => $body,
            NumberLayout::Raw($cells) => $body,
            NumberLayout::Computed($cells) => $body,
            NumberLayout::Constant($cells) => $body,
        }
    };
}

/// `$body` with `$cells` bound to a text layout's cells.
macro_rules! text {
    ($layout:expr, $cells:ident => $body:expr) => {
        match $layout {
            TextLayout::Spans($cells) => $body,
            TextLayout::Offsets($cells) => $body,
            TextLayout::Constant($cells) => $body,
        }
    };
}

/// `$body` with `$cells` bound to a text layout's cells read as bytes.
macro_rules! text_bytes {
    ($layout:expr, $cells:ident => $body:expr) => {
        match $layout {
            TextLayout::Spans(stored) => {
                let $cells = stored.map(SpanBytes);
                $body
            }
            TextLayout::Offsets(stored) => {
                let $cells = stored.map(OffsetBytes);
                $body
            }
            TextLayout::Constant(Constant(text)) => {
                let $cells = Constant(text.as_bytes());
                $body
            }
        }
    };
}

/// `$body` with `$cells` bound to a date or datetime layout's cells.
macro_rules! temporal {
    ($layout:expr, $cells:ident => $body:expr) => {
        match $layout {
            TemporalLayout::Masked($cells) => $body,
            TemporalLayout::Optional($cells) => $body,
            TemporalLayout::Constant($cells) => $body,
        }
    };
}

/// `$body` with `$cells` bound to any layout's cells.
macro_rules! typed {
    ($typed:expr, $cells:ident => $body:expr) => {
        match $typed {
            Typed::Numbers(layout) => numbers!(layout, $cells => $body),
            Typed::Text(layout) => text!(layout, $cells => $body),
            Typed::Dates(layout) => temporal!(layout, $cells => $body),
            Typed::DateTimes(layout) => temporal!(layout, $cells => $body),
        }
    };
}

/// Whether two texts hold the same bytes, compared in line: most texts a
/// condition compares are a few bytes long.
#[inline(always)]
fn same_bytes(left: &[u8], right: &[u8]) -> bool {
    left.len() == right.len() && left.iter().zip(right).all(|(left, right)| left == right)
}

/// One result per selected row, reserved for the selection before it is
/// filled.
#[inline]
fn collect<T>(rows: usize, row: impl FnMut(usize) -> T) -> Result<Vec<T>, TryReserveError> {
    let mut results = Vec::new();
    results.try_reserve_exact(rows)?;
    results.extend((0..rows).map(row));
    Ok(results)
}

/// The truth of each selected row's pair of values, `UNKNOWN` when either
/// is missing.
#[inline]
fn pairwise<A: Cells, B: Cells>(
    rows: usize,
    left: A,
    right: B,
    truth: impl Fn(A::Item, B::Item) -> bool,
) -> Result<Vec<Truth>, TryReserveError> {
    collect(rows, move |row| match (left.cell(row), right.cell(row)) {
        (Some(left), Some(right)) => Truth::from_bool(truth(left, right)),
        _ => Truth::Unknown,
    })
}

/// The truth of each selected row's value, `UNKNOWN` when it is missing.
#[inline]
fn each<A: Cells>(
    rows: usize,
    cells: A,
    mut truth: impl FnMut(A::Item) -> bool,
) -> Result<Vec<Truth>, TryReserveError> {
    collect(rows, move |row| {
        cells
            .cell(row)
            .map_or(Truth::Unknown, |value| Truth::from_bool(truth(value)))
    })
}

/// A comparison of two operands, or None for a pair of kinds the row
/// evaluator refuses.
pub(super) fn compare(
    rows: usize,
    operator: Comparison,
    left: Typed<'_>,
    right: Typed<'_>,
) -> Option<Result<Vec<Truth>, TryReserveError>> {
    Some(match (left, right) {
        (Typed::Numbers(left), Typed::Numbers(right)) => numbers!(left, left => {
            numbers!(right, right => pairwise(rows, left, right, move |a, b| {
                holds(operator, compare_numeric(a, b))
            }))
        }),
        // Two texts are equal exactly when their byte order says so; the
        // lengths settle most unequal pairs before any byte is read.
        (Typed::Text(left), Typed::Text(right)) => match operator {
            Comparison::Equal | Comparison::NotEqual => text_bytes!(left, left => {
                text_bytes!(right, right => pairwise(rows, left, right, move |a: &[u8], b: &[u8]| {
                    same_bytes(a, b) == (operator == Comparison::Equal)
                }))
            }),
            _ => text_bytes!(left, left => {
                text_bytes!(right, right => pairwise(rows, left, right, move |a: &[u8], b: &[u8]| {
                    holds(operator, a.cmp(b))
                }))
            }),
        },
        (Typed::Dates(left), Typed::Dates(right)) => temporal!(left, left => {
            temporal!(right, right => pairwise(rows, left, right, move |a: Date, b: Date| {
                holds(operator, a.cmp(&b))
            }))
        }),
        (Typed::DateTimes(left), Typed::DateTimes(right)) => temporal!(left, left => {
            temporal!(right, right => pairwise(rows, left, right, move |a: DateTime, b: DateTime| {
                holds(operator, a.cmp(&b))
            }))
        }),
        _ => return None,
    })
}

/// `IN` of an operand among literal items of its kind, each compared for
/// equality as [`compare`] compares it; None when an item is missing, of
/// another kind or not a literal.
pub(super) fn member(
    rows: usize,
    value: Typed<'_>,
    items: &[Typed<'_>],
    negated: bool,
) -> Option<Result<Vec<Truth>, TryReserveError>> {
    Some(match value {
        Typed::Numbers(layout) => {
            let items = items
                .iter()
                .map(|item| match item {
                    Typed::Numbers(NumberLayout::Constant(Constant(item))) => Some(*item),
                    _ => None,
                })
                .collect::<Option<Vec<Numeric>>>()?;
            numbers!(layout, cells => each(rows, cells, move |value| {
                items
                    .iter()
                    .any(|&item| compare_numeric(value, item) == Ordering::Equal)
                    != negated
            }))
        }
        Typed::Text(layout) => {
            let items = items
                .iter()
                .map(|item| match item {
                    Typed::Text(TextLayout::Constant(Constant(item))) => Some(*item),
                    _ => None,
                })
                .map(|item| item.map(str::as_bytes))
                .collect::<Option<Vec<&[u8]>>>()?;
            text_bytes!(layout, cells => each(rows, cells, move |value: &[u8]| {
                items.iter().any(|item| same_bytes(value, item)) != negated
            }))
        }
        Typed::Dates(layout) => {
            let items = items
                .iter()
                .map(|item| match item {
                    Typed::Dates(TemporalLayout::Constant(Constant(item))) => Some(*item),
                    _ => None,
                })
                .collect::<Option<Vec<Date>>>()?;
            temporal!(layout, cells => each(rows, cells, move |value: Date| {
                items.iter().any(|item| value.cmp(item) == Ordering::Equal) != negated
            }))
        }
        Typed::DateTimes(layout) => {
            let items = items
                .iter()
                .map(|item| match item {
                    Typed::DateTimes(TemporalLayout::Constant(Constant(item))) => Some(*item),
                    _ => None,
                })
                .collect::<Option<Vec<DateTime>>>()?;
            temporal!(layout, cells => each(rows, cells, move |value: DateTime| {
                items.iter().any(|item| value.cmp(item) == Ordering::Equal) != negated
            }))
        }
    })
}

/// `IS NULL`, or `IS NOT NULL` when negated, of each selected row.
pub(super) fn is_null(
    rows: usize,
    value: Typed<'_>,
    negated: bool,
) -> Result<Vec<Truth>, TryReserveError> {
    typed!(value, cells => collect(rows, move |row| {
        Truth::from_bool(cells.cell(row).is_none() != negated)
    }))
}

/// `LIKE` of a text operand and a pattern read once; None for an operand
/// that may hold another kind.
pub(super) fn like(
    rows: usize,
    value: Typed<'_>,
    pattern: &mut LikePattern,
    negated: bool,
) -> Option<Result<Vec<Truth>, TryReserveError>> {
    let Typed::Text(layout) = value else {
        return None;
    };
    Some(text!(layout, cells => each(rows, cells, |value: &str| {
        pattern.matches(value) != negated
    })))
}

/// `str_contains` of a text operand and its compiled pattern; None for an
/// operand that may hold another kind.
pub(super) fn contains(
    rows: usize,
    value: Typed<'_>,
    pattern: &Regex,
) -> Option<Result<Vec<Truth>, TryReserveError>> {
    let Typed::Text(layout) = value else {
        return None;
    };
    Some(text!(layout, cells => each(rows, cells, |value: &str| pattern.is_match(value))))
}

/// A numeric operand's values as numbers; None for any other kind.
pub(super) fn numbers(
    rows: usize,
    value: Typed<'_>,
) -> Option<Result<Vec<Number>, TryReserveError>> {
    let Typed::Numbers(layout) = value else {
        return None;
    };
    Some(
        numbers!(layout, cells => collect(rows, move |row| match cells.cell(row) {
            None => Number::Missing,
            Some(Numeric::Int(value)) => Number::Int(value),
            Some(Numeric::Float(value)) => Number::float(value),
        })),
    )
}

#[cfg(test)]
mod tests {
    // The typed loops' seam: an expression's column form over columns that
    // lend each typed layout, against the same columns read cell by cell.
    // Every expectation is written by hand from the rule card named beside
    // its case, and both routes must give it; a covered case must also read
    // no cell through `Column::get`.
    use super::super::columns::{Column, ColumnResolver, Lent, Stop};
    use super::super::{compile, Context};
    use super::*;
    use crate::table::ColumnRef;
    use crate::temporal::Precision;
    use crate::value::Value;
    use std::cell::Cell;
    use std::collections::BTreeMap;
    use std::rc::Rc;

    /// The two layouts of each kind: a buffer's (`Values::column`), its
    /// rows in order, and a port's, its rows stored in reverse after one
    /// present placeholder, so each read goes through `rows` and `offset`.
    #[derive(Clone, Copy, Debug)]
    enum Layout {
        Buffer,
        Port,
    }

    /// How a case's columns are lent: not at all, or each in a layout.
    #[derive(Clone, Copy, Debug)]
    enum Strategy {
        PerCell,
        All(Layout),
        /// Columns in name order take the buffer layout and the port layout
        /// in turn, starting with the one given, so two columns meet in
        /// both mixed pairs.
        Alternating(Layout),
    }

    enum Payload {
        Int(Vec<i64>),
        Finite(Vec<FiniteFloat>),
        Raw(Vec<f64>),
        Spans(Vec<(usize, usize)>, String),
        Offsets(Vec<usize>, String),
        MaskedDate(Vec<Date>),
        OptionalDate(Vec<Option<Date>>),
        MaskedDateTime(Vec<DateTime>),
        OptionalDateTime(Vec<Option<DateTime>>),
    }

    struct Stored {
        payload: Payload,
        valid: Vec<u64>,
        rows: Option<Vec<usize>>,
        offset: usize,
    }

    impl Stored {
        /// `values` in `layout`, or None when they hold no single kind the
        /// layouts store.
        fn new(values: &[Value], layout: Layout) -> Option<Self> {
            let count = values.len();
            let port = matches!(layout, Layout::Port);
            // A port stores row `row` at `1 + (count - 1 - row)`, after a
            // present placeholder that no row reads.
            let (len, offset) = if port { (count + 1, 1) } else { (count, 0) };
            let position = |row: usize| if port { count - 1 - row } else { row } + offset;
            let mut valid = vec![0u64; len.div_ceil(64)];
            let mut set = |index: usize| valid[index / 64] |= 1 << (index % 64);
            if port {
                set(0);
            }
            let present = values
                .iter()
                .find(|value| !matches!(value, Value::Missing))?;
            let mut slots: Vec<Option<&Value>> = vec![None; len];
            for (row, value) in values.iter().enumerate() {
                if !matches!(value, Value::Missing) {
                    if std::mem::discriminant(value) != std::mem::discriminant(present) {
                        return None;
                    }
                    slots[position(row)] = Some(value);
                    set(position(row));
                }
            }
            let day = Date::new(1999, 1, 1, Precision::Day).unwrap();
            let moment = DateTime::new(day, 0, 0, 0, Precision::Second).unwrap();
            let payload = match present {
                Value::Int(_) => Payload::Int(
                    slots
                        .iter()
                        .map(|slot| match slot {
                            Some(Value::Int(value)) => *value,
                            _ => 999,
                        })
                        .collect(),
                ),
                Value::Float(_) if port => {
                    // A missing row reads missing from a nonfinite payload
                    // behind a set bit, as VALUE-4 admits it.
                    for row in 0..count {
                        set(position(row));
                    }
                    Payload::Raw(
                        slots
                            .iter()
                            .enumerate()
                            .map(|(index, slot)| match slot {
                                Some(Value::Float(value)) => value.get(),
                                _ if index == 0 => 999.0,
                                _ => f64::NAN,
                            })
                            .collect(),
                    )
                }
                Value::Float(_) => Payload::Finite(
                    slots
                        .iter()
                        .map(|slot| match slot {
                            Some(Value::Float(value)) => *value,
                            _ => FiniteFloat::new(999.0).unwrap(),
                        })
                        .collect(),
                ),
                Value::Str(_) => {
                    let mut bytes = String::new();
                    let mut bounds = Vec::new();
                    for slot in &slots {
                        let start = bytes.len();
                        bytes.push_str(match slot {
                            Some(Value::Str(value)) => value,
                            Some(_) => unreachable!("one kind"),
                            None => "junk",
                        });
                        bounds.push((start, bytes.len()));
                    }
                    if port {
                        let mut offsets: Vec<usize> = bounds.iter().map(|bound| bound.0).collect();
                        offsets.push(bytes.len());
                        Payload::Offsets(offsets, bytes)
                    } else {
                        Payload::Spans(bounds, bytes)
                    }
                }
                Value::Date(_) if port => Payload::OptionalDate(
                    slots
                        .iter()
                        .enumerate()
                        .map(|(index, slot)| match slot {
                            Some(Value::Date(value)) => Some(*value),
                            _ => (index == 0).then_some(day),
                        })
                        .collect(),
                ),
                Value::Date(_) => Payload::MaskedDate(
                    slots
                        .iter()
                        .map(|slot| match slot {
                            Some(Value::Date(value)) => *value,
                            _ => day,
                        })
                        .collect(),
                ),
                Value::DateTime(_) if port => Payload::OptionalDateTime(
                    slots
                        .iter()
                        .enumerate()
                        .map(|(index, slot)| match slot {
                            Some(Value::DateTime(value)) => Some(*value),
                            _ => (index == 0).then_some(moment),
                        })
                        .collect(),
                ),
                Value::DateTime(_) => Payload::MaskedDateTime(
                    slots
                        .iter()
                        .map(|slot| match slot {
                            Some(Value::DateTime(value)) => *value,
                            _ => moment,
                        })
                        .collect(),
                ),
                Value::Missing | Value::Bool(_) => return None,
            };
            Some(Self {
                payload,
                valid,
                rows: port.then(|| (0..count).map(|row| count - 1 - row).collect()),
                offset,
            })
        }

        fn lend(&self) -> Lent<'_> {
            let values = match &self.payload {
                Payload::Int(values) => ColumnValues::Int(values),
                Payload::Finite(values) => ColumnValues::FiniteFloat(values),
                Payload::Raw(values) => ColumnValues::Float(values),
                Payload::Spans(spans, bytes) => ColumnValues::TextSpans { spans, bytes },
                Payload::Offsets(offsets, bytes) => ColumnValues::Str { offsets, bytes },
                Payload::MaskedDate(values) => ColumnValues::MaskedDate(values),
                Payload::OptionalDate(values) => ColumnValues::Date(values),
                Payload::MaskedDateTime(values) => ColumnValues::MaskedDateTime(values),
                Payload::OptionalDateTime(values) => ColumnValues::DateTime(values),
            };
            Lent {
                view: ColumnRef::new(values, Some(&self.valid)).expect("addressable storage"),
                rows: self.rows.as_deref(),
                offset: self.offset,
            }
        }
    }

    /// A column read cell by cell from its values, lending its storage when
    /// it has one; `gets` counts the cells read.
    struct Lending {
        values: Vec<Value>,
        stored: Option<Stored>,
        gets: Rc<Cell<usize>>,
    }

    impl Column for Lending {
        fn get(&self, row: usize) -> ValueRef<'_> {
            self.gets.set(self.gets.get() + 1);
            (&self.values[row]).into()
        }

        fn lend(&self) -> Option<Lent<'_>> {
            self.stored.as_ref().map(Stored::lend)
        }
    }

    struct Columns {
        columns: BTreeMap<&'static str, Vec<Value>>,
        strategy: Strategy,
        gets: Rc<Cell<usize>>,
    }

    impl ColumnResolver for Columns {
        type Error = &'static str;
        type Column = Lending;
        fn column(&mut self, identifier: &str) -> Result<Option<Lending>, Self::Error> {
            let Some(position) = self.columns.keys().position(|name| *name == identifier) else {
                return Ok(None);
            };
            let values = self.columns[identifier].clone();
            let layout = match self.strategy {
                Strategy::PerCell => None,
                Strategy::All(layout) => Some(layout),
                Strategy::Alternating(first) => Some(match (first, position % 2) {
                    (Layout::Buffer, 0) | (Layout::Port, 1) => Layout::Buffer,
                    _ => Layout::Port,
                }),
            };
            Ok(Some(Lending {
                stored: layout.and_then(|layout| Stored::new(&values, layout)),
                values,
                gets: Rc::clone(&self.gets),
            }))
        }
    }

    const STRATEGIES: [Strategy; 5] = [
        Strategy::PerCell,
        Strategy::All(Layout::Buffer),
        Strategy::All(Layout::Port),
        Strategy::Alternating(Layout::Buffer),
        Strategy::Alternating(Layout::Port),
    ];

    fn float(value: f64) -> Value {
        Value::Float(FiniteFloat::new(value).unwrap())
    }

    fn text(value: &str) -> Value {
        Value::Str(value.into())
    }

    fn date(month: u8, day: u8) -> Value {
        Value::Date(Date::new(2024, month, day, Precision::Day).unwrap())
    }

    fn moment(day: u8, hour: u8) -> Value {
        let date = Date::new(2024, 3, day, Precision::Day).unwrap();
        Value::DateTime(DateTime::new(date, hour, 0, 0, Precision::Second).unwrap())
    }

    /// Five rows of each kind, a missing value on several rows.
    fn study() -> BTreeMap<&'static str, Vec<Value>> {
        use Value::Missing as M;
        BTreeMap::from([
            // 2^53 + 1 has no binary64 neighbour.
            (
                "I",
                vec![
                    Value::Int(2),
                    M,
                    Value::Int(9_007_199_254_740_993),
                    Value::Int(-4),
                    Value::Int(0),
                ],
            ),
            (
                "F",
                vec![
                    float(2.0),
                    float(3.5),
                    float(9_007_199_254_740_992.0),
                    M,
                    float(-0.0),
                ],
            ),
            (
                "T",
                vec![text("Y"), text("N"), M, text("été"), text("WEEK 12")],
            ),
            (
                "U",
                vec![text("Y"), text("été"), text("N"), M, text("WEEK 2")],
            ),
            (
                "D",
                vec![date(2, 29), M, date(1, 31), date(3, 1), date(2, 29)],
            ),
            (
                "E",
                vec![date(3, 1), date(2, 29), M, date(3, 1), date(2, 28)],
            ),
            (
                "S",
                vec![moment(1, 8), M, moment(1, 9), moment(2, 8), moment(1, 8)],
            ),
            ("B", vec![M, M, M, M, M]),
        ])
    }

    type Outcome<T> = Result<Vec<T>, &'static str>;

    fn outcome<T>(result: Result<Vec<T>, Stop<&'static str>>) -> Outcome<T> {
        result.map_err(|stop| match stop {
            Stop::Refused => "refused",
            Stop::Resolution(error) => error,
        })
    }

    /// The truths of `text` over `rows` rows of `columns` under each
    /// strategy, with the cells each read through `Column::get`.
    fn truths(
        text: &str,
        rows: usize,
        columns: &BTreeMap<&'static str, Vec<Value>>,
    ) -> Vec<(Strategy, Outcome<Truth>, usize)> {
        let compiled = compile(text, Context::Predicate, "typed.RESULT.condition").unwrap();
        STRATEGIES
            .into_iter()
            .map(|strategy| {
                let mut resolver = Columns {
                    columns: columns.clone(),
                    strategy,
                    gets: Rc::new(Cell::new(0)),
                };
                let result = outcome(compiled.truth_column(rows, &mut resolver));
                (strategy, result, resolver.gets.get())
            })
            .collect()
    }

    #[test]
    fn every_layout_pair_gives_the_row_evaluators_truth() {
        use Truth::{False as F, True as T, Unknown as U};
        // (expression, expected, whether every operator has a typed loop)
        let cases: [(&str, Outcome<Truth>, bool); 26] = [
            // FORMULA-5: an int and a float compare by exact value, both
            // zero signs are equal, and a missing operand is UNKNOWN.
            ("I = F", Ok(vec![T, U, F, U, T]), true),
            ("I > F", Ok(vec![F, U, T, U, F]), true),
            ("F >= 2", Ok(vec![T, T, T, U, F]), true),
            ("I < 9007199254740992.0", Ok(vec![T, U, F, T, T]), true),
            ("F * 2 > I", Ok(vec![T, U, T, U, F]), true),
            // FORMULA-5: text compares by its bytes, multi-byte characters
            // after ASCII.
            ("T = U", Ok(vec![T, F, U, U, F]), true),
            ("T < U", Ok(vec![F, T, U, U, T]), true),
            ("T <> 'été'", Ok(vec![T, T, U, F, T]), true),
            ("'été' = U", Ok(vec![F, T, F, U, F]), true),
            // FORMULA-6: IN expands into equalities joined by OR; a NULL
            // item leaves the per-cell loop to apply it.
            ("T IN ('Y', 'été')", Ok(vec![T, F, U, T, F]), true),
            ("T NOT IN ('Y', 'été')", Ok(vec![F, T, U, F, T]), true),
            ("I IN (0, 2.0)", Ok(vec![T, U, F, F, T]), true),
            ("T IN ('Y', NULL)", Ok(vec![T, U, U, U, U]), false),
            ("D IN (DATE '2024-02-29')", Ok(vec![T, U, F, F, T]), true),
            // FORMULA-6: BETWEEN is two comparisons joined by AND.
            ("I BETWEEN F AND 3", Ok(vec![T, U, F, U, T]), true),
            (
                "D BETWEEN DATE '2024-02-01' AND E",
                Ok(vec![T, U, F, T, F]),
                true,
            ),
            // FORMULA-7: LIKE matches whole text, `_` one character.
            ("T LIKE '%t_'", Ok(vec![F, F, U, T, F]), true),
            ("T NOT LIKE 'WEEK%'", Ok(vec![T, T, U, T, F]), true),
            ("T LIKE U", Ok(vec![T, F, U, U, F]), false),
            ("str_contains(T, '^W')", Ok(vec![F, F, U, F, T]), true),
            // FORMULA-5: dates and datetimes compare in time order.
            ("D < E", Ok(vec![T, U, U, F, F]), true),
            (
                "S >= DATETIME '2024-03-01T09:00'",
                Ok(vec![F, U, T, T, F]),
                true,
            ),
            // FORMULA-8: IS NULL reads presence alone.
            ("I IS NULL AND F IS NOT NULL", Ok(vec![F, T, F, F, F]), true),
            // FORMULA-4: the connectives over typed comparisons.
            ("I = 2 AND T = 'Y'", Ok(vec![T, F, F, F, F]), true),
            // FORMULA-5: a date and a datetime, and text and a number, do
            // not compare; a column of no single kind reads cell by cell.
            ("D = S", Err("refused"), false),
            ("B = 1", Ok(vec![U, U, U, U, U]), false),
        ];
        let study = study();
        for (text, expected, covered) in cases {
            for (strategy, result, gets) in truths(text, 5, &study) {
                assert_eq!(result, expected, "{text} under {strategy:?}");
                let typed = !matches!(strategy, Strategy::PerCell);
                assert_eq!(
                    gets == 0,
                    typed && covered,
                    "whether {text} read no cell under {strategy:?}"
                );
            }
        }
    }

    #[test]
    fn an_empty_selection_gives_no_truth() {
        let empty: BTreeMap<&'static str, Vec<Value>> =
            BTreeMap::from([("I", vec![]), ("T", vec![])]);
        for text in ["I = 1", "T IN ('a')", "T LIKE 'a%'", "I IS NULL"] {
            for (strategy, result, _) in truths(text, 0, &empty) {
                assert_eq!(result, Ok(vec![]), "{text} under {strategy:?}");
            }
        }
    }

    #[test]
    fn numeric_columns_and_their_arithmetic_give_the_row_evaluators_numbers() {
        // VALUE-10: `*` of ints is an int, `/` a float, an int meeting a
        // float is widened, and binary64 operations keep written order;
        // FORMULA-14: a missing operand makes the result missing; VALUE-4: a
        // result that is not finite is missing; FORMULA-15: a zero divisor
        // of a present dividend fails, which the row evaluator reports.
        let float = |value: f64| Number::Float(FiniteFloat::new(value).unwrap());
        use Number::Missing as M;
        let cases: [(&str, Outcome<Number>); 7] = [
            (
                "I * 2",
                Ok(vec![
                    Number::Int(4),
                    M,
                    Number::Int(18_014_398_509_481_986),
                    Number::Int(-8),
                    Number::Int(0),
                ]),
            ),
            (
                "F / 2",
                Ok(vec![
                    float(1.0),
                    float(1.75),
                    float(4_503_599_627_370_496.0),
                    M,
                    float(-0.0),
                ]),
            ),
            (
                "F + F",
                Ok(vec![
                    float(4.0),
                    float(7.0),
                    float(18_014_398_509_481_984.0),
                    M,
                    float(-0.0),
                ]),
            ),
            (
                "2 - F",
                Ok(vec![
                    float(0.0),
                    float(-1.5),
                    float(-9_007_199_254_740_990.0),
                    M,
                    float(2.0),
                ]),
            ),
            (
                "F / (I + 1)",
                Ok(vec![
                    float(2.0 / 3.0),
                    M,
                    float(9_007_199_254_740_992.0 / 9_007_199_254_740_994.0),
                    M,
                    float(-0.0),
                ]),
            ),
            ("F * 1e308 * 10", Ok(vec![M, M, M, M, float(-0.0)])),
            ("F / 0", Err("refused")),
        ];
        let study = study();
        let compiled =
            |text| compile(text, Context::Derivation, "typed.RESULT.derivation").unwrap();
        for (text, expected) in cases {
            for strategy in STRATEGIES {
                let mut resolver = Columns {
                    columns: study.clone(),
                    strategy,
                    gets: Rc::new(Cell::new(0)),
                };
                let result = outcome(compiled(text).number_column(5, &mut resolver));
                assert_eq!(
                    (result, resolver.gets.get() == 0),
                    (expected.clone(), !matches!(strategy, Strategy::PerCell)),
                    "{text} under {strategy:?}"
                );
            }
        }
    }
}
