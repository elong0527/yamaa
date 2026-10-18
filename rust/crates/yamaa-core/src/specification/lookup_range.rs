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

/// A range whose value reads the record of an input that does not drive every
/// row in `drivers` names no value of those rows (LOOKUP-9).
fn refusal(range: &RelationRange, drivers: &[usize], owner: Owner<'_>) -> Option<Diagnostic> {
    matches!(range.current().read, CurrentRead::Record { source, .. }
        if drivers.iter().any(|&driver| driver != source))
    .then(|| {
        lookup::range_reference(
            &format!("{}.value", range.path()),
            owner,
            &range.current().identifier,
        )
    })
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
        if let Some(diagnostic) = refusal(range, drivers, owner) {
            if seen.insert(range.path().to_string()) {
                findings.push(BindFinding::Scalar(diagnostic));
            }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{diagnostic::ConditionCode as C, json::Value as JsonValue};

    fn schema(names: &[&str]) -> TableSchema {
        TableSchema::new(
            names
                .iter()
                .map(|&name| Column {
                    name: name.into(),
                    kind: ColumnType::Int,
                })
                .collect(),
        )
        .unwrap()
    }
    fn fields(schema: &TableSchema) -> Vec<reference_binding::Field<'_>> {
        schema
            .columns()
            .iter()
            .map(|column| reference_binding::Field {
                name: &column.name,
                column_type: column.kind,
            })
            .collect()
    }

    /// The driver is the second input, so source order differs from port
    /// order, and the driver holds another field at the named column.
    #[test]
    fn a_qualified_value_keeps_its_input_port_and_admits_only_that_driver() {
        let other = schema(&["ID", "REFDAY"]);
        let driver = schema(&["ID", "DAY"]);
        let donor = schema(&["ID", "LO", "HI"]);
        let (other_fields, driver_fields, donor_fields) =
            (fields(&other), fields(&driver), fields(&donor));
        // Intermediates bind against the driver first, then the other inputs
        // in source order, each at its own record port.
        let catalog = Catalog::compile(
            &[],
            &[
                reference_binding::Dataset {
                    name: "DRIVER",
                    fields: &driver_fields,
                },
                reference_binding::Dataset {
                    name: "OTHER",
                    fields: &other_fields,
                },
                reference_binding::Dataset {
                    name: "DONOR",
                    fields: &donor_fields,
                },
            ],
        )
        .unwrap();
        let secondary = [
            SecondarySource {
                name: "OTHER".into(),
                schema: other.clone(),
            },
            SecondarySource {
                name: "DONOR".into(),
                schema: donor.clone(),
            },
        ];
        let declaration = Declaration {
            value: "OTHER.REFDAY".into(),
            lower: Some("LO".into()),
            upper: Some("HI".into()),
            path: "intermediates[0].between".into(),
        };
        let mut findings = Vec::new();
        let range = intermediate_range::bind(
            &declaration,
            "W",
            &donor,
            &|column| donor.columns().get(column).map(|column| column.kind),
            &|_| true,
            &catalog,
            &driver,
            &[],
            &secondary,
            &mut findings,
        )
        .unwrap()
        .unwrap();
        assert!(findings.is_empty());
        assert_eq!(
            range.current().read,
            CurrentRead::Record {
                source: 1,
                column: 1
            }
        );
        let owner = Owner::Intermediate("W");
        assert_eq!(refusal(&range, &[1], owner), None);
        let diagnostic = refusal(&range, &[1, 0], owner).expect("another driver refuses");
        assert_eq!(diagnostic.code, C::LookupRangeReference);
        assert_eq!(diagnostic.spec_paths, ["intermediates[0].between.value"]);
        assert_eq!(
            diagnostic.context,
            BTreeMap::from([
                (
                    "identifier".into(),
                    JsonValue::String("OTHER.REFDAY".into())
                ),
                ("intermediate".into(), JsonValue::String("W".into())),
            ])
        );
    }
}
