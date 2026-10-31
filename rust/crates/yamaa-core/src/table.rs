//! Borrowed, normalized cells and ordered schema, independent of storage.

use std::collections::BTreeSet;

use crate::{
    temporal::{Date, DateTime},
    value::{ColumnType, FiniteFloat, Value, ValueType},
};

/// A cell view borrows only text; primitive and temporal values retain exact data.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ValueRef<'a> {
    Missing,
    Str(&'a str),
    Int(i64),
    Float(FiniteFloat),
    Bool(bool),
    Date(Date),
    DateTime(DateTime),
}

impl ValueRef<'_> {
    /// Return the closed type without allocating or materializing borrowed text.
    pub fn value_type(self) -> Option<ValueType> {
        match self {
            Self::Missing => None,
            Self::Str(_) => Some(ValueType::Str),
            Self::Int(_) => Some(ValueType::Int),
            Self::Float(_) => Some(ValueType::Float),
            Self::Bool(_) => Some(ValueType::Bool),
            Self::Date(_) => Some(ValueType::Date),
            Self::DateTime(_) => Some(ValueType::DateTime),
        }
    }
}

impl<'a> From<&'a Value> for ValueRef<'a> {
    /// Borrow an owned runtime value while retaining exact scalar representation.
    fn from(value: &'a Value) -> Self {
        match value {
            Value::Missing => Self::Missing,
            Value::Str(value) => Self::Str(value),
            Value::Int(value) => Self::Int(*value),
            Value::Float(value) => Self::Float(*value),
            Value::Bool(value) => Self::Bool(*value),
            Value::Date(value) => Self::Date(*value),
            Value::DateTime(value) => Self::DateTime(*value),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Column {
    pub name: String,
    pub kind: ColumnType,
}

/// Declaration order survives empty tables. Names are unique and nonempty;
/// specification identifier grammar and binding belong to compilation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TableSchema(Vec<Column>);

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum SchemaError {
    #[error("table column {column} has an empty name")]
    EmptyName { column: usize },
    #[error("table column {column} repeats an earlier name")]
    DuplicateName { column: usize },
}

impl TableSchema {
    /// Validate names once, preserving the supplied column declaration order.
    pub fn new(columns: Vec<Column>) -> Result<Self, SchemaError> {
        Self::validate_names(columns.iter().map(|column| column.name.as_str()))?;
        Ok(Self(columns))
    }

    /// Admit the column names shared by specification and public result
    /// schemas, independently of either schema's column type vocabulary.
    pub fn validate_names<'a>(names: impl IntoIterator<Item = &'a str>) -> Result<(), SchemaError> {
        let mut seen = BTreeSet::new();
        for (column, name) in names.into_iter().enumerate() {
            if name.is_empty() {
                return Err(SchemaError::EmptyName { column });
            }
            if !seen.insert(name) {
                return Err(SchemaError::DuplicateName { column });
            }
        }
        Ok(())
    }

    /// Expose the immutable ordered schema, including when the table has no rows.
    pub fn columns(&self) -> &[Column] {
        &self.0
    }
}

/// Port failures and invalid coordinates never stand in for a missing cell.
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum CellError<E> {
    #[error("table cell ({row}, {column}) is out of bounds")]
    OutOfBounds { row: usize, column: usize },
    #[error("table access failed: {0:?}")]
    Access(E),
}

/// One column's typed storage, lent whole by a port that holds it (#4190):
/// fixed-width payloads, or text in one arena, beside a validity bitmap. A
/// layout, never a value: [`ColumnRef::get`] lends for each row the same
/// `ValueRef` the port's [`TableAccess::cell`] does.
#[derive(Clone, Copy, Debug)]
pub struct ColumnRef<'a> {
    values: ColumnValues<'a>,
    /// One bit per row of `values`, least significant first, set when the
    /// row holds a value; `None` while every row does.
    validity: Option<&'a [u64]>,
    /// The view's first row in `values` and `validity`.
    start: usize,
    len: usize,
}

/// The payloads a [`ColumnRef`] lends, of one column type. A payload behind
/// an unset validity bit is never read.
#[derive(Clone, Copy, Debug)]
pub enum ColumnValues<'a> {
    /// Row `i`'s text is `bytes[offsets[i]..offsets[i + 1]]`.
    Str {
        offsets: &'a [usize],
        bytes: &'a str,
    },
    /// Row `i`'s text is `bytes[spans[i].0..spans[i].1]`, the spans in any
    /// order, as the executor's text buffers hold it (#4201).
    TextSpans {
        spans: &'a [(usize, usize)],
        bytes: &'a str,
    },
    Int(&'a [i64]),
    /// A nonfinite payload reads missing, as VALUE-4 admits it.
    Float(&'a [f64]),
    /// Finite payloads, each present where the bitmap says (#4201).
    FiniteFloat(&'a [FiniteFloat]),
    /// Each row carries its own presence; the bitmap must agree with it.
    Date(&'a [Option<Date>]),
    DateTime(&'a [Option<DateTime>]),
    /// Dates present where the bitmap says; the payload behind an unset bit
    /// is a placeholder (#4201).
    MaskedDate(&'a [Date]),
    MaskedDateTime(&'a [DateTime]),
}

impl ColumnValues<'_> {
    /// The column type of every value the payloads hold.
    pub fn kind(&self) -> ColumnType {
        match self {
            Self::Str { .. } | Self::TextSpans { .. } => ColumnType::Str,
            Self::Int(_) => ColumnType::Int,
            Self::Float(_) | Self::FiniteFloat(_) => ColumnType::Float,
            Self::Date(_) | Self::MaskedDate(_) => ColumnType::Date,
            Self::DateTime(_) | Self::MaskedDateTime(_) => ColumnType::DateTime,
        }
    }

    /// The rows the payloads hold, or `None` for text without an end offset.
    fn len(&self) -> Option<usize> {
        Some(match self {
            Self::Str { offsets, .. } => offsets.len().checked_sub(1)?,
            Self::TextSpans { spans, .. } => spans.len(),
            Self::Int(values) => values.len(),
            Self::Float(values) => values.len(),
            Self::FiniteFloat(values) => values.len(),
            Self::Date(values) => values.len(),
            Self::DateTime(values) => values.len(),
            Self::MaskedDate(values) => values.len(),
            Self::MaskedDateTime(values) => values.len(),
        })
    }
}

impl<'a> ColumnRef<'a> {
    /// A view of every row of `values`, or `None` when the payloads, the
    /// text bounds or the bitmap do not cover the same rows, so that a port
    /// lends only storage every read can address.
    pub fn new(values: ColumnValues<'a>, validity: Option<&'a [u64]>) -> Option<Self> {
        let addressable = match values {
            ColumnValues::Str { offsets, bytes } => {
                let ordered = offsets.windows(2).all(|pair| pair[0] <= pair[1]);
                let bounded = offsets.last().is_none_or(|&end| end <= bytes.len());
                let boundaries = offsets.iter().all(|&offset| bytes.is_char_boundary(offset));
                ordered && bounded && boundaries
            }
            ColumnValues::TextSpans { spans, bytes } => spans.iter().all(|&(start, end)| {
                start <= end
                    && end <= bytes.len()
                    && bytes.is_char_boundary(start)
                    && bytes.is_char_boundary(end)
            }),
            _ => true,
        };
        if !addressable {
            return None;
        }
        Self::lend(values, validity)
    }

    /// [`ColumnRef::new`] for storage whose text bounds always address its
    /// arena, such as the executor's buffers, which record only the bounds
    /// of text they appended (#4201): only the parts' lengths are checked,
    /// in constant time. A bound that did not address the arena would panic
    /// on read, never lend other bytes.
    pub(crate) fn lend(values: ColumnValues<'a>, validity: Option<&'a [u64]>) -> Option<Self> {
        let len = values.len()?;
        if validity.is_some_and(|words| words.len() < len.div_ceil(64)) {
            return None;
        }
        Some(Self {
            values,
            validity,
            start: 0,
            len,
        })
    }

    /// The number of rows.
    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Rows `range` of the view, or `None` when the range leaves it.
    pub fn slice(self, range: std::ops::Range<usize>) -> Option<Self> {
        if range.start > range.end || range.end > self.len {
            return None;
        }
        Some(Self {
            start: self.start + range.start,
            len: range.len(),
            ..self
        })
    }

    /// The payloads, whose row `i` is row `i - start()` of the view.
    pub fn values(&self) -> ColumnValues<'a> {
        self.values
    }

    /// The view's first row in [`ColumnRef::values`].
    pub fn start(&self) -> usize {
        self.start
    }

    /// Whether row `row`, which must be below [`ColumnRef::len`], holds a
    /// value by the bitmap.
    #[inline]
    fn valid(&self, row: usize) -> bool {
        let index = self.start + row;
        self.validity
            .is_none_or(|words| words[index / 64] >> (index % 64) & 1 == 1)
    }

    /// Lend row `row`'s value, which must be below [`ColumnRef::len`].
    #[inline]
    pub fn get(&self, row: usize) -> ValueRef<'a> {
        debug_assert!(row < self.len);
        if !self.valid(row) {
            return ValueRef::Missing;
        }
        let index = self.start + row;
        match self.values {
            ColumnValues::Str { offsets, bytes } => {
                ValueRef::Str(&bytes[offsets[index]..offsets[index + 1]])
            }
            ColumnValues::TextSpans { spans, bytes } => {
                let (start, end) = spans[index];
                ValueRef::Str(&bytes[start..end])
            }
            ColumnValues::Int(values) => ValueRef::Int(values[index]),
            ColumnValues::Float(values) => {
                FiniteFloat::new(values[index]).map_or(ValueRef::Missing, ValueRef::Float)
            }
            ColumnValues::FiniteFloat(values) => ValueRef::Float(values[index]),
            ColumnValues::Date(values) => values[index].map_or(ValueRef::Missing, ValueRef::Date),
            ColumnValues::DateTime(values) => {
                values[index].map_or(ValueRef::Missing, ValueRef::DateTime)
            }
            ColumnValues::MaskedDate(values) => ValueRef::Date(values[index]),
            ColumnValues::MaskedDateTime(values) => ValueRef::DateTime(values[index]),
        }
    }

    /// Visit rows `rows` of the view, which must lie within it, in order,
    /// each with the value [`ColumnRef::get`] lends, until `visit` refuses
    /// one: the payloads' layout is matched once, not once per row (#4201).
    #[inline]
    pub fn try_for_each<R>(
        &self,
        rows: std::ops::Range<usize>,
        mut visit: impl FnMut(ValueRef<'a>) -> Result<(), R>,
    ) -> Result<(), R> {
        debug_assert!(rows.end <= self.len);
        // One loop per layout: `read` lends the payload at an index.
        #[inline]
        fn rows_of<'a, R>(
            view: &ColumnRef<'a>,
            rows: std::ops::Range<usize>,
            visit: &mut impl FnMut(ValueRef<'a>) -> Result<(), R>,
            read: impl Fn(usize) -> ValueRef<'a>,
        ) -> Result<(), R> {
            for row in rows {
                visit(if view.valid(row) {
                    read(view.start + row)
                } else {
                    ValueRef::Missing
                })?;
            }
            Ok(())
        }
        match self.values {
            ColumnValues::Str { offsets, bytes } => rows_of(self, rows, &mut visit, |index| {
                ValueRef::Str(&bytes[offsets[index]..offsets[index + 1]])
            }),
            ColumnValues::TextSpans { spans, bytes } => rows_of(self, rows, &mut visit, |index| {
                ValueRef::Str(&bytes[spans[index].0..spans[index].1])
            }),
            ColumnValues::Int(values) => {
                rows_of(self, rows, &mut visit, |index| ValueRef::Int(values[index]))
            }
            ColumnValues::Float(values) => rows_of(self, rows, &mut visit, |index| {
                FiniteFloat::new(values[index]).map_or(ValueRef::Missing, ValueRef::Float)
            }),
            ColumnValues::FiniteFloat(values) => rows_of(self, rows, &mut visit, |index| {
                ValueRef::Float(values[index])
            }),
            ColumnValues::Date(values) => rows_of(self, rows, &mut visit, |index| {
                values[index].map_or(ValueRef::Missing, ValueRef::Date)
            }),
            ColumnValues::DateTime(values) => rows_of(self, rows, &mut visit, |index| {
                values[index].map_or(ValueRef::Missing, ValueRef::DateTime)
            }),
            ColumnValues::MaskedDate(values) => rows_of(self, rows, &mut visit, |index| {
                ValueRef::Date(values[index])
            }),
            ColumnValues::MaskedDateTime(values) => rows_of(self, rows, &mut visit, |index| {
                ValueRef::DateTime(values[index])
            }),
        }
    }

    /// The bytes row `row`'s text bounds span, which must be below
    /// [`ColumnRef::len`], whether or not the row is present; 0 in a column
    /// of another type.
    #[inline]
    pub fn text_len(&self, row: usize) -> usize {
        let index = self.start + row;
        match self.values {
            ColumnValues::Str { offsets, .. } => offsets[index + 1] - offsets[index],
            ColumnValues::TextSpans { spans, .. } => spans[index].1 - spans[index].0,
            _ => 0,
        }
    }
}

/// One column of a table, read a row at a time: through the column view
/// its port lends (#4190), so that a pass over a column makes no call per
/// cell (#4201), or else through [`TableAccess::cell`]. Each read lends the
/// value `cell` would, and refuses a row the table does not hold as `cell`
/// does.
pub enum ColumnReader<'a, T: TableAccess + ?Sized> {
    View { view: ColumnRef<'a>, column: usize },
    Cells { table: &'a T, column: usize },
}

impl<'a, T: TableAccess + ?Sized> ColumnReader<'a, T> {
    /// Column `column` of `table`; a view that does not cover every row of
    /// the table is read through its cells instead.
    pub fn new(table: &'a T, column: usize) -> Self {
        match table.column(column) {
            Some(view) if view.len() == table.row_count() => Self::View { view, column },
            _ => Self::Cells { table, column },
        }
    }

    /// The column view, when the port lends one.
    pub fn view(&self) -> Option<ColumnRef<'a>> {
        match self {
            Self::View { view, .. } => Some(*view),
            Self::Cells { .. } => None,
        }
    }

    /// Row `row`'s value.
    #[inline]
    pub fn get(&self, row: usize) -> Result<ValueRef<'a>, CellError<T::Error>> {
        match self {
            Self::View { view, .. } if row < view.len() => Ok(view.get(row)),
            Self::View { column, .. } => Err(CellError::OutOfBounds {
                row,
                column: *column,
            }),
            Self::Cells { table, column } => table.cell(row, *column),
        }
    }

    /// Visit rows `rows`, in order, each with the value [`Self::get`]
    /// lends, until `visit` refuses one, or a row the table does not hold
    /// is refused through `refused`: a view's rows in one loop per layout
    /// ([`ColumnRef::try_for_each`]).
    #[inline]
    pub fn try_for_each<R>(
        &self,
        rows: std::ops::Range<usize>,
        mut visit: impl FnMut(ValueRef<'a>) -> Result<(), R>,
        refused: impl Fn(CellError<T::Error>) -> R,
    ) -> Result<(), R> {
        match self {
            Self::View { view, column } => {
                let held = rows.start.min(view.len())..rows.end.min(view.len());
                view.try_for_each(held.clone(), &mut visit)?;
                if rows.end > held.end {
                    return Err(refused(CellError::OutOfBounds {
                        row: held.end.max(rows.start),
                        column: *column,
                    }));
                }
                Ok(())
            }
            Self::Cells { table, column } => {
                for row in rows {
                    visit(table.cell(row, *column).map_err(&refused)?)?;
                }
                Ok(())
            }
        }
    }
}

/// Immutable snapshot with normalized runtime values and stable schema/row order.
/// Implementations must check both coordinates, including on empty tables, and
/// retain backing storage for every borrowed cell. No host calls are implied.
/// Declared column types describe storage; conversion belongs at ingestion, not
/// inside consumers. A port must not mutate its snapshot through interior state.
pub trait TableAccess {
    type Error;

    /// Return the snapshot schema in declared column order.
    fn schema(&self) -> &TableSchema;
    /// Return the number of records, independent of column null counts.
    fn row_count(&self) -> usize;
    /// Borrow one normalized cell or report coordinate/access failure.
    fn cell(&self, row: usize, column: usize) -> Result<ValueRef<'_>, CellError<Self::Error>>;
    /// Lend column `column` whole, every row of it, when the port holds it
    /// as typed storage (#4190); `None` for a port that does not, or for a
    /// column out of bounds, whose reads go through [`TableAccess::cell`].
    fn column(&self, column: usize) -> Option<ColumnRef<'_>> {
        let _ = column;
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // The seam is the column view's own admission (#4190): a port lends
    // only storage every row of the view can read.
    #[test]
    fn a_column_view_refuses_storage_whose_parts_disagree() {
        let refused = [
            (
                ColumnValues::Str {
                    offsets: &[0, 2, 1],
                    bytes: "ab",
                },
                None,
                "offsets out of order",
            ),
            (
                ColumnValues::Str {
                    offsets: &[0, 3],
                    bytes: "ab",
                },
                None,
                "an offset past the arena",
            ),
            (
                ColumnValues::Str {
                    offsets: &[0, 1, 2],
                    bytes: "\u{e9}",
                },
                None,
                "an offset inside a character",
            ),
            (
                ColumnValues::Str {
                    offsets: &[],
                    bytes: "",
                },
                None,
                "no end offset",
            ),
            (
                ColumnValues::Int(&[0; 65]),
                Some(&[0u64][..]),
                "a bitmap of too few words",
            ),
            (
                ColumnValues::TextSpans {
                    spans: &[(0, 1), (2, 1)],
                    bytes: "ab",
                },
                None,
                "a span that ends before it starts",
            ),
            (
                ColumnValues::TextSpans {
                    spans: &[(1, 3)],
                    bytes: "ab",
                },
                None,
                "a span past the arena",
            ),
            (
                ColumnValues::TextSpans {
                    spans: &[(0, 1)],
                    bytes: "\u{e9}",
                },
                None,
                "a span inside a character",
            ),
        ];
        for (values, validity, case) in refused {
            assert!(ColumnRef::new(values, validity).is_none(), "{case}");
        }
        let view = ColumnRef::new(ColumnValues::Int(&[1, 2, 3]), None).unwrap();
        assert_eq!(
            view.slice(1..3).map(|view| view.get(0)),
            Some(ValueRef::Int(2))
        );
        assert!(view.slice(2..4).is_none(), "a range past the view");
    }

    // The seam is a view's two reads (#4201): a pass over a range lends, row
    // by row, what `get` lends, in every layout, missing where the bitmap
    // or the payload says, and stops at the first refusal.
    #[test]
    fn a_pass_over_a_view_lends_what_each_read_lends() {
        let day = |day| Date::new(2024, 2, day, crate::temporal::Precision::Day).unwrap();
        let moment =
            DateTime::new(day(29), 23, 59, 58, crate::temporal::Precision::Second).unwrap();
        let float = |value| FiniteFloat::new(value).unwrap();
        // The payloads the cases lend.
        let finite = [float(-0.0), float(1.0), float(2.5)];
        let dates = [Some(day(1)), Some(day(2)), None];
        let masked_dates = [day(1), day(2), day(3)];
        let datetimes = [None, Some(moment), Some(moment)];
        let masked_datetimes = [moment; 3];
        // Rows 0 and 2 are present, row 1 is missing by the bitmap.
        let bits = [0b101u64];
        let cases = [
            (
                ColumnValues::Str {
                    offsets: &[0, 1, 1, 3],
                    bytes: "abc",
                },
                [ValueRef::Str("a"), ValueRef::Missing, ValueRef::Str("bc")],
                "text by offsets",
            ),
            (
                ColumnValues::TextSpans {
                    spans: &[(2, 3), (0, 3), (0, 2)],
                    bytes: "abc",
                },
                [ValueRef::Str("c"), ValueRef::Missing, ValueRef::Str("ab")],
                "text by spans in any order",
            ),
            (
                ColumnValues::Int(&[7, 8, -9]),
                [ValueRef::Int(7), ValueRef::Missing, ValueRef::Int(-9)],
                "integers",
            ),
            (
                ColumnValues::Float(&[0.5, 1.0, f64::NAN]),
                [
                    ValueRef::Float(float(0.5)),
                    ValueRef::Missing,
                    ValueRef::Missing,
                ],
                "floats, a nonfinite one missing",
            ),
            (
                ColumnValues::FiniteFloat(&finite),
                [
                    ValueRef::Float(float(-0.0)),
                    ValueRef::Missing,
                    ValueRef::Float(float(2.5)),
                ],
                "finite floats",
            ),
            (
                ColumnValues::Date(&dates),
                [ValueRef::Date(day(1)), ValueRef::Missing, ValueRef::Missing],
                "dates with their own presence",
            ),
            (
                ColumnValues::MaskedDate(&masked_dates),
                [
                    ValueRef::Date(day(1)),
                    ValueRef::Missing,
                    ValueRef::Date(day(3)),
                ],
                "dates present by the bitmap",
            ),
            (
                ColumnValues::DateTime(&datetimes),
                [
                    ValueRef::Missing,
                    ValueRef::Missing,
                    ValueRef::DateTime(moment),
                ],
                "datetimes with their own presence",
            ),
            (
                ColumnValues::MaskedDateTime(&masked_datetimes),
                [
                    ValueRef::DateTime(moment),
                    ValueRef::Missing,
                    ValueRef::DateTime(moment),
                ],
                "datetimes present by the bitmap",
            ),
        ];
        for (values, expected, case) in cases {
            let view = ColumnRef::new(values, Some(&bits)).unwrap();
            let mut passed = Vec::new();
            let pass = view.try_for_each(0..3, |value| {
                passed.push(value);
                Ok::<_, ()>(())
            });
            let read: Vec<_> = (0..3).map(|row| view.get(row)).collect();
            assert_eq!(
                (pass, passed, read),
                (Ok(()), expected.to_vec(), expected.to_vec()),
                "{case}"
            );
        }
        let view = ColumnRef::new(ColumnValues::Int(&[1, 2, 3]), None).unwrap();
        let mut passed = Vec::new();
        let refused = view.try_for_each(0..3, |value| {
            passed.push(value);
            if value == ValueRef::Int(2) {
                Err("two")
            } else {
                Ok(())
            }
        });
        assert_eq!(
            (refused, passed),
            (Err("two"), vec![ValueRef::Int(1), ValueRef::Int(2)]),
            "a pass stops at its first refusal"
        );
    }
}
