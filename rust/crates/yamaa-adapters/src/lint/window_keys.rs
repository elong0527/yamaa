//! What each output key stands for when `window_tie` asks whether a window's
//! terms tell the output rows apart.
//!
//! The keys are read through the column derivations of the specification as
//! the engine lowers it ([`yamaa_core::front_end::v1::spec::lower`]), with the
//! names each derivation reads as the engine's traversal reports them
//! ([`yamaa_core::ast::Spec::reads`]), after named windows expand: a key whose every derivation is a
//! literal is the same on every row and tells none apart; a key that reads,
//! directly or through other columns, a column `row_number` derives stands
//! for that numbering's `group_by` and `order_by` terms, since the number is
//! given by them; and a key that reads the column the judged window derives
//! cannot be one of its terms, so the window is not judged. Any other key
//! stands for itself.

use std::collections::{BTreeMap, BTreeSet};

use yamaa_core::ast::{ExprId, ExprKind, Scope, Spec, WindowFunction};
use yamaa_core::schema::Document as Values;

/// How one derivation of a column reads.
enum Derivation {
    /// A constant: the same value on every row.
    Literal,
    /// A `row_number`, with its window's `group_by` and `order_by` terms.
    Numbering(Vec<String>),
    /// Any other derivation, with the bare names it reads.
    Reads(BTreeSet<String>),
}

/// What a key stands for in the judged window.
pub(super) enum Resolved {
    /// The key itself.
    Itself,
    /// Nothing: a constant tells no rows apart.
    Constant,
    /// The terms of the numberings it reads.
    Terms(Vec<String>),
    /// The key reads the judged window's own column, so the window is not
    /// judged.
    Circular,
}

/// The column derivations of a specification's output rows.
#[derive(Default)]
pub(super) struct Columns {
    derivations: BTreeMap<String, Vec<Derivation>>,
}

/// The name a term expression reads.
fn term(spec: &Spec, expr: ExprId) -> Option<String> {
    match &spec.expr(expr).kind {
        ExprKind::Ref(name) => Some(name.as_str().to_owned()),
        _ => None,
    }
}

fn derivation(spec: &Spec, expr: ExprId) -> Derivation {
    match &spec.expr(expr).kind {
        ExprKind::Literal(_) => Derivation::Literal,
        ExprKind::Window(window) if window.function == WindowFunction::RowNumber => {
            let mut named: Vec<String> = window
                .group_by
                .iter()
                .filter_map(|&item| term(spec, item))
                .collect();
            named.extend(
                window
                    .order_by
                    .iter()
                    .filter_map(|item| term(spec, item.variable)),
            );
            Derivation::Numbering(named)
        }
        _ => Derivation::Reads(
            spec.reads(expr, Scope::Row)
                .into_iter()
                .filter(|read| read.scope == Scope::Row && read.name.owner().is_none())
                .map(|read| read.name.as_str().to_owned())
                .collect(),
        ),
    }
}

impl Columns {
    /// The column derivations of `values`, a specification's root, as the
    /// engine lowers it: each column's `derivation` and each row template's
    /// `derivations`.
    pub(super) fn read(values: &Values) -> Self {
        // Named windows expand first, as preparation expands them, so a
        // numbering through a named window reads its terms (WINDOW-2).
        let expanded = super::canonical::view(values)
            .and_then(|(schema, _)| schema.expand_named_windows(values, false).ok());
        let values = expanded
            .as_ref()
            .map_or(values, |expanded| &expanded.document);
        let lowered = yamaa_core::front_end::v1::spec::lower(values, None);
        let spec = lowered.spec();
        let mut columns = Self::default();
        let written = spec
            .columns
            .iter()
            .filter_map(|column| Some((&column.name.name, column.derivation.as_ref()?)))
            .chain(spec.rows.templates.iter().flat_map(|template| {
                template
                    .overrides
                    .iter()
                    .map(|(name, derivation)| (&name.name, derivation))
            }));
        for (name, written) in written {
            columns
                .derivations
                .entry(name.clone())
                .or_default()
                .push(derivation(spec, written.expr));
        }
        columns
    }

    /// The columns `key` reads, itself included, through every derivation.
    fn closure(&self, key: &str) -> BTreeSet<String> {
        let mut seen = BTreeSet::new();
        let mut pending = vec![key.to_owned()];
        while let Some(column) = pending.pop() {
            if !seen.insert(column.clone()) {
                continue;
            }
            for derivation in self.derivations.get(&column).into_iter().flatten() {
                if let Derivation::Reads(read) = derivation {
                    pending.extend(read.iter().cloned());
                }
            }
        }
        seen
    }

    /// What `key` stands for in a window that derives the column `deriving`.
    pub(super) fn resolve(&self, key: &str, deriving: Option<&str>) -> Resolved {
        let closure = self.closure(key);
        if deriving.is_some_and(|column| closure.contains(column)) {
            return Resolved::Circular;
        }
        let own = self
            .derivations
            .get(key)
            .map(Vec::as_slice)
            .unwrap_or_default();
        if !own.is_empty() && own.iter().all(|item| matches!(item, Derivation::Literal)) {
            return Resolved::Constant;
        }
        let mut numbered = Vec::new();
        for column in &closure {
            for derivation in self.derivations.get(column).into_iter().flatten() {
                if let Derivation::Numbering(named) = derivation {
                    for term in named {
                        if !numbered.contains(term) {
                            numbered.push(term.clone());
                        }
                    }
                }
            }
        }
        if numbered.is_empty() {
            Resolved::Itself
        } else {
            Resolved::Terms(numbered)
        }
    }
}
