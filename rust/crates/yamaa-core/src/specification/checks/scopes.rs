//! What binding admits of the rows' reads once every derivation is bound,
//! from the declarations alone (#3229). Each reading context pairs what its
//! rows evaluate with the inputs that drive them, as for a lookup range.
//!
//! - A matched aggregate's key value read from an input's record names the
//!   driver of every row that evaluates it (LOOKUP-5).
//! - A lookup that a row template's derivations or filter, the column
//!   phase for its rows, or a dataset verification read reads the reading
//!   row's own driver record in its key values (LOOKUP-5), its filter
//!   (LOOKUP-7) and its derivations (LOOKUP-15), and in grouped
//!   construction only its group keys there and in its range value (ROW-5,
//!   LOOKUP-7). A range value that reads another lookup runs it for the same
//!   row.
//!
//! Binding admits these after every other finding, over the reads that
//! bound: a derivation or filter with a finding of its own, and a lookup
//! whose key types fail, are not admitted, nor are the derivations of a
//! lookup binding binds without them.

use super::*;
use crate::ast::{Scope, Step};
use std::collections::BTreeSet;

/// One row template's rows, or the rows of a spec without row templates, as
/// binding admits what they read.
pub(super) struct Reading<'a> {
    /// The row template's id and its `dataset`; `None` without row templates.
    pub row: Option<(&'a str, &'a str)>,
    /// The input whose records build the rows.
    pub input: &'a str,
    /// The row template's `group_by`.
    pub groups: Option<&'a [&'a str]>,
    /// The derivations evaluated for the rows that bound, in binding's
    /// order: the row phase's, then the column phase's for these rows.
    pub derivations: Vec<ExprId>,
    /// The lookup fields read outside a derivation: the filter's, then the
    /// dataset verifications'.
    pub names: Vec<&'a str>,
}

/// The admissions of `spec`'s reads, over `prepared`'s declarations.
pub(super) struct Scopes<'a> {
    pub spec: &'a Spec,
    pub prepared: &'a PreparedSpecification,
    /// The lookups whose key types fail, which binding does not bind.
    pub refused: &'a BTreeSet<usize>,
    /// The lookups binding binds without their derivations: one whose
    /// derivation fails, and every one derived after one it does not bind.
    pub underived: &'a BTreeSet<usize>,
}

impl Scopes<'_> {
    fn input(&self, owner: &str) -> bool {
        self.prepared
            .sources
            .iter()
            .any(|source| source.name == owner)
    }

    fn lookup(&self, owner: &str) -> Option<usize> {
        self.spec
            .intermediates
            .iter()
            .position(|item| item.id.name == owner)
    }

    /// LOOKUP-5 for each matched aggregate a context evaluates: a key value
    /// read from an input's record that does not drive every row of the
    /// context, once per key and value. `contexts` pair a derivation with
    /// the inputs that drive the rows it is evaluated for.
    pub(super) fn record_keys(
        &self,
        contexts: &[(ExprId, Vec<&str>)],
        findings: &mut Vec<BindFinding>,
    ) {
        let spec = self.spec;
        let mut reported = BTreeSet::new();
        for (root, drivers) in contexts {
            for e in positions(spec, *root) {
                // Only a reduction of a declared relation matches its keys.
                let ExprKind::Aggregate(aggregate) = &spec.expr(e).kind else {
                    continue;
                };
                let (true, Some(JoinKey::Pairs { pairs, .. })) =
                    (super::declared::qualified(spec, aggregate), &aggregate.key)
                else {
                    continue;
                };
                let at = path(spec, e);
                for (field, value) in pairs {
                    let Ok(value) = match_values::Declaration::from_tree(
                        spec,
                        field.meta,
                        *value,
                        self.prepared.project_calls.is_some(),
                    ) else {
                        continue;
                    };
                    let key = format!("{at}.key.{}", field.name);
                    for reference in value.references() {
                        let Some((owner, _)) = reference.split_once('.') else {
                            continue;
                        };
                        if self.input(owner)
                            && drivers.iter().any(|&driver| driver != owner)
                            && reported.insert((key.clone(), reference.to_owned()))
                        {
                            findings.push(BindFinding::Scalar(
                                crate::diagnostic::lookup::match_value_reference(
                                    &key,
                                    crate::diagnostic::lookup::Owner::Unnamed,
                                    reference,
                                ),
                            ));
                        }
                    }
                }
            }
        }
    }

    /// The lookups each reading's rows read, admitted in binding's order:
    /// each reading in turn, each lookup once per reading, and a finding
    /// several readings share once.
    pub(super) fn lookup_reads(&self, readings: &[Reading<'_>], findings: &mut Vec<BindFinding>) {
        let mut admitted: Vec<BindFinding> = Vec::new();
        for reading in readings {
            let driver = reading.row.map(|(_, driver)| driver);
            let mut lookups = Vec::new();
            for &root in &reading.derivations {
                self.visits(root, driver, &mut lookups);
            }
            lookups.extend(
                reading
                    .names
                    .iter()
                    .filter_map(|name| name.split_once('.'))
                    .filter_map(|(owner, _)| self.lookup(owner)),
            );
            let mut seen = BTreeSet::new();
            let mut own = Vec::new();
            for index in lookups {
                self.admit(reading, index, &mut seen, &mut own);
            }
            for finding in own {
                if !admitted
                    .iter()
                    .any(|prior| relation_predicates::same_finding(prior, &finding))
                {
                    admitted.push(finding);
                }
            }
        }
        findings.extend(admitted);
    }

    /// The lookups `root` reads for the row, in binding's visit order
    /// (`dataset::relation_predicates::visit`): a name of a lookup other
    /// than the rows' driver, a structured source's lookup variable or a
    /// joined variable's range value that reads one, and an aggregate's
    /// range value and derive bindings. A window, an ODM read, a reduction
    /// and, under row templates, a flag read none.
    fn visits(&self, root: ExprId, driver: Option<&str>, out: &mut Vec<usize>) {
        let spec = self.spec;
        let lookup = |name: &str| {
            name.split_once('.')
                .filter(|(owner, _)| Some(*owner) != driver)
                .and_then(|(owner, _)| self.lookup(owner))
        };
        spec.walk(root, Scope::Row, |e, scope| match &spec.expr(e).kind {
            ExprKind::Ref(name) => {
                if scope == Scope::Row {
                    out.extend(lookup(name.as_str()));
                }
                Step::Continue
            }
            ExprKind::Window(_) | ExprKind::Odm(_) => Step::Skip,
            ExprKind::Flag { .. } if driver.is_some() => Step::Skip,
            ExprKind::Aggregate(aggregate) => {
                out.extend(
                    aggregate
                        .between
                        .as_ref()
                        .and_then(|between| name(spec, between.value))
                        .and_then(lookup),
                );
                for binding in &aggregate.bindings {
                    self.visits(binding.derivation.expr, driver, out);
                }
                Step::Skip
            }
            ExprKind::Source(source) => {
                let variable = name(spec, source.variable).unwrap_or_default();
                match lookup(variable) {
                    Some(index) => out.push(index),
                    None => out.extend(
                        source
                            .between
                            .as_ref()
                            .and_then(|between| name(spec, between.value))
                            .and_then(lookup),
                    ),
                }
                Step::Skip
            }
            _ => Step::Continue,
        });
    }

    /// One lookup read for `reading`'s rows, then each lookup its range
    /// value runs for the same row (`relation_predicates::admit_reads`).
    fn admit(
        &self,
        reading: &Reading<'_>,
        mut index: usize,
        seen: &mut BTreeSet<usize>,
        findings: &mut Vec<BindFinding>,
    ) {
        let spec = self.spec;
        loop {
            if !seen.insert(index) || self.refused.contains(&index) {
                return;
            }
            let item = &spec.intermediates[index];
            let intermediate = item.id.name.as_str();
            // In grouped construction a driver field is one value of the row
            // only as a group key.
            let ungrouped = |field: &str| {
                let (Some(groups), Some((_, driver))) = (reading.groups, reading.row) else {
                    return false;
                };
                !groups.iter().any(|group| {
                    group
                        .strip_prefix(driver)
                        .and_then(|tail| tail.strip_prefix('.'))
                        == Some(field)
                })
            };
            let grouped = |path: &str, identifier: &str| {
                let mut finding =
                    LookupFinding::new(path, crate::diagnostic::ConditionCode::GroupedRowReference);
                finding.text("identifier", identifier);
                if let Some((row, dataset)) = reading.row {
                    finding.text("row", row);
                    finding.text("dataset", dataset);
                }
                finding.text("intermediate", intermediate);
                BindFinding::Lookup(finding)
            };
            // A lookup that another lookup's derivations read binds its key
            // values for each context that reads it: on a 1.0 tree a row
            // refuses a value whose reads name no value of it, at the first
            // such read (LOOKUP-5).
            let contextual = spec.header.surface.one_meaning_per_form() && donor_read(spec, index);
            // A key value reads the reading row's own driver record (LOOKUP-5).
            for (path, reads) in self.keys(index) {
                if let Some(name) = reads
                    .iter()
                    .find(|name| contextual && unknown_match_value(spec, self.prepared, name))
                {
                    findings.push(BindFinding::Lookup(LookupFinding::reference(
                        &path,
                        name,
                        Some(intermediate),
                        ReferenceCause::OutputKey,
                        None,
                    )));
                    continue;
                }
                for name in reads {
                    let Some((owner, field)) = name.split_once('.') else {
                        continue;
                    };
                    if !self.input(owner) {
                        continue;
                    }
                    if owner != reading.input {
                        findings.push(BindFinding::Lookup(LookupFinding::reference(
                            &path,
                            name,
                            Some(intermediate),
                            ReferenceCause::OutputKey,
                            None,
                        )));
                    } else if ungrouped(field) {
                        findings.push(grouped(&path, name));
                    }
                }
            }
            // A correlated filter reads the same record (LOOKUP-7).
            if let Some(ExprKind::Formula(ast::Formula {
                compiled: Ok(plan), ..
            })) = item.filter.map(|filter| &spec.expr(filter).kind)
            {
                let donor = item.dataset.name.as_str();
                for identifier in plan.identifiers() {
                    let Some((owner, field)) = identifier.split_once('.') else {
                        continue;
                    };
                    if owner != donor
                        && self.input(owner)
                        && (owner != reading.input || ungrouped(field))
                    {
                        findings.push(BindFinding::Lookup(LookupFinding::reference(
                            plan.spec_path(),
                            identifier,
                            Some(intermediate),
                            ReferenceCause::Selection,
                            None,
                        )));
                    }
                }
            }
            // A derivation reads it in the scope of the filter (LOOKUP-15).
            for (identifier, path) in self.current(index) {
                let Some((owner, field)) = identifier.split_once('.') else {
                    continue;
                };
                if owner != reading.input {
                    let mut finding = LookupFinding::new(
                        &path,
                        crate::diagnostic::ConditionCode::IntermediateDerivationReference,
                    );
                    finding.text("intermediate", intermediate);
                    finding.text("identifier", &identifier);
                    findings.push(BindFinding::Lookup(finding));
                } else if ungrouped(field) {
                    findings.push(grouped(&path, &identifier));
                }
            }
            let Some(value) = item
                .between
                .as_ref()
                .and_then(|between| name(spec, between.value))
            else {
                return;
            };
            let Some((owner, field)) = value.split_once('.') else {
                return;
            };
            if self.input(owner) {
                if owner == reading.input && ungrouped(field) {
                    findings.push(grouped(
                        &format!("{}.between.value", render(spec, item.meta)),
                        value,
                    ));
                }
                return;
            }
            match self.lookup(owner) {
                Some(chained) => index = chained,
                None => return,
            }
        }
    }

    /// Each key value of the lookup at `index`, at the path binding cites
    /// it, with the names it reads. Inferred keys read output columns.
    fn keys(&self, index: usize) -> Vec<(String, Vec<&str>)> {
        let spec = self.spec;
        let item = &spec.intermediates[index];
        let fields = match &item.key {
            Some(JoinKey::Columns { columns, .. }) => columns
                .iter()
                .map(|column| column.name.as_str())
                .collect::<Vec<_>>(),
            Some(JoinKey::Pairs { pairs, .. }) => {
                pairs.iter().map(|(field, _)| field.name.as_str()).collect()
            }
            _ => Vec::new(),
        };
        let at = render(spec, item.meta);
        fields
            .into_iter()
            .filter_map(|field| {
                let value = self
                    .prepared
                    .intermediates
                    .match_declaration(&item.id.name, field)?;
                let path = match value {
                    match_values::Declaration::Variable(_) => format!("{at}.key"),
                    _ => format!("{at}.key.{field}"),
                };
                Some((path, value.references().collect()))
            })
            .collect()
    }

    /// The current record's fields the derivations of the lookup at `index`
    /// read, each once, with the path of the first derivation that reads it:
    /// a field of another input than the one the lookup reads. A lookup bound
    /// without its derivations reads none.
    fn current(&self, index: usize) -> Vec<(String, String)> {
        let spec = self.spec;
        let item = &spec.intermediates[index];
        if self.underived.contains(&index) {
            return Vec::new();
        }
        let scan = crate::specification::reads::Scan {
            relation: true,
            windows: true,
        };
        let calls = self.prepared.project_calls.is_some();
        let mut current: Vec<(String, String)> = Vec::new();
        for binding in &item.bindings {
            let expr = binding.derivation.expr;
            let Ok(references) = crate::specification::reads::names(spec, expr, scan, calls) else {
                continue;
            };
            for reference in references {
                let Some((owner, _)) = reference.split_once('.') else {
                    continue;
                };
                if owner != item.dataset.name
                    && self.lookup(owner).is_none()
                    && self.input(owner)
                    && !current.iter().any(|(prior, _)| *prior == reference)
                {
                    current.push((reference, path(spec, expr)));
                }
            }
        }
        current
    }
}
