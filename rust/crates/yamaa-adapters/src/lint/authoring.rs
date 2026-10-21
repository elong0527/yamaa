//! Authoring lints that read the specification's value, not only its layout.
//!
//! `repeated_row_derivation`, `redundant_intermediate` and `missing_label`
//! name departures that leave the parsed value unchanged only when the author
//! rewrites them: a uniform row-template derivation that belongs at column
//! level, a named intermediate equivalent to the implicit join, and an output
//! column without a label. A finding is not a validation diagnostic; the
//! specification stays valid and runs the same way. No fix is offered: moving
//! a derivation, removing an intermediate or writing a label changes the
//! written text of a value, so its author corrects it.
//!
//! The value is the file's YAML 1.2 value as the style guide's reference
//! reader resolved it: a plain scalar is null, a Boolean, a 64-bit integer
//! (decimal, `0x` or `0o`), a float (an integer too wide for 64 bits among
//! them) or text. Mappings compare and look up keys by value, as that reader's
//! dictionaries do.

use std::collections::{BTreeSet, HashSet};

use super::checks::{sort, Departure};
use super::document::{Document, Style, Value};

const DATASET_LEVEL_OPERATIONS: [&str; 6] = [
    "aggregate",
    "row_number",
    "rank",
    "row_value",
    "locf",
    "baseline_flag",
];

/// One resolved value. A mapping keeps its keys in written order, each once.
#[derive(Clone, Debug)]
enum Data {
    Null,
    Boolean(bool),
    Integer(i64),
    Float(f64),
    Text(String),
    List(Vec<Data>),
    Map(Vec<(Data, Data)>),
}

impl Data {
    fn text(&self) -> Option<&str> {
        match self {
            Self::Text(text) => Some(text),
            _ => None,
        }
    }

    fn map(&self) -> Option<&[(Data, Data)]> {
        match self {
            Self::Map(entries) => Some(entries),
            _ => None,
        }
    }

    fn list(&self) -> Option<&[Data]> {
        match self {
            Self::List(items) => Some(items),
            _ => None,
        }
    }

    fn is_null(&self) -> bool {
        matches!(self, Self::Null)
    }

    /// The value a mapping holds under the text key `name`.
    fn get(&self, name: &str) -> Option<&Data> {
        lookup(self.map()?, &Data::Text(name.to_owned()))
    }

    /// The kind `values_equal` requires both sides to share.
    fn kind(&self) -> u8 {
        match self {
            Self::Null => 0,
            Self::Boolean(_) => 1,
            Self::Integer(_) => 2,
            Self::Float(_) => 3,
            Self::Text(_) => 4,
            Self::List(_) => 5,
            Self::Map(_) => 6,
        }
    }

    fn number(&self) -> Option<Number> {
        match *self {
            Self::Boolean(value) => Some(Number::Integer(i64::from(value))),
            Self::Integer(value) => Some(Number::Integer(value)),
            Self::Float(value) => Some(Number::Float(value)),
            _ => None,
        }
    }
}

#[derive(Clone, Copy)]
enum Number {
    Integer(i64),
    Float(f64),
}

/// Equality as the reference reader's values compare: Booleans, integers and
/// floats by numeric value, text with text, lists item by item and mappings
/// by keys and values.
fn equal(first: &Data, second: &Data) -> bool {
    match (first, second) {
        (Data::Null, Data::Null) => true,
        (Data::Text(a), Data::Text(b)) => a == b,
        (Data::List(a), Data::List(b)) => {
            a.len() == b.len() && a.iter().zip(b).all(|(a, b)| equal(a, b))
        }
        (Data::Map(a), Data::Map(b)) => {
            a.len() == b.len()
                && a.iter()
                    .all(|(key, value)| lookup(b, key).is_some_and(|other| equal(value, other)))
        }
        _ => match (first.number(), second.number()) {
            (Some(Number::Integer(a)), Some(Number::Integer(b))) => a == b,
            (Some(Number::Float(a)), Some(Number::Float(b))) => a == b,
            (Some(Number::Integer(a)), Some(Number::Float(b)))
            | (Some(Number::Float(b)), Some(Number::Integer(a))) => integral_equal(a, b),
            _ => false,
        },
    }
}

/// An integer equals a float only when the float is that exact integer.
fn integral_equal(integer: i64, float: f64) -> bool {
    // Every float in [-2^63, 2^63) converts to an i64 exactly when integral.
    let low = i64::MIN as f64;
    float.is_finite()
        && float.fract() == 0.0
        && (low..-low).contains(&float)
        && float as i64 == integer
}

fn lookup<'a>(entries: &'a [(Data, Data)], key: &Data) -> Option<&'a Data> {
    entries
        .iter()
        .find(|(candidate, _)| equal(candidate, key))
        .map(|(_, value)| value)
}

/// Kind-strict equality: `1`, `1.0` and `true` stay apart at every depth.
fn values_equal(first: &Data, second: &Data) -> bool {
    if first.kind() != second.kind() {
        return false;
    }
    match (first, second) {
        (Data::Map(a), Data::Map(b)) => {
            same_keys(a, b)
                && a.iter().all(|(key, value)| {
                    lookup(b, key).is_some_and(|other| values_equal(value, other))
                })
        }
        (Data::List(a), Data::List(b)) => {
            a.len() == b.len() && a.iter().zip(b).all(|(a, b)| values_equal(a, b))
        }
        _ => equal(first, second),
    }
}

fn same_keys(a: &[(Data, Data)], b: &[(Data, Data)]) -> bool {
    a.len() == b.len() && a.iter().all(|(key, _)| lookup(b, key).is_some())
}

/// Whether a plain scalar reads as text: no null, Boolean or number.
pub(super) fn plain_text(text: &str) -> bool {
    matches!(resolve(text), Data::Text(_))
}

/// Whether a plain scalar reads as the Boolean true.
pub(super) fn plain_true(text: &str) -> bool {
    matches!(resolve(text), Data::Boolean(true))
}

/// A plain scalar's value under the reference reader's YAML 1.2 resolution.
fn resolve(text: &str) -> Data {
    if let Some(digits) = text.strip_prefix("0x") {
        if let Ok(value) = i64::from_str_radix(digits, 16) {
            return Data::Integer(value);
        }
    } else if let Some(digits) = text.strip_prefix("0o") {
        if let Ok(value) = i64::from_str_radix(digits, 8) {
            return Data::Integer(value);
        }
    } else if let Some(digits) = text.strip_prefix('+') {
        if let Ok(value) = digits.parse::<i64>() {
            return Data::Integer(value);
        }
    }
    match text {
        "" | "~" | "null" | "Null" | "NULL" => return Data::Null,
        "true" | "True" | "TRUE" => return Data::Boolean(true),
        "false" | "False" | "FALSE" => return Data::Boolean(false),
        _ => {}
    }
    if let Ok(value) = text.parse::<i64>() {
        return Data::Integer(value);
    }
    let float = match text {
        ".inf" | ".Inf" | ".INF" | "+.inf" | "+.Inf" | "+.INF" => Some(f64::INFINITY),
        "-.inf" | "-.Inf" | "-.INF" => Some(f64::NEG_INFINITY),
        ".nan" | ".NaN" | ".NAN" => Some(f64::NAN),
        // Requiring a digit keeps words such as `inf` text.
        _ if text.bytes().any(|byte| byte.is_ascii_digit()) => text.parse::<f64>().ok(),
        _ => None,
    };
    float.map_or_else(|| Data::Text(text.to_owned()), Data::Float)
}

fn data(document: &Document, id: usize) -> Data {
    match &document.node(id).value {
        Value::Scalar {
            text,
            style: Style::Plain,
        } => resolve(text),
        Value::Scalar { text, .. } => Data::Text(text.clone()),
        Value::Sequence { items, .. } => {
            Data::List(items.iter().map(|&item| data(document, item)).collect())
        }
        Value::Mapping { pairs, .. } => {
            let mut entries: Vec<(Data, Data)> = Vec::with_capacity(pairs.len());
            for &(key, value) in pairs {
                let (key, value) = (data(document, key), data(document, value));
                // A later equal key replaces the value and keeps the place.
                match entries
                    .iter_mut()
                    .find(|(existing, _)| equal(existing, &key))
                {
                    Some(entry) => entry.1 = value,
                    None => entries.push((key, value)),
                }
            }
            Data::Map(entries)
        }
    }
}

/// Every text in a value, mapping keys included, in written order.
fn strings(value: &Data) -> Vec<&str> {
    fn visit<'a>(value: &'a Data, out: &mut Vec<&'a str>) {
        match value {
            Data::Text(text) => out.push(text),
            Data::Map(entries) => {
                for (key, item) in entries {
                    visit(key, out);
                    visit(item, out);
                }
            }
            Data::List(items) => items.iter().for_each(|item| visit(item, out)),
            _ => {}
        }
    }
    let mut out = Vec::new();
    visit(value, &mut out);
    out
}

fn is_word(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

fn starts_name(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

/// Conservative names written in a filter, for the hoisting lint: each run of
/// word characters that starts at a word boundary with a letter or `_`. Extra
/// names can only retain a row derivation, never change behavior.
fn filter_identifiers(predicate: Option<&Data>) -> HashSet<String> {
    let Some(text) = predicate.and_then(Data::text) else {
        return HashSet::new();
    };
    let bytes = text.as_bytes();
    let mut names = HashSet::new();
    let mut index = 0;
    while index < bytes.len() {
        if !is_word(bytes[index]) {
            index += 1;
            continue;
        }
        let start = index;
        while index < bytes.len() && is_word(bytes[index]) {
            index += 1;
        }
        if starts_name(bytes[start]) {
            names.insert(text[start..index].to_owned());
        }
    }
    names
}

/// Identifiers `text` reads bare, not as part of a qualified name: a
/// predicate or flag string reads the columns it names (`"AGG > 0"` reads
/// AGG), while a token beside a `.` is a dataset or a qualified column.
fn bare_identifiers(text: &str) -> HashSet<&str> {
    let bytes = text.as_bytes();
    let mut names = HashSet::new();
    let mut index = 0;
    while index < bytes.len() {
        if !starts_name(bytes[index]) {
            index += 1;
            continue;
        }
        let start = index;
        index += 1;
        while index < bytes.len() && is_word(bytes[index]) {
            index += 1;
        }
        if (start == 0 || bytes[start - 1] != b'.') && bytes.get(index) != Some(&b'.') {
            names.insert(&text[start..index]);
        }
    }
    names
}

/// True when `text` reads `IDENTIFIER.` at an identifier boundary, so
/// `ADSL.AGE` reads ADSL, never an intermediate named DSL.
fn reads_qualified_name(text: &str, identifier: &str) -> bool {
    let bytes = text.as_bytes();
    let wanted = identifier.as_bytes();
    let width = identifier.len();
    (0..bytes.len().saturating_sub(width)).any(|start| {
        (start == 0 || !is_word(bytes[start - 1]))
            && bytes[start..].starts_with(wanted)
            && bytes.get(start + width) == Some(&b'.')
    })
}

fn uses_dataset_operation(value: &Data) -> bool {
    match value {
        Data::Map(entries) => entries.iter().any(|(key, item)| {
            key.text()
                .is_some_and(|key| DATASET_LEVEL_OPERATIONS.contains(&key))
                || uses_dataset_operation(item)
        }),
        Data::List(items) => items.iter().any(uses_dataset_operation),
        _ => false,
    }
}

fn reads_intermediate(value: &Data, intermediates: &BTreeSet<&str>) -> bool {
    !intermediates.is_empty()
        && strings(value).into_iter().any(|text| {
            intermediates
                .iter()
                .any(|identifier| reads_qualified_name(text, identifier))
        })
}

/// True when `value` is a literal derivation, driver-independent: in a 1.x
/// file quoted text, a YAML number or null (D18), and in a 0.5 file
/// `{literal: X}`.
fn is_literal(value: &Data, versioned: bool) -> bool {
    if !versioned {
        return value
            .map()
            .is_some_and(|entries| entries.len() == 1 && entries[0].0.text() == Some("literal"));
    }
    match value {
        Data::Null | Data::Integer(_) | Data::Float(_) => true,
        Data::Text(text) => yamaa_core::expression::parse(text).is_ok_and(|parsed| {
            matches!(
                parsed.nodes()[parsed.ungroup(parsed.root())].kind,
                yamaa_core::expression::NodeKind::String(_)
            )
        }),
        _ => false,
    }
}

/// Column names whose column-level derivations are not row-local (SPEC-6):
/// a derivation that holds an aggregate or a window operation anywhere, reads
/// a `SELF` intermediate, or reads another column that is not row-local.
fn blocked_columns<'a>(columns: &[&'a Data], intermediates: &BTreeSet<&str>) -> HashSet<&'a str> {
    let mut reads: Vec<(&'a str, HashSet<&'a str>)> = Vec::new();
    let mut blocked = HashSet::new();
    for entry in columns {
        let (Some(name), Some(derivation)) = (
            entry.get("name").and_then(Data::text),
            entry.get("derivation").filter(|value| !value.is_null()),
        ) else {
            continue;
        };
        if uses_dataset_operation(derivation) || reads_intermediate(derivation, intermediates) {
            blocked.insert(name);
        }
        let bare = strings(derivation)
            .into_iter()
            .flat_map(bare_identifiers)
            .collect::<HashSet<_>>();
        match reads.iter_mut().find(|(existing, _)| *existing == name) {
            Some(entry) => entry.1 = bare,
            None => reads.push((name, bare)),
        }
    }
    let mut changed = true;
    while changed {
        changed = false;
        for (name, names) in &reads {
            if !blocked.contains(name) && names.iter().any(|read| blocked.contains(read)) {
                blocked.insert(*name);
                changed = true;
            }
        }
    }
    blocked
}

/// The mapping entries of a list field.
fn entries<'a>(value: &'a Data, field: &str) -> Vec<&'a Data> {
    indexed(value, field)
        .into_iter()
        .map(|(_, entry)| entry)
        .collect()
}

/// (raw index, entry) for the mapping entries of a list field. A stray entry
/// that is not a mapping keeps its place, so node lookups name the entry as
/// written.
fn indexed<'a>(value: &'a Data, field: &str) -> Vec<(usize, &'a Data)> {
    value
        .get(field)
        .and_then(Data::list)
        .map(|items| {
            items
                .iter()
                .enumerate()
                .filter(|(_, entry)| entry.map().is_some())
                .collect()
        })
        .unwrap_or_default()
}

/// The mapping node at `index` of the root list `field`.
fn mapping_at(document: &Document, field: &str, index: usize) -> Option<usize> {
    let sequence = document.member(document.root, field)?;
    let item = *document.items(sequence)?.get(index)?;
    document.pairs(item).map(|_| item)
}

/// The position of an entry's `name` (or `id`) key, else of the entry itself,
/// with the entry's spec path.
fn entry_key(
    document: &Document,
    field: &str,
    index: usize,
    key: &str,
) -> Option<(usize, usize, String)> {
    let mapping = mapping_at(document, field, index)?;
    let start = match document.key(Some(mapping), key) {
        Some(key) => document.node(key).start,
        None => document.node(mapping).start,
    };
    Some((start.line, start.column, document.path(mapping).to_owned()))
}

fn row_derivation_key(
    document: &Document,
    row: usize,
    column: &str,
) -> Option<(usize, usize, String)> {
    let mapping = mapping_at(document, "rows", row)?;
    let derivations = document.member(Some(mapping), "derivations")?;
    let key = document.key(Some(derivations), column)?;
    let start = document.node(key).start;
    Some((start.line, start.column, document.path(key).to_owned()))
}

/// `missing_label`: every output column declares `label:`.
fn missing_labels(document: &Document, value: &Data) -> Vec<Departure> {
    let mut found = Vec::new();
    let Some(output) = value.get("output").filter(|output| output.map().is_some()) else {
        return found;
    };
    if value.get("parent").is_some_and(|parent| !parent.is_null()) {
        // A child layer inherits labels; only a standalone file owns them.
        return found;
    }
    let Some(output_columns) = output.get("columns").and_then(Data::list) else {
        return found;
    };
    let mut by_name: Vec<(&str, usize, &Data)> = Vec::new();
    for (index, entry) in indexed(value, "columns") {
        if let Some(name) = entry.get("name").and_then(Data::text) {
            if !by_name.iter().any(|(existing, _, _)| *existing == name) {
                by_name.push((name, index, entry));
            }
        }
    }
    let mut seen = HashSet::new();
    for name in output_columns {
        // One column has one label departure, however often the output
        // list repeats it.
        let Some(name) = name.text().filter(|name| seen.insert(*name)) else {
            continue;
        };
        // Undeclared output columns fail validation; the lint stays silent.
        let Some(&(_, index, entry)) = by_name.iter().find(|(existing, _, _)| *existing == name)
        else {
            continue;
        };
        if entry
            .get("label")
            .and_then(Data::text)
            .is_some_and(|label| !label.is_empty())
        {
            continue;
        }
        let Some((line, column, spec_path)) = entry_key(document, "columns", index, "name") else {
            continue;
        };
        found.push(Departure::new(
            "missing_label",
            line,
            column,
            spec_path,
            format!("output column `{name}` has no label; declare `label:`"),
        ));
    }
    found
}

/// `repeated_row_derivation`: a uniform row-template derivation belongs at
/// column level. Only row-local derivations are reported (SPEC-6); a
/// derivation reading a driver dataset stays when the templates build from
/// different datasets, and a column a row filter reads stays, since filters
/// resolve only columns their own template derives.
fn repeated_row_derivations(document: &Document, value: &Data) -> Vec<Departure> {
    let mut found = Vec::new();
    let rows = entries(value, "rows");
    if rows.len() < 2 {
        return found;
    }
    let columns = entries(value, "columns");
    let column_derivations = columns
        .iter()
        .filter(|entry| {
            entry
                .get("derivation")
                .is_some_and(|value| !value.is_null())
        })
        .filter_map(|entry| entry.get("name").and_then(Data::text))
        .collect::<HashSet<_>>();
    // In a 1.x file a read of a `SELF` intermediate is not row-local, and a
    // read of an intermediate an input backs is (SPEC-6). A 0.5 file keeps
    // the 0.5 reading, in which a read of any named intermediate uses a
    // lookup.
    let intermediates = entries(value, "intermediates")
        .into_iter()
        .filter(|entry| {
            !document.versioned || entry.get("dataset").and_then(Data::text) == Some("SELF")
        })
        .filter_map(|entry| entry.get("id").and_then(Data::text))
        .collect::<BTreeSet<_>>();
    let blocked = blocked_columns(&columns, &intermediates);
    let null = Data::Null;
    let datasets = rows
        .iter()
        .map(|row| row.get("dataset").unwrap_or(&null))
        .collect::<Vec<_>>();
    let uniform_driver = datasets[1..]
        .iter()
        .all(|dataset| equal(dataset, datasets[0]));
    let mut filter_reads = HashSet::new();
    for row in &rows {
        filter_reads.extend(filter_identifiers(row.get("filter")));
    }
    if rows
        .iter()
        .any(|row| row.get("derivations").and_then(Data::map).is_none())
    {
        return found;
    }
    let first_index = indexed(value, "rows")[0].0;
    let Some(first) = rows[0].get("derivations").and_then(Data::map) else {
        return found;
    };
    for (column, _) in first {
        let Some(column) = column.text() else {
            continue;
        };
        // A column-level default already exists; row overrides stay.
        if column_derivations.contains(column) {
            continue;
        }
        if filter_reads.contains(column) {
            continue;
        }
        let values = rows
            .iter()
            .map(|row| {
                row.get("derivations")
                    .and_then(|derivations| derivations.get(column))
            })
            .collect::<Option<Vec<_>>>();
        let Some(values) = values else {
            continue;
        };
        if !values[1..].iter().all(|item| values_equal(values[0], item)) {
            continue;
        }
        let derivation = values[0];
        if uses_dataset_operation(derivation) || reads_intermediate(derivation, &intermediates) {
            continue;
        }
        // Different drivers bind different records; only a literal is
        // driver-independent.
        if !uniform_driver && !is_literal(derivation, document.versioned) {
            continue;
        }
        if strings(derivation).into_iter().any(|text| {
            bare_identifiers(text)
                .iter()
                .any(|name| blocked.contains(name))
        }) {
            continue;
        }
        let Some((line, column_position, spec_path)) =
            row_derivation_key(document, first_index, column)
        else {
            continue;
        };
        found.push(Departure::new(
            "repeated_row_derivation",
            line,
            column_position,
            spec_path,
            format!(
                "derivation of `{column}` is identical in every row template; \
                 move it to columns[].derivation"
            ),
        ));
    }
    found
}

/// True when an explicit key states exactly the output keys.
fn key_equals_output_keys(key: &Data, output_keys: &[&str]) -> bool {
    let wanted = output_keys.iter().copied().collect::<HashSet<_>>();
    if let Some(items) = key.list() {
        let texts = items.iter().map(Data::text).collect::<Option<HashSet<_>>>();
        return texts.is_some_and(|texts| texts == wanted);
    }
    if let Some(entries) = key.map() {
        let names = entries
            .iter()
            .map(|(name, _)| name.text())
            .collect::<Option<HashSet<_>>>();
        return names.is_some_and(|names| names == wanted)
            && entries
                .iter()
                .all(|(column, item)| item.text().is_some() && column.text() == item.text());
    }
    false
}

/// True when `key` is a mapping pairing exactly the columns `input_key`
/// names, each with the current row's same-named value (LOOKUP-5).
fn key_pairs_input_key(key: &Data, input_key: &[&str]) -> bool {
    let Some(entries) = key.map() else {
        return false;
    };
    let wanted = input_key.iter().copied().collect::<HashSet<_>>();
    let names = entries
        .iter()
        .map(|(name, _)| name.text())
        .collect::<Option<HashSet<_>>>();
    names.is_some_and(|names| names == wanted)
        && entries
            .iter()
            .all(|(column, item)| item.text().is_some() && column.text() == item.text())
}

/// A field among `fields` that `entry` declares with a value other than null.
fn declares_any(entry: &Data, fields: &[&str]) -> bool {
    fields
        .iter()
        .any(|field| entry.get(field).is_some_and(|value| !value.is_null()))
}

/// The fields with which a named intermediate narrows, derives or reshapes
/// its dataset (LOOKUP-4).
const NARROWING: [&str; 7] = [
    "filter",
    "between",
    "order_by",
    "keep",
    "derivations",
    "verifications",
    "columns",
];

/// `redundant_intermediate` in a 1.x file: a named intermediate equal to the
/// implicit join to its input (LOOKUP-1, LOOKUP-4). An intermediate that
/// narrows nothing, answers absence with the join's missing (`absent`
/// omitted or null), requires no match, and whose `key` pairs exactly the
/// columns of its input's declared `key`, each with the same-named value,
/// reads what `DATASET.COLUMN` already reads. A `key` written as that list
/// fails validation as `rename_only_intermediate` instead, so only the
/// mapping that spells the same pairs is reported.
fn redundant_intermediates_1_0(document: &Document, value: &Data) -> Vec<Departure> {
    let mut found = Vec::new();
    for (index, entry) in indexed(value, "intermediates") {
        let (Some(identifier), Some(dataset)) = (
            entry.get("id").and_then(Data::text),
            entry.get("dataset").and_then(Data::text),
        ) else {
            continue;
        };
        if dataset == "SELF" || declares_any(entry, &NARROWING) {
            continue;
        }
        let Some(input) = value.get("input").and_then(|inputs| inputs.get(dataset)) else {
            continue;
        };
        // An input without `key` has no implicit join to read instead, and
        // one that requires a match fails where the intermediate answers.
        let Some(input_key) = input
            .get("key")
            .and_then(Data::list)
            .and_then(|key| key.iter().map(Data::text).collect::<Option<Vec<_>>>())
        else {
            continue;
        };
        let required = |declaration: &Data| {
            declaration
                .get("require_match")
                .is_some_and(|flag| matches!(flag, Data::Boolean(true)))
        };
        if required(input)
            || required(entry)
            || entry.get("absent").is_some_and(|answer| !answer.is_null())
        {
            continue;
        }
        if !entry
            .get("key")
            .is_some_and(|key| key_pairs_input_key(key, &input_key))
        {
            continue;
        }
        if !strings(value)
            .into_iter()
            .any(|text| reads_qualified_name(text, identifier))
        {
            continue;
        }
        let Some((line, column, spec_path)) = entry_key(document, "intermediates", index, "id")
        else {
            continue;
        };
        found.push(Departure::new(
            "redundant_intermediate",
            line,
            column,
            spec_path,
            format!(
                "intermediate `{identifier}` reads exactly what the implicit \
                 join to `{dataset}` reads; read `{dataset}` directly"
            ),
        ));
    }
    found
}

/// `redundant_intermediate`: a named intermediate equal to the implicit join.
/// A simple intermediate with no filter, selection, custom key or handler
/// that changes behavior reads what `DATASET.COLUMN` already reads; only an
/// explicit key that restates the output keys is reported. A 1.x file reads
/// it by its 1.0 meaning ([`redundant_intermediates_1_0`]).
fn redundant_intermediates(document: &Document, value: &Data) -> Vec<Departure> {
    if document.versioned {
        return redundant_intermediates_1_0(document, value);
    }
    let mut found = Vec::new();
    let Some(keys) = value
        .get("keys")
        .and_then(Data::list)
        .and_then(|keys| keys.iter().map(Data::text).collect::<Option<Vec<_>>>())
    else {
        return found;
    };
    let Some(raw_intermediates) = value.get("intermediates").and_then(Data::list) else {
        return found;
    };
    let required = value
        .get("input")
        .and_then(Data::map)
        .map(|inputs| {
            inputs
                .iter()
                .filter(|(_, declaration)| {
                    declaration
                        .get("require_match")
                        .is_some_and(|flag| matches!(flag, Data::Boolean(true)))
                })
                .map(|(name, _)| name)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    for (index, entry) in indexed(value, "intermediates") {
        let (Some(identifier), Some(dataset)) = (
            entry.get("id").and_then(Data::text),
            entry.get("dataset").and_then(Data::text),
        ) else {
            continue;
        };
        let dataset_value = Data::Text(dataset.to_owned());
        if dataset == "SELF" || required.iter().any(|name| equal(name, &dataset_value)) {
            continue;
        }
        if declares_any(entry, &NARROWING) {
            continue;
        }
        let raw = raw_intermediates
            .get(index)
            .filter(|raw| raw.map().is_some());
        let field = |name: &str| raw.and_then(|raw| raw.get(name));
        // A required match fails where the join yields missing, and another
        // literal answers there. Omitted, cleared by a bare null or
        // `{literal: null}`, the answer is the join's missing value.
        let cleared = Data::Map(vec![(Data::Text("literal".into()), Data::Null)]);
        if field("require_match").is_some_and(|flag| matches!(flag, Data::Boolean(true)))
            || field("no_match").is_some_and(|answer| !answer.is_null() && !equal(answer, &cleared))
        {
            continue;
        }
        // An omitted key is a rename that validation owns.
        let Some(key) = entry.get("key").filter(|key| !key.is_null()) else {
            continue;
        };
        if !key_equals_output_keys(key, &keys) {
            continue;
        }
        if !strings(value)
            .into_iter()
            .any(|text| reads_qualified_name(text, identifier))
        {
            continue;
        }
        let Some((line, column, spec_path)) = entry_key(document, "intermediates", index, "id")
        else {
            continue;
        };
        found.push(Departure::new(
            "redundant_intermediate",
            line,
            column,
            spec_path,
            format!(
                "intermediate `{identifier}` reads exactly what the implicit \
                 join to `{dataset}` reads; read `{dataset}` directly"
            ),
        ));
    }
    found
}

/// Every authoring departure in `document`; none when its root is not a mapping.
pub(super) fn check(document: &Document) -> Vec<Departure> {
    let Some(root) = document.root else {
        return Vec::new();
    };
    let value = data(document, root);
    if value.map().is_none() {
        return Vec::new();
    }
    let mut found = missing_labels(document, &value);
    found.extend(repeated_row_derivations(document, &value));
    found.extend(redundant_intermediates(document, &value));
    if document.versioned {
        found.extend(unused_input_keys(document, &value));
    }
    sort(&mut found);
    found
}

/// True when every row the specification builds comes from the input
/// `dataset`: each row template's driver, or without templates the root
/// `base`, is that input, a missing one being the sole declared input.
fn every_row_from(value: &Data, dataset: &str) -> bool {
    let inputs: Vec<&str> = value
        .get("input")
        .and_then(Data::map)
        .map(|inputs| inputs.iter().filter_map(|(name, _)| name.text()).collect())
        .unwrap_or_default();
    let sole = |driver: Option<&str>| match driver {
        Some(driver) => driver == dataset,
        None => inputs == [dataset],
    };
    let rows = entries(value, "rows");
    if rows.is_empty() {
        return sole(value.get("base").and_then(Data::text));
    }
    rows.iter()
        .all(|row| sole(row.get("dataset").and_then(Data::text)))
}

/// Every `aggregate` operation in `value`: its payload, the shorthand text or
/// the class form.
fn aggregates<'a>(value: &'a Data, out: &mut Vec<&'a Data>) {
    match value {
        Data::Map(entries) => {
            for (key, item) in entries {
                if key.text() == Some("aggregate") {
                    out.push(item);
                }
                aggregates(item, out);
            }
        }
        Data::List(items) => items.iter().for_each(|item| aggregates(item, out)),
        _ => {}
    }
}

/// Whether `value` reads `dataset` through a qualified name outside an
/// aggregate that declares its own `key`, which matches the input by it.
fn reads_joined(value: &Data, dataset: &str) -> bool {
    match value {
        Data::Text(text) => reads_qualified_name(text, dataset),
        Data::List(items) => items.iter().any(|item| reads_joined(item, dataset)),
        Data::Map(entries) => entries.iter().any(|(key, item)| {
            let own_key = key.text() == Some("aggregate")
                && item.get("key").is_some_and(|key| !key.is_null());
            !own_key && (reads_joined(key, dataset) || reads_joined(item, dataset))
        }),
        _ => false,
    }
}

/// `unused_input_key`: an input's declared `key` that no read uses (D111,
/// P-JOIN-1). A read uses it when it is an implicit join, a qualified read of
/// the input that is not a read of the current row's driver, or a qualified
/// aggregate over the input that declares no `key` of its own (D63); a `copy`
/// entry reads the input through the implicit join too (SPEC-9). A named
/// intermediate over the input reads its records by its own `key`, and when
/// every row comes from the input, a qualified read of it reads the driver.
/// Validation admits such a key, so a shared parent layer can declare it for
/// its children; a file that declares `parent` is not read, since its
/// columns may come from a layer it inherits.
fn unused_input_keys(document: &Document, value: &Data) -> Vec<Departure> {
    let mut found = Vec::new();
    if value.get("parent").is_some_and(|parent| !parent.is_null()) {
        return found;
    }
    let Some(inputs) = value.get("input").and_then(Data::map) else {
        return found;
    };
    let mut payloads = Vec::new();
    aggregates(value, &mut payloads);
    for (name, declaration) in inputs {
        let (Some(dataset), Some(_)) = (name.text(), declaration.get("key")) else {
            continue;
        };
        let reads = |value: &Data| {
            strings(value)
                .into_iter()
                .any(|text| reads_qualified_name(text, dataset))
        };
        let keyless_aggregate = payloads.iter().any(|payload| match payload {
            Data::Map(_) => {
                payload.get("key").is_none_or(Data::is_null)
                    && payload.get("expr").is_some_and(reads)
            }
            other => reads(other),
        });
        let copied = entries(value, "columns").iter().any(|entry| {
            entry
                .get("copy")
                .and_then(|copy| copy.get("dataset"))
                .and_then(Data::text)
                == Some(dataset)
        });
        // Reads outside the input declarations and the intermediates over
        // this input, which read its records as their donors.
        let joined = !every_row_from(value, dataset)
            && value.map().is_some_and(|fields| {
                fields.iter().any(|(field, item)| match field.text() {
                    Some("input") => false,
                    Some("intermediates") => item.list().is_some_and(|entries| {
                        entries.iter().any(|entry| {
                            entry.get("dataset").and_then(Data::text) != Some(dataset)
                                && reads_joined(entry, dataset)
                        })
                    }),
                    _ => reads_joined(item, dataset),
                })
            });
        if keyless_aggregate || copied || joined {
            continue;
        }
        let Some(key) = document
            .member(document.root, "input")
            .and_then(|inputs| {
                document
                    .pairs(inputs)?
                    .iter()
                    .find(|&&(key, _)| document.scalar(key) == Some(dataset))
            })
            .and_then(|&(_, declaration)| document.key(Some(declaration), "key"))
        else {
            continue;
        };
        let start = document.node(key).start;
        found.push(Departure::new(
            "unused_input_key",
            start.line,
            start.column,
            document.path(key),
            format!(
                "no implicit join or keyless aggregate reads input `{dataset}` by its \
                 declared `key`; remove the `key`"
            ),
        ));
    }
    found
}
