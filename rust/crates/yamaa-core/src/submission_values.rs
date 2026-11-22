//! Value-level entries keyed by a declared where clause (SUB-12, SUB-13): the
//! entries a column declares, the static checks that need no data, and what
//! each row construction proves about the rows an entry selects. Shared by the
//! domain build, which reads the typed tree, and by composition, which still
//! reads each specification's document; no study, host or data access. The
//! same checks read the where clause of an analysis dataset (DEFINE-32).
use crate::ast::{self, CodelistType, Comparator, ExprId, ExprKind, Literal, Order};
use crate::dataset::{WhereBound, WhereTerm};
use crate::project_terminology::Catalogue;
use crate::schema::{Document, DocumentNode as N};
use crate::specification_submission_document::SubmissionMode;
use crate::value::{ColumnType, Value};
use std::cmp::Ordering;
use std::collections::BTreeSet;

/// One where column as written: its name, its comparator and the values it
/// lists (SUB-12).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Term<'a> {
    pub(crate) column: &'a str,
    pub(crate) comparator: Comparator,
    pub(crate) values: Vec<&'a str>,
}
impl Term<'_> {
    /// Whether a listed term selects a row whose column holds the text
    /// `value`, `None` when it is missing: one of the values, or under `NE`
    /// and `NOTIN` none of them, a missing value included.
    pub(crate) fn selects(&self, value: Option<&str>) -> bool {
        value.is_some_and(|value| self.values.contains(&value)) != self.comparator.excludes()
    }
}

/// One where clause as written: each column, in written order.
pub(crate) type Clause<'a> = Vec<Term<'a>>;

/// One value-level entry as written: its identifier, its where clause in
/// written order and its mapping node.
#[derive(Clone, Debug)]
pub(crate) struct Entry<'a> {
    pub(crate) id: &'a str,
    pub(crate) node: usize,
    pub(crate) clause: Clause<'a>,
}

fn present(d: &Document, node: usize, name: &str) -> Option<usize> {
    d.field(node, name)
        .filter(|&id| !matches!(d.nodes()[id], N::Null))
}
fn text(d: &Document, node: usize) -> Option<&str> {
    match &d.nodes()[node] {
        N::Text(value) => Some(value),
        _ => None,
    }
}
fn items(d: &Document, node: Option<usize>) -> &[usize] {
    match node.map(|id| &d.nodes()[id]) {
        Some(N::Sequence(items)) => items,
        _ => &[],
    }
}

/// A column's value-level entries in declaration order. A shape schema
/// admission refuses is skipped; it never reaches a compiled specification.
pub(crate) fn entries(d: &Document, column: usize) -> Vec<Entry<'_>> {
    let Some(submission) = present(d, column, "submission") else {
        return Vec::new();
    };
    items(d, present(d, submission, "value_level"))
        .iter()
        .filter_map(|&node| {
            let id = text(d, present(d, node, "id")?)?;
            let N::Mapping(terms) = &d.nodes()[present(d, node, "where")?] else {
                return None;
            };
            let clause = terms
                .iter()
                .map(|&(column, values)| {
                    // A resolved 1.0 specification holds a where condition in
                    // its long form, `{values: [...]}`, with an `NE` or
                    // `NOTIN` comparator when one is written (P-VLM-1).
                    let (comparator, values) = match &d.nodes()[values] {
                        N::Mapping(_) => (
                            match present(d, values, "comparator").and_then(|id| text(d, id)) {
                                Some(written) => Comparator::written_as(written),
                                None => Comparator::In,
                            },
                            present(d, values, "values").unwrap_or(values),
                        ),
                        _ => (Comparator::In, values),
                    };
                    Some(Term {
                        column: text(d, column)?,
                        comparator,
                        values: items(d, Some(values))
                            .iter()
                            .map(|&value| text(d, value))
                            .collect::<Option<Vec<_>>>()?,
                    })
                })
                .collect::<Option<Vec<_>>>()?;
            Some(Entry { id, node, clause })
        })
        .collect()
}

/// The path of one entry, `columns.<column>.submission.value_level.<id>`.
pub(crate) fn path(column: &str, id: &str) -> String {
    format!("columns.{column}.submission.value_level.{id}")
}

/// Why a where clause cannot select rows (SUB-12).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum WhereFault<'a> {
    /// The clause names no column.
    NoColumns,
    /// The clause names the column whose entry it is.
    Itself,
    /// The clause names no declared column.
    Undeclared,
    /// The clause names a declared column `output.columns` does not carry.
    NotOutput,
    /// The clause lists values for a column whose declared type is not
    /// `str`.
    NotText,
    /// The clause orders a column whose declared type is `str` (D314).
    OrderedText,
    /// A clause column lists no value.
    NoValues,
    /// A clause column lists a number of values its comparator does not
    /// take: `NE` takes one and `NOTIN` two or more.
    ValueCount(Comparator),
    /// A clause column lists one value twice.
    DuplicateValue,
    /// An ordered clause column's value does not convert to the column's
    /// declared type (VALUE-3).
    ValueType,
    /// A clause column lists values its codelists do not admit: those
    /// values, in declared order.
    NotInCodelist(Vec<&'a str>),
}
impl<'a> WhereFault<'a> {
    pub(crate) fn reason(&self) -> &'static str {
        match self {
            Self::NoColumns => "no_columns",
            Self::Itself => "self",
            Self::Undeclared => "undeclared",
            Self::NotOutput => "not_output",
            Self::NotText => "not_str",
            Self::OrderedText => "ordered_str",
            Self::NoValues => "no_values",
            Self::ValueCount(_) => "value_count",
            Self::DuplicateValue => "duplicate_value",
            Self::ValueType => "value_type",
            Self::NotInCodelist(_) => "not_in_codelist",
        }
    }
    /// The listed values the fault refuses, which only `not_in_codelist`
    /// reports.
    pub(crate) fn values(&self) -> Option<&[&'a str]> {
        match self {
            Self::NotInCodelist(values) => Some(values),
            _ => None,
        }
    }
    /// The written comparator whose value count the fault refuses, which only
    /// `value_count` reports.
    pub(crate) fn comparator(&self) -> Option<&'static str> {
        match self {
            Self::ValueCount(comparator) => comparator.written(),
            _ => None,
        }
    }
}

/// One static finding over a column's entries.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Finding<'a> {
    /// Two entries of one column share an identifier.
    DuplicateId { id: &'a str },
    /// A where clause, or one of its columns, cannot select rows.
    Where {
        id: &'a str,
        column: Option<&'a str>,
        fault: WhereFault<'a>,
    },
    /// The where clauses of `other`, declared first, and `id` can select one row.
    Overlap { id: &'a str, other: &'a str },
}

/// A declared column a where clause may name: whether `output.columns`
/// carries it, its declared type, and the values its codelists admit when
/// they govern every one of its rows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Declared<'c> {
    pub(crate) output: bool,
    pub(crate) kind: ColumnType,
    pub(crate) codelist: Option<BTreeSet<&'c str>>,
}

/// The values the rows of a where column can hold under its terminology
/// (CT-7): the items of its own codelist and of the codelist each of its
/// entries declares, sponsor-extended items included. `None` when a row may
/// hold any value: the column declares no codelist of its own, or one of
/// these codelists is external. An undeclared or non-text codelist, which its
/// binding refuses on its own, checks nothing either.
pub(crate) fn admitted<'c, 'n>(
    catalogue: &'c Catalogue,
    column: Option<&'n str>,
    entries: impl IntoIterator<Item = &'n str>,
) -> Option<BTreeSet<&'c str>> {
    let mut values = BTreeSet::new();
    for id in std::iter::once(column?).chain(entries) {
        let list = catalogue
            .get(id)
            .filter(|list| list.data_type == CodelistType::Text)?;
        values.extend(
            list.items
                .as_deref()?
                .iter()
                .filter_map(|item| match &item.value {
                    Value::Str(value) => Some(value.as_str()),
                    _ => None,
                }),
        );
    }
    Some(values)
}

/// What a where clause is checked as (SUB-12): a value-level entry's in
/// validated or in historical metadata, or an analysis dataset's
/// (DEFINE-32), which describes no column of its own and selects no rows in
/// a build.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Scope {
    Validated,
    Historical,
    Analysis,
}
impl From<SubmissionMode> for Scope {
    fn from(mode: SubmissionMode) -> Self {
        match mode {
            SubmissionMode::Validated => Self::Validated,
            SubmissionMode::Historical => Self::Historical,
        }
    }
}

/// The static checks of SUB-12 over one column's entries: identifiers unique
/// within the column, where clauses over declared output columns other than
/// the column itself in validated mode, listing nonempty distinct values
/// their codelists admit for a `str` column or ordering a column of another
/// type against one value that converts to it, and no two entries whose where clauses can select one
/// row. Historical composition keeps structural/name checks and the type of
/// an ordered term, without asserting current rows. An analysis clause is
/// checked as a validated one is, except that it has no column to name as
/// its own, overlaps nothing, and may list values for a column that is not
/// `str`, each converting to the column's type and checked against no
/// codelist (DEFINE-32).
pub(crate) fn admit<'a, 'b, 'c>(
    entries: impl IntoIterator<Item = (&'a str, &'b Clause<'a>)>,
    column: &str,
    scope: impl Into<Scope>,
    declared: impl Fn(&str) -> Option<Declared<'c>>,
) -> Vec<Finding<'a>>
where
    'a: 'b,
{
    let scope = scope.into();
    let validated = scope == Scope::Validated;
    let mut findings = Vec::new();
    let mut ids = BTreeSet::new();
    let mut valid = Vec::<(&'a str, &'b Clause<'a>, Vec<Option<Value>>)>::new();
    for (id, clause) in entries {
        if !ids.insert(id) {
            findings.push(Finding::DuplicateId { id });
        }
        let before = findings.len();
        if clause.is_empty() {
            findings.push(Finding::Where {
                id,
                column: None,
                fault: WhereFault::NoColumns,
            });
        }
        let mut bounds = Vec::with_capacity(clause.len());
        for Term {
            column: name,
            comparator,
            values,
        } in clause
        {
            let ordered = comparator.order().is_some();
            let mut codelist = None;
            let mut kind = None;
            // The type each value an analysis clause lists converts to.
            let mut listed = None;
            // A historical value list may be keyed by its own column, as a
            // received document records it (DEFINE-30).
            let fault = if *name == column && validated {
                Some(WhereFault::Itself)
            } else {
                match declared(name) {
                    None => Some(WhereFault::Undeclared),
                    Some(Declared { output: false, .. }) => Some(WhereFault::NotOutput),
                    // An ordered term is checked in historical composition
                    // too: without its type, it has nothing to compare by.
                    Some(declared) => match (ordered, declared.kind == ColumnType::Str) {
                        (false, false) if validated => Some(WhereFault::NotText),
                        (false, false) => {
                            if scope == Scope::Analysis {
                                listed = Some(declared.kind);
                            }
                            None
                        }
                        (true, true) => Some(WhereFault::OrderedText),
                        (false, true) => {
                            if scope != Scope::Historical {
                                codelist = declared.codelist;
                            }
                            None
                        }
                        (true, false) => {
                            kind = Some(declared.kind);
                            None
                        }
                    },
                }
            }
            .or_else(|| values.is_empty().then_some(WhereFault::NoValues))
            .or_else(|| {
                (!comparator.takes(values.len())).then_some(WhereFault::ValueCount(*comparator))
            })
            .or_else(|| {
                values
                    .iter()
                    .enumerate()
                    .any(|(index, value)| values[..index].contains(value))
                    .then_some(WhereFault::DuplicateValue)
            })
            .or_else(|| {
                let admitted = codelist?;
                let refused = values
                    .iter()
                    .copied()
                    .filter(|value| !admitted.contains(*value))
                    .collect::<Vec<_>>();
                (!refused.is_empty()).then_some(WhereFault::NotInCodelist(refused))
            })
            .or_else(|| {
                converted(values.first()?, kind?)
                    .is_none()
                    .then_some(WhereFault::ValueType)
            })
            .or_else(|| {
                let kind = listed?;
                values
                    .iter()
                    .any(|value| converted(value, kind).is_none())
                    .then_some(WhereFault::ValueType)
            });
            // The ordered term's converted value, for the overlap check.
            bounds.push(
                kind.zip(values.first())
                    .and_then(|(kind, value)| converted(value, kind)),
            );
            if let Some(fault) = fault {
                findings.push(Finding::Where {
                    id,
                    column: Some(name),
                    fault,
                });
            }
        }
        if findings.len() != before {
            continue;
        }
        if validated {
            if let Some(&(other, _, _)) = valid
                .iter()
                .find(|(_, other, others)| overlap(other, others, clause, &bounds))
            {
                findings.push(Finding::Overlap { id, other });
            }
        }
        valid.push((id, clause, bounds));
    }
    findings
}

/// A where value as the where column's type `kind` holds it, when it
/// converts under VALUE-3 to a present value: an ordered term's one value,
/// or a value an analysis clause lists for a column that is not `str`.
pub(crate) fn converted(value: &str, kind: ColumnType) -> Option<Value> {
    crate::conversion::convert(&Value::Str(value.to_owned()), kind)
        .ok()
        .filter(|value| *value != Value::Missing)
}

/// The executable form of one where column at `column` of the output row:
/// its listed values, or, under an ordered comparator, its one value
/// converted to `kind`, the where column's type. `None` when an ordered term
/// orders a `str` column or its value does not convert, which admission
/// refuses first.
pub(crate) fn where_term(
    column: usize,
    comparator: Comparator,
    values: Vec<String>,
    kind: ColumnType,
) -> Option<WhereTerm> {
    let bound = match comparator.order() {
        Some(order) => {
            let [value] = values.as_slice() else {
                return None;
            };
            if kind == ColumnType::Str {
                return None;
            }
            Some(WhereBound {
                order,
                value: converted(value, kind)?,
            })
        }
        None => None,
    };
    Some(WhereTerm {
        column,
        values,
        excludes: comparator.excludes(),
        bound,
    })
}

/// Whether two where clauses can select one row, as validation decides it:
/// one clause's columns are all columns of the other, and every column they
/// share admits a common value. A clause with an empty ordered term cannot
/// select a row. Each clause comes with the converted value of
/// each of its ordered terms, in term order. Two clauses over columns neither
/// of which contains the other are left to the build, which fails a row both
/// select.
pub(crate) fn overlap(
    left: &[Term<'_>],
    left_bounds: &[Option<Value>],
    right: &[Term<'_>],
    right_bounds: &[Option<Value>],
) -> bool {
    fn columns<'a>(clause: &[Term<'a>]) -> BTreeSet<&'a str> {
        clause.iter().map(|term| term.column).collect()
    }
    fn empty_clause(clause: &[Term<'_>], bounds: &[Option<Value>]) -> bool {
        clause.iter().zip(bounds).any(|(term, bound)| {
            term.comparator
                .order()
                .zip(bound.as_ref())
                .is_some_and(empty)
        })
    }
    if empty_clause(left, left_bounds) || empty_clause(right, right_bounds) {
        return false;
    }
    let (a, b) = (columns(left), columns(right));
    (a.is_subset(&b) || b.is_subset(&a))
        && left.iter().zip(left_bounds).all(|(term, bound)| {
            match right
                .iter()
                .zip(right_bounds)
                .find(|(other, _)| other.column == term.column)
            {
                Some((other, other_bound)) => share((term, bound), (other, other_bound)),
                None => true,
            }
        })
}

/// Whether two terms over one column admit a common value: two lists that
/// meet, a list holding a value an exclusion does not list, any two
/// exclusions, which both select the missing value, and two ordered terms
/// whose half-lines meet (SUB-12). A list and an ordered term never share a
/// column that admission passes, and such a pair is taken to meet.
fn share(
    (left, left_bound): (&Term<'_>, &Option<Value>),
    (right, right_bound): (&Term<'_>, &Option<Value>),
) -> bool {
    match (
        left.comparator.order().zip(left_bound.as_ref()),
        right.comparator.order().zip(right_bound.as_ref()),
    ) {
        (Some(left), Some(right)) => return meet(left, right),
        (None, None) => {}
        _ => return true,
    }
    match (left.comparator.excludes(), right.comparator.excludes()) {
        (false, false) => left.values.iter().any(|value| right.values.contains(value)),
        (false, true) => left
            .values
            .iter()
            .any(|value| !right.values.contains(value)),
        (true, false) => right
            .values
            .iter()
            .any(|value| !left.values.contains(value)),
        (true, true) => true,
    }
}

/// Whether two ordered terms over one column select a common value. Two
/// nonempty bounds on one side always meet; an upper and a lower bound meet
/// when the lower lies below the upper, or on it when both select it. Two
/// exclusive bounds share no value when their endpoints are consecutive
/// values of the column's type, including finite binary64 floats.
fn meet(left: (Order, &Value), right: (Order, &Value)) -> bool {
    if empty(left) || empty(right) {
        return false;
    }
    if left.0.upper() == right.0.upper() {
        return true;
    }
    let (upper, lower) = if left.0.upper() {
        (left, right)
    } else {
        (right, left)
    };
    match crate::value::compare_present(lower.1, upper.1) {
        Ok(Ordering::Less) => {
            upper.0.inclusive() || lower.0.inclusive() || !adjacent(lower.1, upper.1)
        }
        Ok(Ordering::Equal) => upper.0.inclusive() && lower.0.inclusive(),
        Ok(Ordering::Greater) => false,
        Err(_) => true,
    }
}

/// An exclusive bound beyond its type's first or last value selects no row.
fn empty((order, value): (Order, &Value)) -> bool {
    let first = match order {
        Order::Lt => true,
        Order::Gt => false,
        Order::Le | Order::Ge => return false,
    };
    let second = if first {
        0
    } else {
        crate::temporal::Time::DAY_SECONDS - 1
    };
    match value {
        Value::Int(value) => *value == if first { i64::MIN } else { i64::MAX },
        Value::Float(value) => value.get() == if first { -f64::MAX } else { f64::MAX },
        Value::Date(date) => calendar_endpoint(*date, first),
        Value::DateTime(moment) => {
            calendar_endpoint(moment.date(), first) && moment.time().seconds() == second
        }
        Value::Time(time) => time.seconds() == second,
        _ => false,
    }
}

/// The calendar type owns its year range; probe the adjacent year's validity
/// instead of restating that range here.
fn calendar_endpoint(date: crate::temporal::Date, first: bool) -> bool {
    let (year, month, day) = date.fields();
    let end = if first { (1, 1) } else { (12, 31) };
    let beyond = if first { year - 1 } else { year + 1 };
    (month, day) == end
        && crate::temporal::Date::new(beyond, 1, 1, crate::temporal::Precision::Day).is_err()
}

/// Whether `upper` is the value right after `lower` in a type with no value
/// between consecutive ones: the next integer, binary64, day or whole second.
fn adjacent(lower: &Value, upper: &Value) -> bool {
    match (lower, upper) {
        (Value::Int(lower), Value::Int(upper)) => lower.checked_add(1) == Some(*upper),
        (Value::Float(lower), Value::Float(upper)) => {
            // IEEE-754 encodings step toward positive infinity, except that
            // both zero signs step to the least positive subnormal.
            let value = lower.get();
            let bits = if value == 0.0 {
                1
            } else if value < 0.0 {
                value.to_bits() - 1
            } else {
                value.to_bits() + 1
            };
            f64::from_bits(bits) == upper.get()
        }
        (Value::Date(lower), Value::Date(upper)) => next_day(lower.fields()) == upper.fields(),
        (Value::Time(lower), Value::Time(upper)) => lower.seconds() + 1 == upper.seconds(),
        (Value::DateTime(lower), Value::DateTime(upper)) => {
            let (year, month, day, hour, minute, second) = lower.fields();
            let next = match (hour, minute, second) {
                (23, 59, 59) => {
                    let (year, month, day) = next_day((year, month, day));
                    (year, month, day, 0, 0, 0)
                }
                (hour, 59, 59) => (year, month, day, hour + 1, 0, 0),
                (hour, minute, 59) => (year, month, day, hour, minute + 1, 0),
                (hour, minute, second) => (year, month, day, hour, minute, second + 1),
            };
            next == upper.fields()
        }
        _ => false,
    }
}

/// The calendar day after `(year, month, day)`.
fn next_day((year, month, day): (u16, u8, u8)) -> (u16, u8, u8) {
    match crate::temporal::month_length(year, month) {
        Some(length) if day < length => (year, month, day + 1),
        _ if month < 12 => (year, month + 1, 1),
        _ => (year.saturating_add(1), 1, 1),
    }
}

/// What one row construction proves about the rows an entry selects.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Reach {
    /// The construction derives a where column as a literal outside the
    /// entry's values, so none of its rows is selected.
    Never,
    /// The construction derives every where column as a literal among the
    /// entry's values, so every one of its rows is selected.
    Only,
    /// Its rows may or may not be selected.
    Unknown,
}

/// One entry's where clause as written in the typed tree.
pub(crate) fn clause(entry: &ast::ValueLevel) -> Clause<'_> {
    clause_of(&entry.conditions)
}

/// A where clause as the typed tree holds it, a value-level entry's or an
/// analysis dataset's, each column in written order.
pub(crate) fn clause_of(conditions: &[ast::WhereClause]) -> Clause<'_> {
    conditions
        .iter()
        .map(|condition| Term {
            column: condition.column.name.as_str(),
            comparator: condition.comparator,
            values: condition
                .values
                .iter()
                .map(|value| value.text.as_str())
                .collect(),
        })
        .collect()
}

/// A column's value-level entries in the typed tree, in declaration order.
pub(crate) fn levels(column: &ast::Column) -> &[ast::ValueLevel] {
    column
        .submission
        .as_ref()
        .map_or(&[], |submission| submission.value_level.as_slice())
}

/// The row constructions of a spec: each row template, or the one
/// construction from column derivations when it declares none.
fn templates(spec: &ast::Spec) -> Vec<Option<&ast::Template>> {
    if spec.rows.templates.is_empty() {
        vec![None]
    } else {
        spec.rows.templates.iter().map(Some).collect()
    }
}

/// The derivation one construction gives a column: the row template's own,
/// or the column's when the template inherits it.
fn derivation_of(construction: Option<&ast::Template>, column: &ast::Column) -> Option<ExprId> {
    construction
        .and_then(|template| {
            template
                .overrides
                .iter()
                .find(|(name, _)| name.name == column.name.name)
        })
        .map(|(_, derivation)| derivation.expr)
        .or_else(|| column.derivation.as_ref().map(|derivation| derivation.expr))
}

/// What a construction proves about the rows a where clause selects.
fn reach_of(spec: &ast::Spec, construction: Option<&ast::Template>, clause: &[Term<'_>]) -> Reach {
    let mut every = true;
    for term in clause {
        let Some(column) = spec
            .columns
            .iter()
            .find(|column| column.name.name == term.column)
        else {
            every = false;
            continue;
        };
        // A bare literal gives every row the same value. Ordered terms read
        // it after the column's VALUE-3 conversion, using the same term as
        // execution; a conversion that cannot be proved stays unknown.
        let selected =
            derivation_of(construction, column).and_then(|expr| match &spec.expr(expr).kind {
                ExprKind::Literal(literal) if term.comparator.order().is_some() => {
                    let value = crate::conversion::convert(&literal.value()?, column.kind).ok()?;
                    let comparison = where_term(
                        0,
                        term.comparator,
                        term.values
                            .iter()
                            .map(|value| (*value).to_owned())
                            .collect(),
                        column.kind,
                    )?;
                    Some(comparison.selects((&value).into()))
                }
                ExprKind::Literal(Literal::Text(value)) => Some(term.selects(Some(value))),
                ExprKind::Literal(Literal::Missing) => Some(term.selects(None)),
                _ => None,
            });
        match selected {
            Some(true) => {}
            Some(false) => return Reach::Never,
            None => every = false,
        }
    }
    if every {
        Reach::Only
    } else {
        Reach::Unknown
    }
}

/// SUB-13: the derivations of a column that can produce a row the entry
/// selects. A construction that derives a where column as a literal outside
/// the entry's values cannot, and every other construction may.
pub(crate) fn entry_expressions(
    spec: &ast::Spec,
    column: &ast::Column,
    entry: &ast::ValueLevel,
) -> Vec<ExprId> {
    let clause = clause(entry);
    templates(spec)
        .into_iter()
        .filter(|&construction| reach_of(spec, construction, &clause) != Reach::Never)
        .filter_map(|construction| derivation_of(construction, column))
        .collect()
}

/// SUB-19: the derivations the column-level origin answers for. A
/// construction every row of which an entry declaring its own origin selects
/// is answered for by that entry instead.
pub(crate) fn column_expressions(spec: &ast::Spec, column: &ast::Column) -> Vec<ExprId> {
    let owned = levels(column)
        .iter()
        .filter(|entry| !entry.metadata.origins.is_empty())
        .map(clause)
        .collect::<Vec<_>>();
    templates(spec)
        .into_iter()
        .filter(|&construction| {
            !owned
                .iter()
                .any(|clause| reach_of(spec, construction, clause) == Reach::Only)
        })
        .filter_map(|construction| derivation_of(construction, column))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clause<'a>(terms: &[(&'a str, &[&'a str])]) -> Clause<'a> {
        terms
            .iter()
            .map(|(name, values)| Term {
                column: name,
                comparator: Comparator::In,
                values: values.to_vec(),
            })
            .collect()
    }

    /// Whether two clauses without an ordered term can select one row.
    fn listed_overlap(left: &[Term<'_>], right: &[Term<'_>]) -> bool {
        overlap(
            left,
            &vec![None; left.len()],
            right,
            &vec![None; right.len()],
        )
    }

    /// One term over `column` under `comparator`.
    fn compared<'a>(column: &'a str, comparator: Comparator, values: &[&'a str]) -> Clause<'a> {
        vec![Term {
            column,
            comparator,
            values: values.to_vec(),
        }]
    }

    #[test]
    fn an_exclusion_shares_the_values_it_does_not_list_and_the_missing_value() {
        let average = clause(&[("DTYPE", &["AVERAGE"])]);
        let observed = compared("DTYPE", Comparator::Ne, &["AVERAGE"]);
        // `NE [X]` and `[X]` on one column share no value.
        assert!(!listed_overlap(&observed, &average));
        assert!(!listed_overlap(&average, &observed));
        // A list holding another value meets the exclusion.
        let both = clause(&[("DTYPE", &["AVERAGE", "LOCF"])]);
        assert!(listed_overlap(&observed, &both));
        assert!(listed_overlap(&both, &observed));
        // Two exclusions on one column always share the missing value.
        let not_locf = compared("DTYPE", Comparator::Ne, &["LOCF"]);
        assert!(listed_overlap(&observed, &not_locf));
        let neither = compared("DTYPE", Comparator::NotIn, &["AVERAGE", "LOCF"]);
        assert!(listed_overlap(&neither, &observed));
        // NOTIN [X, Y] and [X, Y] share no value.
        assert!(!listed_overlap(&neither, &both));
    }

    #[test]
    fn ne_takes_one_value_and_notin_two_or_more() {
        let declared = |_: &str| {
            Some(Declared {
                output: true,
                kind: ColumnType::Str,
                codelist: None,
            })
        };
        let entries = [
            ("ONE", compared("DTYPE", Comparator::Ne, &["AVERAGE"])),
            ("TWO", compared("PARAMCD", Comparator::NotIn, &["X", "Y"])),
            ("NE2", compared("AVISIT", Comparator::Ne, &["V1", "V2"])),
            ("NOTIN1", compared("PARCAT1", Comparator::NotIn, &["CHEM"])),
            ("EMPTY", compared("PARCAT2", Comparator::Ne, &[])),
        ];
        let fault = |id, column, fault| Finding::Where {
            id,
            column: Some(column),
            fault,
        };
        assert_eq!(
            admit(
                entries.iter().map(|(id, clause)| (*id, clause)),
                "AVAL",
                SubmissionMode::Validated,
                declared
            ),
            [
                fault("NE2", "AVISIT", WhereFault::ValueCount(Comparator::Ne)),
                fault(
                    "NOTIN1",
                    "PARCAT1",
                    WhereFault::ValueCount(Comparator::NotIn)
                ),
                fault("EMPTY", "PARCAT2", WhereFault::NoValues),
            ]
        );
        assert_eq!(
            WhereFault::ValueCount(Comparator::Ne).comparator(),
            Some("NE")
        );
        assert_eq!(
            WhereFault::ValueCount(Comparator::Ne).reason(),
            "value_count"
        );
    }

    #[test]
    fn clauses_overlap_when_one_column_set_contains_the_other_and_shared_values_meet() {
        let paramcd = clause(&[("PARAMCD", &["ALT", "AST"])]);
        for (other, expected) in [
            // Same columns: the values must meet.
            (clause(&[("PARAMCD", &["AST", "BILI"])]), true),
            (clause(&[("PARAMCD", &["BILI"])]), false),
            // A subset: every row the narrower clause selects the wider may.
            (
                clause(&[("PARAMCD", &["ALT"]), ("AVISIT", &["WEEK 2"])]),
                true,
            ),
            (
                clause(&[("PARAMCD", &["BILI"]), ("AVISIT", &["WEEK 2"])]),
                false,
            ),
            // Neither contains the other: the build decides.
            (clause(&[("AVISIT", &["WEEK 2"])]), false),
            (
                clause(&[("PARCAT1", &["CHEM"]), ("AVISIT", &["WEEK 2"])]),
                false,
            ),
        ] {
            assert_eq!(listed_overlap(&paramcd, &other), expected, "{other:?}");
            assert_eq!(listed_overlap(&other, &paramcd), expected, "{other:?}");
        }
    }

    #[test]
    fn static_findings_name_the_entry_column_and_fault() {
        // PARAMCD's codelists admit X and Y; AVISIT binds none.
        let declared = |name: &str| match name {
            "PARAMCD" => Some(Declared {
                output: true,
                kind: ColumnType::Str,
                codelist: Some(BTreeSet::from(["X", "Y"])),
            }),
            "AVISIT" => Some(Declared {
                output: true,
                kind: ColumnType::Str,
                codelist: None,
            }),
            "INTERNAL" => Some(Declared {
                output: false,
                kind: ColumnType::Str,
                codelist: None,
            }),
            "AVISITN" => Some(Declared {
                output: true,
                kind: ColumnType::Int,
                codelist: None,
            }),
            _ => None,
        };
        let entry = |id, clause| Entry {
            id,
            node: 0,
            clause,
        };
        let entries = [
            entry("A", clause(&[("PARAMCD", &["X"])])),
            entry("A", clause(&[("PARAMCD", &["Y"])])),
            entry("B", clause(&[])),
            entry("C", clause(&[("AVAL", &["1"])])),
            entry("D", clause(&[("BOGUS", &["1"])])),
            entry("E", clause(&[("INTERNAL", &["1"])])),
            entry("F", clause(&[("AVISITN", &["1"])])),
            entry("G", clause(&[("AVISIT", &[])])),
            entry("H", clause(&[("AVISIT", &["V1", "V1"])])),
            entry("I", clause(&[("PARAMCD", &["X"]), ("AVISIT", &["V1"])])),
            // Values outside the codelists fail, named in declared order; a
            // repeated value is reported first.
            entry("J", clause(&[("PARAMCD", &["Z", "Y", "W"])])),
            entry("K", clause(&[("PARAMCD", &["Z", "Z"])])),
            // A where column without a codelist admits any value.
            entry("L", clause(&[("AVISIT", &["V2"])])),
        ];
        let fault = |id, column, fault| Finding::Where { id, column, fault };
        assert_eq!(
            admit(
                entries.iter().map(|entry| (entry.id, &entry.clause)),
                "AVAL",
                SubmissionMode::Validated,
                declared
            ),
            [
                Finding::DuplicateId { id: "A" },
                fault("B", None, WhereFault::NoColumns),
                fault("C", Some("AVAL"), WhereFault::Itself),
                fault("D", Some("BOGUS"), WhereFault::Undeclared),
                fault("E", Some("INTERNAL"), WhereFault::NotOutput),
                fault("F", Some("AVISITN"), WhereFault::NotText),
                fault("G", Some("AVISIT"), WhereFault::NoValues),
                fault("H", Some("AVISIT"), WhereFault::DuplicateValue),
                Finding::Overlap {
                    id: "I",
                    other: "A"
                },
                fault(
                    "J",
                    Some("PARAMCD"),
                    WhereFault::NotInCodelist(vec!["Z", "W"])
                ),
                fault("K", Some("PARAMCD"), WhereFault::DuplicateValue),
            ]
        );
    }

    #[test]
    fn ordered_terms_overlap_where_their_ranges_meet() {
        // An internal seam: validation's overlap over converted bounds, which
        // the benchmarks reach only for the one pair each pins.
        let date = |text: &str| Value::Date(text.parse().unwrap());
        let time = |text: &str| Value::Time(text.parse().unwrap());
        let moment = |text: &str| Value::DateTime(text.parse().unwrap());
        let term = |order| Term {
            column: "ADT",
            comparator: Comparator::Ordered(order),
            values: vec!["?"],
        };
        let cases = [
            (
                (Order::Lt, date("2026-03-01")),
                (Order::Ge, date("2026-03-01")),
                false,
                "LT and GE of one day",
            ),
            (
                (Order::Le, date("2026-03-01")),
                (Order::Ge, date("2026-03-01")),
                true,
                "LE and GE share the day",
            ),
            (
                (Order::Lt, date("2026-03-01")),
                (Order::Gt, date("2026-02-28")),
                false,
                "no day lies between consecutive days",
            ),
            (
                (Order::Lt, date("2026-03-02")),
                (Order::Gt, date("2026-02-28")),
                true,
                "one day lies between",
            ),
            (
                (Order::Lt, date("2026-01-01")),
                (Order::Gt, date("2025-12-31")),
                false,
                "consecutive days across a year",
            ),
            (
                (Order::Lt, date("2026-03-01")),
                (Order::Lt, date("2025-01-01")),
                true,
                "two upper bounds",
            ),
            (
                (Order::Ge, date("2026-03-01")),
                (Order::Gt, date("2027-01-01")),
                true,
                "two lower bounds",
            ),
            (
                (Order::Le, date("2026-01-01")),
                (Order::Ge, date("2026-03-01")),
                false,
                "the lower bound above the upper",
            ),
            (
                (Order::Lt, Value::Int(3)),
                (Order::Gt, Value::Int(2)),
                false,
                "no integer lies between 2 and 3",
            ),
            (
                (Order::Lt, Value::float(3.0)),
                (Order::Gt, Value::float(2.0)),
                true,
                "every float between 2 and 3",
            ),
            (
                (Order::Lt, time("10:00:01")),
                (Order::Gt, time("10:00:00")),
                false,
                "no time lies between consecutive seconds",
            ),
            (
                (Order::Lt, time("10:00:02")),
                (Order::Gt, time("10:00:00")),
                true,
                "one second lies between",
            ),
            (
                (Order::Lt, moment("2026-03-01T00:00:00")),
                (Order::Gt, moment("2026-02-28T23:59:59")),
                false,
                "consecutive seconds across a day",
            ),
            (
                (Order::Lt, moment("2026-03-01T10:01:00")),
                (Order::Gt, moment("2026-03-01T10:00:59")),
                false,
                "consecutive seconds across a minute",
            ),
            (
                (Order::Lt, moment("2026-03-01T10:00:02")),
                (Order::Gt, moment("2026-03-01T10:00:00")),
                true,
                "one second lies between datetimes",
            ),
        ];
        for ((left, left_bound), (right, right_bound), expected, label) in cases {
            for (a, a_bound, b, b_bound) in [
                (left, &left_bound, right, &right_bound),
                (right, &right_bound, left, &left_bound),
            ] {
                assert_eq!(
                    overlap(
                        &[term(a)],
                        &[Some(a_bound.clone())],
                        &[term(b)],
                        &[Some(b_bound.clone())]
                    ),
                    expected,
                    "{label}"
                );
            }
        }
    }

    #[test]
    fn ordered_bounds_beyond_type_endpoints_select_no_common_value() {
        // Internal seam: typed half-lines at VALUE-7/13/15/21 endpoints,
        // including inclusive singleton ranges that must stay nonempty.
        let endpoints = [
            (Value::Int(i64::MIN), Value::Int(0), Value::Int(i64::MAX)),
            (
                Value::float(-f64::MAX),
                Value::float(0.0),
                Value::float(f64::MAX),
            ),
            (
                Value::Date("0001-01-01".parse().unwrap()),
                Value::Date("2026-01-01".parse().unwrap()),
                Value::Date("9999-12-31".parse().unwrap()),
            ),
            (
                Value::DateTime("0001-01-01T00:00:00".parse().unwrap()),
                Value::DateTime("2026-01-01T12:00:00".parse().unwrap()),
                Value::DateTime("9999-12-31T23:59:59".parse().unwrap()),
            ),
            (
                Value::Time("00:00:00".parse().unwrap()),
                Value::Time("12:00:00".parse().unwrap()),
                Value::Time("23:59:59".parse().unwrap()),
            ),
        ];
        for (first, middle, last) in endpoints {
            for empty in [(Order::Lt, &first), (Order::Gt, &last)] {
                assert!(!meet(empty, empty), "{empty:?} is empty");
                for nonempty in [
                    (Order::Lt, &middle),
                    (Order::Le, &first),
                    (Order::Gt, &middle),
                    (Order::Ge, &last),
                ] {
                    assert!(!meet(empty, nonempty), "{empty:?} and {nonempty:?}");
                    assert!(!meet(nonempty, empty), "{nonempty:?} and {empty:?}");
                }
            }
            assert!(meet((Order::Le, &first), (Order::Ge, &first)));
            assert!(meet((Order::Ge, &last), (Order::Le, &last)));
        }
    }

    #[test]
    fn ordered_bounds_between_consecutive_binary64_values_do_not_overlap() {
        // Internal seam: binary64 spacing, including both zero signs and
        // the smallest subnormal. Inclusive bounds retain their endpoint.
        for (lower, upper, shared) in [
            (1.0, 1.0000000000000002, false),
            (-1.0000000000000002, -1.0, false),
            (-f64::from_bits(1), 0.0, false),
            (-0.0, f64::from_bits(1), false),
            (1.7976931348623155e308, f64::MAX, false),
            (0.0, f64::from_bits(2), true),
            (-f64::from_bits(2), 0.0, true),
            (1.0, 1.0000000000000004, true),
        ] {
            let (lower, upper) = (Value::float(lower), Value::float(upper));
            assert_eq!(
                meet((Order::Gt, &lower), (Order::Lt, &upper)),
                shared,
                "{lower:?} to {upper:?}"
            );
            assert_eq!(meet((Order::Lt, &upper), (Order::Gt, &lower)), shared);
            assert!(meet((Order::Ge, &lower), (Order::Lt, &upper)));
            assert!(meet((Order::Gt, &lower), (Order::Le, &upper)));
        }
    }

    #[test]
    fn an_empty_unshared_ordered_term_prevents_overlap_in_both_clause_orders() {
        // Internal seam: subset clauses may share only listed columns, so
        // their empty ordered term need not reach the pairwise comparison.
        let broad = clause(&[("FLAG", &["Y"])]);
        for (order, expected) in [(Order::Gt, false), (Order::Ge, true)] {
            let mut narrow = broad.clone();
            narrow.extend(compared(
                "N",
                Comparator::Ordered(order),
                &["9223372036854775807"],
            ));
            let bounds = [None, Some(Value::Int(i64::MAX))];
            assert_eq!(overlap(&narrow, &bounds, &broad, &[None]), expected);
            assert_eq!(overlap(&broad, &[None], &narrow, &bounds), expected);
        }
    }

    #[test]
    fn an_ordered_comparator_takes_one_value_of_a_where_column_that_is_not_text() {
        // An internal seam: the static faults of ordered terms, beside the
        // listed ones `static_findings_name_the_entry_column_and_fault` pins.
        let declared = |name: &str| {
            Some(Declared {
                output: true,
                kind: match name {
                    "PARAMCD" => ColumnType::Str,
                    "AVISITN" => ColumnType::Int,
                    _ => ColumnType::Date,
                },
                codelist: None,
            })
        };
        let ordered = |column, order, values: &[&'static str]| {
            compared(column, Comparator::Ordered(order), values)
        };
        let entries = [
            ("TEXT", ordered("PARAMCD", Order::Lt, &["M"])),
            (
                "TWO",
                ordered("ADT", Order::Ge, &["2026-01-01", "2026-02-01"]),
            ),
            ("WORDS", ordered("ADT", Order::Ge, &["March 2026"])),
            ("FRACTION", ordered("AVISITN", Order::Gt, &["2.5"])),
            ("EARLY", ordered("ADT", Order::Lt, &["2026-03-01"])),
            ("LATE", ordered("ADT", Order::Ge, &["2026-03-01"])),
            ("LATER", ordered("ADT", Order::Gt, &["2026-06-30"])),
        ];
        let fault = |id, column, fault| Finding::Where {
            id,
            column: Some(column),
            fault,
        };
        assert_eq!(
            admit(
                entries.iter().map(|(id, clause)| (*id, clause)),
                "AVAL",
                SubmissionMode::Validated,
                declared
            ),
            [
                fault("TEXT", "PARAMCD", WhereFault::OrderedText),
                fault(
                    "TWO",
                    "ADT",
                    WhereFault::ValueCount(Comparator::Ordered(Order::Ge))
                ),
                fault("WORDS", "ADT", WhereFault::ValueType),
                fault("FRACTION", "AVISITN", WhereFault::ValueType),
                Finding::Overlap {
                    id: "LATER",
                    other: "LATE"
                },
            ]
        );
        assert_eq!(
            [
                WhereFault::OrderedText.reason(),
                WhereFault::ValueType.reason(),
                WhereFault::ValueCount(Comparator::Ordered(Order::Ge))
                    .comparator()
                    .unwrap(),
            ],
            ["ordered_str", "value_type", "GE"]
        );
        // Historical composition still refuses an ordered term it could not
        // compare by, and asserts no overlap.
        assert_eq!(
            admit(
                entries.iter().map(|(id, clause)| (*id, clause)),
                "AVAL",
                SubmissionMode::Historical,
                declared
            ),
            [
                fault("TEXT", "PARAMCD", WhereFault::OrderedText),
                fault(
                    "TWO",
                    "ADT",
                    WhereFault::ValueCount(Comparator::Ordered(Order::Ge))
                ),
                fault("WORDS", "ADT", WhereFault::ValueType),
                fault("FRACTION", "AVISITN", WhereFault::ValueType),
            ]
        );
    }

    #[test]
    fn an_analysis_clause_lists_values_of_any_type_and_names_no_column_of_its_own() {
        // An internal seam: the one checker read as each scope reads it, over
        // the same clauses, which no composition shows side by side (SUB-12,
        // DEFINE-32).
        let declared = |name: &str| {
            Some(Declared {
                output: true,
                kind: match name {
                    "AVISITN" => ColumnType::Int,
                    _ => ColumnType::Str,
                },
                codelist: (name == "PARAMCD").then(|| ["GLUC"].into()),
            })
        };
        let entries = [
            ("OWN", clause(&[("AVAL", &["1"])])),
            ("LISTED", clause(&[("AVISITN", &["24"])])),
            ("WORDS", clause(&[("AVISITN", &["Week 24"])])),
            ("CODED", clause(&[("PARAMCD", &["HGB"])])),
            ("FIRST", clause(&[("PARAMCD", &["GLUC"])])),
            ("AGAIN", clause(&[("PARAMCD", &["GLUC"])])),
        ];
        let fault = |id, column, fault| Finding::Where {
            id,
            column: Some(column),
            fault,
        };
        let uncoded = || fault("CODED", "PARAMCD", WhereFault::NotInCodelist(vec!["HGB"]));
        for (scope, expected) in [
            (
                Scope::Validated,
                vec![
                    fault("OWN", "AVAL", WhereFault::Itself),
                    fault("LISTED", "AVISITN", WhereFault::NotText),
                    fault("WORDS", "AVISITN", WhereFault::NotText),
                    uncoded(),
                    Finding::Overlap {
                        id: "AGAIN",
                        other: "FIRST",
                    },
                ],
            ),
            (
                Scope::Analysis,
                vec![fault("WORDS", "AVISITN", WhereFault::ValueType), uncoded()],
            ),
            (Scope::Historical, Vec::new()),
        ] {
            assert_eq!(
                admit(
                    entries.iter().map(|(id, clause)| (*id, clause)),
                    "AVAL",
                    scope,
                    declared
                ),
                expected,
                "{scope:?}"
            );
        }
    }

    #[test]
    fn an_ordered_term_selects_present_values_on_its_side_of_the_bound() {
        // An internal seam: the executable term each row check, composition
        // and the transport share, including the missing value no benchmark
        // row can show apart from an unselected one.
        use crate::table::ValueRef;
        let date = |text: &str| text.parse().unwrap();
        let term = |comparator| {
            where_term(0, comparator, vec!["2026-03-01".into()], ColumnType::Date).unwrap()
        };
        let rows = [
            ValueRef::Date(date("2026-02-28")),
            ValueRef::Date(date("2026-03-01")),
            ValueRef::Date(date("2026-03-02")),
            ValueRef::Missing,
        ];
        for (order, expected) in [
            (Order::Lt, [true, false, false, false]),
            (Order::Le, [true, true, false, false]),
            (Order::Gt, [false, false, true, false]),
            (Order::Ge, [false, true, true, false]),
        ] {
            let term = term(Comparator::Ordered(order));
            assert_eq!(rows.map(|row| term.selects(row)), expected, "{order:?}");
            assert_eq!(term.comparator(), order.name(), "{order:?}");
        }
        // A value that does not convert, or an ordered str column, builds no
        // term; admission has refused both.
        assert_eq!(
            where_term(
                0,
                Comparator::Ordered(Order::Lt),
                vec!["soon".into()],
                ColumnType::Date
            ),
            None
        );
        assert_eq!(
            where_term(
                0,
                Comparator::Ordered(Order::Lt),
                vec!["M".into()],
                ColumnType::Str
            ),
            None
        );
    }
}
