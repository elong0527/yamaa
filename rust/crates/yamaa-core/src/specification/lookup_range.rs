//! A lookup's `between` range (LOOKUP-9), for every kind of lookup: a named
//! intermediate, a qualified aggregate and an implicit join's structured
//! `source`. The bounds are fields of the lookup's relation; the value is a
//! value of the current row: an output column, a field of the record of the
//! row's driver, or a column a named intermediate selects for the row.
use super::*;
use crate::{
    dataset::{
        CurrentField, CurrentRead, Intermediate, RelationRange, RelationRangeBound,
        RelationRangeError, SecondarySource,
    },
    diagnostic::{
        lookup::{self, Owner},
        Diagnostic,
    },
    value::ValueType,
};

#[derive(Clone, Debug)]
pub(super) struct Declaration {
    pub value: String,
    lower: Option<String>,
    upper: Option<String>,
    path: String,
}

/// The stated bounds, bound to fields of the lookup's relation.
pub(super) type Bounds = (Option<RelationRangeBound>, Option<RelationRangeBound>);

impl Declaration {
    /// Read the `between` field of a lookup declaration, written at `path`.
    pub(super) fn prepare(
        d: &Document,
        node: usize,
        path: &str,
    ) -> Result<Option<Self>, PrepareError> {
        let Some(node) = d
            .field(node, "between")
            .filter(|&node| !matches!(d.nodes()[node], N::Null))
        else {
            return Ok(None);
        };
        let bound = |name| {
            d.field(node, name)
                .filter(|&node| !matches!(d.nodes()[node], N::Null))
                .map(|node| text(d, node).map(String::from))
                .transpose()
        };
        Ok(Some(Self {
            value: text(d, field(d, node, "value")?)?.into(),
            lower: bound("lower")?,
            upper: bound("upper")?,
            path: format!("{path}.between"),
        }))
    }
    pub(super) fn path(&self) -> &str {
        &self.path
    }
    pub(super) fn donor_fields(&self) -> impl Iterator<Item = &str> {
        self.lower
            .iter()
            .chain(self.upper.iter())
            .map(String::as_str)
    }
    /// A range states at least one bound (LOOKUP-9).
    pub(super) fn bounds_finding(&self, owner: Owner<'_>) -> Option<Diagnostic> {
        (self.lower.is_none() && self.upper.is_none())
            .then(|| lookup::range_declaration(&self.path, owner))
    }
    /// Bind each stated bound to a field of the relation the lookup reads. A
    /// bound naming no admitted field fails, after every bound is tried.
    pub(super) fn bind_bounds(
        &self,
        owner: Owner<'_>,
        donor: &TableSchema,
        donor_kind: &dyn Fn(usize) -> Option<ColumnType>,
        admits: &dyn Fn(usize) -> bool,
        findings: &mut Vec<Diagnostic>,
    ) -> Option<Bounds> {
        let before = findings.len();
        let mut bound = |name: &Option<String>| {
            let name = name.as_ref()?;
            let found = donor
                .columns()
                .iter()
                .enumerate()
                .find(|(column, field)| field.name == *name && admits(*column))
                .map(|(column, field)| RelationRangeBound {
                    column,
                    identifier: field.name.clone(),
                    kind: donor_kind(column).map(Into::into),
                });
            if found.is_none() {
                findings.push(lookup::range_reference(&self.path, owner, name));
            }
            found
        };
        let lower = bound(&self.lower);
        let upper = bound(&self.upper);
        (findings.len() == before).then_some((lower, upper))
    }
    /// The value as a read of the current row: an output column, a field of
    /// an input's record at that input's record port, or a column a named
    /// intermediate selects. Whether the input drives every row that reads
    /// the lookup is admitted later, per context ([`admit`]).
    pub(super) fn current_value(
        &self,
        catalog: &Catalog,
        secondary: &[SecondarySource],
        intermediates: &[Option<Intermediate>],
        available: Option<&[usize]>,
        edges: &mut Vec<usize>,
    ) -> Result<Option<(CurrentRead, Option<ValueType>)>, BindError> {
        let name = self.value.as_str();
        let Some((qualifier, field)) = name.split_once('.') else {
            return Ok(match catalog.bind(name) {
                Some(reference_binding::Binding::Output {
                    column,
                    column_type,
                }) if available.is_none_or(|available| available.contains(&column)) => {
                    edges.push(column);
                    Some((CurrentRead::Output(column), Some(column_type.into())))
                }
                _ => None,
            });
        };
        if let Some((index, input)) = secondary
            .iter()
            .enumerate()
            .find(|(_, source)| source.name == qualifier)
        {
            return Ok(input
                .schema
                .columns()
                .iter()
                .position(|column| column.name == field)
                .map(|column| {
                    (
                        CurrentRead::Record {
                            source: index + 1,
                            column,
                        },
                        Some(input.schema.columns()[column].kind.into()),
                    )
                }));
        }
        if let Some((index, item)) = intermediates.iter().enumerate().find_map(|(index, item)| {
            item.as_ref()
                .filter(|item| item.identifier == qualifier)
                .map(|item| (index, item))
        }) {
            let Some(column) = item.source_column(secondary, field) else {
                return Ok(None);
            };
            edges.extend(row_intermediates::dependencies(index, intermediates)?);
            return Ok(Some((
                CurrentRead::Intermediate { index, column },
                item.source_kind(secondary, column).map(Into::into),
            )));
        }
        // The catalog's own dataset is the primary input, record port 0.
        Ok(match catalog.bind(name) {
            Some(reference_binding::Binding::Dataset {
                dataset: 0,
                field,
                column_type,
            }) => Some((
                CurrentRead::Record {
                    source: 0,
                    column: field,
                },
                Some(column_type.into()),
            )),
            _ => None,
        })
    }
    /// Admit the bound range, or report a value or bound whose types cannot
    /// compare (LOOKUP-9).
    pub(super) fn finish(
        &self,
        read: CurrentRead,
        kind: Option<ValueType>,
        (lower, upper): Bounds,
        intermediate: Option<&str>,
    ) -> Result<Result<RelationRange, Diagnostic>, BindError> {
        match RelationRange::new(
            self.path.clone(),
            CurrentField {
                read,
                identifier: self.value.clone(),
            },
            kind,
            lower,
            upper,
        ) {
            Ok(range) => Ok(Ok(range)),
            Err(RelationRangeError::Incomparable(types)) => {
                Ok(Err(types.diagnostic_for(&self.path, intermediate)))
            }
            Err(_) => Err(BindError::Internal),
        }
    }
    /// A value that names nothing the current row holds.
    pub(super) fn unknown_value(&self, owner: Owner<'_>) -> Diagnostic {
        lookup::range_reference(&format!("{}.value", self.path), owner, &self.value)
    }
}

/// A range value that reads an input's record must name the driver of every
/// row that reads the lookup (LOOKUP-9). Each context pairs the expressions a
/// row evaluates with the record ports of the drivers of those rows. A named
/// intermediate's range is admitted wherever the intermediate is read, through
/// a chain of range values too. Each refused range is reported once.
pub(super) fn admit<'a>(
    contexts: impl IntoIterator<Item = (Vec<&'a Expression>, Vec<usize>)>,
    intermediates: &[Option<Intermediate>],
    findings: &mut Vec<BindFinding>,
) {
    let mut seen = std::collections::BTreeSet::new();
    let mut refuse = |range: &RelationRange,
                      drivers: &[usize],
                      owner: Owner<'_>,
                      findings: &mut Vec<BindFinding>| {
        if matches!(range.current().read, CurrentRead::Record { source, .. }
            if drivers.iter().any(|&driver| driver != source))
            && seen.insert(range.path().to_string())
        {
            findings.push(BindFinding::Scalar(lookup::range_reference(
                &format!("{}.value", range.path()),
                owner,
                &range.current().identifier,
            )));
        }
    };
    for (expressions, drivers) in contexts {
        for expression in expressions {
            crate::dataset::relation_predicates::visit_ranges(expression, &mut |range| {
                refuse(range, &drivers, Owner::Unnamed, findings);
            });
            let mut named = Vec::new();
            crate::dataset::relation_predicates::visit(expression, &mut |index| named.push(index));
            let mut visited = std::collections::BTreeSet::new();
            while let Some(index) = named.pop() {
                if !visited.insert(index) {
                    continue;
                }
                let Some(item) = intermediates.get(index).and_then(Option::as_ref) else {
                    continue;
                };
                let Some(range) = &item.range else {
                    continue;
                };
                refuse(
                    range,
                    &drivers,
                    Owner::Intermediate(&item.identifier),
                    findings,
                );
                if let CurrentRead::Intermediate { index, .. } = range.current().read {
                    named.push(index);
                }
            }
        }
    }
}
