//! What a column's derivation proves about its declared origin. The derivation
//! refutes an origin and never supplies one (SUB-19), and a Predecessor names
//! the column it copies (SUB-16, SUB-20). Shared by the domain build and by
//! composition; no study, host or data access.
use crate::{
    expression,
    schema::{Document, DocumentNode as N},
    specification_submission_document::OriginType,
    text_template::StringTemplate,
};
use std::collections::BTreeSet;

/// The four derivation rows of SUB-19's table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DerivationKind {
    /// A bare `source` or `odm` read: nothing was computed.
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
    intermediates: BTreeSet<String>,
}

impl Reads {
    pub(crate) fn of(document: &Document) -> Self {
        let root = document.root();
        let inputs = match document.field(root, "input").map(|n| &document.nodes()[n]) {
            Some(N::Mapping(entries)) => entries
                .iter()
                .filter_map(|&(key, _)| text(document, key).map(String::from))
                .collect(),
            _ => BTreeSet::new(),
        };
        let intermediates = items(document, document.field(root, "intermediates"))
            .iter()
            .filter_map(|&item| document.field(item, "id").and_then(|id| text(document, id)))
            .map(String::from)
            .collect();
        Self {
            inputs,
            intermediates,
        }
    }
    /// A read that looks no record up through a named intermediate.
    fn bare(&self, name: &str) -> bool {
        name.split_once('.')
            .is_none_or(|(qualifier, _)| !self.intermediates.contains(qualifier))
    }
    /// A read of a value a declared input dataset carries.
    fn input(&self, name: &str) -> bool {
        name.split_once('.')
            .is_some_and(|(qualifier, _)| self.inputs.contains(qualifier))
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
/// The variable a `source` payload, a filtered source or a bare string names.
fn variable(document: &Document, node: usize) -> Option<&str> {
    text(document, node).or_else(|| text(document, document.field(node, "variable")?))
}
/// The one registered keyword of an expression and its payload.
fn keyword(document: &Document, node: usize) -> Option<(&str, usize)> {
    match &document.nodes()[node] {
        N::Mapping(entries) => match entries.as_slice() {
            [(key, payload)] => Some((text(document, *key)?, *payload)),
            _ => None,
        },
        _ => None,
    }
}

/// Classify one derivation, a column's or a row template's, by SUB-19's table.
pub(crate) fn classify(document: &Document, node: usize, reads: &Reads) -> DerivationKind {
    let node = document.field(node, "value").unwrap_or(node);
    if let Some(name) = text(document, node) {
        // A bare string derivation is the source shorthand (EXPR-5).
        return if reads.bare(name) {
            DerivationKind::Source
        } else {
            DerivationKind::Computed
        };
    }
    match keyword(document, node) {
        Some(("literal", _)) => DerivationKind::Literal,
        Some(("odm", _)) => DerivationKind::Source,
        Some(("source", payload)) => match variable(document, payload) {
            Some(name) if reads.bare(name) => DerivationKind::Source,
            _ => DerivationKind::Computed,
        },
        _ if recode(document, node, reads, false) => DerivationKind::Recode,
        _ => DerivationKind::Computed,
    }
}

/// Whether `node` is a recode verb of SUB-19 whose every read is a bare read or,
/// when `strict`, a declared input value. A `case` is always strict: every
/// condition and result reads only declared input values or literals.
fn recode(document: &Document, node: usize, reads: &Reads, strict: bool) -> bool {
    let Some((verb, payload)) = keyword(document, node) else {
        return false;
    };
    let reads_variable = |field: &str| {
        document
            .field(payload, field)
            .and_then(|source| variable(document, source))
            .is_some_and(|name| reads.reads(name, strict))
    };
    match verb {
        "mapping" | "str_case" | "str_pad" | "str_replace" | "str_extract" | "to_date" => {
            reads_variable("source")
        }
        "first_available" => {
            let sources = items(document, document.field(payload, "sources"));
            !sources.is_empty()
                && sources.iter().all(|&source| {
                    variable(document, source).is_some_and(|name| reads.reads(name, strict))
                })
        }
        "str_template" => {
            let template = text(document, payload)
                .or_else(|| text(document, document.field(payload, "template")?));
            template.is_some_and(|template| {
                StringTemplate::new(template.into(), "origin".into(), None).is_ok_and(|parsed| {
                    parsed.declaration_diagnostic().is_none()
                        && parsed
                            .identifiers()
                            .iter()
                            .all(|name| reads.reads(name, strict))
                })
            })
        }
        "case" => {
            let branches = items(document, Some(payload));
            !branches.is_empty()
                && branches.iter().all(|&branch| {
                    let condition = document.field(branch, "when").map_or(Some(true), |when| {
                        let parsed = expression::parse(text(document, when)?);
                        Some(parsed.ok()?.identifiers().all(|name| reads.input(name)))
                    });
                    let result = document
                        .field(branch, "then")
                        .or_else(|| document.field(branch, "otherwise"));
                    condition == Some(true)
                        && result.is_some_and(|result| nested(document, result, reads, true))
                })
        }
        _ => false,
    }
}

/// A nested expression of a recode: a literal, a bare read, or another recode.
fn nested(document: &Document, node: usize, reads: &Reads, strict: bool) -> bool {
    if let Some(name) = text(document, node) {
        // A bare string case result is the source shorthand (EXPR-5).
        return reads.reads(name, strict);
    }
    match keyword(document, node) {
        Some(("literal", _)) => true,
        // An `odm` read identifies one record of a declared ODM input.
        Some(("odm", _)) => true,
        Some(("source", payload)) => {
            variable(document, payload).is_some_and(|name| reads.reads(name, strict))
        }
        _ => recode(document, node, reads, strict),
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

/// Check that a bare `source` derivation reads the predecessor column and,
/// when the description writes a where clause, declares a `filter`.
pub(crate) fn copies(
    document: &Document,
    node: usize,
    predecessor: &Predecessor<'_>,
) -> Result<(), CopyFailure> {
    let node = document.field(node, "value").unwrap_or(node);
    let source = match keyword(document, node) {
        Some(("source", payload)) => Some(payload),
        _ => text(document, node).map(|_| node),
    };
    let named = source
        .and_then(|source| variable(document, source))
        .and_then(|name| name.split_once('.'));
    if named != Some((predecessor.dataset, predecessor.column)) {
        return Err(CopyFailure::Source);
    }
    if predecessor.selected && source.and_then(|s| document.field(s, "filter")).is_none() {
        return Err(CopyFailure::Filter);
    }
    Ok(())
}

/// Whether a specification declares a Predecessor origin on a column or a
/// row template's value-level entry; composition then reads the predecessor
/// columns the study's other sections declare (SUB-20).
pub(crate) fn names_predecessor(document: &Document) -> bool {
    let root = document.root();
    let predecessor = |submission: usize| {
        document
            .field(submission, "origin")
            .and_then(|origin| document.field(origin, "type"))
            .and_then(|kind| text(document, kind))
            == Some(OriginType::Predecessor.name())
    };
    let columns = items(document, document.field(root, "columns"));
    let rows = items(document, document.field(root, "rows"));
    columns.iter().any(|&column| {
        document
            .field(column, "submission")
            .is_some_and(predecessor)
    }) || rows.iter().any(|&row| {
        match document
            .field(row, "submission")
            .map(|n| &document.nodes()[n])
        {
            Some(N::Mapping(entries)) => entries.iter().any(|&(_, entry)| predecessor(entry)),
            _ => false,
        }
    })
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
