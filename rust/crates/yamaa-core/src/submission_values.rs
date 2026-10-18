//! Value-level entries keyed by a declared where clause (SUB-12, SUB-13): the
//! entries a column declares, the static checks that need no data, and what
//! each row construction proves about the rows an entry selects. Shared by the
//! domain build and by composition; no study, host or data access.
use crate::schema::{Document, DocumentNode as N};
use std::collections::{BTreeMap, BTreeSet};

/// One where clause as written: each column and the values it admits.
pub(crate) type Clause<'a> = Vec<(&'a str, Vec<&'a str>)>;

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

impl Entry<'_> {
    /// Whether the entry itself declares the metadata field.
    pub(crate) fn declares(&self, d: &Document, field: &str) -> bool {
        present(d, self.node, field).is_some()
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
                    Some((
                        text(d, column)?,
                        items(d, Some(values))
                            .iter()
                            .map(|&value| text(d, value))
                            .collect::<Option<Vec<_>>>()?,
                    ))
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
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum WhereFault {
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
    /// A clause column lists one value twice.
    DuplicateValue,
}
impl WhereFault {
    pub(crate) fn reason(self) -> &'static str {
        match self {
            Self::NoColumns => "no_columns",
            Self::Itself => "self",
            Self::Undeclared => "undeclared",
            Self::NotOutput => "not_output",
            Self::NotText => "not_str",
            Self::NoValues => "no_values",
            Self::DuplicateValue => "duplicate_value",
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
        fault: WhereFault,
    },
    /// The where clauses of `other`, declared first, and `id` can select one row.
    Overlap { id: &'a str, other: &'a str },
}

/// A declared column a where clause may name: whether `output.columns`
/// carries it and whether its declared type is `str`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Declared {
    pub(crate) output: bool,
    pub(crate) text: bool,
}

/// The static checks of SUB-12 over one column's entries: identifiers unique
/// within the column, where clauses over declared `str` output columns other
/// than the column itself with nonempty distinct values, and no two entries
/// whose where clauses can select one row.
pub(crate) fn admit<'a>(
    entries: &[Entry<'a>],
    column: &str,
    declared: impl Fn(&str) -> Option<Declared>,
) -> Vec<Finding<'a>> {
    let mut findings = Vec::new();
    let mut ids = BTreeSet::new();
    let mut valid = Vec::new();
    for entry in entries {
        if !ids.insert(entry.id) {
            findings.push(Finding::DuplicateId { id: entry.id });
        }
        let before = findings.len();
        if entry.clause.is_empty() {
            findings.push(Finding::Where {
                id: entry.id,
                column: None,
                fault: WhereFault::NoColumns,
            });
        }
        for (name, values) in &entry.clause {
            let fault = if *name == column {
                Some(WhereFault::Itself)
            } else {
                match declared(name) {
                    None => Some(WhereFault::Undeclared),
                    Some(Declared { output: false, .. }) => Some(WhereFault::NotOutput),
                    Some(Declared { text: false, .. }) => Some(WhereFault::NotText),
                    Some(_) => None,
                }
            }
            .or_else(|| values.is_empty().then_some(WhereFault::NoValues))
            .or_else(|| {
                values
                    .iter()
                    .enumerate()
                    .any(|(index, value)| values[..index].contains(value))
                    .then_some(WhereFault::DuplicateValue)
            });
            if let Some(fault) = fault {
                findings.push(Finding::Where {
                    id: entry.id,
                    column: Some(name),
                    fault,
                });
            }
        }
        if findings.len() != before {
            continue;
        }
        if let Some(other) = valid
            .iter()
            .find(|other: &&&Entry<'a>| overlap(&other.clause, &entry.clause))
        {
            findings.push(Finding::Overlap {
                id: entry.id,
                other: other.id,
            });
        }
        valid.push(entry);
    }
    findings
}

/// Whether two where clauses can select one row, as validation decides it:
/// one clause's columns are all columns of the other, and every column they
/// share admits a common value. Two clauses over columns neither of which
/// contains the other are left to the build, which fails a row both select.
pub(crate) fn overlap(left: &[(&str, Vec<&str>)], right: &[(&str, Vec<&str>)]) -> bool {
    fn columns<'a>(clause: &[(&'a str, Vec<&'a str>)]) -> BTreeSet<&'a str> {
        clause.iter().map(|(name, _)| *name).collect()
    }
    let (a, b) = (columns(left), columns(right));
    (a.is_subset(&b) || b.is_subset(&a))
        && left.iter().all(
            |(name, values)| match right.iter().find(|(other, _)| other == name) {
                Some((_, others)) => values.iter().any(|value| others.contains(value)),
                None => true,
            },
        )
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

/// The row constructions of a specification: each row template, or the one
/// construction from column derivations when it declares none.
pub(crate) fn constructions(d: &Document) -> Vec<Option<usize>> {
    let rows = items(d, present(d, d.root(), "rows"));
    if rows.is_empty() {
        vec![None]
    } else {
        rows.iter().copied().map(Some).collect()
    }
}

/// The derivation one construction gives a column: the row template's own,
/// or the column's when the template inherits it.
pub(crate) fn derivation(
    d: &Document,
    construction: Option<usize>,
    column: usize,
    name: &str,
) -> Option<usize> {
    construction
        .and_then(|row| present(d, row, "derivations"))
        .and_then(|derivations| present(d, derivations, name))
        .or_else(|| present(d, column, "derivation"))
}

/// The value a derivation gives every row when it is a bare `literal`:
/// `Some(Some(text))` for a string and `Some(None)` for missing.
fn literal(d: &Document, derivation: usize) -> Option<Option<&str>> {
    let node = d.field(derivation, "value").unwrap_or(derivation);
    let N::Mapping(fields) = &d.nodes()[node] else {
        return None;
    };
    let [(key, value)] = fields.as_slice() else {
        return None;
    };
    if text(d, *key) != Some("literal") {
        return None;
    }
    match &d.nodes()[*value] {
        N::Text(value) => Some(Some(value)),
        N::Null => Some(None),
        _ => None,
    }
}

/// What a construction proves about an entry's rows. `columns` maps each
/// declared column name to its declaration node.
pub(crate) fn reach(
    d: &Document,
    construction: Option<usize>,
    columns: &BTreeMap<&str, usize>,
    clause: &[(&str, Vec<&str>)],
) -> Reach {
    let mut every = true;
    for (name, values) in clause {
        let Some(&column) = columns.get(name) else {
            every = false;
            continue;
        };
        match derivation(d, construction, column, name).and_then(|node| literal(d, node)) {
            Some(Some(value)) if values.contains(&value) => {}
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
pub(crate) fn entry_derivations(
    d: &Document,
    column: usize,
    name: &str,
    columns: &BTreeMap<&str, usize>,
    entry: &Entry<'_>,
) -> Vec<usize> {
    constructions(d)
        .into_iter()
        .filter(|&construction| reach(d, construction, columns, &entry.clause) != Reach::Never)
        .filter_map(|construction| derivation(d, construction, column, name))
        .collect()
}

/// SUB-19: the derivations the column-level origin answers for. A
/// construction every row of which an entry declaring its own origin selects
/// is answered for by that entry instead.
pub(crate) fn column_derivations(
    d: &Document,
    column: usize,
    name: &str,
    columns: &BTreeMap<&str, usize>,
    entries: &[Entry<'_>],
) -> Vec<usize> {
    constructions(d)
        .into_iter()
        .filter(|&construction| {
            !entries.iter().any(|entry| {
                entry.declares(d, "origin")
                    && reach(d, construction, columns, &entry.clause) == Reach::Only
            })
        })
        .filter_map(|construction| derivation(d, construction, column, name))
        .collect()
}

/// Every declared column name and its declaration node.
pub(crate) fn declared_columns(d: &Document) -> BTreeMap<&str, usize> {
    items(d, present(d, d.root(), "columns"))
        .iter()
        .filter_map(|&node| Some((text(d, present(d, node, "name")?)?, node)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clause<'a>(terms: &[(&'a str, &[&'a str])]) -> Clause<'a> {
        terms
            .iter()
            .map(|(name, values)| (*name, values.to_vec()))
            .collect()
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
        let declared = |name: &str| match name {
            "PARAMCD" | "AVISIT" => Some(Declared {
                output: true,
                text: true,
            }),
            "INTERNAL" => Some(Declared {
                output: false,
                text: true,
            }),
            "AVISITN" => Some(Declared {
                output: true,
                text: false,
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
        ];
        let fault = |id, column, fault| Finding::Where { id, column, fault };
        assert_eq!(
            admit(&entries, "AVAL", declared),
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
            ]
        );
    }
}
