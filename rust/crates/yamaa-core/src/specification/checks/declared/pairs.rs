//! The declaration findings of a pair relation (LOOKUP-27), as binding finds
//! them (`pairs::bind`) as far as the declarations decide: each side an
//! intermediate gives, then the key pairs, the filter's names and operand
//! types, the derivations over the joined record, and once they bind the
//! exposed fields and the columns of its `unique` checks. A side's field its
//! records may not hold is binding's to decide. Like the rest of this pass,
//! it goes when `check()` binds over the declared types (#3805).
use super::*;
use crate::specification::checks::donors::DonorReads;

/// How the declarations type the names a pair relation reads: a side's
/// field, from an input's declared types or the fields an intermediate side
/// gives, and a derivation of its own by its bare name.
struct Sides<'c, 'd> {
    spec: &'c Spec,
    donor: &'c Donor<'d>,
    donors: &'c [Donor<'d>],
    stored: &'c Typing<'d>,
    left: &'c str,
    right: &'c str,
}

impl Sides<'_, '_> {
    /// The type of the field `field` of the side `side`.
    fn field(&self, side: &str, field: &str) -> Option<ResultType> {
        if self.spec.inputs.iter().any(|input| input.name.name == side) {
            return (self.stored)(side, field);
        }
        self.donors
            .iter()
            .find(|other| other.item.id.name == side)?
            .field(field, self.stored)
    }

    /// The type of a name the filter or a derivation reads: a side's field
    /// by its qualifier, or a derivation by its bare name.
    fn name(&self, name: &str) -> Option<ResultType> {
        match name.split_once('.') {
            Some((owner, field)) if owner == self.left || owner == self.right => {
                self.field(owner, field)
            }
            Some(_) => None,
            None => self.donor.fields.get(name).copied(),
        }
    }

    /// The column type of a left field that a key value's expression reads
    /// by the left side's qualifier.
    fn left_column(&self, name: &str) -> Option<ColumnType> {
        name.split_once('.')
            .filter(|(owner, _)| *owner == self.left)
            .and_then(|(_, field)| self.field(self.left, field))
            .and_then(ResultType::column_type)
    }
}

/// The left value of one key pair, as the key writes it.
enum Left<'s> {
    /// A field of the left side: a list entry, or a mapping value naming
    /// the field by the side's qualifier.
    Field(&'s str),
    /// A mapping value naming anything else alone.
    Name(&'s str),
    /// A mapping value that is an expression over the left record.
    Expression(ExprId),
}

/// The rows a pair relation's key values and derivations evaluate on: one
/// for each record of its left side, in an ungrouped row phase.
fn record_scope<'a>(drivers: &'a [&'a str]) -> Scope<'a> {
    Scope {
        drivers,
        row: None,
        phase: Some(Phase::Row { group_by: None }),
        available: None,
    }
}

impl Checks<'_> {
    /// A pair relation's findings, in binding's order. True when one is
    /// refused: binding then binds nothing of the relation.
    pub(super) fn pair(
        &self,
        donor: &Donor<'_>,
        donors: &[Donor<'_>],
        stored: &Typing<'_>,
        findings: &mut Vec<BindFinding>,
    ) -> Result<bool, BindError> {
        let item = donor.item;
        let Some(right) = &item.inner_join else {
            return Ok(false);
        };
        let start = findings.len();
        for side in [&item.dataset, right] {
            if let Some(other) = donors.iter().find(|other| other.item.id.name == side.name) {
                self.pair_side(side, other, findings);
            }
        }
        if findings.len() != start {
            return Ok(true);
        }
        let sides = Sides {
            spec: self.spec,
            donor,
            donors,
            stored,
            left: &item.dataset.name,
            right: &right.name,
        };
        self.pair_keys(&sides, findings)?;
        self.pair_filter(&sides, findings)?;
        let derivations = findings.len();
        self.pair_derivations(&sides, findings)?;
        if findings.len() != derivations {
            return Ok(true);
        }
        pair_exposed(self.spec, donor, findings);
        Ok(findings.len() != start)
    }

    /// An intermediate side's records are its own: neither its filter nor
    /// its derivations read another input's record, and each field it
    /// exposes has one type (ROW-2).
    fn pair_side(&self, side: &ast::Named, other: &Donor<'_>, findings: &mut Vec<BindFinding>) {
        let spec = self.spec;
        let own = other.item.dataset.name.as_str();
        let foreign = |name: &str| {
            name.split_once('.').is_some_and(|(owner, _)| {
                owner != own && spec.inputs.iter().any(|input| input.name.name == owner)
            })
        };
        let mut prohibited = Vec::new();
        let filtered = other.item.filter.is_some_and(|filter| {
            matches!(&spec.expr(filter).kind, ExprKind::Formula(ast::Formula {
                compiled: Ok(plan), ..
            }) if plan.identifiers().any(foreign))
        });
        if filtered {
            prohibited.push("filter");
        }
        let calls = self.prepared.project_calls.is_some();
        let scan = reads::Scan {
            relation: true,
            windows: true,
        };
        let derived = other.item.bindings.iter().any(|binding| {
            reads::names(spec, binding.derivation.expr, scan, calls)
                .unwrap_or_default()
                .iter()
                .any(|name| foreign(name))
        });
        if derived {
            prohibited.push("derivations");
        }
        if !prohibited.is_empty() {
            findings.push(BindFinding::Lookup(LookupFinding::from_diagnostic(
                lookup::pair_side(&render(spec, side.meta), &side.name, &prohibited),
            )));
            return;
        }
        let untyped = untyped_fields(other.item, &other.fields);
        if !untyped.is_empty() {
            findings.push(BindFinding::Lookup(LookupFinding::from_diagnostic(
                lookup::pair_field_type(&render(spec, side.meta), &side.name, &untyped),
            )));
        }
    }

    /// Each key pair's left value is a field of the left side, by its
    /// qualifier when a mapping writes a name alone, or an expression over
    /// the left record (LOOKUP-6); its two types compare (LOOKUP-9).
    fn pair_keys(
        &self,
        sides: &Sides<'_, '_>,
        findings: &mut Vec<BindFinding>,
    ) -> Result<(), BindError> {
        let spec = self.spec;
        let item = sides.donor.item;
        let identifier = item.id.name.as_str();
        let key = format!("{}.key", spec.cite(item.meta));
        let keys = match &item.key {
            Some(JoinKey::Columns { columns, .. }) => columns
                .iter()
                .map(|column| (column.name.as_str(), Left::Field(&column.name), key.clone()))
                .collect::<Vec<_>>(),
            Some(JoinKey::Pairs { pairs, .. }) => pairs
                .iter()
                .map(|(column, value)| {
                    let alone = spec.expr(*value).meta.path == column.meta.path;
                    match &spec.expr(*value).kind {
                        ExprKind::Ref(read) if alone => (
                            column.name.as_str(),
                            match read.as_str().split_once('.') {
                                Some((owner, field)) if owner == sides.left => Left::Field(field),
                                _ => Left::Name(read.as_str()),
                            },
                            key.clone(),
                        ),
                        _ => (
                            column.name.as_str(),
                            Left::Expression(*value),
                            render(spec, column.meta),
                        ),
                    }
                })
                .collect(),
            _ => Vec::new(),
        };
        for (right_field, left_value, at) in keys {
            let expected = match left_value {
                Left::Name(written) => {
                    findings.push(BindFinding::Lookup(LookupFinding::from_diagnostic(
                        lookup::pair_key_reference(&at, identifier, written),
                    )));
                    continue;
                }
                Left::Field(field) => sides
                    .field(sides.left, field)
                    .and_then(ResultType::column_type)
                    .map(ResultType::column),
                Left::Expression(value) => self.pair_key_value(sides, value, findings)?,
            };
            let (Some(expected), Some(actual)) = (
                expected,
                sides
                    .field(sides.right, right_field)
                    .and_then(ResultType::column_type),
            ) else {
                continue;
            };
            if !expected.comparable(ResultType::column(actual)) {
                findings.push(BindFinding::Lookup(LookupFinding::from_diagnostic(
                    lookup::pair_key_type(
                        &at,
                        identifier,
                        &format!("{}.{right_field}", sides.right),
                        expected,
                        ResultType::column(actual),
                    ),
                )));
            }
        }
        Ok(())
    }

    /// The type of a key value written as an expression over the left
    /// record, once its reads bind; none when one is refused or the type is
    /// decided per value.
    fn pair_key_value(
        &self,
        sides: &Sides<'_, '_>,
        value: ExprId,
        findings: &mut Vec<BindFinding>,
    ) -> Result<Option<ResultType>, BindError> {
        let before = findings.len();
        let typing = |name: &str| sides.left_column(name);
        let reads = DonorReads::pair(
            self.spec,
            self.prepared,
            (sides.left, sides.left),
            &sides.donor.item.id.name,
            &[],
            &typing,
        );
        let drivers = [sides.left];
        self.defects(value, findings)?;
        self.donor_walk(&reads, record_scope(&drivers), value, findings)?;
        if findings.len() != before {
            return Ok(None);
        }
        let typed = |name: &str| {
            sides
                .left_column(name)
                .map_or(ResultType::UNKNOWN, ResultType::column)
        };
        Ok(
            Some(self.result_type(value, ast::Scope::Donor(sides.donor.index), &typed))
                .filter(|&result| result != ResultType::UNKNOWN),
        )
    }

    /// The filter reads each side's fields by its qualifier, typed by the
    /// side.
    fn pair_filter(
        &self,
        sides: &Sides<'_, '_>,
        findings: &mut Vec<BindFinding>,
    ) -> Result<(), BindError> {
        let item = sides.donor.item;
        let Some(filter) = item.filter else {
            return Ok(());
        };
        self.grammar(filter, findings)?;
        let ExprKind::Formula(ast::Formula {
            compiled: Ok(plan), ..
        }) = &self.spec.expr(filter).kind
        else {
            return Ok(());
        };
        let mut seen = BTreeSet::new();
        for read in plan.identifiers() {
            let field = read
                .split_once('.')
                .is_some_and(|(owner, _)| owner == sides.left || owner == sides.right);
            if seen.insert(read) && !field {
                findings.push(BindFinding::Lookup(LookupFinding::from_diagnostic(
                    lookup::pair_reference(plan.spec_path(), &item.id.name, read),
                )));
            }
        }
        findings.extend(
            types::predicate_kinds(plan, |name| sides.name(name)?.single())
                .into_iter()
                .map(BindFinding::Predicate),
        );
        Ok(())
    }

    /// The derivations over the joined record, each typed as binding types
    /// it, and converted when typed (VALUE-24).
    fn pair_derivations(
        &self,
        sides: &Sides<'_, '_>,
        findings: &mut Vec<BindFinding>,
    ) -> Result<(), BindError> {
        let spec = self.spec;
        let donor = sides.donor;
        let item = donor.item;
        let derived = item
            .bindings
            .iter()
            .map(|binding| binding.name.name.as_str())
            .collect::<Vec<_>>();
        let drivers = [sides.left];
        let typing = |name: &str| sides.name(name).and_then(ResultType::column_type);
        for (at, (binding, &result)) in item.bindings.iter().zip(&donor.results).enumerate() {
            let before = findings.len();
            let reads = DonorReads::pair(
                spec,
                self.prepared,
                (sides.left, sides.right),
                &item.id.name,
                &derived[..at],
                &typing,
            );
            self.defects(binding.derivation.expr, findings)?;
            self.donor_walk(
                &reads,
                record_scope(&drivers),
                binding.derivation.expr,
                findings,
            )?;
            if let (Some(typed), None, true) = (
                binding.declared,
                &binding.derivation.unconvertible,
                findings.len() == before,
            ) {
                findings.extend(
                    types::conversion_finding(&render(spec, binding.meta), result, typed)
                        .map(BindFinding::Scalar),
                );
            }
        }
        Ok(())
    }
}

/// The fields a pair relation exposes are derivations, each of one type,
/// and so are the columns its `unique` checks name.
fn pair_exposed(spec: &Spec, donor: &Donor<'_>, findings: &mut Vec<BindFinding>) {
    let item = donor.item;
    let identifier = item.id.name.as_str();
    let path = spec.cite(item.meta);
    let derived = |name: &str| {
        item.bindings
            .iter()
            .any(|binding| binding.name.name == name)
    };
    for column in item.columns.iter().flatten() {
        if !derived(&column.name) {
            findings.push(BindFinding::Lookup(LookupFinding::from_diagnostic(
                lookup::pair_reference(&format!("{path}.columns"), identifier, &column.name),
            )));
        }
    }
    let untyped = untyped_fields(item, &donor.fields);
    if !untyped.is_empty() {
        findings.push(BindFinding::Lookup(LookupFinding::from_diagnostic(
            lookup::pair_field_type(&format!("{path}.derivations"), identifier, &untyped),
        )));
    }
    for check in &item.checks {
        for column in &check.columns {
            if !derived(&column.name) {
                let mut finding =
                    LookupFinding::new(&render(spec, column.meta), C::IntermediateUniqueField);
                finding.text("intermediate", identifier);
                finding.text("identifier", &format!("{identifier}.{}", column.name));
                findings.push(BindFinding::Lookup(finding));
            }
        }
    }
}

/// The derived fields `item` exposes whose type is known yet no one column
/// type: a union of types, or missing only (EXPR-14).
fn untyped_fields<'i>(
    item: &'i ast::Intermediate,
    fields: &BTreeMap<String, ResultType>,
) -> Vec<&'i str> {
    item.bindings
        .iter()
        .filter(|binding| {
            item.columns.as_ref().is_none_or(|columns| {
                columns
                    .iter()
                    .any(|column| column.name == binding.name.name)
            })
        })
        .filter(|binding| {
            fields
                .get(&binding.name.name)
                .is_some_and(|&kind| kind != ResultType::UNKNOWN && kind.column_type().is_none())
        })
        .map(|binding| binding.name.name.as_str())
        .collect()
}
