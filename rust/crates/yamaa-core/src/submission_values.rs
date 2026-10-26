//! Value-level entries keyed by a declared where clause (SUB-12, SUB-13): the
//! entries a column declares, the static checks that need no data, and what
//! each row construction proves about the rows an entry selects. Shared by the
//! domain build, which reads the typed tree, and by composition, which still
//! reads each specification's document; no study, host or data access.
use crate::ast::{self, CodelistType, Comparator, ExprId, ExprKind, Literal};
use crate::project_terminology::Catalogue;
use crate::schema::{Document, DocumentNode as N};
use crate::specification_submission_document::SubmissionMode;
use crate::value::Value;
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
    /// Whether the term selects a row whose column holds `value`, `None`
    /// when it is missing: one of the values, or under `NE` and `NOTIN` none
    /// of them, a missing value included.
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
    /// The clause names a column whose declared type is not `str`.
    NotText,
    /// A clause column lists no value.
    NoValues,
    /// A clause column lists a number of values its comparator does not
    /// take: `NE` takes one and `NOTIN` two or more.
    ValueCount(Comparator),
    /// A clause column lists one value twice.
    DuplicateValue,
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
            Self::NoValues => "no_values",
            Self::ValueCount(_) => "value_count",
            Self::DuplicateValue => "duplicate_value",
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
/// carries it, whether its declared type is `str`, and the values its
/// codelists admit when they govern every one of its rows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Declared<'c> {
    pub(crate) output: bool,
    pub(crate) text: bool,
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

/// The static checks of SUB-12 over one column's entries: identifiers unique
/// within the column, where clauses over declared `str` output columns other
/// than the column itself with nonempty distinct values their codelists
/// admit, and no two entries whose where clauses can select one row. Historical
/// composition keeps structural/name checks without asserting current rows.
pub(crate) fn admit<'a, 'b, 'c>(
    entries: impl IntoIterator<Item = (&'a str, &'b Clause<'a>)>,
    column: &str,
    mode: SubmissionMode,
    declared: impl Fn(&str) -> Option<Declared<'c>>,
) -> Vec<Finding<'a>>
where
    'a: 'b,
{
    let mut findings = Vec::new();
    let mut ids = BTreeSet::new();
    let mut valid = Vec::<(&'a str, &'b Clause<'a>)>::new();
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
        for Term {
            column: name,
            comparator,
            values,
        } in clause
        {
            let mut codelist = None;
            let fault = if *name == column {
                Some(WhereFault::Itself)
            } else {
                match declared(name) {
                    None => Some(WhereFault::Undeclared),
                    Some(Declared { output: false, .. }) => Some(WhereFault::NotOutput),
                    Some(Declared { text: false, .. }) if mode == SubmissionMode::Validated => {
                        Some(WhereFault::NotText)
                    }
                    Some(declared) => {
                        if mode == SubmissionMode::Validated {
                            codelist = declared.codelist;
                        }
                        None
                    }
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
            });
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
        if mode == SubmissionMode::Validated {
            if let Some(&(other, _)) = valid.iter().find(|(_, other)| overlap(other, clause)) {
                findings.push(Finding::Overlap { id, other });
            }
        }
        valid.push((id, clause));
    }
    findings
}

/// Whether two where clauses can select one row, as validation decides it:
/// one clause's columns are all columns of the other, and every column they
/// share admits a common value. Two clauses over columns neither of which
/// contains the other are left to the build, which fails a row both select.
pub(crate) fn overlap(left: &[Term<'_>], right: &[Term<'_>]) -> bool {
    fn columns<'a>(clause: &[Term<'a>]) -> BTreeSet<&'a str> {
        clause.iter().map(|term| term.column).collect()
    }
    let (a, b) = (columns(left), columns(right));
    (a.is_subset(&b) || b.is_subset(&a))
        && left.iter().all(
            |term| match right.iter().find(|other| other.column == term.column) {
                Some(other) => share(term, other),
                None => true,
            },
        )
}

/// Whether two terms over one column admit a common value: two lists that
/// meet, a list holding a value an exclusion does not list, and any two
/// exclusions, which both select the missing value (SUB-12).
fn share(left: &Term<'_>, right: &Term<'_>) -> bool {
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
    entry
        .conditions
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
        // A bare `literal` gives every row the same value, which an `NE` or
        // `NOTIN` term selects when it is missing.
        let literal =
            derivation_of(construction, column).and_then(|expr| match &spec.expr(expr).kind {
                ExprKind::Literal(Literal::Text(value)) => Some(Some(value.as_str())),
                ExprKind::Literal(Literal::Missing) => Some(None),
                _ => None,
            });
        match literal {
            Some(value) if term.selects(value) => {}
            Some(_) => return Reach::Never,
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
        assert!(!overlap(&observed, &average));
        assert!(!overlap(&average, &observed));
        // A list holding another value meets the exclusion.
        let both = clause(&[("DTYPE", &["AVERAGE", "LOCF"])]);
        assert!(overlap(&observed, &both));
        assert!(overlap(&both, &observed));
        // Two exclusions on one column always share the missing value.
        let not_locf = compared("DTYPE", Comparator::Ne, &["LOCF"]);
        assert!(overlap(&observed, &not_locf));
        let neither = compared("DTYPE", Comparator::NotIn, &["AVERAGE", "LOCF"]);
        assert!(overlap(&neither, &observed));
        // NOTIN [X, Y] and [X, Y] share no value.
        assert!(!overlap(&neither, &both));
    }

    #[test]
    fn ne_takes_one_value_and_notin_two_or_more() {
        let declared = |_: &str| {
            Some(Declared {
                output: true,
                text: true,
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
            assert_eq!(overlap(&paramcd, &other), expected, "{other:?}");
            assert_eq!(overlap(&other, &paramcd), expected, "{other:?}");
        }
    }

    #[test]
    fn static_findings_name_the_entry_column_and_fault() {
        // PARAMCD's codelists admit X and Y; AVISIT binds none.
        let declared = |name: &str| match name {
            "PARAMCD" => Some(Declared {
                output: true,
                text: true,
                codelist: Some(BTreeSet::from(["X", "Y"])),
            }),
            "AVISIT" => Some(Declared {
                output: true,
                text: true,
                codelist: None,
            }),
            "INTERNAL" => Some(Declared {
                output: false,
                text: true,
                codelist: None,
            }),
            "AVISITN" => Some(Declared {
                output: true,
                text: false,
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
}
