//! `window_tie`: a window whose positions decide a carried or neighbouring
//! value, under an order that leaves records tied (WINDOW-6, P5).
//!
//! `locf`, `row_value` and `row_number` read positions, and records equal on
//! every `group_by` and `order_by` term keep their stored order (WINDOW-6), so
//! the value they read then depends on the order of the input records. The
//! lint cannot see the data, so it judges by the declared keys: the window's
//! terms tell the records apart when they include every key of the records it
//! orders. Rows built by row templates, or a `SELF` intermediate, are the
//! output rows, which the root `keys` identifies; the records of a named
//! intermediate over a declared input are identified by that input's `key`,
//! whose fields its terms name bare or qualified by the input.
//! Each output key is first resolved through the column derivations
//! ([`super::window_keys`]): a constant key drops out, a key a `row_number`
//! gives stands for that numbering's terms, and a window whose own column a
//! key reads is not judged. A window over records with no declared key, a
//! pair or counted relation among them, is not judged either. `rank`
//! compares the terms alone and is not read, and `baseline_flag` fails a tie
//! instead (WINDOW-9).
//!
//! A layer, a file that declares `parent` or `include` or a fragment another
//! linted file includes, is not read, since its keys and windows may come
//! from the layers it composes on.

use yamaa_core::expression_table::Verb;

use super::checks::Departure;
use super::document::Document;
use super::window_keys::{Columns, Resolved};

/// The window verbs whose value depends on positions, each with the value it
/// reads, as its finding names it.
const POSITIONAL: [(Verb, &str); 3] = [
    (Verb::Locf, "the carried value"),
    (Verb::RowValue, "the neighbouring value"),
    (Verb::RowNumber, "the row number"),
];

/// The declared keys of the records a window orders, and how the finding
/// names them.
struct Records {
    keys: Vec<String>,
    /// The input an intermediate's terms qualify its donor fields by.
    qualifier: Option<String>,
    /// The column derivations the output keys resolve through; none for an
    /// input's records.
    columns: Option<Columns>,
    named: String,
}

/// The texts of a list node's scalar items; none when it is not a list of
/// text.
fn texts(document: &Document, node: Option<usize>) -> Option<Vec<String>> {
    document
        .items(node?)?
        .iter()
        .map(|&item| document.text_scalar(item).map(str::to_owned))
        .collect()
}

/// The variable each `order_by` or `group_by` term names: a bare variable,
/// or the `variable` of a term mapping.
fn terms(document: &Document, node: Option<usize>) -> Vec<String> {
    let Some(items) = node.and_then(|node| document.items(node)) else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|&item| {
            document
                .text_scalar(item)
                .or_else(|| {
                    document
                        .member(Some(item), "variable")
                        .and_then(|variable| document.text_scalar(variable))
                })
                .map(str::to_owned)
        })
        .collect()
}

fn output_records(document: &Document) -> Option<Records> {
    Some(Records {
        keys: texts(document, document.member(document.root, "keys"))?,
        qualifier: None,
        columns: document
            .decoded
            .as_ref()
            .map(|decoded| Columns::read(&decoded.document)),
        named: "`keys`".to_owned(),
    })
}

/// The records a named intermediate's derivations order: its donor records,
/// identified by the `key` of the declared input it reads, or the output rows
/// for `SELF`.
fn intermediate_records(document: &Document, intermediate: usize) -> Option<Records> {
    let dataset = document.text_scalar(document.member(Some(intermediate), "dataset")?)?;
    if dataset == "SELF" {
        return output_records(document);
    }
    let input = document.member(document.member(document.root, "input"), dataset)?;
    Some(Records {
        keys: texts(document, document.member(Some(input), "key"))?,
        qualifier: Some(dataset.to_owned()),
        columns: None,
        named: format!("the `key` of input `{dataset}`"),
    })
}

struct Walk<'a> {
    document: &'a Document,
    windows: Option<usize>,
    found: Vec<Departure>,
}

impl Walk<'_> {
    /// Visit `node`, which belongs to the derivation of the column
    /// `deriving` when one is known.
    fn visit(&mut self, node: usize, records: Option<&Records>, deriving: Option<&str>) {
        let document = self.document;
        if let Some(items) = document.items(node) {
            for &item in items {
                self.visit(item, records, deriving);
            }
        }
        let Some(pairs) = document.pairs(node) else {
            return;
        };
        // A column entry derives the column it names.
        let named = document
            .member(Some(node), "derivation")
            .and(document.member(Some(node), "name"))
            .and_then(|name| document.text_scalar(name));
        let deriving = named.or(deriving);
        for &(key, value) in pairs {
            let name = document.scalar(key);
            let verb = name.and_then(|name| {
                POSITIONAL
                    .iter()
                    .find(|(verb, _)| verb.name() == name)
                    .copied()
            });
            if let (Some((verb, what)), Some(records)) = (verb, records) {
                self.judge(verb, what, value, records, deriving);
            }
            if name == Some("derivations") {
                // A row template's or an intermediate's derivations derive
                // the columns they name.
                for &(column, derivation) in document.pairs(value).unwrap_or_default() {
                    let column = document.text_scalar(column);
                    self.visit(derivation, records, column.or(deriving));
                }
            } else {
                self.visit(value, records, deriving);
            }
        }
    }

    /// Report the window of one positional operation whose terms leave out a
    /// key of the records it orders.
    fn judge(
        &mut self,
        verb: Verb,
        what: &str,
        operation: usize,
        records: &Records,
        deriving: Option<&str>,
    ) {
        let document = self.document;
        let Some(window) = document.member(Some(operation), "window") else {
            return;
        };
        // A named reference reads its root `windows` definition (WINDOW-1)
        // and is reported where it is used.
        let (definition, at) = match document.text_scalar(window) {
            Some(name) => (document.member(self.windows, name), window),
            None => (
                Some(window),
                document.member(Some(window), "order_by").unwrap_or(window),
            ),
        };
        let Some(definition) = definition else {
            return;
        };
        let order_by = document.member(Some(definition), "order_by");
        if order_by.is_none() {
            // A positional window without an order fails validation (WINDOW-5).
            return;
        }
        let mut named = terms(document, document.member(Some(definition), "group_by"));
        named.extend(terms(document, order_by));
        // An intermediate's terms name a donor field bare or qualified by
        // its input.
        if let Some(qualifier) = &records.qualifier {
            for term in &mut named {
                if let Some(field) = term
                    .split_once('.')
                    .filter(|(dataset, _)| dataset == qualifier)
                    .map(|(_, field)| field.to_owned())
                {
                    *term = field;
                }
            }
        }
        let mut wanted: Vec<String> = Vec::new();
        for key in &records.keys {
            // A key among the terms tells the records apart itself.
            if named.contains(key) {
                continue;
            }
            let resolved = match &records.columns {
                Some(columns) => columns.resolve(key, deriving),
                None => Resolved::Itself,
            };
            let stands_for = match resolved {
                Resolved::Circular => return,
                Resolved::Constant => Vec::new(),
                Resolved::Itself => vec![key.clone()],
                Resolved::Terms(terms) => terms,
            };
            for term in stands_for {
                if !wanted.contains(&term) {
                    wanted.push(term);
                }
            }
        }
        let missing: Vec<String> = wanted
            .iter()
            .filter(|key| !named.contains(key))
            .map(|key| format!("`{key}`"))
            .collect();
        if missing.is_empty() {
            return;
        }
        let start = document.node(at).start;
        self.found.push(Departure::new(
            "window_tie",
            start.line,
            start.column,
            document.path(at),
            format!(
                "`{}` reads positions, and records tied on every `group_by` and `order_by` \
                 term keep input record order, so {what} depends on that order; add {} to \
                 `order_by` to tell apart the records {} identifies",
                verb.name(),
                missing.join(", "),
                records.named,
            ),
        ));
    }
}

/// Every `window_tie` departure in a specification `document`.
pub(super) fn check(document: &Document) -> Vec<Departure> {
    let (Some(root), false) = (document.root, document.layered()) else {
        return Vec::new();
    };
    let Some(pairs) = document.pairs(root) else {
        return Vec::new();
    };
    let mut walk = Walk {
        document,
        windows: document.member(Some(root), "windows"),
        found: Vec::new(),
    };
    let output = output_records(document);
    for &(key, value) in pairs {
        match document.scalar(key) {
            // Definitions are judged where they are used.
            Some("windows") => {}
            Some("intermediates") => {
                for &intermediate in document.items(value).unwrap_or_default() {
                    let records = intermediate_records(document, intermediate);
                    walk.visit(intermediate, records.as_ref(), None);
                }
            }
            _ => walk.visit(value, output.as_ref(), None),
        }
    }
    walk.found
}
