//! What binding decides of each read of a lookup's own derivations
//! (`intermediate_derivations::bind`) and of an aggregate's derive bindings
//! (`aggregates::bind_derivations`) from the declarations alone (#3474,
//! owner decision 3 on #2844). Both bind over the records of one relation,
//! the donor, as an ungrouped row phase that joins no other input, so a
//! structured source's `require_match` and `between` fail once its read
//! binds (LOOKUP-24, LOOKUP-9).
//!
//! A lookup's derivation reads its donor record (the fields its input
//! stores, or the completed row over `SELF`, and the derivations before it),
//! the current record of any input, and the lookups derived before it.
//! Binding refuses, at the read:
//!
//! - a derivation not yet derived, or a record no input or lookup is named
//!   by (`unknown_field`, LOOKUP-15); in a flag or `case` predicate, a
//!   qualified one is unknown to the predicate (FORMULA-9), and so is a
//!   flag's read of a lookup;
//! - a window's read of anything but the donor record (LOOKUP-15), or of a
//!   lookup (LOOKUP-16);
//! - a read of a lookup over `SELF` (`phase_boundary`, LOOKUP-16), of a 1.0
//!   row driver, which declares no key (LOOKUP-3), and of a lookup whose key
//!   value reads another record than the donor's (`unknown_field`,
//!   LOOKUP-16).
//!
//! An aggregate's derive bindings read the relation it reduces and the
//! bindings before them, which preparation admits (LOOKUP-22).
//!
//! Binding keeps what the declarations alone cannot decide: a field the
//! donor or an input does not store, and a lookup whose key value reads a
//! bare name or an expression, which the donor record may supply.

use super::*;
use crate::diagnostic::ConditionCode as C;
use std::collections::BTreeSet;

/// How a donor scope types a name it reads, as binding's captured types do.
pub(super) type Typing<'a> = dyn Fn(&str) -> Option<ColumnType> + 'a;

/// The reads of one derivation over a donor's records.
pub(super) struct DonorReads<'a> {
    spec: &'a Spec,
    prepared: &'a PreparedSpecification,
    /// The relation whose records the derivations read: a lookup's
    /// `dataset`, or the relation an aggregate reduces.
    relation: &'a str,
    /// The lookup whose derivations these are; `None` for an aggregate's
    /// derive bindings.
    reader: Option<&'a str>,
    /// The names derived over the relation, in written order.
    derived: &'a [&'a str],
    /// How many of them precede the derivation being read.
    position: usize,
    /// The lookups whose key types fail, which binding does not bind.
    refused: &'a BTreeSet<usize>,
    typing: &'a Typing<'a>,
}

impl<'a> DonorReads<'a> {
    /// The reads of the derivation at `position` of the lookup `reader` over
    /// `relation`.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn lookup(
        spec: &'a Spec,
        prepared: &'a PreparedSpecification,
        relation: &'a str,
        reader: &'a str,
        derived: &'a [&'a str],
        position: usize,
        refused: &'a BTreeSet<usize>,
        typing: &'a Typing<'a>,
    ) -> Self {
        Self {
            spec,
            prepared,
            relation,
            reader: Some(reader),
            derived,
            position,
            refused,
            typing,
        }
    }

    /// The reads of the derive binding at `position` of an aggregate over
    /// `relation`.
    pub(super) fn derive(
        spec: &'a Spec,
        prepared: &'a PreparedSpecification,
        relation: &'a str,
        derived: &'a [&'a str],
        position: usize,
        refused: &'a BTreeSet<usize>,
        typing: &'a Typing<'a>,
    ) -> Self {
        Self {
            spec,
            prepared,
            relation,
            reader: None,
            derived,
            position,
            refused,
            typing,
        }
    }

    fn input(&self, owner: &str) -> bool {
        self.prepared
            .sources
            .iter()
            .any(|source| source.name == owner)
    }

    fn named(&self, owner: &str) -> Option<usize> {
        self.spec
            .intermediates
            .iter()
            .position(|item| item.id.name == owner)
    }

    /// Whether a lookup's derivation reads its donor's `field` before it is
    /// derived: at or after the derivation being read.
    fn later(&self, field: &str) -> bool {
        self.reader.is_some() && self.derived[self.position..].contains(&field)
    }

    /// Whether a lookup's derivation reads, qualified by `owner`, a record
    /// nothing names there: not its donor, an input or a lookup.
    fn undeclared(&self, owner: &str) -> bool {
        self.reader.is_some()
            && owner != self.relation
            && !self.input(owner)
            && self.named(owner).is_none()
    }

    /// A lookup's derivation names each read binding finds unknown there,
    /// such as a filter's field of a record nothing declares, as its own
    /// (LOOKUP-15, `intermediate_derivations::donor_scope_findings`).
    pub(super) fn name_unknown(&self, findings: &mut [BindFinding]) {
        if self.reader.is_none() {
            return;
        }
        for finding in findings {
            if let BindFinding::UnknownReference { path, name } = finding {
                *finding = self.refusal(C::IntermediateDerivationReference, path, name);
            }
        }
    }

    /// A lookup derivation's refusal at `path` of a read of `name`, with
    /// `code`: LOOKUP-15, or LOOKUP-16 for a window's read of a lookup.
    fn refusal(&self, code: C, path: &str, name: &str) -> BindFinding {
        let mut finding = LookupFinding::new(path, code);
        finding.text("intermediate", self.reader.unwrap_or_default());
        finding.text("identifier", name);
        BindFinding::Lookup(finding)
    }

    /// One read at `path`, as binding reads an operand there. True when it
    /// binds, as far as the declarations decide: an undecided read of a
    /// lookup does not count as bound.
    pub(super) fn read(&self, path: &str, name: &str, findings: &mut Vec<BindFinding>) -> bool {
        if self.reader.is_none() {
            return true;
        }
        let unknown = match name.split_once('.') {
            None => self.later(name),
            Some((owner, field)) if owner == self.relation => self.later(field),
            Some((owner, _)) => match self.named(owner) {
                Some(index) => return self.lookup_read(index, path, name, findings),
                None => !self.input(owner),
            },
        };
        if unknown {
            findings.push(self.refusal(C::IntermediateDerivationReference, path, name));
        }
        !unknown
    }

    /// On a 1.0 tree, the first read of the lookup at `index` that a donor
    /// read cannot give it, in binding's order
    /// (`intermediate_derivations::rebase`): a field of another input's
    /// record than the donor's in its correlated filter, its range value or a
    /// derivation's current fields; then, for a lookup a range value reads
    /// (not `direct`), one in its key values. A range value read through
    /// another lookup gives that lookup's first such read. A bare range value
    /// is a field of the donor record, which only its records decide
    /// (LOOKUP-16).
    fn unavailable(&self, index: usize, direct: bool, followed: &mut Vec<usize>) -> Option<String> {
        if followed.contains(&index) {
            return None;
        }
        followed.push(index);
        let spec = self.spec;
        let item = &spec.intermediates[index];
        let dataset = item.dataset.name.as_str();
        let foreign = |name: &str| {
            name.split_once('.').is_some_and(|(owner, _)| {
                owner != self.relation && self.input(owner) && self.named(owner).is_none()
            })
        };
        // A correlated filter reads each field of another input than its own.
        if let Some(ExprKind::Formula(ast::Formula {
            compiled: Ok(plan), ..
        })) = item.filter.map(|filter| &spec.expr(filter).kind)
        {
            let mut seen = BTreeSet::new();
            for identifier in plan.identifiers() {
                if seen.insert(identifier)
                    && identifier
                        .split_once('.')
                        .is_some_and(|(owner, _)| owner != dataset)
                    && foreign(identifier)
                {
                    return Some(identifier.to_owned());
                }
            }
        }
        if let Some(value) = item
            .between
            .as_ref()
            .and_then(|between| name(spec, between.value))
        {
            if foreign(value) {
                return Some(value.to_owned());
            }
            if let Some(chained) = value
                .split_once('.')
                .and_then(|(owner, _)| self.named(owner))
            {
                if let Some(identifier) = self.unavailable(chained, false, followed) {
                    return Some(identifier);
                }
            }
        }
        let scan = crate::specification::reads::Scan {
            relation: true,
            windows: true,
        };
        let calls = self.prepared.project_calls.is_some();
        for binding in &item.bindings {
            let references =
                crate::specification::reads::names(spec, binding.derivation.expr, scan, calls)
                    .unwrap_or_default();
            if let Some(reference) = references.into_iter().find(|reference| {
                reference
                    .split_once('.')
                    .is_some_and(|(owner, _)| owner != dataset)
                    && foreign(reference)
            }) {
                return Some(reference);
            }
        }
        if direct {
            return None;
        }
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
        fields.into_iter().find_map(|field| {
            self.prepared
                .intermediates
                .match_declaration(&item.id.name, field)?
                .references()
                .find(|reference| foreign(reference))
                .map(String::from)
        })
    }

    /// The read at `path` of the lookup at `index`, which a lookup's
    /// derivation reads as binding does there (`row_intermediates::bind`):
    /// a lookup whose key types fail binds nothing, so the read is unknown,
    /// as binding names a lookup's read there (LOOKUP-16,
    /// `intermediate_derivations::donor_scope_findings`); a lookup over
    /// `SELF` completes after the lookups (LOOKUP-16); then each key value
    /// that reads another record than the donor's is no value there
    /// (LOOKUP-16); then a 1.0 row driver, which declares no key, is no keyed
    /// lookup (LOOKUP-3). True when the read binds: each key value reads the
    /// donor's record, or is a key column of a lookup over the donor's own
    /// input, which that input stores. Any other bare name, which the donor
    /// record may store, and an expression leave it undecided.
    fn lookup_read(
        &self,
        index: usize,
        path: &str,
        name: &str,
        findings: &mut Vec<BindFinding>,
    ) -> bool {
        let spec = self.spec;
        let item = &spec.intermediates[index];
        if self.refused.contains(&index) {
            findings.push(self.refusal(C::IntermediateDerivationWindowReference, path, name));
            return false;
        }
        // A read the donor record cannot give leaves the lookup no value
        // there (LOOKUP-16).
        if let Some(identifier) = spec
            .header
            .surface
            .one_meaning_per_form()
            .then(|| self.unavailable(index, true, &mut Vec::new()))
            .flatten()
        {
            let mut finding = LookupFinding::new(path, C::IntermediateDerivationDependency);
            finding.text("intermediate", self.reader.unwrap_or_default());
            finding.text("identifier", &identifier);
            finding.text("read", &item.id.name);
            findings.push(BindFinding::Lookup(finding));
            return false;
        }
        if item.dataset.name == "SELF" {
            findings.push(self.refusal(C::IntermediateDerivationSelfPhase, path, name));
            return false;
        }
        let fields = match &item.key {
            Some(JoinKey::Columns { .. }) if item.dataset.name == self.relation => return true,
            Some(JoinKey::Columns { columns, .. }) => columns
                .iter()
                .map(|column| column.name.as_str())
                .collect::<Vec<_>>(),
            Some(JoinKey::Pairs { pairs, .. }) => {
                pairs.iter().map(|(field, _)| field.name.as_str()).collect()
            }
            _ if !spec.header.surface.infers_join_keys() => {
                findings.push(BindFinding::Lookup(LookupFinding::from_diagnostic(
                    crate::diagnostic::lookup::key_required(&format!(
                        "{}.key",
                        render(spec, item.meta)
                    )),
                )));
                return false;
            }
            _ => return false,
        };
        let mut decided = true;
        let mut refused = false;
        for field in fields {
            let Some(value) = self
                .prepared
                .intermediates
                .match_declaration(&item.id.name, field)
            else {
                decided = false;
                continue;
            };
            match value {
                match_values::Declaration::Variable(read)
                | match_values::Declaration::Source(read) => match read.split_once('.') {
                    Some((owner, _)) if owner == self.relation => {}
                    Some(_) => {
                        let mut finding =
                            LookupFinding::new(path, C::IntermediateDerivationDependency);
                        finding.text("intermediate", self.reader.unwrap_or_default());
                        finding.text("identifier", read);
                        finding.text("read", &item.id.name);
                        findings.push(BindFinding::Lookup(finding));
                        refused = true;
                    }
                    None => decided = false,
                },
                match_values::Declaration::Literal(..) => {}
                _ => decided = false,
            }
        }
        decided && !refused
    }

    /// A name of a flag's condition (`flag`) or of a `case` predicate. In a
    /// lookup's derivation a bare derivation not yet derived fails as a read
    /// does (LOOKUP-15), and a qualified name the predicate cannot read, one
    /// nothing names or a field not yet derived, is unknown to it
    /// (FORMULA-9), as is a flag's read of a lookup and any read of a lookup
    /// whose key types fail; a `case` predicate reads another lookup as any
    /// read does. True when it binds.
    pub(super) fn predicate(
        &self,
        path: &str,
        name: &str,
        flag: bool,
        findings: &mut Vec<BindFinding>,
    ) -> bool {
        if self.reader.is_none() {
            return true;
        }
        let Some((owner, field)) = name.split_once('.') else {
            return self.read(path, name, findings);
        };
        let lookup = self.named(owner);
        if (owner == self.relation && self.later(field))
            || self.undeclared(owner)
            || lookup.is_some_and(|index| flag || self.refused.contains(&index))
        {
            findings.push(super::predicate_unknown(path, name));
            return false;
        }
        lookup.is_none() || self.read(path, name, findings)
    }

    /// A window's read in a lookup's derivation, which reads the donor record
    /// alone: a lookup's field (LOOKUP-16), any other record's, or a
    /// derivation not yet derived (LOOKUP-15).
    pub(super) fn window(&self, path: &str, name: &str, findings: &mut Vec<BindFinding>) {
        if self.reader.is_none() {
            return;
        }
        let code = match name.split_once('.') {
            None if self.later(name) => C::IntermediateDerivationReference,
            None => return,
            Some((owner, field)) if owner == self.relation => {
                if !self.later(field) {
                    return;
                }
                C::IntermediateDerivationReference
            }
            Some((owner, _)) if self.named(owner).is_some() => {
                C::IntermediateDerivationWindowReference
            }
            Some(_) => C::IntermediateDerivationReference,
        };
        findings.push(self.refusal(code, path, name));
    }

    /// The type of a name the scope reads, as binding's captured types type
    /// it, when the declarations give one.
    pub(super) fn kind(&self, name: &str) -> Option<ColumnType> {
        (self.typing)(name)
    }
}
