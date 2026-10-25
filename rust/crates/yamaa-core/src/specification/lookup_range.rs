//! A lookup's `between` range (LOOKUP-9), for every kind of lookup: a named
//! intermediate, a qualified aggregate and an implicit join's structured
//! `source`. The bounds are fields of the lookup's relation; the value is a
//! value of the current row: an output column, a field of the record of the
//! row's driver, or a column a named intermediate selects for the row.
use super::*;
use crate::{
    dataset::{
        CurrentField, CurrentRead, Intermediate, IntermediateSource, RelationRange,
        RelationRangeBound, RelationRangeError, SecondarySource,
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
    /// A lookup's range from the tree.
    pub(super) fn from_tree(
        spec: &ast::Spec,
        between: &ast::Between,
    ) -> Result<Self, PrepareError> {
        let name = |e| checks::name(spec, e).map(String::from);
        Ok(Self {
            value: name(between.value).ok_or(PrepareError::Internal)?,
            lower: between.lower.and_then(name),
            upper: between.upper.and_then(name),
            path: checks::render(spec, between.meta),
        })
    }
    pub(super) fn path(&self) -> &str {
        &self.path
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
    /// intermediate selects. A value qualified by `driver`, the named
    /// intermediate whose records build the row, reads the record that built
    /// the row at its input's record port, as the driver's qualifier always
    /// does (LOOKUP-1, LOOKUP-2), never a selection of the intermediate.
    /// Whether the input drives every row that reads the lookup is admitted
    /// later, per context ([`admit`]).
    pub(super) fn current_value(
        &self,
        catalog: &Catalog,
        donors: binder::Donors<'_>,
        driver: Option<&str>,
        available: Option<&[usize]>,
        edges: &mut Vec<usize>,
    ) -> Result<Option<(CurrentRead, Option<ValueType>)>, BindError> {
        let binder::Donors {
            secondary,
            items: intermediates,
            inputs,
            ..
        } = donors;
        let name = self.value.as_str();
        let Some((qualifier, field)) = name.split_once('.') else {
            return Ok(match catalog.bind(name) {
                Some(reference_binding::Binding::Output {
                    column,
                    column_type,
                }) if available.is_none_or(|available| available.contains(&column)) => {
                    edges.push(column);
                    Some((CurrentRead::Output(column), column_type.map(Into::into)))
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
                        inputs
                            .kind_of(&input.name, field, input.schema.columns()[column].kind)
                            .map(Into::into),
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
            if driver == Some(qualifier) {
                return Ok(stored_record(item, column).map(|source| {
                    (
                        CurrentRead::Record { source, column },
                        inputs.source_kind(item, secondary, column).map(Into::into),
                    )
                }));
            }
            edges.extend(row_intermediates::dependencies(index, intermediates)?);
            return Ok(Some((
                CurrentRead::Intermediate { index, column },
                inputs.source_kind(item, secondary, column).map(Into::into),
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
                column_type.map(Into::into),
            )),
            _ => None,
        })
    }
    /// Admit the bound range, or report a value or bound whose types cannot
    /// compare (LOOKUP-9), naming the lookup that declares the range.
    pub(super) fn finish(
        &self,
        read: CurrentRead,
        kind: Option<ValueType>,
        (lower, upper): Bounds,
        owner: Owner<'_>,
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
                let mut diagnostic = types.diagnostic_for(&self.path, None);
                owner.add(&mut diagnostic.context);
                Ok(Err(diagnostic))
            }
            Err(_) => Err(BindError::Internal),
        }
    }
    /// A value that names nothing the current row holds.
    pub(super) fn unknown_value(&self, owner: Owner<'_>) -> Diagnostic {
        lookup::range_reference(&format!("{}.value", self.path), owner, &self.value)
    }
    /// Whether the value is a bare name that no output column takes. In 1.0
    /// a bare name is an output column alone (NAME-3), so such a value names
    /// nothing a row holds, whatever field an input stores (LOOKUP-9).
    pub(super) fn names_no_column(&self, catalog: &Catalog) -> bool {
        !self.value.contains('.')
            && !matches!(
                catalog.bind(&self.value),
                Some(reference_binding::Binding::Output { .. })
            )
    }
    /// Whether the value names a field that `driver`, the named intermediate
    /// whose records build the row, derives. That field is a value of the row
    /// (ROW-2), but a range value reads only the stored record that built the
    /// row, so the read is refused as unsupported.
    pub(super) fn names_derived_driver_field(
        &self,
        driver: &str,
        secondary: &[SecondarySource],
        intermediates: &[Option<Intermediate>],
    ) -> bool {
        self.value
            .split_once('.')
            .filter(|(qualifier, _)| *qualifier == driver)
            .and_then(|(_, field)| {
                let item = intermediates
                    .iter()
                    .flatten()
                    .find(|item| item.identifier == driver)?;
                let column = item.source_column(secondary, field)?;
                Some(stored_record(item, column).is_none())
            })
            .unwrap_or(false)
    }
}

/// The record port at which a driver intermediate's field `column` is stored:
/// the port of the input the intermediate reads. A field it derives has none.
fn stored_record(item: &Intermediate, column: usize) -> Option<usize> {
    if item
        .derivations
        .as_ref()
        .is_some_and(|derived| column >= derived.stored_columns)
    {
        return None;
    }
    match item.source {
        IntermediateSource::Input(index) => Some(index + 1),
        IntermediateSource::Primary { .. } => Some(0),
        IntermediateSource::SelfRows { .. } | IntermediateSource::Pairs(_) => None,
    }
}

/// The driver of a row: the record port of the input whose records build it,
/// and the named intermediate that selects those records, when one does
/// (ROW-2).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct RowDriver {
    pub port: usize,
    pub intermediate: Option<usize>,
}
impl RowDriver {
    /// Without row templates, the base builds every row.
    pub(super) const BASE: Self = Self {
        port: 0,
        intermediate: None,
    };
    /// The driver of the rows a row template builds.
    pub(super) fn of(mode: &RowMode) -> Self {
        Self {
            port: mode.source_index(),
            intermediate: match mode {
                RowMode::IntermediateDriver { index, .. } => Some(*index),
                _ => None,
            },
        }
    }
}

/// What refuses a range read for the rows of `drivers`: a value that reads
/// the record of an input that does not drive every one of them, or a
/// contextual bare name, names no value of those rows (LOOKUP-9). A value
/// qualified by a named intermediate that builds one of the rows reads that
/// row's record, which a range value cannot read there, so it is
/// unsupported; and one qualified by a row driver that builds none of them
/// would read it as a keyed lookup, which it is not, since it declares no key
/// (LOOKUP-3, ROW-2).
fn refusal(
    range: &RelationRange,
    drivers: &[RowDriver],
    owner: Owner<'_>,
    named: &intermediates::Declarations,
) -> Option<BindFinding> {
    let path = format!("{}.value", range.path());
    let unknown = || {
        BindFinding::Scalar(lookup::range_reference(
            &path,
            owner,
            &range.current().identifier,
        ))
    };
    match range.current().read {
        CurrentRead::Record { source, .. }
            if drivers.iter().any(|driver| driver.port != source) =>
        {
            Some(unknown())
        }
        CurrentRead::Contextual => Some(unknown()),
        CurrentRead::Intermediate { index, .. }
            if drivers
                .iter()
                .any(|driver| driver.intermediate == Some(index)) =>
        {
            Some(BindFinding::Scalar(crate::application_issue::unsupported(
                "intermediate_driver_range_value",
                Some(&path),
            )))
        }
        CurrentRead::Intermediate { index, .. } => named.key_required(index).map(|key| {
            BindFinding::Lookup(LookupFinding::from_diagnostic(lookup::key_required(key)))
        }),
        _ => None,
    }
}

/// A range value that reads an input's record must name the driver of every
/// row that reads the lookup (LOOKUP-9). Each context pairs the expressions a
/// row evaluates with the drivers of those rows ([`RowDriver`]). A named
/// intermediate's range is admitted wherever the intermediate is read, through
/// a chain of range values too. Each refused range is reported once, and a
/// refusal another read already reported is not repeated.
pub(super) fn admit<'a>(
    contexts: impl IntoIterator<Item = (Vec<&'a Expression>, Vec<RowDriver>)>,
    intermediates: &[Option<Intermediate>],
    named: &intermediates::Declarations,
    findings: &mut Vec<BindFinding>,
) {
    let mut seen = std::collections::BTreeSet::new();
    let mut refuse = |range: &RelationRange,
                      drivers: &[RowDriver],
                      owner: Owner<'_>,
                      findings: &mut Vec<BindFinding>| {
        if let Some(finding) = refusal(range, drivers, owner, named) {
            if seen.insert(range.path().to_string())
                && !row_intermediates::reported(findings, &finding)
            {
                findings.push(finding);
            }
        }
    };
    for (expressions, drivers) in contexts {
        for expression in expressions {
            crate::dataset::relation_predicates::visit_ranges(expression, &mut |range| {
                // A source binding's range names its variable, as binding does.
                let owner = range.variable().map_or(Owner::Unnamed, Owner::Source);
                refuse(range, &drivers, owner, findings);
            });
            let mut read = Vec::new();
            crate::dataset::relation_predicates::visit(expression, &mut |index| read.push(index));
            let mut visited = std::collections::BTreeSet::new();
            while let Some(index) = read.pop() {
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
                    read.push(index);
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
                column_type: Some(column.kind),
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
            &intermediates::Donor {
                lookup: "W",
                dataset: "DONOR",
                schema: &donor,
                kind: &|column| donor.columns().get(column).map(|column| column.kind),
                admits: &|_| true,
            },
            &catalog,
            false,
            binder::Donors {
                named: &intermediates::Declarations::default(),
                secondary: &secondary,
                items: &[],
                inputs: binder::Inputs {
                    sources: &[],
                    schemas: &[],
                    driver: 0,
                },
            },
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
        let named = intermediates::Declarations::default();
        let input = |port| RowDriver {
            port,
            intermediate: None,
        };
        assert!(refusal(&range, &[input(1)], owner, &named).is_none());
        let Some(BindFinding::Scalar(diagnostic)) =
            refusal(&range, &[input(1), input(0)], owner, &named)
        else {
            panic!("another driver refuses");
        };
        assert_eq!(diagnostic.code, C::LookupRangeReference);
        assert_eq!(diagnostic.spec_paths, ["intermediates[0].between.value"]);
        assert_eq!(
            diagnostic.context,
            crate::json::Map::from([
                (
                    "identifier".into(),
                    JsonValue::String("OTHER.REFDAY".into())
                ),
                ("intermediate".into(), JsonValue::String("W".into())),
            ])
        );
    }
}
