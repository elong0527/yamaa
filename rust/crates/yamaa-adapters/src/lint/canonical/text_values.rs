//! The findings that read a specification's `str` values through its
//! schema: `number_for_text`, a YAML number written where a `str` column
//! takes the value, whose written spelling is not the text it converts to
//! (EXPR-5, VALUE-3), and `empty_text_comparison`, a comparison with `''` of
//! a field that never holds an empty string (INPUT-8, INPUT-9, FORMULA-4).
//!
//! Both read only what the file itself declares: a column's declared type, an
//! input's `empty_string` and a column's own derivation. A column, input or
//! derivation the file inherits or does not write is not read, so neither
//! finding is reported for it.

use yamaa_core::conversion::convert;
use yamaa_core::expression::{Comparison, NodeKind};
use yamaa_core::schema::{DocumentNode as N, SiteRole};
use yamaa_core::value::{ColumnType, Value};

use super::super::checks::{Departure, Rewrite};
use super::Read;

/// One column this file declares: its name, its declared type when written,
/// and its column-level derivation when written.
struct Column<'a> {
    name: &'a str,
    declared: Option<&'a str>,
    derivation: Option<usize>,
}

impl<'a> Read<'a> {
    fn text_of(&self, node: usize) -> Option<&'a str> {
        match &self.values.nodes()[node] {
            N::Text(text) => Some(text.as_str()),
            _ => None,
        }
    }

    /// The entries of the root list `field`.
    fn root_entries(&self, field: &str) -> &'a [usize] {
        let values = self.values;
        match values
            .field(values.root(), field)
            .map(|node| &values.nodes()[node])
        {
            Some(N::Sequence(entries)) => entries,
            _ => &[],
        }
    }

    /// Each column of `columns` that names itself.
    fn declared_columns(&self) -> Vec<Column<'a>> {
        self.root_entries("columns")
            .iter()
            .filter_map(|&entry| {
                Some(Column {
                    name: self.text_of(self.values.field(entry, "name")?)?,
                    declared: self
                        .values
                        .field(entry, "type")
                        .and_then(|node| self.text_of(node)),
                    derivation: self.values.field(entry, "derivation"),
                })
            })
            .collect()
    }

    /// Each derivation a row template writes, with the column it derives.
    fn row_derivations(&self) -> Vec<(&'a str, usize)> {
        let mut found = Vec::new();
        for &entry in self.root_entries("rows") {
            let Some(derivations) = self.values.field(entry, "derivations") else {
                continue;
            };
            if let N::Mapping(pairs) = &self.values.nodes()[derivations] {
                found.extend(
                    pairs
                        .iter()
                        .filter_map(|&(key, value)| Some((self.text_of(key)?, value))),
                );
            }
        }
        found
    }

    /// Whether `node` is read as a constant a handler, flag or mapping
    /// returns (COLUMN-10).
    fn substitute(&self, node: usize) -> bool {
        self.sites.iter().any(|site| {
            site.node == node
                && matches!(&site.role, SiteRole::Alias { name } if name == "substitute_value")
        })
    }

    /// The values a derivation `node` takes as its result, each with whether
    /// it is read as grammar text: the derivation itself, the handled form's
    /// `value` and `unconvertible`, a `case` result, and the constants its
    /// operation returns, a `dict` value or a handler's answer. An operand,
    /// such as an operation's `source` with its own `absent`, is not a result.
    fn results(&self, node: usize, out: &mut Vec<(usize, bool)>) {
        let values = self.values;
        let N::Mapping(pairs) = &values.nodes()[node] else {
            out.push((node, true));
            return;
        };
        if let Some(value) = values.field(node, "value") {
            self.results(value, out);
            out.extend(
                values
                    .field(node, "unconvertible")
                    .map(|node| (node, false)),
            );
            return;
        }
        let [(operation, payload)] = pairs.as_slice() else {
            return;
        };
        if self.text_of(*operation) == Some("case") {
            if let N::Sequence(items) = &values.nodes()[*payload] {
                for &item in items {
                    for field in ["then", "otherwise"] {
                        if let Some(result) = values.field(item, field) {
                            self.results(result, out);
                        }
                    }
                }
            }
            return;
        }
        let N::Mapping(fields) = &values.nodes()[*payload] else {
            return;
        };
        for &(key, value) in fields {
            if self.substitute(value) {
                out.push((value, false));
            } else if self.text_of(key) == Some("dict") {
                if let N::Mapping(entries) = &values.nodes()[value] {
                    out.extend(
                        entries
                            .iter()
                            .filter(|&&(_, entry)| self.substitute(entry))
                            .map(|&(_, entry)| (entry, false)),
                    );
                }
            }
        }
    }

    /// `number_for_text`: a YAML number that a `str` column takes as its
    /// value, as a derivation, a `case` result, a `dict` value or a handler's
    /// answer, written otherwise than the decimal or shortest positional text
    /// it converts to (VALUE-3), such as `012` for `12` or `1.50` for `1.5`.
    /// The fix quotes the written spelling, as grammar text in an expression
    /// field and as YAML text in a value field, so the column keeps what was
    /// written.
    pub(super) fn numbers_for_text(&self) -> Vec<Departure> {
        let columns = self.declared_columns();
        let text_column = |name: &str| {
            columns
                .iter()
                .any(|column| column.name == name && column.declared == Some("str"))
        };
        let mut results = Vec::new();
        for column in &columns {
            if let Some(derivation) = column.derivation.filter(|_| text_column(column.name)) {
                self.results(derivation, &mut results);
            }
        }
        for (name, derivation) in self.row_derivations() {
            if text_column(name) {
                self.results(derivation, &mut results);
            }
        }
        let mut found = Vec::new();
        for (result, grammar) in results {
            let value = match &self.values.nodes()[result] {
                N::Integer(text) => match text.parse() {
                    Ok(value) => Value::Int(value),
                    Err(_) => continue,
                },
                N::Float(value) => Value::float(*value),
                _ => continue,
            };
            let Ok(Value::Str(converted)) = convert(&value, ColumnType::Str) else {
                continue;
            };
            let node = self.nodes[result];
            let written = self.document.source(node);
            if written == converted {
                continue;
            }
            let quoted = if grammar {
                format!("\"'{written}'\"")
            } else {
                format!("\"{written}\"")
            };
            let start = self.document.node(node).start;
            found.push(
                Departure::new(
                    "number_for_text",
                    start.line,
                    start.column,
                    self.document.path(node),
                    format!(
                        "a number written for a `str` column converts to `{converted}`; quote \
                         it to keep `{written}`"
                    ),
                )
                .with_fix(Rewrite::Scalar { node, text: quoted }),
            );
        }
        found
    }

    /// Whether the named type `name` is read as grammar text.
    fn is_grammar(&self, name: &str) -> bool {
        use yamaa_core::schema::SchemaAliasKind;
        let Some((_, alias)) = self.view.alias_named(name) else {
            return false;
        };
        let SchemaAliasKind::Descriptor(descriptor) = alias.kind else {
            return false;
        };
        self.view.descriptors()[descriptor].descriptor.kind() == Some("expression")
    }

    /// Whether the read `name` never holds an empty string: a field of an
    /// input whose stored empty string is missing (INPUT-9), or, where a bare
    /// name reads an output column (`bare`), a column whose only derivation
    /// copies such a read; a column a row template derives is not read.
    fn never_empty(&self, name: &str, columns: &[Column<'_>], bare: bool, depth: usize) -> bool {
        if let Some((dataset, field)) = name.split_once('.') {
            return !field.contains('.') && self.missing_empty_input(dataset);
        }
        if !bare || depth == 0 || self.row_derivations().iter().any(|&(row, _)| row == name) {
            return false;
        }
        columns.iter().any(|column| {
            column.name == name
                && column
                    .derivation
                    .and_then(|node| self.text_of(node))
                    .and_then(copied)
                    .is_some_and(|read| self.never_empty(&read, columns, true, depth - 1))
        })
    }

    /// Whether `dataset` is an input this file declares whose stored empty
    /// string is missing, as an omitted `empty_string` declares (INPUT-9). A
    /// layer merges an input with the one it inherits (LAYER-6), so there
    /// only an `empty_string: missing` the layer writes is read.
    fn missing_empty_input(&self, dataset: &str) -> bool {
        let values = self.values;
        let Some(input) = values.field(values.root(), "input") else {
            return false;
        };
        values.field(input, dataset).is_some_and(|declared| {
            match values
                .field(declared, "empty_string")
                .and_then(|node| self.text_of(node))
            {
                Some(declared) => declared == "missing",
                None => !self.document.layered(),
            }
        })
    }

    /// Whether a bare name in the grammar text `node`, at `path`, reads an
    /// output column: in a column, a row template's derivation or a
    /// verification, outside the filters and expressions of a lookup, which
    /// read the fields of the dataset they narrow (LOOKUP-15).
    fn reads_columns(&self, node: usize, path: &str) -> bool {
        let placed = path.starts_with("columns")
            || path.starts_with("verifications")
            || (path.starts_with("rows") && path.contains(".derivations."));
        placed
            && !self.sites.iter().any(|site| {
                site.node == node
                    && matches!(
                        &site.role,
                        SiteRole::Field { owner, .. } if matches!(
                            owner.as_str(),
                            "source_binding_class" | "intermediate_class" | "aggregate_class"
                        )
                    )
            })
    }

    /// `empty_text_comparison`: `=`, `<>`, `IN` or `NOT IN` against the
    /// literal `''` whose other operand never holds an empty string, so the
    /// comparison is never TRUE for a missing value (FORMULA-4) and the
    /// author means `IS NULL`: the message writes the test with `IS NULL`, or
    /// `IS NOT NULL` when negated, keeping a list's other items. A column a text operation builds can hold
    /// `''`, so the comparison stays the author's to rewrite.
    pub(super) fn empty_text_comparisons(&self) -> Vec<Departure> {
        let columns = self.declared_columns();
        let mut found = Vec::new();
        let mut seen = Vec::new();
        for site in &self.sites {
            let SiteRole::Alias { name } = &site.role else {
                continue;
            };
            let Some(text) = self.text_of(site.node) else {
                continue;
            };
            if seen.contains(&site.node) || !self.is_grammar(name) {
                continue;
            }
            seen.push(site.node);
            let bare = self.reads_columns(site.node, &site.path);
            let Ok(parsed) = yamaa_core::expression::parse(text) else {
                continue;
            };
            let empty = |node: usize| {
                matches!(
                    &parsed.nodes()[parsed.ungroup(node)].kind,
                    NodeKind::String(text) if text.is_empty()
                )
            };
            for (index, node) in parsed.nodes().iter().enumerate() {
                // The operand, whether the test is negated, and the other
                // items of a list.
                let (operand, negated, rest) = match &node.kind {
                    NodeKind::Compare {
                        operator: operator @ (Comparison::Equal | Comparison::NotEqual),
                        left,
                        right,
                    } => match (empty(*left), empty(*right)) {
                        (false, true) => (*left, *operator == Comparison::NotEqual, Vec::new()),
                        (true, false) => (*right, *operator == Comparison::NotEqual, Vec::new()),
                        _ => continue,
                    },
                    NodeKind::In {
                        value,
                        items,
                        negated,
                    } if items.iter().any(|&item| empty(item)) => (
                        *value,
                        *negated,
                        items
                            .iter()
                            .filter(|&&item| !empty(item))
                            .map(|&item| parsed.text(item))
                            .collect::<Vec<_>>(),
                    ),
                    _ => continue,
                };
                let operand = parsed.ungroup(operand);
                if parsed.nodes()[operand].kind != NodeKind::Identifier {
                    continue;
                }
                let read = parsed.text(operand);
                if !self.never_empty(read, &columns, bare, columns.len()) {
                    continue;
                }
                let rest = rest.join(", ");
                let test = match (negated, rest.is_empty()) {
                    (false, true) => format!("{read} IS NULL"),
                    (true, true) => format!("{read} IS NOT NULL"),
                    (false, false) => format!("{read} IS NULL OR {read} IN ({rest})"),
                    (true, false) => format!("{read} IS NOT NULL AND {read} NOT IN ({rest})"),
                };
                let at = self.nodes[site.node];
                let start = self.document.node(at).start;
                found.push(Departure::new(
                    "empty_text_comparison",
                    start.line,
                    start.column,
                    self.document.path(at),
                    format!(
                        "`{read}` is never an empty string, since an empty field is \
                         missing; write `{test}` for `{}`",
                        parsed.text(index)
                    ),
                ));
            }
        }
        found
    }
}

/// The one name `text` reads when the expression is that read alone.
fn copied(text: &str) -> Option<String> {
    let parsed = yamaa_core::expression::parse(text).ok()?;
    let root = parsed.ungroup(parsed.root());
    (parsed.nodes()[root].kind == NodeKind::Identifier).then(|| parsed.text(root).to_owned())
}
