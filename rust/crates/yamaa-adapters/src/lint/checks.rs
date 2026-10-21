//! The departures the style guide names.
//!
//! Each check reads one parsed file and returns departures with zero-based
//! positions. None reads data, resolves inheritance or validates the file: a
//! finding describes the text, never its meaning.

use std::collections::{HashMap, HashSet};

use yamaa_core::language::FileType;

use super::document::{Document, Value};
use super::{authoring, canonical, fields, FINDINGS};

pub(super) const LINE_WIDTH: usize = 79;

/// One schema class `field_order` orders: its fields in schema order and,
/// for each field whose value is or holds a mapping of an ordered class, how
/// it holds that class. `fields.rs` holds one tree per file type, generated
/// from the schemas.
pub(super) struct Class {
    pub(super) fields: &'static [&'static str],
    pub(super) members: &'static [(&'static str, Holds, &'static Class)],
}

/// How a field holds the mappings of its class.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Holds {
    /// The field's value is the mapping.
    Value,
    /// Each entry of the field's list.
    Items,
    /// Each value of the field's mapping.
    Values,
}

/// How one file type is written: its field orders, the root field that opens
/// its sections and the root lists whose entries are spaced.
pub(super) struct Layout {
    pub(super) orders: &'static Class,
    /// Every root field the order places before this one belongs to the
    /// header and may stand together; one blank line precedes each other.
    pub(super) sections: &'static str,
    /// Root lists whose entries `entry_spacing` separates.
    pub(super) spaced: &'static [&'static str],
    /// The authoring findings read a specification.
    pub(super) specification: bool,
}

// Statics, not consts: the field orders they refer to are statics, and a
// const that refers to a static needs Rust 1.83, newer than the workspace's
// rust-version.
static LEGACY: Layout = Layout {
    orders: fields::LEGACY_SPEC,
    sections: "output",
    spaced: &["intermediates", "columns", "rows"],
    specification: true,
};
static SPEC: Layout = Layout {
    orders: fields::SPEC,
    sections: "output",
    spaced: &["intermediates", "columns", "rows"],
    specification: true,
};
static PROJECT: Layout = Layout {
    orders: fields::PROJECT,
    sections: "sdtm",
    spaced: &[],
    specification: false,
};
static CODELISTS: Layout = Layout {
    orders: fields::CODELISTS,
    sections: "codelists",
    spaced: &["codelists"],
    specification: false,
};
static FUNCTION: Layout = Layout {
    orders: fields::FUNCTION,
    sections: "tests",
    spaced: &["tests"],
    specification: false,
};

/// The layout `document` is written in. A root that declares the 1.x header,
/// `yamaa` and no `schema_version` as the engine reads it (SCHEMA-2), follows
/// the 1.0 schema of the type it declares, a missing or unknown type that of
/// a specification. Any other root is a 0.5 specification.
pub(super) fn layout(document: &Document) -> &'static Layout {
    if !document.versioned {
        return &LEGACY;
    }
    let declared = document
        .member(document.root, "type")
        .and_then(|value| document.scalar(value))
        .and_then(FileType::parse);
    match declared {
        None | Some(FileType::Spec) => &SPEC,
        Some(FileType::Project) => &PROJECT,
        Some(FileType::Codelists) => &CODELISTS,
        Some(FileType::Function) => &FUNCTION,
    }
}

/// One finding before it is bound to a file: zero-based position.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Departure {
    pub name: &'static str,
    pub line: usize,
    pub column: usize,
    pub spec_path: String,
    pub message: String,
    /// The canonical rewrite a schema finding names, by the nodes of the
    /// document it was found in.
    pub fix: Option<Rewrite>,
}

/// A rewrite to a value's canonical spelling, which keeps its meaning but not
/// its written value (P-HND-3, P-KIND-1, P-LONG-1).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Rewrite {
    /// Remove the mapping entry of `key`, which restates its default.
    Remove { key: usize },
    /// Write the scalar `node` as `text`, the YAML scalar its grammar text
    /// stands for.
    Scalar { node: usize, text: String },
    /// Write the long form `node` as its shorthand: the node `shorthand`, or
    /// the bare keyword when there is none.
    Shorthand {
        node: usize,
        shorthand: Option<usize>,
        keyword: String,
    },
}

impl Departure {
    pub(super) fn new(
        name: &'static str,
        line: usize,
        column: usize,
        spec_path: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            name,
            line,
            column,
            spec_path: spec_path.into(),
            message: message.into(),
            fix: None,
        }
    }

    pub(super) fn with_fix(self, fix: Rewrite) -> Self {
        Self {
            fix: Some(fix),
            ..self
        }
    }
}

/// Order departures by line, column and finding, keeping the order of equal ones.
pub(super) fn sort(found: &mut [Departure]) {
    found.sort_by(|a, b| (a.line, a.column, a.name).cmp(&(b.line, b.column, b.name)));
}

/// Every mapping `field_order` orders: its schema field order and its node,
/// the root first, then each mapping its layout's classes hold, depth first.
pub(super) fn ordered_mappings(document: &Document) -> Vec<(&'static [&'static str], usize)> {
    let mut mappings = Vec::new();
    if let Some(root) = document.root.filter(|&root| document.pairs(root).is_some()) {
        collect_mappings(document, root, layout(document).orders, &mut mappings);
    }
    mappings
}

fn collect_mappings(
    document: &Document,
    mapping: usize,
    class: &'static Class,
    mappings: &mut Vec<(&'static [&'static str], usize)>,
) {
    mappings.push((class.fields, mapping));
    for &(field, holds, held) in class.members {
        let Some(value) = document.member(Some(mapping), field) else {
            continue;
        };
        let values = match holds {
            Holds::Value => vec![value],
            Holds::Items => document.items(value).map(<[_]>::to_vec).unwrap_or_default(),
            Holds::Values => document
                .pairs(value)
                .map(|pairs| pairs.iter().map(|&(_, item)| item).collect())
                .unwrap_or_default(),
        };
        for value in values {
            if document.pairs(value).is_some() {
                collect_mappings(document, value, held, mappings);
            }
        }
    }
}

/// A field's place in a schema order.
pub(super) fn rank(order: &[&str], name: &str) -> Option<usize> {
    order.iter().rposition(|field| *field == name)
}

/// The first field written after a field the schema orders it before: its
/// key node, its name and the name of that later field.
pub(super) fn order_violation(
    document: &Document,
    mapping: usize,
    order: &[&str],
) -> Option<(usize, String, String)> {
    let mut latest: Option<(usize, &str)> = None;
    for &(key, _) in document.pairs(mapping)? {
        let name = document.scalar(key).unwrap_or("");
        let Some(position) = rank(order, name) else {
            continue;
        };
        if let Some((rank, later)) = latest {
            if position < rank {
                return Some((key, name.to_owned(), later.to_owned()));
            }
        }
        if latest.is_none_or(|(rank, _)| position > rank) {
            latest = Some((position, name));
        }
    }
    None
}

/// The root fields of `document`'s header, written before its first section,
/// which `root_spacing` lets stay together.
pub(super) fn header_fields(document: &Document) -> HashSet<&'static str> {
    let layout = layout(document);
    let order = layout.orders.fields;
    let sections = order
        .iter()
        .position(|field| *field == layout.sections)
        .unwrap_or(order.len());
    order[..sections].iter().copied().collect()
}

/// Every departure in `document`, before suppression.
pub(super) fn check(document: &Document) -> Vec<Departure> {
    if document.root.is_none() {
        return Vec::new();
    }
    let paths = line_paths(document);
    let mut found = field_order(document);
    found.extend(root_spacing(document));
    found.extend(entry_spacing(document));
    found.extend(blank_lines(document));
    found.extend(line_width(document, &paths));
    if layout(document).specification {
        // The authoring lints read the specification's value; they never
        // change it.
        found.extend(authoring::check(document));
    }
    // A 1.x file's canonical spellings, read through its schema.
    found.extend(canonical::check(document));
    sort(&mut found);
    found
}

fn line_paths(document: &Document) -> HashMap<usize, String> {
    let mut paths = HashMap::new();
    for (path, id) in document.walk() {
        paths.entry(document.node(id).start.line).or_insert(path);
    }
    paths
}

fn field_order(document: &Document) -> Vec<Departure> {
    let mut found = Vec::new();
    for (order, mapping) in ordered_mappings(document) {
        let Some((key, name, later)) = order_violation(document, mapping, order) else {
            continue;
        };
        let start = document.node(key).start;
        found.push(Departure::new(
            "field_order",
            start.line,
            start.column,
            document.path(key),
            format!("write `{name}` before `{later}`"),
        ));
    }
    found
}

fn gap_has_blank(document: &Document, previous: usize, start: usize) -> bool {
    (document.last_line(previous) + 1..start).any(|line| document.is_blank(line))
}

fn root_spacing(document: &Document) -> Vec<Departure> {
    let mut found = Vec::new();
    let Some(root) = document.root else {
        return found;
    };
    let Value::Mapping { flow: false, pairs } = &document.node(root).value else {
        return found;
    };
    let header = header_fields(document);
    for window in pairs.windows(2) {
        let (previous, key) = (window[0].1, window[1].0);
        let name = document.scalar(key).unwrap_or("");
        if header.contains(name) {
            continue;
        }
        let start = document.node(key).start;
        if !gap_has_blank(document, previous, document.lead_start(start.line)) {
            found.push(Departure::new(
                "root_spacing",
                start.line,
                start.column,
                document.path(key),
                format!("leave one blank line before `{name}`"),
            ));
        }
    }
    found
}

fn entry_spacing(document: &Document) -> Vec<Departure> {
    let mut found = Vec::new();
    for &field in layout(document).spaced {
        let Some(entries) = document.member(document.root, field) else {
            continue;
        };
        let Value::Sequence { flow: false, items } = &document.node(entries).value else {
            continue;
        };
        for index in 1..items.len() {
            let (previous, entry) = (items[index - 1], items[index]);
            let multiline = document.last_line(previous) > document.node(previous).start.line
                || document.last_line(entry) > document.node(entry).start.line;
            let start = document.node(entry).start;
            if multiline && !gap_has_blank(document, previous, document.lead_start(start.line)) {
                found.push(Departure::new(
                    "entry_spacing",
                    start.line,
                    start.column,
                    document.path(entry),
                    format!("leave one blank line between `{field}` entries"),
                ));
            }
        }
    }
    found
}

/// Blank lines between a key and the block value that begins below it.
pub(super) fn blank_after_openers(document: &Document) -> Vec<usize> {
    let mut blanks = Vec::new();
    for (_, id) in document.walk() {
        let Value::Mapping { flow: false, pairs } = &document.node(id).value else {
            continue;
        };
        for &(key, value) in pairs {
            let opener = document.node(key).start.line;
            let block = matches!(
                document.node(value).value,
                Value::Mapping { flow: false, .. } | Value::Sequence { flow: false, .. }
            );
            let start = document.node(value).start.line;
            if block && start > opener {
                blanks.extend((opener + 1..start).filter(|&line| document.is_blank(line)));
            }
        }
    }
    blanks
}

fn blank_lines(document: &Document) -> Vec<Departure> {
    let lines = &document.lines;
    let mut found = Vec::new();
    let mut reported = HashSet::new();
    for line in 0..lines.len() {
        if !document.is_blank(line) {
            continue;
        }
        let message = if line == 0 {
            "remove the leading blank line"
        } else if line == lines.len() - 1 {
            "remove the blank line at the end"
        } else if document.is_blank(line - 1) {
            "remove the second blank line"
        } else {
            continue;
        };
        reported.insert(line);
        found.push(Departure::new("blank_lines", line, 0, "$", message));
    }
    for line in blank_after_openers(document) {
        if !reported.contains(&line) {
            found.push(Departure::new(
                "blank_lines",
                line,
                0,
                "$",
                "remove the blank line after the key that opens this block",
            ));
        }
    }
    if !document.text.is_empty() && !document.text.ends_with(['\r', '\n']) {
        let last = lines.len() - 1;
        found.push(Departure::new(
            "blank_lines",
            last,
            lines[last].len(),
            "$",
            "end the file with a line break",
        ));
    }
    found
}

fn line_width(document: &Document, paths: &HashMap<usize, String>) -> Vec<Departure> {
    let mut found = Vec::new();
    for (line, text) in document.lines.iter().enumerate() {
        if text.len() > LINE_WIDTH {
            found.push(Departure::new(
                "line_width",
                line,
                LINE_WIDTH,
                paths.get(&line).map_or("$", String::as_str),
                format!("line is {} characters; wrap it at {LINE_WIDTH}", text.len()),
            ));
        }
    }
    found
}

/// The departures no valid suppression covers, with an `invalid_suppression`
/// finding for each suppression that names no finding, names one this lint
/// does not report (or `invalid_suppression` itself) or gives no reason.
pub(super) fn suppressed(document: &Document, found: Vec<Departure>) -> Vec<Departure> {
    let mut kept = Vec::new();
    let mut valid = Vec::new();
    for suppression in &document.suppressions {
        let unknown = suppression
            .names
            .iter()
            .find(|name| !FINDINGS.contains(&name.as_str()) || *name == "invalid_suppression");
        if suppression.names.is_empty() || unknown.is_some() || suppression.reason.is_empty() {
            let problem = match unknown {
                Some(name) => format!("unknown finding `{name}`"),
                None => "name the findings and give a reason after `--`".to_owned(),
            };
            let column = document.lines[suppression.line].find('#').unwrap_or(0);
            kept.push(Departure::new(
                "invalid_suppression",
                suppression.line,
                column,
                "$",
                problem,
            ));
            continue;
        }
        valid.push(suppression);
    }
    for item in found {
        let covered = valid.iter().any(|suppression| {
            suppression.names.iter().any(|name| name == item.name)
                && suppression.first <= item.line
                && item.line <= suppression.last
        });
        if !covered {
            kept.push(item);
        }
    }
    sort(&mut kept);
    kept
}
