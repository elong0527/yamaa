//! The result type of an expression (EXPR-14): the runtime types its
//! non-missing results can take, decided before any data is read.
//!
//! Each registered expression has one, fixed by the expression, set by a
//! declared parameter or taken from the types of what it reads. The
//! specification compiler computes it once per prepared operation, and every
//! context reads that answer: matching, intermediate fields and drivers,
//! derive bindings, range bounds, inferred keys, operand checks and column
//! conversion. One type is the rule. A larger set is a type decided per value,
//! which a consumer checks when a value is used, and the empty set a result
//! that is always missing.

use crate::value::{ColumnType, Value, ValueType};

/// Every runtime type, in the order VALUE-1 lists the column types.
const TYPES: [ValueType; 6] = [
    ValueType::Str,
    ValueType::Int,
    ValueType::Float,
    ValueType::Date,
    ValueType::DateTime,
    ValueType::Bool,
];

const fn bit(kind: ValueType) -> u8 {
    match kind {
        ValueType::Str => 1,
        ValueType::Int => 2,
        ValueType::Float => 4,
        ValueType::Date => 8,
        ValueType::DateTime => 16,
        ValueType::Bool => 32,
    }
}

/// The set of runtime types an expression's non-missing results can take.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ResultType(u8);

impl ResultType {
    /// Every result is missing, as `{literal: null}`'s is.
    pub const MISSING: Self = Self(0);
    /// A name no declaration types yet: any type.
    pub const UNKNOWN: Self = Self(63);
    pub const STR: Self = Self::of(ValueType::Str);
    pub const INT: Self = Self::of(ValueType::Int);
    pub const FLOAT: Self = Self::of(ValueType::Float);
    pub const DATE: Self = Self::of(ValueType::Date);
    pub const DATETIME: Self = Self::of(ValueType::DateTime);
    /// `int` or `float`, decided per value.
    pub const NUMBER: Self = Self(bit(ValueType::Int) | bit(ValueType::Float));

    pub const fn of(kind: ValueType) -> Self {
        Self(bit(kind))
    }
    pub fn column(kind: ColumnType) -> Self {
        Self::of(kind.into())
    }
    /// A literal's type; the missing value has none.
    pub fn of_value(value: &Value) -> Self {
        value.value_type().map_or(Self::MISSING, Self::of)
    }
    /// A declared type, or unknown where no declaration states one.
    pub fn declared(kind: Option<ColumnType>) -> Self {
        kind.map_or(Self::UNKNOWN, Self::column)
    }
    /// A runtime type, or unknown where none is known.
    pub fn known(kind: Option<ValueType>) -> Self {
        kind.map_or(Self::UNKNOWN, Self::of)
    }
    /// A value of either type: a selection among inputs of both.
    pub fn or(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
    pub fn contains(self, kind: ValueType) -> bool {
        self.0 & bit(kind) != 0
    }
    pub fn is_missing(self) -> bool {
        self == Self::MISSING
    }
    /// The member types, in VALUE-1 order.
    pub fn types(self) -> impl Iterator<Item = ValueType> {
        TYPES.into_iter().filter(move |&kind| self.contains(kind))
    }
    /// The one type every non-missing result has, if there is one.
    pub fn single(self) -> Option<ValueType> {
        let mut types = self.types();
        let first = types.next()?;
        types.next().is_none().then_some(first)
    }
    /// The one column type every non-missing result has, if there is one.
    pub fn column_type(self) -> Option<ColumnType> {
        match self.single()? {
            ValueType::Str => Some(ColumnType::Str),
            ValueType::Int => Some(ColumnType::Int),
            ValueType::Float => Some(ColumnType::Float),
            ValueType::Date => Some(ColumnType::Date),
            ValueType::DateTime => Some(ColumnType::DateTime),
            ValueType::Bool => None,
        }
    }
    /// The numeric members alone.
    pub fn numeric(self) -> Self {
        Self(self.0 & Self::NUMBER.0)
    }
    /// Whether every result is a number.
    pub fn is_numeric(self) -> bool {
        !self.is_missing() && self.numeric() == self
    }
    /// The slot a field of this type occupies in a donor layout. A type
    /// decided per value keeps its raw values; its slot is `float` for a
    /// number and `str` otherwise, which no reader takes as a promise.
    pub fn layout(self) -> ColumnType {
        self.column_type().unwrap_or(if self.is_numeric() {
            ColumnType::Float
        } else {
            ColumnType::Str
        })
    }
    /// Whether some non-missing result can convert to `target` under
    /// VALUE-3; a result that is always missing converts to every type.
    pub fn converts_to(self, target: ColumnType) -> bool {
        self.is_missing()
            || self
                .types()
                .any(|kind| crate::conversion::convertible(kind, target))
    }
    /// Whether some pair of member types is mutually comparable under
    /// VALUE-19: the same type, or `int` with `float`. A result that is
    /// always missing has nothing to compare.
    pub fn comparable(self, other: Self) -> bool {
        let comparable = |left: ValueType, right: ValueType| {
            left == right
                || matches!(
                    (left, right),
                    (
                        ValueType::Int | ValueType::Float,
                        ValueType::Int | ValueType::Float
                    )
                )
        };
        self.is_missing()
            || other.is_missing()
            || self
                .types()
                .any(|left| other.types().any(|right| comparable(left, right)))
    }
    /// Whether an order term over values of this type is refused before any
    /// record is read (VALUE-22): two of its member types, or one with
    /// itself, do not compare under VALUE-19 (`value::comparable_types`), so
    /// `int` beside `float` orders and `str` beside `int`, `date` beside
    /// `datetime` or `bool` alone does not. A type no source decides yet is
    /// compared with nothing, and a result that is always missing has
    /// nothing to order.
    pub fn unordered(self) -> bool {
        self != Self::UNKNOWN
            && self.types().any(|left| {
                self.types()
                    .any(|right| !crate::value::comparable_types(left, right))
            })
    }
    /// The type's spelling in diagnostic context: one type's name, the
    /// alternatives of a type decided per value, or `missing`.
    pub fn name(self) -> String {
        if self.is_missing() {
            return "missing".into();
        }
        self.types()
            .map(ValueType::name)
            .collect::<Vec<_>>()
            .join(" or ")
    }
}

impl From<ColumnType> for ResultType {
    fn from(kind: ColumnType) -> Self {
        Self::column(kind)
    }
}

impl From<ValueType> for ResultType {
    fn from(kind: ValueType) -> Self {
        Self::of(kind)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_type_is_single_and_a_set_is_decided_per_value() {
        assert_eq!(ResultType::INT.column_type(), Some(ColumnType::Int));
        assert_eq!(ResultType::of(ValueType::Bool).column_type(), None);
        assert_eq!(ResultType::NUMBER.single(), None);
        assert_eq!(ResultType::NUMBER.layout(), ColumnType::Float);
        assert_eq!(
            ResultType::INT.or(ResultType::STR).layout(),
            ColumnType::Str
        );
        assert_eq!(ResultType::MISSING.or(ResultType::DATE), ResultType::DATE);
        assert_eq!(ResultType::NUMBER.name(), "int or float");
        assert_eq!(ResultType::MISSING.name(), "missing");
    }

    #[test]
    fn conversion_fails_only_when_every_member_meets_a_fail_cell() {
        assert!(!ResultType::INT.converts_to(ColumnType::Date));
        assert!(!ResultType::NUMBER.converts_to(ColumnType::DateTime));
        assert!(!ResultType::DATE.converts_to(ColumnType::DateTime));
        assert!(!ResultType::of(ValueType::Bool).converts_to(ColumnType::Str));
        assert!(ResultType::INT
            .or(ResultType::STR)
            .converts_to(ColumnType::Date));
        assert!(ResultType::STR.converts_to(ColumnType::Date));
        assert!(ResultType::FLOAT.converts_to(ColumnType::Int));
        assert!(ResultType::MISSING.converts_to(ColumnType::Date));
        assert!(ResultType::UNKNOWN.converts_to(ColumnType::Int));
    }

    #[test]
    fn comparison_fails_only_when_no_member_pair_compares() {
        assert!(ResultType::INT.comparable(ResultType::FLOAT));
        assert!(!ResultType::NUMBER.comparable(ResultType::STR));
        assert!(!ResultType::DATE.comparable(ResultType::DATETIME));
        assert!(ResultType::of(ValueType::Bool).comparable(ResultType::of(ValueType::Bool)));
        assert!(ResultType::MISSING.comparable(ResultType::DATE));
        assert!(ResultType::UNKNOWN.comparable(ResultType::DATE));
    }
}
