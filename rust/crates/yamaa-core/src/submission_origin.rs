//! What a column's derivation proves about its declared origin. The derivation
//! refutes an origin and never supplies one (SUB-19), and a Predecessor names
//! the column it copies (SUB-16, SUB-20). Shared by the domain build and by
//! composition, which both read each specification's typed tree; no study,
//! host or data access.
use crate::{
    ast::{self, ExprId, ExprKind, Slot},
    cdisc_vocabulary::OriginType,
    expression::{self, NodeKind, Reducer},
    scalar_verb::Recode,
    schema::{Document, DocumentNode as N},
};
use std::collections::{BTreeMap, BTreeSet};

/// The four derivation rows of SUB-19's table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DerivationKind {
    /// A bare read: a `source`, an `odm` read or an `ONLY` pivot of one
    /// variable, none through an intermediate that computes. Nothing was
    /// computed.
    Source,
    /// A bare `literal`: the same value in every row, read from no input.
    Literal,
    /// A copy, recode, reformat or assembly of declared values.
    Recode,
    /// Every other derivation computes its value.
    Computed,
}

impl DerivationKind {
    /// The name diagnostics report for the derivation that refutes an origin.
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Source => "source",
            Self::Literal => "literal",
            Self::Recode => "recode",
            Self::Computed => "computed",
        }
    }
    /// Whether this derivation leaves the origin type unrefuted.
    pub(crate) fn admits(self, kind: OriginType) -> bool {
        use OriginType as T;
        match self {
            Self::Source => true,
            Self::Literal => !matches!(kind, T::Derived | T::Predecessor),
            Self::Recode => !matches!(kind, T::Protocol | T::Predecessor),
            Self::Computed => !matches!(kind, T::Collected | T::Protocol | T::Predecessor),
        }
    }
    /// The admitted origin type names, in the vocabulary's published order.
    pub(crate) fn admitted(self) -> Vec<&'static str> {
        OriginType::all()
            .filter(|&kind| self.admits(kind))
            .map(OriginType::name)
            .collect()
    }
}

/// The names a derivation may read, from the specification's declarations: the
/// declared input datasets and the named intermediates that look records up.
#[derive(Clone, Debug, Default)]
pub(crate) struct Reads {
    inputs: BTreeSet<String>,
    /// Plain lookups: named intermediates over a declared input that declare
    /// no `derivations`, so every field read through one is a stored value of
    /// the record it selects.
    plain: BTreeSet<String>,
    /// The plain lookups that select records of one input dataset, by name; a
    /// pair relation reads two (SUB-20).
    selections: BTreeMap<String, Selection>,
    /// Every other named intermediate: one that declares `derivations` or
    /// reads `SELF`, the completed output rows.
    computing: BTreeSet<String>,
}

/// The input dataset a plain lookup selects a record of, and whether the
/// lookup declares a `filter`.
#[derive(Clone, Debug)]
struct Selection {
    dataset: String,
    filtered: bool,
}

impl Reads {
    /// The inputs and intermediates a specification's typed tree declares.
    pub(crate) fn of_spec(spec: &ast::Spec) -> Self {
        let inputs: BTreeSet<String> = spec
            .inputs
            .iter()
            .map(|input| input.name.name.clone())
            .collect();
        let mut plain = BTreeSet::new();
        let mut selections = BTreeMap::new();
        let mut computing = BTreeSet::new();
        for item in &spec.intermediates {
            if inputs.contains(&item.dataset.name) && item.bindings.is_empty() {
                plain.insert(item.id.name.clone());
                if !item.pairs() {
                    let selection = Selection {
                        dataset: item.dataset.name.clone(),
                        filtered: item.filter.is_some(),
                    };
                    selections.insert(item.id.name.clone(), selection);
                }
            } else {
                computing.insert(item.id.name.clone());
            }
        }
        Self {
            inputs,
            plain,
            selections,
            computing,
        }
    }
    /// A read that looks no record up through a named intermediate that
    /// computes: a plain lookup copies a stored value, as a bare read does.
    fn bare(&self, name: &str) -> bool {
        name.split_once('.')
            .is_none_or(|(qualifier, _)| !self.computing.contains(qualifier))
    }
    /// A read of a value a declared input dataset carries, directly or
    /// through a plain lookup of one.
    fn input(&self, name: &str) -> bool {
        name.split_once('.').is_some_and(|(qualifier, _)| {
            self.inputs.contains(qualifier) || self.plain.contains(qualifier)
        })
    }
    fn reads(&self, name: &str, strict: bool) -> bool {
        if strict {
            self.input(name)
        } else {
            self.bare(name)
        }
    }
}

fn text(document: &Document, node: usize) -> Option<&str> {
    match document.nodes().get(node)? {
        N::Text(value) => Some(value),
        _ => None,
    }
}
fn items(document: &Document, node: Option<usize>) -> &[usize] {
    match node.map(|n| &document.nodes()[n]) {
        Some(N::Sequence(items)) => items,
        _ => &[],
    }
}
/// The variable an aggregate formula pivots: exactly `ONLY(<variable>)`.
fn only(expr: &str) -> Option<String> {
    let parsed = expression::parse(expr).ok()?;
    let root = parsed.ungroup(parsed.root());
    let NodeKind::Reduction {
        reducer: Reducer::Only,
        operand,
        ..
    } = parsed.nodes()[root].kind
    else {
        return None;
    };
    let operand = parsed.ungroup(operand);
    (parsed.nodes()[operand].kind == NodeKind::Identifier).then(|| parsed.text(operand).into())
}

/// The variable a reference or a structured source of the typed tree reads.
fn variable_of(spec: &ast::Spec, expr: ExprId) -> Option<&str> {
    let reference = match &spec.expr(expr).kind {
        ExprKind::Source(read) => read.variable,
        _ => expr,
    };
    match &spec.expr(reference).kind {
        ExprKind::Ref(name) => Some(name.as_str()),
        _ => None,
    }
}

/// The one variable an aggregate pivots: its `expr` is exactly
/// `ONLY(<variable>)`, which selects one stored value and computes nothing,
/// and it declares no bindings, which would compute the value read.
fn pivot_of(spec: &ast::Spec, aggregate: &ast::Aggregate) -> Option<String> {
    if !aggregate.bindings.is_empty() {
        return None;
    }
    match &spec.expr(aggregate.expr).kind {
        ExprKind::Formula(formula) => only(&formula.text.text),
        _ => None,
    }
}

/// Classify one derivation, a column's or a row template's, by SUB-19's table.
pub(crate) fn classify_expr(spec: &ast::Spec, expr: ExprId, reads: &Reads) -> DerivationKind {
    let bare = |name: Option<&str>| match name {
        Some(name) if reads.bare(name) => DerivationKind::Source,
        _ => DerivationKind::Computed,
    };
    match &spec.expr(expr).kind {
        ExprKind::Literal(_) => DerivationKind::Literal,
        ExprKind::Odm(_) => DerivationKind::Source,
        ExprKind::Ref(_) | ExprKind::Source(_) => bare(variable_of(spec, expr)),
        // A pivot of long records to their one value reads it as stored.
        ExprKind::Aggregate(aggregate) => bare(pivot_of(spec, aggregate).as_deref()),
        _ if recode_expr(spec, expr, reads, false) => DerivationKind::Recode,
        _ => DerivationKind::Computed,
    }
}

/// [`recode`] over a derivation of the typed tree.
fn recode_expr(spec: &ast::Spec, expr: ExprId, reads: &Reads, strict: bool) -> bool {
    let reads_variable =
        |expr: ExprId| variable_of(spec, expr).is_some_and(|name| reads.reads(name, strict));
    match &spec.expr(expr).kind {
        ExprKind::Scalar { op, operands, .. } => match op.recode() {
            Recode::Source => operands
                .iter()
                .find(|operand| operand.slot == Slot::Source)
                .is_some_and(|operand| reads_variable(operand.expr)),
            // For example an ISO 8601 assembly of collected date parts.
            Recode::Parts => {
                !operands.is_empty() && operands.iter().all(|operand| reads_variable(operand.expr))
            }
            // A valid template's placeholders are its operands.
            Recode::Template => operands.iter().all(|operand| reads_variable(operand.expr)),
            Recode::Computed => false,
        },
        ExprKind::FirstAvailable { sources, .. } => {
            !sources.is_empty() && sources.iter().all(|&source| reads_variable(source))
        }
        ExprKind::Case(arms) => {
            !arms.is_empty()
                && arms.iter().all(|arm| {
                    let condition =
                        arm.condition
                            .is_none_or(|condition| match &spec.expr(condition).kind {
                                ExprKind::Formula(formula) => expression::parse(&formula.text.text)
                                    .is_ok_and(|parsed| {
                                        parsed.identifiers().all(|name| reads.input(name))
                                    }),
                                _ => false,
                            });
                    condition && nested_expr(spec, arm.result, reads, true)
                })
        }
        _ => false,
    }
}

/// [`nested`] over an expression of the typed tree.
fn nested_expr(spec: &ast::Spec, expr: ExprId, reads: &Reads, strict: bool) -> bool {
    match &spec.expr(expr).kind {
        ExprKind::Literal(_) | ExprKind::Odm(_) => true,
        ExprKind::Ref(_) | ExprKind::Source(_) => {
            variable_of(spec, expr).is_some_and(|name| reads.reads(name, strict))
        }
        ExprKind::Aggregate(aggregate) => {
            pivot_of(spec, aggregate).is_some_and(|name| reads.reads(&name, strict))
        }
        _ => recode_expr(spec, expr, reads, strict),
    }
}

/// The column a Predecessor description names (SUB-16): `<dataset>.<column>`,
/// optionally followed by ` where ` and a condition written for reviewers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Predecessor<'a> {
    pub(crate) dataset: &'a str,
    pub(crate) column: &'a str,
    /// Whether the description writes a where clause.
    pub(crate) selected: bool,
}

impl Predecessor<'_> {
    pub(crate) fn name(&self) -> String {
        format!("{}.{}", self.dataset, self.column)
    }
}

/// Read a Predecessor description; `None` when it does not have that form.
pub(crate) fn predecessor(description: &str) -> Option<Predecessor<'_>> {
    let (named, condition) = match description.split_once(" where ") {
        Some((named, condition)) => (named, Some(condition)),
        None => (description, None),
    };
    if condition.is_some_and(|condition| condition.trim().is_empty()) {
        return None;
    }
    let (dataset, column) = named.split_once('.')?;
    (identifier(dataset) && identifier(column)).then_some(Predecessor {
        dataset,
        column,
        selected: condition.is_some(),
    })
}
fn identifier(name: &str) -> bool {
    let mut bytes = name.bytes();
    bytes
        .next()
        .is_some_and(|byte| byte.is_ascii_alphabetic() || byte == b'_')
        && bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}

/// Why a derivation is not the copy a Predecessor origin claims (SUB-20).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CopyFailure {
    /// The bare `source` reads another dataset or column.
    Source,
    /// The description writes a where clause and the `source` has no `filter`.
    Filter,
}

impl CopyFailure {
    pub(crate) fn reason(self) -> &'static str {
        match self {
            Self::Source => "predecessor_source",
            Self::Filter => "predecessor_filter",
        }
    }
}

/// Check that a bare `source` derivation reads the predecessor column through
/// the input dataset declared under the predecessor's name, directly or
/// through a plain lookup of it, and, when the description writes a where
/// clause, declares a `filter` on the `source` or the lookup.
pub(crate) fn copies_expr(
    spec: &ast::Spec,
    expr: ExprId,
    predecessor: &Predecessor<'_>,
    reads: &Reads,
) -> Result<(), CopyFailure> {
    let filtered = match &spec.expr(expr).kind {
        ExprKind::Source(read) => read.filter.is_some(),
        _ => false,
    };
    let named = match &spec.expr(expr).kind {
        ExprKind::Ref(_) | ExprKind::Source(_) => variable_of(spec, expr),
        _ => None,
    }
    .and_then(|name| name.split_once('.'));
    let Some((qualifier, column)) = named else {
        return Err(CopyFailure::Source);
    };
    let lookup = reads
        .selections
        .get(qualifier)
        .filter(|lookup| lookup.dataset == predecessor.dataset);
    if column != predecessor.column
        || !reads.inputs.contains(predecessor.dataset)
        || (qualifier != predecessor.dataset && lookup.is_none())
    {
        return Err(CopyFailure::Source);
    }
    let filtered = filtered || lookup.is_some_and(|lookup| lookup.filtered);
    if predecessor.selected && !filtered {
        return Err(CopyFailure::Filter);
    }
    Ok(())
}

/// Whether a specification declares a Predecessor origin on a column or on
/// one of its value-level entries; composition then reads the predecessor
/// columns the study's other sections declare (SUB-20).
pub(crate) fn names_predecessor(document: &Document) -> bool {
    let root = document.root();
    let predecessor = |submission: usize| {
        declared_origins(document, submission)
            .iter()
            .any(|&origin| {
                document
                    .field(origin, "type")
                    .and_then(|kind| text(document, kind))
                    == Some(OriginType::Predecessor.name())
            })
    };
    let columns = items(document, document.field(root, "columns"));
    columns.iter().any(|&column| {
        document
            .field(column, "submission")
            .is_some_and(predecessor)
            || crate::submission_values::entries(document, column)
                .iter()
                .any(|entry| predecessor(entry.node))
    })
}

/// The origins a submission mapping declares: 0.5's one `origin`, or each
/// entry of 1.0's `origins` (P-ORIGIN-1).
fn declared_origins(document: &Document, submission: usize) -> Vec<usize> {
    match document.field(submission, "origin") {
        Some(origin) => vec![origin],
        None => items(document, document.field(submission, "origins")).to_vec(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn predecessor_descriptions_name_one_column_and_an_optional_condition() {
        assert_eq!(
            predecessor("DM.AGE"),
            Some(Predecessor {
                dataset: "DM",
                column: "AGE",
                selected: false
            })
        );
        assert_eq!(
            predecessor("SUPPDM.QVAL where QNAM = 'RACEOTH'"),
            Some(Predecessor {
                dataset: "SUPPDM",
                column: "QVAL",
                selected: true
            })
        );
        for invalid in [
            "",
            "AGE",
            "DM.",
            ".AGE",
            "DM.AGE.X",
            "DM AGE",
            "Copied from DM.AGE",
            "DM.AGE where ",
            "DM.AGE WHERE X = 1",
            "1DM.AGE",
        ] {
            assert_eq!(predecessor(invalid), None, "{invalid:?}");
        }
    }

    #[test]
    fn only_the_reduction_of_one_variable_to_its_one_value_is_a_pivot() {
        assert_eq!(only("ONLY(ODM.Value)").as_deref(), Some("ODM.Value"));
        assert_eq!(only(" ( ONLY( (LBORRES) ) ) ").as_deref(), Some("LBORRES"));
        for computed in [
            "ONLY(ODM.Value) + 1",
            "ONLY(ODM.Value * 2)",
            "-ONLY(ODM.Value)",
            "MAX(ODM.Value)",
            "COUNT(ODM.*)",
            "ONLY('text')",
            "ODM.Value",
            "ONLY(",
        ] {
            assert_eq!(only(computed), None, "{computed}");
        }
    }

    #[test]
    fn every_derivation_row_refutes_exactly_its_published_types() {
        use DerivationKind as K;
        let refuted = |kind: K| {
            OriginType::all()
                .filter(|&origin| !kind.admits(origin))
                .map(OriginType::name)
                .collect::<Vec<_>>()
        };
        assert!(refuted(K::Source).is_empty());
        assert_eq!(refuted(K::Literal), ["Derived", "Predecessor"]);
        assert_eq!(refuted(K::Recode), ["Predecessor", "Protocol"]);
        assert_eq!(
            refuted(K::Computed),
            ["Collected", "Predecessor", "Protocol"]
        );
        assert_eq!(
            K::Recode.admitted(),
            ["Assigned", "Collected", "Derived", "Not Available", "Other"]
        );
    }
}
