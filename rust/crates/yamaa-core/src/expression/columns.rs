//! The column form of the one evaluator: a compiled expression evaluated
//! over a whole selection of rows, one node at a time, instead of one row at
//! a time.
//!
//! Each identifier occurrence is read once over the selection, as the column
//! its site lends; then every operator applies over the operand columns, row
//! by row, through the same judgements the row evaluator applies
//! (`evaluate.rs`): comparison, `LIKE`, `str_contains`, the three-valued
//! connectives and the numeric operations. Evaluation is eager, as there:
//! every operand is read for every row, so a selection that succeeds reads
//! exactly what the row evaluator reads.
//!
//! The form keeps no diagnostics. When any row would fail, or the site's
//! read fails, the evaluation stops, and the site evaluates the selection
//! through the row evaluator, which reports the first failure where it
//! meets it.
//!
//! An operator whose operands are literals, computed numbers or columns
//! that lend their typed storage runs one loop specialised to their layouts
//! (`typed.rs`, #4218); any other operator applies cell by cell.

use super::bind::CompiledExpression;
use super::evaluate::{
    between, compare, contains_truth, is_null, like_truth, Arithmetic, Condition, LikePattern,
    Truth,
};
use super::syntax::{Comparison, NodeKind};
use super::typed::{self, Floats, Other, Typed};
use crate::numeric::{float_binary, zero_divisor, ArithmeticError, Number};
use crate::table::{ColumnRef, ValueRef};
use std::collections::TryReserveError;

/// One identifier occurrence's values over the selection, one per row.
pub trait Column {
    fn get(&self, row: usize) -> ValueRef<'_>;

    /// The typed storage the values are read from, when the column holds
    /// it, so that an operator reads the payloads without a `ValueRef` per
    /// cell; a column that answers None is read through [`Column::get`].
    fn lend(&self) -> Option<Lent<'_>> {
        None
    }
}

/// A column's typed storage over the selection: selected row `row` is row
/// `offset + rows[row]` of `view`, or `offset + row` without `rows`, and
/// lends the value [`Column::get`] gives for it.
#[derive(Clone, Copy, Debug)]
pub struct Lent<'c> {
    pub view: ColumnRef<'c>,
    pub rows: Option<&'c [usize]>,
    pub offset: usize,
}

/// The owning site reads each occurrence over the whole selection.
pub trait ColumnResolver {
    type Error;
    type Column: Column;
    /// Read one occurrence for every row of the selection, or answer None
    /// when the name is absent.
    fn column(&mut self, identifier: &str) -> Result<Option<Self::Column>, Self::Error>;
}

/// Why a column evaluation stopped. Either way the site evaluates the
/// selection again through the row evaluator, which owns the diagnostic.
#[derive(Debug)]
pub enum Stop<E> {
    /// The site's read failed.
    Resolution(E),
    /// A row met a language condition, or a buffer could not be reserved.
    Refused,
}

impl<E> From<Condition> for Stop<E> {
    fn from(_: Condition) -> Self {
        Self::Refused
    }
}

impl<E> From<ArithmeticError> for Stop<E> {
    fn from(_: ArithmeticError) -> Self {
        Self::Refused
    }
}

impl<E> From<TryReserveError> for Stop<E> {
    fn from(_: TryReserveError) -> Self {
        Self::Refused
    }
}

impl CompiledExpression {
    /// The truth of a predicate on each of `rows` rows.
    pub fn truth_column<R: ColumnResolver>(
        &self,
        rows: usize,
        resolver: &mut R,
    ) -> Result<Vec<Truth>, Stop<R::Error>> {
        Columns {
            compiled: self,
            resolver,
            rows,
        }
        .truth(self.syntax.root())
    }

    /// The number of a numeric expression on each of `rows` rows.
    pub fn number_column<R: ColumnResolver>(
        &self,
        rows: usize,
        resolver: &mut R,
    ) -> Result<Vec<Number>, Stop<R::Error>> {
        let mut columns = Columns {
            compiled: self,
            resolver,
            rows,
        };
        match columns.number(self.syntax.root())? {
            Numbers::Rows(numbers) => Ok(numbers),
            Numbers::Floats(floats) => Ok(floats.numbers()?),
            Numbers::Scalar(number) => columns.each(|_| Ok::<_, Condition>(number)),
        }
    }
}

/// A numeric operand: one number for every row, as a literal is, or one
/// per row, as numbers of either type or as floats with their presence.
enum Numbers {
    Scalar(Number),
    Rows(Vec<Number>),
    Floats(Floats),
}

impl Numbers {
    fn at(&self, row: usize) -> Number {
        match self {
            Self::Scalar(number) => *number,
            Self::Rows(numbers) => numbers[row],
            Self::Floats(floats) => floats.number(row),
        }
    }

    /// The same numbers as a scalar or one number per row.
    fn listed(self) -> Result<Self, TryReserveError> {
        Ok(match self {
            Self::Floats(floats) => Self::Rows(floats.numbers()?),
            numbers => numbers,
        })
    }
}

/// A comparison operand: a literal, a name's column with its own types, or
/// arithmetic.
enum Lane<'c, C> {
    Literal(ValueRef<'c>),
    Column(C),
    Numbers(Numbers),
}

impl<C: Column> Lane<'_, C> {
    fn get(&self, row: usize) -> ValueRef<'_> {
        match self {
            Self::Literal(value) => *value,
            Self::Column(column) => column.get(row),
            Self::Numbers(numbers) => numbers.at(row).into(),
        }
    }

    /// The operand's layout, when the typed loops cover it.
    fn typed(&self) -> Option<Typed<'_>> {
        match self {
            Self::Literal(value) => Typed::literal(*value),
            Self::Column(column) => column.lend().map(Typed::lent),
            Self::Numbers(Numbers::Rows(numbers)) => Some(Typed::computed(numbers)),
            Self::Numbers(Numbers::Floats(floats)) => Some(floats.typed()),
            Self::Numbers(Numbers::Scalar(number)) => Typed::literal((*number).into()),
        }
    }
}

struct Columns<'a, R> {
    compiled: &'a CompiledExpression,
    resolver: &'a mut R,
    rows: usize,
}

impl<'a, R: ColumnResolver> Columns<'a, R> {
    /// One result per row, reserved for the selection before it is filled.
    fn each<T, F>(
        &self,
        mut row: impl FnMut(usize) -> Result<T, F>,
    ) -> Result<Vec<T>, Stop<R::Error>>
    where
        Stop<R::Error>: From<F>,
    {
        let mut results = Vec::new();
        results.try_reserve_exact(self.rows)?;
        for index in 0..self.rows {
            results.push(row(index)?);
        }
        Ok(results)
    }

    fn read(&mut self, id: usize) -> Result<R::Column, Stop<R::Error>> {
        let identifier = self.compiled.syntax.text(id);
        match self.resolver.column(identifier) {
            Ok(Some(column)) => Ok(column),
            Ok(None) => Err(Stop::Refused),
            Err(error) => Err(Stop::Resolution(error)),
        }
    }

    fn truth(&mut self, id: usize) -> Result<Vec<Truth>, Stop<R::Error>> {
        let compiled = self.compiled;
        match &compiled.syntax.nodes()[id].kind {
            NodeKind::Group(inner) => self.truth(*inner),
            NodeKind::Boolean(value) => {
                let truth = Truth::from_bool(*value);
                self.each(|_| Ok::<_, Condition>(truth))
            }
            NodeKind::Not(operand) => {
                let mut truths = self.truth(*operand)?;
                for truth in &mut truths {
                    *truth = truth.negate();
                }
                Ok(truths)
            }
            NodeKind::And(left, right) | NodeKind::Or(left, right) => {
                let mut truths = self.truth(*left)?;
                let right = self.truth(*right)?;
                let and = matches!(compiled.syntax.nodes()[id].kind, NodeKind::And(..));
                for (truth, right) in truths.iter_mut().zip(right) {
                    *truth = if and {
                        truth.and(right)
                    } else {
                        truth.or(right)
                    };
                }
                Ok(truths)
            }
            NodeKind::Compare {
                operator,
                left,
                right,
            } => {
                let left = self.lane(*left)?;
                let right = self.lane(*right)?;
                let typed = left.typed().zip(right.typed());
                if let Some(truths) = typed
                    .and_then(|(left, right)| typed::compare(self.rows, *operator, left, right))
                {
                    return Ok(truths?);
                }
                self.each(|row| compare(*operator, left.get(row), right.get(row)))
            }
            NodeKind::IsNull { value, negated } => {
                let value = self.lane(*value)?;
                if let Some(typed) = value.typed() {
                    return Ok(typed::is_null(self.rows, typed, *negated)?);
                }
                self.each(|row| Ok::<_, Condition>(is_null(value.get(row), *negated)))
            }
            NodeKind::In {
                value,
                items,
                negated,
            } => {
                let value = self.lane(*value)?;
                let items = items
                    .iter()
                    .map(|&item| self.lane(item))
                    .collect::<Result<Vec<_>, _>>()?;
                let typed = value
                    .typed()
                    .zip(items.iter().map(Lane::typed).collect::<Option<Vec<_>>>());
                if let Some(truths) = typed
                    .and_then(|(value, items)| typed::member(self.rows, value, &items, *negated))
                {
                    return Ok(truths?);
                }
                self.each(|row| {
                    let value = value.get(row);
                    let mut truth = Truth::False;
                    for item in &items {
                        truth = truth.or(compare(Comparison::Equal, value, item.get(row))?);
                    }
                    Ok::<_, Condition>(if *negated { truth.negate() } else { truth })
                })
            }
            NodeKind::Between {
                value,
                lower,
                upper,
                negated,
            } => {
                // The subject is read again for its second comparison, as
                // each occurrence is in the row evaluator.
                let first = self.lane(*value)?;
                let lower = self.lane(*lower)?;
                let second = self.lane(*value)?;
                let upper = self.lane(*upper)?;
                if let Some(truths) =
                    self.typed_between([&first, &lower, &second, &upper], *negated)
                {
                    return Ok(truths?);
                }
                self.each(|row| {
                    between(
                        [first.get(row), lower.get(row)],
                        [second.get(row), upper.get(row)],
                        *negated,
                    )
                })
            }
            NodeKind::Like {
                value,
                pattern,
                escape,
                negated,
            } => {
                let value = self.lane(*value)?;
                let pattern = self.lane(*pattern)?;
                // A literal pattern is read once; one that does not read
                // leaves its refusal to the per-cell loop.
                if let (Some(typed), Lane::Literal(ValueRef::Str(text))) = (value.typed(), &pattern)
                {
                    if let Ok(mut read) = LikePattern::new(text, *escape) {
                        if let Some(truths) = typed::like(self.rows, typed, &mut read, *negated) {
                            return Ok(truths?);
                        }
                    }
                }
                self.each(|row| like_truth(value.get(row), pattern.get(row), *escape, *negated))
            }
            NodeKind::Call { arguments, .. } => {
                let value = self.lane(arguments[0])?;
                let pattern = compiled.patterns[id].as_ref().expect("admitted pattern");
                let typed = value.typed();
                if let Some(truths) =
                    typed.and_then(|typed| typed::contains(self.rows, typed, pattern))
                {
                    return Ok(truths?);
                }
                self.each(|row| contains_truth(value.get(row), pattern))
            }
            _ => unreachable!("the binder admits only truth values here"),
        }
    }

    /// `BETWEEN` over its subject and bounds through the typed loops, its
    /// two comparisons joined as [`between`] joins them; None when either
    /// comparison's pair of layouts is not covered.
    fn typed_between(
        &self,
        [first, lower, second, upper]: [&Lane<'a, R::Column>; 4],
        negated: bool,
    ) -> Option<Result<Vec<Truth>, TryReserveError>> {
        let (first, lower) = (first.typed()?, lower.typed()?);
        let (second, upper) = (second.typed()?, upper.typed()?);
        let low = typed::compare(self.rows, Comparison::GreaterEqual, first, lower)?;
        let high = typed::compare(self.rows, Comparison::LessEqual, second, upper)?;
        Some(low.and_then(|mut truths| {
            for (truth, high) in truths.iter_mut().zip(high?) {
                let both = truth.and(high);
                *truth = if negated { both.negate() } else { both };
            }
            Ok(truths)
        }))
    }

    /// A comparison operand: a name keeps its type; arithmetic is numeric.
    fn lane(&mut self, id: usize) -> Result<Lane<'a, R::Column>, Stop<R::Error>> {
        let compiled = self.compiled;
        match &compiled.syntax.nodes()[id].kind {
            NodeKind::Group(inner) => self.lane(*inner),
            NodeKind::Number { .. }
            | NodeKind::String(_)
            | NodeKind::Temporal { .. }
            | NodeKind::Null => Ok(Lane::Literal(
                compiled.literals[id]
                    .as_ref()
                    .expect("a decoded literal")
                    .into(),
            )),
            NodeKind::Identifier => self.read(id).map(Lane::Column),
            _ => self.number(id).map(Lane::Numbers),
        }
    }

    fn number(&mut self, id: usize) -> Result<Numbers, Stop<R::Error>> {
        let compiled = self.compiled;
        match &compiled.syntax.nodes()[id].kind {
            NodeKind::Group(inner) => self.number(*inner),
            NodeKind::Number { .. }
            | NodeKind::Null
            | NodeKind::String(_)
            | NodeKind::Temporal { .. } => {
                let value = compiled.literals[id].as_ref().expect("a decoded literal");
                Number::try_from(value)
                    .map(Numbers::Scalar)
                    .map_err(|_| Stop::Refused)
            }
            NodeKind::Identifier => {
                let column = self.read(id)?;
                if let Some(typed) = column.lend().map(Typed::lent) {
                    if let Some(floats) = Floats::read(self.rows, typed) {
                        return Ok(Numbers::Floats(floats?));
                    }
                    if let Some(numbers) = typed::numbers(self.rows, typed) {
                        return Ok(Numbers::Rows(numbers?));
                    }
                }
                self.each(|row| Number::try_from(column.get(row)).map_err(|_| Stop::Refused))
                    .map(Numbers::Rows)
            }
            // An aggregate's arithmetic over its reductions stays with the
            // row evaluator.
            NodeKind::Reduction { .. } => Err(Stop::Refused),
            NodeKind::Unary { .. } | NodeKind::Binary { .. } | NodeKind::Call { .. } => {
                let (operation, first, second) = Arithmetic::of(compiled, id);
                let left = self.number(first)?;
                let right = second.map(|second| self.number(second)).transpose()?;
                arithmetic(operation, left, right, compiled.expression())
            }
            _ => unreachable!("the binder admits only numbers here"),
        }
    }
}

/// One operation over its operand columns, each row's result written over
/// an operand's own buffer, so no buffer is allocated for it. A binary
/// operator over floats and a present number or other floats stays over
/// floats ([`Floats::binary`]); any other operation reads them as numbers.
fn arithmetic<E>(
    operation: Arithmetic,
    left: Numbers,
    right: Option<Numbers>,
    expression: &str,
) -> Result<Numbers, Stop<E>> {
    let floats = match (operation, left, right) {
        (Arithmetic::Binary(operator), Numbers::Floats(left), Some(right)) => match right {
            Numbers::Floats(right) => Ok(left.binary(operator, Other::Floats(&right), false)),
            Numbers::Scalar(right) if right.as_float().is_some() => {
                let right = right.as_float().expect("a present number");
                Ok(left.binary(operator, Other::Number(right), false))
            }
            right => Err((Numbers::Floats(left), Some(right))),
        },
        (Arithmetic::Binary(operator), Numbers::Scalar(left), Some(Numbers::Floats(right)))
            if left.as_float().is_some() =>
        {
            let left = left.as_float().expect("a present number");
            Ok(right.binary(operator, Other::Number(left), true))
        }
        (_, left, right) => Err((left, right)),
    };
    let (left, right) = match floats {
        Ok(floats) => return floats.map(Numbers::Floats).ok_or(Stop::Refused),
        Err((left, right)) => (left.listed()?, right.map(Numbers::listed).transpose()?),
    };
    Ok(match (left, right) {
        (Numbers::Scalar(left), None) => Numbers::Scalar(operation.apply(left, None, expression)?),
        (Numbers::Scalar(left), Some(Numbers::Scalar(right))) => {
            Numbers::Scalar(operation.apply(left, Some(right), expression)?)
        }
        (Numbers::Rows(mut numbers), None) => {
            in_place(
                operation,
                &mut numbers,
                |_, number| (number, None),
                expression,
            )?;
            Numbers::Rows(numbers)
        }
        (Numbers::Rows(mut numbers), Some(Numbers::Scalar(right))) => {
            in_place(
                operation,
                &mut numbers,
                |_, number| (number, Some(right)),
                expression,
            )?;
            Numbers::Rows(numbers)
        }
        (Numbers::Rows(mut numbers), Some(Numbers::Rows(right))) => {
            in_place(
                operation,
                &mut numbers,
                |row, number| (number, Some(right[row])),
                expression,
            )?;
            Numbers::Rows(numbers)
        }
        (Numbers::Scalar(left), Some(Numbers::Rows(mut numbers))) => {
            in_place(
                operation,
                &mut numbers,
                |_, number| (left, Some(number)),
                expression,
            )?;
            Numbers::Rows(numbers)
        }
        (Numbers::Floats(_), _) | (_, Some(Numbers::Floats(_))) => {
            unreachable!("floats are listed as numbers first")
        }
    })
}

/// Apply `operation` to each row's operands, which `operands` gives from
/// the row and the buffer's own number, writing the result over that
/// number. A binary operator over two present numbers that are not both
/// ints applies the float arithmetic `numeric::binary` applies to them
/// (#4218), each result stored in its branch; every other pair goes through
/// `Arithmetic::apply`.
#[inline]
fn in_place<E>(
    operation: Arithmetic,
    numbers: &mut [Number],
    operands: impl Fn(usize, Number) -> (Number, Option<Number>),
    expression: &str,
) -> Result<(), Stop<E>> {
    if let Arithmetic::Binary(operator) = operation {
        for (row, number) in numbers.iter_mut().enumerate() {
            let (left, right) = operands(row, *number);
            match (left.as_float(), right.and_then(Number::as_float)) {
                (Some(a), Some(b))
                    if !matches!((left, right), (Number::Int(_), Some(Number::Int(_)))) =>
                {
                    if zero_divisor(operator, b) {
                        return Err(Stop::Refused);
                    }
                    *number = float_binary(operator, a, b);
                }
                _ => *number = operation.apply(left, right, expression)?,
            }
        }
        return Ok(());
    }
    for (row, number) in numbers.iter_mut().enumerate() {
        let (left, right) = operands(row, *number);
        *number = operation.apply(left, right, expression)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    // The column form's own seam: an expression evaluated over columns a
    // resolver lends, without a dataset around it. Each expectation is
    // derived from the rule card named beside it.
    use super::super::{compile, Context};
    use super::*;
    use crate::value::{FiniteFloat, Value};
    use std::collections::BTreeMap;

    struct Owned(Vec<Value>);

    impl Column for Owned {
        fn get(&self, row: usize) -> ValueRef<'_> {
            (&self.0[row]).into()
        }
    }

    /// Named columns of `rows` rows; `failing` fails its read, and every
    /// read is counted by name.
    struct Columns {
        columns: BTreeMap<&'static str, Vec<Value>>,
        failing: Option<&'static str>,
        reads: BTreeMap<String, usize>,
    }

    impl Columns {
        fn new(columns: &[(&'static str, Vec<Value>)]) -> Self {
            Self {
                columns: columns.iter().cloned().collect(),
                failing: None,
                reads: BTreeMap::new(),
            }
        }
    }

    impl ColumnResolver for Columns {
        type Error = &'static str;
        type Column = Owned;
        fn column(&mut self, identifier: &str) -> Result<Option<Owned>, Self::Error> {
            *self.reads.entry(identifier.into()).or_default() += 1;
            if self.failing == Some(identifier) {
                return Err("read failed");
            }
            Ok(self.columns.get(identifier).cloned().map(Owned))
        }
    }

    fn float(value: f64) -> Value {
        Value::Float(FiniteFloat::new(value).unwrap())
    }

    fn number(value: f64) -> Number {
        Number::Float(FiniteFloat::new(value).unwrap())
    }

    fn numbers(
        text: &str,
        rows: usize,
        columns: &mut Columns,
    ) -> Result<Vec<Number>, Stop<&'static str>> {
        compile(text, Context::Derivation, "columns.RESULT.derivation")
            .unwrap()
            .number_column(rows, columns)
    }

    fn truths(
        text: &str,
        rows: usize,
        columns: &mut Columns,
    ) -> Result<Vec<Truth>, Stop<&'static str>> {
        compile(text, Context::Predicate, "columns.RESULT.condition")
            .unwrap()
            .truth_column(rows, columns)
    }

    #[test]
    fn arithmetic_applies_in_written_association_and_propagates_missing() {
        // FORMULA-12 and VALUE-10: binary64 operations in written
        // association; FORMULA-14: a missing operand makes the result
        // missing.
        let mut columns = Columns::new(&[
            (
                "W",
                vec![float(70.0), float(81.5), Value::Missing, float(50.0)],
            ),
            (
                "H",
                vec![float(175.0), float(180.0), float(160.0), Value::Missing],
            ),
        ]);
        let bmi = |w: f64, h: f64| w / ((h / 100.0) * (h / 100.0));
        assert_eq!(
            numbers("W / ((H / 100) * (H / 100))", 4, &mut columns).unwrap(),
            vec![
                number(bmi(70.0, 175.0)),
                number(bmi(81.5, 180.0)),
                Number::Missing,
                Number::Missing,
            ],
            "BMI over four rows"
        );
    }

    #[test]
    fn int_arithmetic_stays_int_and_division_is_float() {
        // VALUE-10: `+` and `*` of ints are ints; `/` is a float.
        let mut columns =
            Columns::new(&[("N", vec![Value::Int(3), Value::Int(-4), Value::Missing])]);
        let cases = [
            (
                "N * 2 + 1",
                vec![Number::Int(7), Number::Int(-7), Number::Missing],
            ),
            ("N / 2", vec![number(1.5), number(-2.0), Number::Missing]),
            (
                "NULLIF(N, 3)",
                vec![Number::Missing, Number::Int(-4), Number::Missing],
            ),
        ];
        for (text, expected) in cases {
            assert_eq!(numbers(text, 3, &mut columns).unwrap(), expected, "{text}");
        }
    }

    #[test]
    fn a_row_that_fails_stops_the_column_for_the_row_evaluator() {
        // FORMULA-15 (division by zero, int overflow), FORMULA-12 (a text
        // operand) and FORMULA-9 (an absent name) fail; the column form
        // leaves the diagnostic to the row evaluator.
        let mut columns = Columns::new(&[
            ("N", vec![Value::Int(1), Value::Int(0)]),
            ("BIG", vec![Value::Int(1), Value::Int(i64::MAX)]),
            ("T", vec![Value::Missing, Value::Str("x".into())]),
        ]);
        for text in ["10 / N", "BIG * 2", "T + 1", "ABSENT + 1", "MOD(1, N)"] {
            assert!(
                matches!(numbers(text, 2, &mut columns), Err(Stop::Refused)),
                "{text}"
            );
        }
        columns.failing = Some("N");
        assert!(
            matches!(
                numbers("N + 1", 2, &mut columns),
                Err(Stop::Resolution("read failed"))
            ),
            "a failing read"
        );
    }

    #[test]
    fn predicates_are_three_valued_over_columns() {
        // FORMULA-4: AND, OR and NOT over TRUE, FALSE and UNKNOWN;
        // FORMULA-5: an int and a float compare by exact value, and a
        // missing operand is UNKNOWN; FORMULA-6: IN and BETWEEN expand into
        // comparisons; FORMULA-7: LIKE matches whole text.
        use Truth::{False as F, True as T, Unknown as U};
        let mut columns = Columns::new(&[
            (
                "A",
                vec![
                    Value::Str("Y".into()),
                    Value::Str("N".into()),
                    Value::Missing,
                    Value::Str("WEEK 2".into()),
                ],
            ),
            (
                "N",
                vec![Value::Int(2), Value::Missing, Value::Int(5), Value::Int(1)],
            ),
        ]);
        let cases = [
            ("A = 'Y'", vec![T, F, U, F]),
            ("N = 2.0", vec![T, U, F, F]),
            ("A = 'Y' AND N > 1", vec![T, F, U, F]),
            ("A = 'Y' OR N > 1", vec![T, U, T, F]),
            ("NOT (N > 1)", vec![F, U, F, T]),
            ("A IS NULL", vec![F, F, T, F]),
            ("N IN (1, 5)", vec![F, U, T, T]),
            ("N IN (1, NULL)", vec![U, U, U, T]),
            ("N BETWEEN 2 AND 4", vec![T, U, F, F]),
            ("N NOT BETWEEN 2 AND 4", vec![F, U, T, T]),
            ("A LIKE 'WEEK%'", vec![F, F, U, T]),
            ("str_contains(A, '^W')", vec![F, F, U, T]),
            ("TRUE", vec![T, T, T, T]),
        ];
        for (text, expected) in cases {
            assert_eq!(truths(text, 4, &mut columns).unwrap(), expected, "{text}");
        }
        // FORMULA-5: text and a number do not compare.
        assert!(
            matches!(truths("A = 1", 4, &mut columns), Err(Stop::Refused)),
            "an incompatible pair"
        );
    }

    #[test]
    fn every_occurrence_is_read_as_the_row_evaluator_resolves_it() {
        // evaluate.rs: each identifier occurrence, including the repeated
        // subject of BETWEEN, is resolved afresh.
        let mut columns = Columns::new(&[("N", vec![Value::Int(1), Value::Int(2)])]);
        truths("N BETWEEN 1 AND N + 1", 2, &mut columns).unwrap();
        numbers("N + N", 2, &mut columns).unwrap();
        assert_eq!(
            columns.reads,
            BTreeMap::from([("N".to_string(), 5)]),
            "three reads for BETWEEN and two for the sum"
        );
    }
}
