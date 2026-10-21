//! What binding decides of each read of a row template's row phase, of the
//! column phase and of a row template's filter from the declarations alone,
//! at binding's sites and in binding's order (#2754, #3229):
//!
//! - a bare read of a column the row template leaves to the column phase
//!   (COLUMN-4);
//! - a read of a driver field outside the row template's `group_by`
//!   (ROW-5), or in the column phase outside the `group_by` of a grouped
//!   template over that driver (NAME-4);
//! - a row template's formula, flag or `case` predicate that reads another
//!   input, a column formula that reads a qualified name other than a
//!   lookup's, and a flag that reads a lookup or a name nothing declares
//!   (FORMULA-9);
//! - a read of a lookup whose key value reads another input's record than
//!   the driver's (LOOKUP-5), or that matches on a column the row template
//!   leaves to the column phase, through its key or range value (LOOKUP-2);
//!   and a read of a 1.0 row driver, which declares no key, as a lookup of
//!   rows it does not drive (LOOKUP-3);
//! - a read of another 1.0 input that declares no `key` through its implicit
//!   join, which has nothing to match on (LOOKUP-5, D111). A column that
//!   holds an aggregate binds once over the whole dataset, and when a row
//!   template's driver differs from the primary input those rows share no
//!   record, so the primary is read through its join too;
//! - `require_match` or `between` on a structured source that reads anything
//!   but another input through its implicit join (LOOKUP-24, LOOKUP-9);
//! - a row filter's read of a column its template does not derive, a
//!   grouped filter's qualified read, and an ungrouped filter's read of a
//!   window result, directly or through a lookup's match (ROW-6).
//!
//! Binding keeps what the declarations alone cannot decide: a field the
//! input does not store, a lookup whose keys are inferred from the captured
//! schemas or that another lookup's derivations read, and the output keys a
//! 0.5 implicit join matches on.

use super::*;
use crate::ast::Scope;
use crate::reference_scope::Phase;
use std::{cell::Cell, collections::BTreeSet};

/// The reads of one derivation, as binding scopes them: a row template's
/// row phase, or the column phase for the rows of the templates its
/// bindings complete.
pub(super) struct Reads<'a> {
    spec: &'a Spec,
    prepared: &'a PreparedSpecification,
    /// The driver of each binding the derivation gets: a row template's own,
    /// or in the column phase each binding's. Its groups, lookups and nested
    /// scopes follow it.
    drivers: &'a [&'a str],
    /// Whether each binding's rows read their driver's record directly. A
    /// column-phase binding over the whole dataset has no record owner when
    /// a row template's driver differs from the primary input
    /// (`rows::Templates::bind`): it reads the primary through its implicit
    /// join too.
    owned: bool,
    phase: Phase<'a>,
    /// A row template's id and the columns it derives in the row phase,
    /// which its row-phase reads may read.
    row: Option<(&'a str, &'a [usize])>,
    /// The lookups whose key types fail, which binding does not bind: it
    /// reports each read of one as unknown, and the read binds nothing.
    refused: &'a BTreeSet<usize>,
    /// Whether a lookup read since the last [`Self::bound`] failed. A refused
    /// lookup's read fails without a finding of its own, and so does a read
    /// whose phase boundary its site already reported.
    unbound: Cell<bool>,
}

impl<'a> Reads<'a> {
    /// The row phase of `template`, in the scope binding binds its
    /// derivations in ([`rows::TemplatePhase::scope`]).
    pub(super) fn row(
        spec: &'a Spec,
        prepared: &'a PreparedSpecification,
        template: &'a rows::TemplatePhase<'a>,
        refused: &'a BTreeSet<usize>,
    ) -> Self {
        let scope = template.scope();
        Self {
            spec,
            prepared,
            drivers: std::slice::from_ref(&template.driver),
            owned: true,
            phase: scope.phase,
            row: Some((template.id, scope.available.unwrap_or_default())),
            refused,
            unbound: Cell::new(false),
        }
    }

    /// The column phase, for bindings over `drivers`, with every grouped
    /// template's driver and `group_by`; `owned` when their rows read their
    /// driver's record directly.
    pub(super) fn column(
        spec: &'a Spec,
        prepared: &'a PreparedSpecification,
        drivers: &'a [&'a str],
        owned: bool,
        groups: &'a [(&'a str, &'a [&'a str])],
        refused: &'a BTreeSet<usize>,
    ) -> Self {
        Self {
            spec,
            prepared,
            drivers,
            owned,
            phase: Phase::Column { groups },
            row: None,
            refused,
            unbound: Cell::new(false),
        }
    }

    /// Whether every lookup read since the last call bound, as far as the
    /// declarations decide; the other reads that fail report a finding.
    pub(super) fn bound(&self) -> bool {
        !self.unbound.replace(false)
    }

    /// An unqualified name of an output column.
    fn output(&self, name: &str) -> Option<usize> {
        (!name.contains('.'))
            .then(|| {
                self.spec
                    .columns
                    .iter()
                    .position(|column| column.name.name == name)
            })
            .flatten()
    }

    /// Whether a row-phase read may read `column`; the column phase reads any.
    fn available(&self, column: usize) -> bool {
        self.row
            .is_none_or(|(_, available)| available.contains(&column))
    }

    /// The lookup `owner` names, unless it drives the rows: a read qualified
    /// by the driver reads the row's record.
    fn lookup(&self, owner: &str) -> Option<usize> {
        if self.drivers.contains(&owner) {
            return None;
        }
        self.named(owner)
    }

    /// The lookup `owner` names, whatever rows it drives.
    fn named(&self, owner: &str) -> Option<usize> {
        self.spec
            .intermediates
            .iter()
            .position(|item| item.id.name == owner)
    }

    fn input(&self, owner: &str) -> bool {
        self.prepared
            .sources
            .iter()
            .any(|source| source.name == owner)
    }

    /// Whether a read qualified by `owner` reads the row's own record: the
    /// driver's, when the rows read it directly.
    fn record(&self, owner: &str) -> bool {
        self.owned && self.drivers.contains(&owner)
    }

    /// Whether `owner` is another input, which joins the rows on its implicit
    /// join's key.
    fn joined(&self, owner: &str) -> bool {
        self.input(owner) && !self.record(owner)
    }

    /// A read at `path` of the input `owner` through its implicit join. On a
    /// 1.0 tree the join matches on the input's declared `key`, so an input
    /// that declares none fails at each read (LOOKUP-5, D111); the output
    /// keys a 0.5 join matches on, only its captured schema decides. True
    /// when it binds, as far as the declarations decide.
    fn join(&self, path: &str, owner: &str, findings: &mut Vec<BindFinding>) -> bool {
        if !super::keyless(self.spec, owner) {
            return true;
        }
        findings.push(super::undeclared_key(path, owner));
        false
    }

    /// One named read, as binding reads an operand: a bare column, a lookup
    /// through its selector, the driver's record, or another input's
    /// through its implicit join ([`Self::join`]). In the column phase a
    /// lookup or an input that drives the rows of some bindings is a lookup
    /// or a join for the others, each binding in turn, and without a record
    /// owner the driver is read through its join too. True when the read
    /// binds, as far as the declarations decide.
    pub(super) fn read(&self, path: &str, name: &str, findings: &mut Vec<BindFinding>) -> bool {
        let Some((owner, _)) = name.split_once('.') else {
            return self.bare(path, name, findings);
        };
        if let Some(index) = self.lookup(owner) {
            return self.lookup_read(index, path, findings);
        }
        if !self.record(owner) {
            return !self.input(owner) || self.join(path, owner, findings);
        }
        let named = self.named(owner);
        let input = self.input(owner);
        let mut bound = true;
        for &driver in self.drivers {
            bound &= match named {
                _ if driver == owner => self.group(path, name, findings),
                Some(index) => self.lookup_read(index, path, findings),
                None if input => self.join(path, owner, findings),
                None => true,
            };
        }
        bound
    }

    /// A bare read of a column the row template leaves to the column phase
    /// (COLUMN-4). True when it binds.
    pub(super) fn bare(&self, path: &str, name: &str, findings: &mut Vec<BindFinding>) -> bool {
        let Some((row, _)) = self.row else {
            return true;
        };
        let Some(column) = self.output(name).filter(|&column| !self.available(column)) else {
            return true;
        };
        findings.push(BindFinding::OutputReference {
            path: path.into(),
            name: name.into(),
            row: Some(row.into()),
            finding: crate::reference_binding::Diagnostic::PhaseBoundary { column },
        });
        false
    }

    /// A read of the driver's field `name` names one value of the row only
    /// as a group key (ROW-5, NAME-4). True when it binds.
    fn group(&self, path: &str, name: &str, findings: &mut Vec<BindFinding>) -> bool {
        let Some(finding) = crate::reference_scope::group_finding(name, self.phase) else {
            return true;
        };
        findings.push(BindFinding::QualifiedReference {
            path: path.into(),
            name: name.into(),
            row: self.row.map(|(row, _)| row.into()),
            finding,
        });
        false
    }

    /// A row template's formula, flag or `case` predicate reads only its
    /// driver's record and lookups (FORMULA-9).
    fn source_scope(&self, path: &str, name: &str, findings: &mut Vec<BindFinding>) {
        let Phase::Row { group_by } = self.phase else {
            return;
        };
        let mut finding = LookupFinding::new(
            path,
            if group_by.is_some() {
                crate::diagnostic::ConditionCode::GroupedRowNumericSourceScope
            } else {
                crate::diagnostic::ConditionCode::RowNumericSourceScope
            },
        );
        finding.text("identifier", name);
        finding.texts("drivers", self.drivers);
        findings.push(BindFinding::Lookup(finding));
    }

    /// A name of a formula's text `expression`: in a row template another
    /// input's field, and in the column phase any qualified name but a
    /// lookup's, fail (FORMULA-9); any other name is read as an operand.
    pub(super) fn formula(
        &self,
        path: &str,
        expression: &str,
        name: &str,
        findings: &mut Vec<BindFinding>,
    ) {
        if let Some((owner, _)) = name.split_once('.') {
            if self.prepared.intermediates.reference(name).is_none() {
                match self.phase {
                    Phase::Column { .. } => {
                        findings.push(BindFinding::QualifiedNumericReference {
                            path: path.into(),
                            expression: expression.into(),
                            identifier: name.into(),
                        });
                        return;
                    }
                    Phase::Row { .. } if self.joined(owner) => {
                        self.source_scope(path, name, findings);
                        return;
                    }
                    Phase::Row { .. } => {}
                }
            }
        }
        self.read(path, name, findings);
    }

    /// A name of a `case` predicate: in a row template another input's field
    /// fails (FORMULA-9); any other name is read as an operand.
    pub(super) fn predicate(&self, path: &str, name: &str, findings: &mut Vec<BindFinding>) {
        if matches!(self.phase, Phase::Row { .. })
            && name
                .split_once('.')
                .is_some_and(|(owner, _)| self.joined(owner))
        {
            self.source_scope(path, name, findings);
            return;
        }
        self.read(path, name, findings);
    }

    /// A name of a flag's condition, which reads columns, its driver's record
    /// and, in the column phase, another input's through its join
    /// ([`Self::join`]). Every other qualified name, a lookup's field or a
    /// name nothing declares, is unknown there (FORMULA-9), in each binding
    /// whose driver it does not name.
    pub(super) fn flag(&self, path: &str, name: &str, findings: &mut Vec<BindFinding>) {
        let Some((owner, _)) = name.split_once('.') else {
            self.bare(path, name, findings);
            return;
        };
        if self.joined(owner) {
            match self.phase {
                Phase::Row { .. } => self.source_scope(path, name, findings),
                Phase::Column { .. } => {
                    self.join(path, owner, findings);
                }
            }
            return;
        }
        let input = self.input(owner);
        for &driver in self.drivers {
            if driver == owner {
                self.group(path, name, findings);
            } else if input {
                self.join(path, owner, findings);
            } else {
                findings.push(BindFinding::Predicate(crate::diagnostic::Diagnostic {
                    code: crate::diagnostic::ConditionCode::PredicateUnknownField,
                    spec_paths: vec![path.into()],
                    context: [("identifier".into(), crate::json::Value::String(name.into()))]
                        .into(),
                    source_span: None,
                    operand_route: None,
                }));
            }
        }
    }

    /// A project call's argument: a field of the row's own record must be a
    /// group key (ROW-5, NAME-4), and another input's field is read through
    /// its implicit join ([`Self::join`]). Preparation refuses a column
    /// outside the row phase.
    pub(super) fn argument(&self, path: &str, name: &str, findings: &mut Vec<BindFinding>) {
        let Some((owner, _)) = name.split_once('.') else {
            return;
        };
        if self.record(owner) {
            self.group(path, name, findings);
        } else if self.joined(owner) {
            self.join(path, owner, findings);
        }
    }

    /// A structured source binding's variable, read at the binding's own
    /// path, then, once it binds, the range value of the implicit join it
    /// keeps (LOOKUP-25): one naming the driver reads one value of its record
    /// (ROW-5, NAME-4), and a bare one a column of the row (COLUMN-4). A
    /// read that is no implicit join, once it binds with no finding since
    /// `start`, refuses `require_match` and `between`
    /// ([`super::declared::join_options`]), unless it reads a lookup whose
    /// matching binding defers ([`Self::deferred`]). True when the binding
    /// binds.
    pub(super) fn source(&self, e: ExprId, start: usize, findings: &mut Vec<BindFinding>) -> bool {
        let spec = self.spec;
        let ExprKind::Source(source) = &spec.expr(e).kind else {
            return true;
        };
        let Some(variable) = name(spec, source.variable) else {
            return true;
        };
        let at = path(spec, e);
        let bound = self.read(&at, variable, findings);
        let owner = variable.split_once('.').map(|(owner, _)| owner);
        let lookup = owner.and_then(|owner| self.lookup(owner));
        if owner.is_none_or(|owner| lookup.is_some() || !self.joined(owner)) {
            if bound && findings.len() == start && lookup.is_none_or(|index| !self.deferred(index))
            {
                super::declared::join_options(spec, e, findings);
            }
        } else if let Some(value) = source
            .between
            .as_ref()
            .filter(|_| bound)
            .and_then(|between| name(spec, between.value))
        {
            let at = format!("{at}.between.value");
            match value.split_once('.') {
                Some((owner, _)) if self.drivers.contains(&owner) => {
                    self.group(&at, value, findings);
                }
                Some(_) => {}
                None => {
                    self.bare(&at, value, findings);
                }
            }
        }
        bound && findings.len() == start
    }

    /// Each key match value of the lookup at `index`, at the path binding
    /// cites it, with the names it reads; `None` when the declarations do not
    /// decide them: keys inferred from the captured schemas.
    fn keys(&self, index: usize) -> Option<Vec<(String, Vec<&'a str>)>> {
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
            _ => return None,
        };
        let at = render(spec, item.meta);
        fields
            .into_iter()
            .map(|field| {
                let value = self
                    .prepared
                    .intermediates
                    .match_declaration(&item.id.name, field)?;
                // A name written as the value is cited at the key, any other
                // value at its own entry.
                let path = match value {
                    match_values::Declaration::Variable(_) => format!("{at}.key"),
                    _ => format!("{at}.key.{field}"),
                };
                Some((path, value.references().collect()))
            })
            .collect()
    }

    /// The output columns the lookup at `index` matches on, sorted: those its
    /// key values and range value read, and those of each lookup a range
    /// value reads. `None` when a lookup on the way has inferred keys.
    fn dependencies(
        &self,
        index: usize,
        followed: &mut Vec<usize>,
        out: &mut BTreeSet<usize>,
    ) -> Option<()> {
        if followed.contains(&index) {
            return Some(());
        }
        followed.push(index);
        for (_, reads) in self.keys(index)? {
            out.extend(reads.into_iter().filter_map(|name| self.output(name)));
        }
        let Some(between) = &self.spec.intermediates[index].between else {
            return Some(());
        };
        if let ExprKind::Ref(name) = &self.spec.expr(between.value).kind {
            match name.owner() {
                None => out.extend(self.output(name.as_str())),
                Some(owner) => {
                    if let Some(next) = self.lookup(owner) {
                        self.dependencies(next, followed, out)?;
                    }
                }
            }
        }
        Some(())
    }

    /// The value sites through which the lookup at `index` matches on
    /// `column`: each range value followed into a further lookup, then the
    /// value that reads the column, as binding names them.
    fn sites(&self, index: usize, column: usize, followed: &mut Vec<usize>) -> Option<Vec<String>> {
        if followed.contains(&index) {
            return None;
        }
        followed.push(index);
        if let Some((path, _)) = self
            .keys(index)?
            .into_iter()
            .find(|(_, reads)| reads.iter().any(|&name| self.output(name) == Some(column)))
        {
            return Some(vec![path]);
        }
        let between = self.spec.intermediates[index].between.as_ref()?;
        let value = path(self.spec, between.value);
        let ExprKind::Ref(name) = &self.spec.expr(between.value).kind else {
            return None;
        };
        match name.owner() {
            None => (self.output(name.as_str()) == Some(column)).then(|| vec![value]),
            Some(owner) => {
                let mut sites = vec![value];
                sites.extend(self.sites(self.lookup(owner)?, column, followed)?);
                Some(sites)
            }
        }
    }

    /// Whether another lookup's derivations read the lookup at `index`:
    /// binding then defers its matching and refuses a read of it on other
    /// grounds.
    fn deferred(&self, index: usize) -> bool {
        let spec = self.spec;
        let item = &spec.intermediates[index];
        spec.intermediates
            .iter()
            .enumerate()
            .any(|(reader, other)| {
                other.bindings.iter().any(|binding| {
                    spec.reads(binding.derivation.expr, Scope::Donor(reader))
                        .iter()
                        .any(|read| read.name.owner() == Some(item.id.name.as_str()))
                })
            })
    }

    /// The read at `path` of the lookup at `index`, as binding admits it
    /// there (`row_intermediates::bind`). True when it binds, as far as the
    /// declarations decide; a read that does not marks the reads unbound
    /// ([`Self::bound`]).
    fn lookup_read(&self, index: usize, path: &str, findings: &mut Vec<BindFinding>) -> bool {
        let bound = self.lookup_binds(index, path, findings);
        if !bound {
            self.unbound.set(true);
        }
        bound
    }

    /// A lookup whose key types fail binds nothing, and a 1.0 row driver,
    /// which declares no key, fails at its `key` wherever it is read as a
    /// lookup (LOOKUP-3, ROW-2). Otherwise each key value in order, refused
    /// when it reads a column the row template leaves to the column phase
    /// (LOOKUP-2) or another input's record than the driver's (LOOKUP-5);
    /// then, once its keys are admitted, each other column it matches on that
    /// the template leaves there, naming each range value followed
    /// (LOOKUP-2). One site reports a phase boundary once, and it still
    /// refuses the read. A value read from the primary input's record is
    /// admitted for every row that reads the lookup once the rows are bound
    /// ([`super::scopes`]).
    fn lookup_binds(&self, index: usize, path: &str, findings: &mut Vec<BindFinding>) -> bool {
        let spec = self.spec;
        let item = &spec.intermediates[index];
        if self.refused.contains(&index) {
            return false;
        }
        if item.key.is_none() && !spec.header.surface.infers_join_keys() {
            findings.push(BindFinding::Lookup(LookupFinding::from_diagnostic(
                crate::diagnostic::lookup::key_required(&format!(
                    "{}.key",
                    render(spec, item.meta)
                )),
            )));
            return false;
        }
        let Some(keys) = self.keys(index).filter(|_| !self.deferred(index)) else {
            return true;
        };
        let primary = self.prepared.source().name.as_str();
        // A bare value that names no column, or a value qualified by no
        // input, fails the lookup's own binding.
        if keys
            .iter()
            .flat_map(|(_, reads)| reads)
            .any(|name| match name.split_once('.') {
                None => self.output(name).is_none(),
                Some((owner, _)) => !self.input(owner),
            })
        {
            return false;
        }
        let push = |finding: BindFinding, findings: &mut Vec<BindFinding>| {
            if !row_intermediates::reported(findings, &finding) {
                findings.push(finding);
            }
        };
        let start = findings.len();
        let mut refused = false;
        for (at, reads) in &keys {
            for &name in reads {
                match (name.split_once('.'), self.output(name), self.row) {
                    (None, Some(column), Some((row, _))) if !self.available(column) => {
                        let finding =
                            row_intermediates::match_phase_boundary(path, name, &item.id.name, row);
                        push(finding, findings);
                        refused = true;
                    }
                    (Some((owner, _)), _, _)
                        if owner != primary
                            && self.drivers.iter().any(|&driver| driver != owner) =>
                    {
                        findings.push(BindFinding::Lookup(LookupFinding::reference(
                            at,
                            name,
                            Some(&item.id.name),
                            ReferenceCause::OutputKey,
                            None,
                        )));
                    }
                    _ => {}
                }
            }
        }
        if refused || findings.len() != start {
            return false;
        }
        let Some((row, _)) = self.row else {
            return true;
        };
        let mut dependencies = BTreeSet::new();
        if self
            .dependencies(index, &mut Vec::new(), &mut dependencies)
            .is_none()
        {
            return true;
        }
        let mut bound = true;
        for column in dependencies
            .into_iter()
            .filter(|&column| !self.available(column))
        {
            let BindFinding::Lookup(mut finding) = row_intermediates::match_phase_boundary(
                path,
                &spec.columns[column].name.name,
                &item.id.name,
                row,
            ) else {
                continue;
            };
            if let Some(sites) = self
                .sites(index, column, &mut Vec::new())
                .filter(|sites| sites.len() > 1)
            {
                finding.extend_paths(sites);
            }
            push(BindFinding::Lookup(finding), findings);
            bound = false;
        }
        bound
    }

    /// A grouped row template's filter, before its derivations bind: each
    /// qualified name it reads, once, names no value of a group (ROW-6).
    pub(super) fn grouped_filter(
        &self,
        filter: ExprId,
        path: &str,
        findings: &mut Vec<BindFinding>,
    ) {
        let (Some((row, _)), Phase::Row { group_by: Some(_) }) = (self.row, self.phase) else {
            return;
        };
        let Some(plan) = compiled(self.spec, filter) else {
            return;
        };
        for identifier in plan.identifiers().filter(|name| name.contains('.')) {
            findings.push(BindFinding::RowFilterPhase {
                path: path.into(),
                identifier: identifier.into(),
                row: row.into(),
                grouped: true,
                qualified: true,
            });
        }
    }

    /// A row template's filter, after its derivations bind, each name once
    /// (ROW-6): a bare name its template does not derive fails, and an
    /// ungrouped filter reads a lookup, and another input through its
    /// implicit join ([`Self::join`]), as a derivation does. Once nothing
    /// refuses it, an ungrouped filter fails at each read of a window result,
    /// directly or through a lookup's match, which completes after the filter
    /// runs. `deferred` holds the template's columns that complete after its
    /// window pass; `None` when the declarations leave one undecided. True
    /// when binding binds the filter as an ungrouped template's predicate.
    pub(super) fn filter(
        &self,
        filter: ExprId,
        path: &str,
        deferred: Option<&[bool]>,
        findings: &mut Vec<BindFinding>,
    ) -> bool {
        let (Some((row, _)), Phase::Row { group_by }) = (self.row, self.phase) else {
            return false;
        };
        let Some(plan) = compiled(self.spec, filter) else {
            return false;
        };
        let grouped = group_by.is_some();
        let start = findings.len();
        for identifier in plan.identifiers() {
            match identifier.split_once('.') {
                // A grouped filter's qualified names were refused before the
                // derivations bound.
                Some(_) if grouped => {}
                Some((owner, _)) => {
                    if let Some(index) = self.lookup(owner) {
                        self.lookup_read(index, path, findings);
                    } else if self.joined(owner) {
                        self.join(path, owner, findings);
                    }
                }
                None => {
                    if self
                        .output(identifier)
                        .is_none_or(|column| !self.available(column))
                    {
                        findings.push(BindFinding::RowFilterPhase {
                            path: path.into(),
                            identifier: identifier.into(),
                            row: row.into(),
                            grouped,
                            qualified: false,
                        });
                    }
                }
            }
        }
        if grouped || findings.len() != start || !self.bound() {
            return false;
        }
        let Some(deferred) = deferred else {
            return true;
        };
        let mut late = Vec::new();
        for identifier in plan.identifiers() {
            match identifier.split_once('.') {
                None => {
                    if self
                        .output(identifier)
                        .is_some_and(|column| deferred[column])
                    {
                        late.push((identifier, None));
                    }
                }
                Some((owner, _)) => {
                    // A driver field is read from the row's record, and
                    // another input's join matches on the driver's fields.
                    let Some(index) = self.lookup(owner) else {
                        continue;
                    };
                    match self.lookup_late(index, deferred) {
                        Some((true, lookup_match)) => late.push((identifier, lookup_match)),
                        Some((false, _)) => {}
                        None => return true,
                    }
                }
            }
        }
        for (identifier, lookup_match) in late {
            let mut context: crate::diagnostic::Context = [
                (
                    "identifier".into(),
                    crate::json::Value::String(identifier.into()),
                ),
                ("row".into(), crate::json::Value::String(row.into())),
                (
                    "available_phase".into(),
                    crate::json::Value::String("window_derivation".into()),
                ),
                (
                    "required_phase".into(),
                    crate::json::Value::String("row_filter".into()),
                ),
            ]
            .into();
            if let Some(column) = lookup_match {
                context.insert(
                    "lookup_match".into(),
                    crate::json::Value::String(self.spec.columns[column].name.name.clone()),
                );
            }
            findings.push(BindFinding::Scalar(crate::diagnostic::Diagnostic {
                code: crate::diagnostic::ConditionCode::RowFilterPhaseBoundary,
                spec_paths: vec![path.into()],
                context,
                source_span: None,
                operand_route: None,
            }));
        }
        // A read the window pass completes refuses the filter after it bound.
        true
    }

    /// Whether the lookup at `index` completes only after the template's
    /// window pass, with the first column it matches on that does
    /// (`dataset::read_requires_window`): one it matches on, a key value that
    /// is a window, or a range value that reads a lookup derived after it.
    /// `None` when a lookup on the way has inferred keys.
    fn lookup_late(&self, index: usize, deferred: &[bool]) -> Option<(bool, Option<usize>)> {
        let mut dependencies = BTreeSet::new();
        self.dependencies(index, &mut Vec::new(), &mut dependencies)?;
        if let Some(column) = dependencies.into_iter().find(|&column| deferred[column]) {
            return Some((true, Some(column)));
        }
        let spec = self.spec;
        let order = self.prepared.intermediates.names().collect::<Vec<_>>();
        let position = |index: usize| {
            order
                .iter()
                .position(|&name| name == spec.intermediates[index].id.name)
        };
        let mut pending = vec![index];
        let mut seen = BTreeSet::new();
        while let Some(index) = pending.pop() {
            if !seen.insert(index) {
                continue;
            }
            let item = &spec.intermediates[index];
            if let Some(JoinKey::Pairs { pairs, .. }) = &item.key {
                if pairs.iter().any(|&(_, value)| windowed(spec, value)) {
                    return Some((true, None));
                }
            }
            let chained = item
                .between
                .as_ref()
                .and_then(|between| name(spec, between.value))
                .and_then(|value| value.split_once('.'))
                .and_then(|(owner, _)| self.lookup(owner));
            if let Some(child) = chained {
                if position(child) >= position(index) {
                    return Some((true, None));
                }
                pending.push(child);
            }
        }
        Some((false, None))
    }

    /// Which of a row template's columns complete after its window pass,
    /// as `dataset::window_stages` derives them: `derivations` are the
    /// template's derivations that bind, in dependency order, each a window,
    /// or a reader of a later column, of a lookup that matches on one, or of
    /// a window key. `None` when the declarations leave one undecided.
    pub(super) fn stages(&self, derivations: &[(usize, ExprId)]) -> Option<Vec<bool>> {
        let mut deferred = vec![false; self.spec.columns.len()];
        for &(column, root) in derivations {
            deferred[column] = self.late(root, &deferred)?;
        }
        Some(deferred)
    }

    /// Whether `e` reads a window result, as `dataset::expression_deferred`
    /// decides it for the expression binding builds from it.
    fn late(&self, e: ExprId, deferred: &[bool]) -> Option<bool> {
        let spec = self.spec;
        let any = |children: &mut dyn Iterator<Item = ExprId>| -> Option<bool> {
            for child in children {
                if self.late(child, deferred)? {
                    return Some(true);
                }
            }
            Some(false)
        };
        let names = |e: ExprId| -> Option<bool> {
            for read in spec.reads(e, Scope::Row) {
                if self.late_read(read.name.as_str(), deferred)? {
                    return Some(true);
                }
            }
            Some(false)
        };
        match &spec.expr(e).kind {
            ExprKind::Window(_) => Some(true),
            // A row aggregate reduces its group's records.
            ExprKind::Literal(_)
            | ExprKind::Odm(_)
            | ExprKind::Aggregate(_)
            | ExprKind::Unsupported(_) => Some(false),
            ExprKind::Ref(name) => self.late_read(name.as_str(), deferred),
            ExprKind::Source(source) => {
                let variable = name(spec, source.variable).unwrap_or_default();
                if self.late_read(variable, deferred)? {
                    return Some(true);
                }
                match source.between.as_ref().and_then(|b| name(spec, b.value)) {
                    Some(value) => self.late_read(value, deferred),
                    None => Some(false),
                }
            }
            ExprKind::Formula(_) | ExprKind::Flag { .. } => names(e),
            ExprKind::Case(arms) => {
                for arm in arms {
                    if let Some(condition) = arm.condition {
                        if names(condition)? {
                            return Some(true);
                        }
                    }
                    if self.late(arm.result, deferred)? {
                        return Some(true);
                    }
                }
                Some(false)
            }
            ExprKind::FirstAvailable { sources, .. } => any(&mut sources.iter().copied()),
            ExprKind::Scalar { operands, .. } => {
                any(&mut operands.iter().map(|operand| operand.expr))
            }
            ExprKind::Call { args, .. } => any(&mut args.iter().map(|&(_, value)| value)),
        }
    }

    /// Whether a read of `name` reads a window result: a later column, or a
    /// lookup that completes after the window pass. A driver field reads the
    /// row's record, and another input's join matches on the driver's fields.
    fn late_read(&self, name: &str, deferred: &[bool]) -> Option<bool> {
        match name.split_once('.') {
            None => Some(self.output(name).is_some_and(|column| deferred[column])),
            Some((owner, _)) => match self.lookup(owner) {
                Some(index) => self.lookup_late(index, deferred).map(|(late, _)| late),
                None => Some(false),
            },
        }
    }
}

/// A predicate's compiled text, unless its grammar failed.
fn compiled(spec: &Spec, e: ExprId) -> Option<&CompiledExpression> {
    match &spec.expr(e).kind {
        ExprKind::Formula(ast::Formula {
            compiled: Ok(plan), ..
        }) => Some(plan),
        _ => None,
    }
}

/// Whether a key's written value is or holds a window, whose result needs
/// the window pass of the rows that read the lookup.
fn windowed(spec: &Spec, value: ExprId) -> bool {
    let mut found = false;
    spec.walk(value, Scope::Row, |e, _| {
        found |= matches!(spec.expr(e).kind, ExprKind::Window(_));
        if found {
            crate::ast::Step::Stop
        } else {
            crate::ast::Step::Continue
        }
    });
    found
}
