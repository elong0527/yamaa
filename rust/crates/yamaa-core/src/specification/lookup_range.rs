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
use std::collections::BTreeSet;

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
    /// intermediate selects. A value qualified by `driver`, the named
    /// intermediate whose records build the row, reads the record that built
    /// the row at its input's record port, as the driver's qualifier always
    /// does (LOOKUP-1, LOOKUP-2), never a selection of the intermediate.
    /// Whether the input drives every row that reads the lookup is admitted
    /// later, per context ([`admit`]).
    pub(super) fn current_value(
        &self,
        catalog: &Catalog,
        secondary: &[SecondarySource],
        intermediates: &[Option<Intermediate>],
        driver: Option<&str>,
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
            if driver == Some(qualifier) {
                return Ok(stored_record(item, column).map(|source| {
                    (
                        CurrentRead::Record { source, column },
                        item.source_kind(secondary, column).map(Into::into),
                    )
                }));
            }
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
        IntermediateSource::SelfRows { .. } => None,
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
                // A source binding's range names its variable, as binding does.
                let owner = range.variable().map_or(Owner::Unnamed, Owner::Source);
                refuse(range, &drivers, owner, findings);
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

/// Report a refused range once, however many contexts read it.
fn report(
    path: &str,
    diagnostic: Diagnostic,
    seen: &mut BTreeSet<String>,
    findings: &mut Vec<BindFinding>,
) {
    if seen.insert(path.to_string()) {
        findings.push(BindFinding::Scalar(diagnostic));
    }
}

/// The rows of one context of [`check`]: what each row evaluates, as the
/// specification declares it, and the input whose records drive each of
/// those rows.
pub(super) struct Declared<'a> {
    pub evaluated: Vec<Evaluated<'a>>,
    pub drivers: Vec<&'a str>,
}

/// One thing a row evaluates, as declared.
#[derive(Clone, Copy)]
pub(super) enum Evaluated<'a> {
    /// An operation, with the dataset whose qualifier reads the row's own
    /// record there rather than joining it; with `None` every input joins.
    Operation(&'a Operation, Option<&'a str>),
    /// A name read outside an operation: a row derivation that copies a
    /// qualified variable, or an identifier of a row template's filter.
    Name(&'a str),
    /// A project function call, by its index.
    Call(usize),
}

/// `check` applies [`admit`] to the declarations alone. The rows of a context
/// evaluate the range of each lookup their operations evaluate directly, and
/// the range of each named intermediate they read: directly, from an
/// aggregate's derive binding or through a range value, and onward through
/// that intermediate's own range value. A range value qualified by an input
/// that does not drive every one of those rows fails whether or not the input
/// stores the field, because binding refuses an unknown value the same way
/// (LOOKUP-9). Contexts are admitted in binding's order, and each refused
/// range is reported once.
pub(super) fn check<'a>(
    contexts: impl IntoIterator<Item = Declared<'a>>,
    named: &'a intermediates::Declarations,
    inputs: &[SourceDeclaration],
    project: Option<&'a crate::project_calls::ProjectCalls>,
    findings: &mut Vec<BindFinding>,
) {
    let refused = |value: &str, drivers: &[&str]| {
        value.split_once('.').is_some_and(|(qualifier, _)| {
            inputs.iter().any(|input| input.name == qualifier)
                && drivers.iter().any(|&driver| driver != qualifier)
        })
    };
    let mut seen = BTreeSet::new();
    for context in contexts {
        let drivers = context.drivers.as_slice();
        for evaluated in context.evaluated {
            let mut reads = Reads::default();
            reads.evaluated(evaluated, inputs, project);
            for (range, owner) in reads.ranges {
                if refused(&range.value, drivers) {
                    report(
                        range.path(),
                        range.unknown_value(owner),
                        &mut seen,
                        findings,
                    );
                }
            }
            let mut stack = reads
                .names
                .into_iter()
                .filter_map(|name| named.reference(name).map(|(index, _)| index))
                .collect::<Vec<_>>();
            let mut visited = BTreeSet::new();
            while let Some(index) = stack.pop() {
                if !visited.insert(index) {
                    continue;
                }
                let Some((identifier, range)) = named.range(index) else {
                    continue;
                };
                if refused(&range.value, drivers) {
                    report(
                        range.path(),
                        range.unknown_value(Owner::Intermediate(identifier)),
                        &mut seen,
                        findings,
                    );
                }
                if let Some((chained, _)) = named.reference(&range.value) {
                    stack.push(chained);
                }
            }
        }
    }
}

/// What a row reads in one declared operation, as binding finds it in the
/// bound expression: the range of each lookup the row evaluates directly,
/// with its owner, and every name the row reads, a range value among them.
#[derive(Default)]
struct Reads<'a> {
    ranges: Vec<(&'a Declaration, Owner<'a>)>,
    names: Vec<&'a str>,
}

impl<'a> Reads<'a> {
    fn evaluated(
        &mut self,
        evaluated: Evaluated<'a>,
        inputs: &[SourceDeclaration],
        project: Option<&'a crate::project_calls::ProjectCalls>,
    ) {
        match evaluated {
            Evaluated::Operation(operation, own) => self.operation(operation, own, inputs, project),
            Evaluated::Name(name) => self.names.push(name),
            Evaluated::Call(call) => self.call(call, project),
        }
    }

    fn operation(
        &mut self,
        operation: &'a Operation,
        own: Option<&str>,
        inputs: &[SourceDeclaration],
        project: Option<&'a crate::project_calls::ProjectCalls>,
    ) {
        match operation {
            Operation::Source(source) => self.source(source, own, inputs),
            Operation::FirstAvailable(selection) => {
                for source in &selection.sources {
                    self.source(source, own, inputs);
                }
            }
            Operation::MappingCut(mapping) => self.source(&mapping.source, own, inputs),
            Operation::Aggregate(aggregate) => {
                if let Some(range) = aggregate.current_range() {
                    self.ranges.push((range, Owner::Unnamed));
                    self.names.push(&range.value);
                }
                // A derive binding evaluates for each record of the relation,
                // so only the names it reads for the row are the row's reads.
                for operation in aggregate.derive_operations() {
                    let mut derived = Reads::default();
                    derived.operation(operation, own, inputs, project);
                    self.names.extend(derived.names);
                }
            }
            Operation::Case(branches) => {
                for branch in branches {
                    if let Some(Ok(predicate)) = branch.predicate() {
                        self.names.extend(predicate.identifiers());
                    }
                    self.operation(&branch.operation, own, inputs, project);
                }
            }
            Operation::Flag(flag) => {
                if let Some(condition) = flag.condition() {
                    self.names.extend(condition.identifiers());
                }
            }
            Operation::Compute(compiled) => self.names.extend(compiled.identifiers()),
            Operation::Scalar(extreme) => {
                self.names
                    .extend(extreme.sources().iter().map(String::as_str));
            }
            Operation::StrExtract(operation) => self.names.push(operation.source()),
            Operation::StrReplace(operation) => self.names.push(operation.source()),
            Operation::StrCase(operation) => self.names.push(operation.source()),
            Operation::StrPad(operation) => self.names.push(operation.source()),
            Operation::Round(operation) => self.names.push(operation.source()),
            Operation::StrTemplate(operation) => {
                self.names
                    .extend(operation.identifiers().iter().map(String::as_str));
            }
            Operation::Temporal(operation) => {
                self.names
                    .extend(operation.sources().iter().map(String::as_str));
            }
            Operation::Imputation(operation) => {
                self.names
                    .extend(operation.operands().map(|(_, name)| name));
            }
            Operation::ProjectFunction { call, .. } => self.call(*call, project),
            // A window reads the output rows of its partition and an ODM
            // operation the items of the driver's record, never a lookup.
            Operation::Odm(_)
            | Operation::Literal(_)
            | Operation::Window(_)
            | Operation::InvalidNumeric { .. } => (),
        }
    }

    /// A source binding reads its variable. Only an implicit join matches a
    /// range, so its range is read only when the variable is qualified by an
    /// input other than `own`; binding refuses `between` on any other read
    /// before it reads the value (LOOKUP-9).
    fn source(
        &mut self,
        source: &'a source_expressions::Declaration,
        own: Option<&str>,
        inputs: &[SourceDeclaration],
    ) {
        self.names.push(&source.variable);
        let Some(range) = source.current_range() else {
            return;
        };
        if source
            .variable
            .split_once('.')
            .is_some_and(|(qualifier, _)| {
                own != Some(qualifier) && inputs.iter().any(|input| input.name == qualifier)
            })
        {
            self.ranges.push((range, Owner::Source(&source.variable)));
            self.names.push(&range.value);
        }
    }

    fn call(&mut self, call: usize, project: Option<&'a crate::project_calls::ProjectCalls>) {
        let Some(call) = project.and_then(|project| project.calls().get(call)) else {
            return;
        };
        for argument in &call.located.call.arguments {
            if let crate::project_call_document::Input::Reference(name) = &argument.input {
                self.names.push(name);
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
