//! Phase boundaries inside row templates, from each template's phase set
//! (#2754): a row-phase derivation reads a column its template leaves to
//! the column phase (COLUMN-4), or reads a lookup that matches on one
//! through its key or its range value (LOOKUP-2). Binding reports the same
//! findings at the same sites, so `check()` and `run()` agree on them.
//!
//! Binding keeps what the declarations alone cannot decide: a lookup whose
//! keys are inferred from the captured schemas, one whose key value reads
//! another input's record, one that another lookup's derivations read, and
//! a row filter's reads.

use super::*;
use crate::ast::{Scope, Step};
use std::collections::{BTreeMap, BTreeSet};

/// The phase boundaries of `root`, one row-phase derivation of `template`,
/// in the order binding reports them: each read in written order. True
/// when there is one, after which binding binds nothing more of the
/// derivation.
pub(super) fn derivation(
    spec: &Spec,
    template: &rows::TemplatePhase<'_>,
    root: ExprId,
    findings: &mut Vec<BindFinding>,
) -> bool {
    // Preparation refuses a bare read of a column outside the row phase.
    if matches!(&spec.expr(root).kind, ExprKind::Ref(name) if name.owner().is_none()) {
        return false;
    }
    let start = findings.len();
    let phase = Phase {
        spec,
        template,
        available: template
            .derivations
            .iter()
            .map(|derivation| derivation.column)
            .collect(),
    };
    for (path, name) in phase.reads(root) {
        match name.split_once('.') {
            None => {
                if let Some(column) = phase.output(name).filter(|&c| !phase.available(c)) {
                    findings.push(BindFinding::OutputReference {
                        path,
                        name: name.into(),
                        row: Some(template.id.into()),
                        finding: crate::reference_binding::Diagnostic::PhaseBoundary { column },
                    });
                }
            }
            // A read qualified by the template's own driver reads its record.
            Some((owner, _)) if owner != template.driver => {
                if let Some(lookup) = phase.lookup(owner) {
                    phase.lookup_read(lookup, &path, findings);
                }
            }
            Some(_) => {}
        }
    }
    findings.len() != start
}

struct Phase<'a> {
    spec: &'a Spec,
    template: &'a rows::TemplatePhase<'a>,
    available: BTreeSet<usize>,
}

impl<'a> Phase<'a> {
    fn available(&self, column: usize) -> bool {
        self.available.contains(&column)
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

    fn lookup(&self, owner: &str) -> Option<usize> {
        self.spec
            .intermediates
            .iter()
            .position(|item| item.id.name == owner)
    }

    /// The names `root` reads in the row's scope, in written order, with
    /// where binding cites each: its own path, or a template's text for a
    /// placeholder. An ODM item names its dataset instead.
    fn reads(&self, root: ExprId) -> Vec<(String, &'a str)> {
        let spec = self.spec;
        let mut reads = Vec::new();
        let mut items = BTreeSet::new();
        let mut placeholders = BTreeMap::new();
        spec.walk(root, Scope::Row, |at, scope| {
            match &spec.expr(at).kind {
                ExprKind::Ref(name) if scope == Scope::Row && !items.contains(&at) => {
                    let at = placeholders.remove(&at).unwrap_or_else(|| path(spec, at));
                    reads.push((at, name.as_str()));
                }
                ExprKind::Odm(read) => {
                    items.insert(read.item);
                }
                ExprKind::Scalar {
                    op: ast::ScalarOp::StrTemplate { .. },
                    operands,
                    ..
                } => {
                    let text = format!("{}.template", path(spec, at));
                    for operand in operands {
                        placeholders.insert(operand.expr, text.clone());
                    }
                }
                _ => {}
            }
            Step::Continue
        });
        reads
    }

    /// Each key match value of the lookup at `index` with where binding
    /// cites it, or `None` when the declarations do not decide them: keys
    /// inferred from the captured schemas.
    fn keys(&self, index: usize) -> Option<Vec<(String, Vec<&'a str>)>> {
        let spec = self.spec;
        match &spec.intermediates[index].key {
            Some(JoinKey::Columns { columns, meta }) => Some(
                columns
                    .iter()
                    .map(|column| (render(spec, *meta), vec![column.name.as_str()]))
                    .collect(),
            ),
            Some(JoinKey::Pairs { pairs, meta }) => Some(
                pairs
                    .iter()
                    .map(|(field, value)| {
                        // A bare name is cited at the key, a written value at
                        // its own entry.
                        let at = match spec.expr(*value).kind {
                            ExprKind::Ref(_) => *meta,
                            _ => field.meta,
                        };
                        let reads = self.reads(*value).into_iter().map(|(_, name)| name);
                        (render(spec, at), reads.collect())
                    })
                    .collect(),
            ),
            _ => None,
        }
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

    /// The template's read at `path` of the lookup at `index` (LOOKUP-2):
    /// a key value that reads a column the template leaves to the column
    /// phase, then any other column the lookup matches on that the template
    /// leaves there, naming each range value followed. One site reports a
    /// finding once.
    fn lookup_read(&self, index: usize, path: &str, findings: &mut Vec<BindFinding>) {
        let spec = self.spec;
        let item = &spec.intermediates[index];
        // Another lookup's derivations read this one, so binding defers its
        // matching and refuses this read on other grounds.
        let deferred = spec
            .intermediates
            .iter()
            .enumerate()
            .any(|(reader, other)| {
                other.bindings.iter().any(|binding| {
                    spec.reads(binding.derivation.expr, Scope::Donor(reader))
                        .iter()
                        .any(|read| read.name.owner() == Some(item.id.name.as_str()))
                })
            });
        let Some(keys) = self.keys(index).filter(|_| !deferred) else {
            return;
        };
        // A key value that reads another input's record is driver scope,
        // which binding decides first.
        if keys.iter().flat_map(|(_, reads)| reads).any(|name| {
            name.split_once('.')
                .is_some_and(|(owner, _)| owner != self.template.driver)
        }) {
            return;
        }
        let push = |finding: BindFinding, findings: &mut Vec<BindFinding>| {
            if !row_intermediates::reported(findings, &finding) {
                findings.push(finding);
            }
        };
        let mut refused = false;
        for name in keys.iter().flat_map(|(_, reads)| reads) {
            if self
                .output(name)
                .is_some_and(|column| !self.available(column))
            {
                let finding = row_intermediates::match_phase_boundary(
                    path,
                    name,
                    &item.id.name,
                    self.template.id,
                );
                push(finding, findings);
                refused = true;
            }
        }
        if refused {
            return;
        }
        let mut dependencies = BTreeSet::new();
        if self
            .dependencies(index, &mut Vec::new(), &mut dependencies)
            .is_none()
        {
            return;
        }
        for column in dependencies
            .into_iter()
            .filter(|&column| !self.available(column))
        {
            let BindFinding::Lookup(mut finding) = row_intermediates::match_phase_boundary(
                path,
                &spec.columns[column].name.name,
                &item.id.name,
                self.template.id,
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
        }
    }
}
