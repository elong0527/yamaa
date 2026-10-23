//! Fixes, each kept only when proved.
//!
//! A layout fix rearranges the lines of a file and never rewrites what a value
//! says. Each candidate edit is applied to the text, the text is admitted again
//! by the shared document admission, and the edit is kept only when the values,
//! compared with their types, and the comment lines are unchanged. A canonical
//! rewrite (P-HND-3, P-KIND-1, P-LONG-1) is kept only when the text reads as
//! the values with exactly that rewrite applied, with the same comment lines.
//! A refused edit leaves its finding reported, and a fixed 1.x file must give
//! no validation finding that the file as written did not.

use std::collections::{HashMap, HashSet};

use super::canonical;
use super::checks::{
    blank_after_openers, check, header_fields, order_violation, ordered_mappings, rank, suppressed,
    Departure, Rewrite, LINE_WIDTH,
};
use super::document::{head, rstrip, split_lines, strip, tail, Document, Style, Value};

/// A continuation aligned after `[` starting past this column leaves too
/// little room, so the wrapped list steps in from its key instead.
const ALIGN_LIMIT: usize = 40;
const MAX_EDITS: usize = 1000;

/// What makes an edit the same edit across passes, so a refused one is not
/// tried again.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum Key {
    FieldOrder(String),
    LineWidth(String, String),
    Spacing,
    BlankLines,
    Rewrite(&'static str, String),
}

enum Edit {
    FieldOrder {
        mapping: usize,
        order: &'static [&'static str],
        root: bool,
    },
    LineWidth(usize),
    Spacing(Vec<usize>),
    BlankLines,
    Rewrite(Rewrite),
}

fn comments(text: &str) -> HashMap<String, usize> {
    let mut lines = HashMap::new();
    for line in split_lines(text) {
        if line.contains('#') {
            *lines.entry(strip(&line).to_owned()).or_insert(0) += 1;
        }
    }
    lines
}

/// True when `after` reads as the values of `before` with its comment lines,
/// both admitted by the engine's own document admission.
pub(super) fn proved(before: &str, after: &str) -> bool {
    crate::style_values::same_value(before.as_bytes(), after.as_bytes())
        && comments(before) == comments(after)
}

fn join(lines: &[String], newline: &str) -> String {
    let mut text = lines.join(newline);
    text.push_str(newline);
    text
}

/// Apply every proved fix to `text`, a fragment another linted file includes
/// when `included`. A result that a second pass would change again, or a 1.x
/// file whose result gives a validation finding the file as written did not,
/// is refused, leaving `text` as written.
pub(super) fn fix_text(text: &str, included: bool) -> String {
    let result = fix_once(text, included);
    if result == text {
        return result;
    }
    if fix_once(&result, included) != result {
        return text.to_owned();
    }
    // A removed default can leave a long form that only its rewrite in a
    // later pass makes valid again; a chain left halfway is no fix.
    if let Some(before) = canonical::refusals(text) {
        if !canonical::refusals(&result).is_some_and(|after| after.is_subset(&before)) {
            return text.to_owned();
        }
    }
    result
}

fn fix_once(text: &str, included: bool) -> String {
    let mut text = text.to_owned();
    let mut refused = HashSet::new();
    for _ in 0..MAX_EDITS {
        let Ok(mut document) = Document::parse(text.as_bytes()) else {
            return text;
        };
        document.included = included;
        let kept = suppressed(&document, check(&document));
        let mut applied = false;
        for (key, edit) in edits(&document, &kept) {
            if refused.contains(&key) {
                continue;
            }
            let candidate = apply(&document, &edit);
            if let Some(candidate) = candidate.filter(|candidate| {
                *candidate != text
                    && match &edit {
                        Edit::Rewrite(rewrite) => {
                            canonical::same_meaning(&document, rewrite, candidate)
                                && comments(&text) == comments(candidate)
                        }
                        _ => proved(&text, candidate),
                    }
            }) {
                text = candidate;
                applied = true;
                break;
            }
            refused.insert(key);
        }
        if !applied {
            return text;
        }
    }
    text
}

fn edits(document: &Document, found: &[Departure]) -> Vec<(Key, Edit)> {
    let mut edits = Vec::new();
    let wanted = found
        .iter()
        .filter(|item| item.name == "field_order")
        .map(|item| item.spec_path.as_str())
        .collect::<HashSet<_>>();
    if !wanted.is_empty() {
        for (order, mapping) in ordered_mappings(document) {
            let Some((key, _, _)) = order_violation(document, mapping, order) else {
                continue;
            };
            if wanted.contains(document.path(key)) {
                let root = Some(mapping) == document.root;
                edits.push((
                    Key::FieldOrder(document.path(mapping).to_owned()),
                    Edit::FieldOrder {
                        mapping,
                        order,
                        root,
                    },
                ));
            }
        }
    }
    for item in found.iter().filter(|item| item.name == "line_width") {
        edits.push((
            Key::LineWidth(
                item.spec_path.clone(),
                strip(&document.lines[item.line]).to_owned(),
            ),
            Edit::LineWidth(item.line),
        ));
    }
    let mut starts = found
        .iter()
        .filter(|item| matches!(item.name, "root_spacing" | "entry_spacing"))
        .map(|item| document.lead_start(item.line))
        .collect::<Vec<_>>();
    starts.sort_unstable();
    starts.dedup();
    if !starts.is_empty() {
        edits.push((Key::Spacing, Edit::Spacing(starts)));
    }
    if found.iter().any(|item| item.name == "blank_lines") {
        edits.push((Key::BlankLines, Edit::BlankLines));
    }
    for item in found {
        if let Some(rewrite) = &item.fix {
            edits.push((
                Key::Rewrite(item.name, item.spec_path.clone()),
                Edit::Rewrite(rewrite.clone()),
            ));
        }
    }
    edits
}

fn apply(document: &Document, edit: &Edit) -> Option<String> {
    match edit {
        Edit::FieldOrder {
            mapping,
            order,
            root,
        } => reorder(document, *mapping, order, *root),
        Edit::LineWidth(line) => wrap(document, *line),
        Edit::Spacing(starts) => Some(insert_blanks(document, starts)),
        Edit::BlankLines => Some(remove_blanks(document)),
        Edit::Rewrite(rewrite) => rewrite_text(document, rewrite),
    }
}

/// `document`'s text with `[start, end)` replaced by `text`.
fn splice(document: &Document, start: usize, end: usize, text: &str) -> String {
    format!("{}{text}{}", &document.text[..start], &document.text[end..])
}

/// The key that names `node` as a mapping value.
fn holder(document: &Document, node: usize) -> Option<usize> {
    document.nodes.iter().find_map(|item| {
        let Value::Mapping { pairs, .. } = &item.value else {
            return None;
        };
        pairs
            .iter()
            .find(|&&(_, value)| value == node)
            .map(|&(key, _)| key)
    })
}

/// Write one canonical rewrite into the text, or `None` where the text gives
/// it no place this edit can write: the proof then decides nothing, and the
/// finding stays reported.
fn rewrite_text(document: &Document, rewrite: &Rewrite) -> Option<String> {
    match rewrite {
        Rewrite::Remove { key } => remove_entry(document, *key),
        Rewrite::Scalar { node, text } => {
            let node = document.node(*node);
            Some(splice(document, node.start.index, node.end.index, text))
        }
        Rewrite::Shorthand {
            node,
            shorthand,
            keyword,
        } => write_shorthand(document, *node, *shorthand, keyword),
    }
}

/// Remove the mapping entry `key` names. In a flow mapping its text goes with
/// the separator before it, or after it when it is first. In a block mapping
/// its lines go, when the key starts its line, no comment stands directly
/// above it, and another entry remains.
fn remove_entry(document: &Document, key: usize) -> Option<String> {
    let mapping = document.nodes.iter().position(|item| {
        matches!(&item.value, Value::Mapping { pairs, .. } if pairs.iter().any(|&(k, _)| k == key))
    })?;
    let Value::Mapping { flow, pairs } = &document.node(mapping).value else {
        return None;
    };
    let index = pairs.iter().position(|&(k, _)| k == key)?;
    let value = pairs[index].1;
    if *flow {
        let (start, end) = if pairs.len() == 1 {
            (
                document.node(key).start.index,
                document.node(value).end.index,
            )
        } else if index > 0 {
            (
                document.node(pairs[index - 1].1).end.index,
                document.node(value).end.index,
            )
        } else {
            (
                document.node(key).start.index,
                document.node(pairs[1].0).start.index,
            )
        };
        return Some(splice(document, start, end, ""));
    }
    let start = document.node(key).start;
    if pairs.len() < 2
        || !strip(head(&document.lines[start.line], start.column)).is_empty()
        || (start.line > 0 && document.is_comment(start.line - 1))
    {
        return None;
    }
    let mut lines = document.lines.clone();
    lines.drain(start.line..=document.last_line(value));
    Some(join(&lines, document.newline))
}

/// Write the long form `node` as its shorthand: the one-line text of the
/// value `shorthand`, or `keyword`. A flow long form is replaced where it
/// stands, a one-line block entry after its list dash, and a one-line block
/// mapping below its key is written after that key.
fn write_shorthand(
    document: &Document,
    node: usize,
    shorthand: Option<usize>,
    keyword: &str,
) -> Option<String> {
    let written = match shorthand {
        Some(value) => document.source(value),
        None => keyword,
    };
    if written.contains(['\n', '\r']) {
        return None;
    }
    let long = document.node(node);
    match &long.value {
        Value::Mapping { flow: true, .. } => {
            Some(splice(document, long.start.index, long.end.index, written))
        }
        Value::Mapping { flow: false, pairs } if pairs.len() == 1 => {
            let line = long.start.line;
            let value = pairs[0].1;
            if document.last_line(node) != line || document.node(value).start.line != line {
                return None;
            }
            let before = strip(head(&document.lines[line], long.start.column));
            if before.ends_with('-') {
                return Some(splice(
                    document,
                    long.start.index,
                    document.node(value).end.index,
                    written,
                ));
            }
            let key = holder(document, node)?;
            let opener = document.node(key).start.line;
            if !before.is_empty()
                || opener + 1 != line
                || !rstrip(&document.lines[opener]).ends_with(':')
            {
                return None;
            }
            let mut lines = document.lines.clone();
            lines[opener] = format!("{} {written}", rstrip(&lines[opener]));
            lines.remove(line);
            Some(join(&lines, document.newline))
        }
        _ => None,
    }
}

fn insert_blanks(document: &Document, starts: &[usize]) -> String {
    let mut lines = document.lines.clone();
    for &start in starts.iter().rev() {
        lines.insert(start, String::new());
    }
    join(&lines, document.newline)
}

fn remove_blanks(document: &Document) -> String {
    let mut drop = blank_after_openers(document)
        .into_iter()
        .collect::<HashSet<_>>();
    let lines = &document.lines;
    for line in 0..lines.len() {
        if document.is_blank(line)
            && (line == 0 || line == lines.len() - 1 || document.is_blank(line - 1))
        {
            drop.insert(line);
        }
    }
    let mut kept = lines
        .iter()
        .enumerate()
        .filter(|(line, _)| !drop.contains(line))
        .map(|(_, text)| text.clone())
        .collect::<Vec<_>>();
    let leading = kept
        .iter()
        .position(|text| !strip(text).is_empty())
        .unwrap_or(kept.len());
    kept.drain(..leading);
    while kept.last().is_some_and(|text| strip(text).is_empty()) {
        kept.pop();
    }
    join(&kept, document.newline)
}

/// Move whole fields of a block mapping into schema order.
fn reorder(
    document: &Document,
    mapping: usize,
    order: &[&'static str],
    root: bool,
) -> Option<String> {
    let Value::Mapping { flow: false, pairs } = &document.node(mapping).value else {
        return None;
    };
    if pairs.len() < 2 {
        return None;
    }
    let lines = &document.lines;
    let mut sequence = (0..pairs.len()).collect::<Vec<_>>();
    sequence.sort_by_key(|&index| {
        let name = document.scalar(pairs[index].0).unwrap_or("");
        (rank(order, name).unwrap_or(order.len()), index)
    });
    if sequence
        .iter()
        .enumerate()
        .all(|(position, &index)| position == index)
    {
        return None;
    }
    let key_lines = pairs
        .iter()
        .map(|&(key, _)| document.node(key).start.line)
        .collect::<Vec<_>>();
    let ends = pairs
        .iter()
        .map(|&(_, value)| document.last_line(value))
        .collect::<Vec<_>>();
    let starts = std::iter::once(key_lines[0])
        .chain(key_lines[1..].iter().map(|&line| document.lead_start(line)))
        .collect::<Vec<_>>();
    if (1..pairs.len()).any(|index| starts[index] <= ends[index - 1]) {
        return None;
    }
    // Lines `from..to`, empty where a Python slice would be.
    let segment = |from: usize, to: usize| {
        let to = to.min(lines.len());
        lines[from.min(to)..to].to_vec()
    };
    let mut segments = (0..pairs.len())
        .map(|index| segment(starts[index], ends[index] + 1))
        .collect::<Vec<_>>();
    let gaps = std::iter::once(Vec::new())
        .chain((1..pairs.len()).map(|index| segment(ends[index - 1] + 1, starts[index])))
        .collect::<Vec<_>>();
    if root {
        return Some(reorder_root(
            document, pairs, &sequence, &starts, &ends, &segments, &gaps,
        ));
    }
    if gaps.iter().any(|gap| !gap.is_empty()) {
        // Blank lines or detached comments between the fields of one entry
        // have no unambiguous owner once the fields move.
        return None;
    }
    let column = document.node(pairs[0].0).start.column;
    let prefix = head(&lines[key_lines[0]], column).to_owned();
    let first = sequence[0];
    if first != 0 {
        if starts[first] != key_lines[first]
            || !strip(head(&lines[key_lines[first]], column)).is_empty()
        {
            return None;
        }
        segments[first][0] = format!("{prefix}{}", tail(&segments[first][0], column));
        segments[0][0] = format!("{}{}", " ".repeat(column), tail(&segments[0][0], column));
    }
    let mut out = lines[..starts[0]].to_vec();
    for &index in &sequence {
        out.extend(segments[index].iter().cloned());
    }
    out.extend(segment(ends[ends.len() - 1] + 1, lines.len()));
    Some(join(&out, document.newline))
}

/// A comment separated from the next field by a blank line stays with the
/// field it follows, keeping the blank line written above it.
fn detached(gap: &[String]) -> Vec<String> {
    let mut content = gap.to_vec();
    while content.last().is_some_and(|text| strip(text).is_empty()) {
        content.pop();
    }
    while content.len() > 1 && strip(&content[0]).is_empty() && strip(&content[1]).is_empty() {
        content.remove(0);
    }
    if content.iter().any(|text| !strip(text).is_empty()) {
        content
    } else {
        Vec::new()
    }
}

fn reorder_root(
    document: &Document,
    pairs: &[(usize, usize)],
    sequence: &[usize],
    starts: &[usize],
    ends: &[usize],
    segments: &[Vec<String>],
    gaps: &[Vec<String>],
) -> String {
    let lines = &document.lines;
    let header = header_fields(document);
    let names = pairs
        .iter()
        .map(|&(key, _)| document.scalar(key).unwrap_or(""))
        .collect::<Vec<_>>();
    let blanks = |gap: &[String]| gap.iter().filter(|text| strip(text).is_empty()).count();
    let mut trailing = (0..pairs.len() - 1)
        .map(|index| detached(&gaps[index + 1]))
        .collect::<Vec<_>>();
    trailing.push(detached(
        &lines[(ends[ends.len() - 1] + 1).min(lines.len())..],
    ));
    let header_style = (1..pairs.len()).any(|index| {
        blanks(&gaps[index]) > 0
            && header.contains(names[index])
            && header.contains(names[index - 1])
    });
    let mut out = lines[..starts[0]].to_vec();
    let mut previous: Option<usize> = None;
    for &index in sequence {
        if let Some(previous) = previous {
            let separation = if !header.contains(names[index]) {
                1
            } else if index == previous + 1 {
                blanks(&gaps[index]).min(1)
            } else {
                usize::from(header_style)
            };
            out.extend(std::iter::repeat_n(String::new(), separation));
        }
        out.extend(segments[index].iter().cloned());
        out.extend(trailing[index].iter().cloned());
        previous = Some(index);
    }
    join(&out, document.newline)
}

/// Values written in block context: (node, parent column, role).
fn block_context(document: &Document) -> Vec<(usize, usize, &'static str)> {
    let mut context = Vec::new();
    for (_, id) in document.walk() {
        match &document.node(id).value {
            Value::Mapping { flow: false, pairs } => {
                for &(key, value) in pairs {
                    let key = document.node(key).start;
                    if document.node(value).start.line == key.line {
                        context.push((value, key.column, "value"));
                    }
                }
            }
            Value::Sequence { flow: false, items } => {
                for &item in items {
                    context.push((item, document.node(item).start.column, "item"));
                }
            }
            _ => {}
        }
    }
    context
}

/// Rewrite one overlong line by the first strategy that applies.
fn wrap(document: &Document, line: usize) -> Option<String> {
    for (id, column, role) in block_context(document) {
        let node = document.node(id);
        let (start, end) = (node.start, node.end);
        let scalar = matches!(node.value, Value::Scalar { .. });
        if !scalar && !document.is_flow(id) {
            continue;
        }
        if !(start.line <= line && line <= end.line) {
            continue;
        }
        // A block scalar at the end of the file ends on the line after the
        // last written line, so there is no line to read trailing text from.
        if end.line < document.lines.len()
            && !strip(tail(&document.lines[end.line], end.column)).is_empty()
        {
            continue;
        }
        if let Value::Sequence { flow: true, .. } = node.value {
            return wrap_sequence(document, id, column);
        }
        if start.line != line || end.line != line {
            continue;
        }
        match node.value {
            Value::Mapping { flow: true, .. } => {
                return block_mapping(document, id, column, role, line);
            }
            Value::Scalar {
                style: Style::Double | Style::Single | Style::Plain,
                ..
            } => {
                let indent = if role == "value" {
                    column + 2
                } else {
                    start.column
                };
                return fold_scalar(document, id, indent, line);
            }
            _ => {}
        }
    }
    None
}

fn replace(document: &Document, line: usize, new: Vec<String>) -> String {
    let lines = &document.lines;
    let mut out = lines[..line].to_vec();
    out.extend(new);
    out.extend(lines[line + 1..].iter().cloned());
    join(&out, document.newline)
}

/// Continue a flow list of scalars on following lines.
fn wrap_sequence(document: &Document, id: usize, column: usize) -> Option<String> {
    let items = document.items(id)?;
    if items.is_empty() || items.iter().any(|&item| document.scalar(item).is_none()) {
        return None;
    }
    let node = document.node(id);
    let (first, last) = (node.start.line, node.end.line);
    if (first..=last).any(|line| {
        document
            .lines
            .get(line)
            .is_some_and(|text| text.contains('#'))
    }) {
        return None;
    }
    let opening = node.start.column + 1;
    let indent = if opening <= ALIGN_LIMIT {
        opening
    } else {
        column + 4
    };
    let mut out = Vec::new();
    let mut current = head(&document.lines[first], opening).to_owned();
    let mut filled = false;
    for (index, &item) in items.iter().enumerate() {
        let piece = format!(
            "{}{}",
            document.source(item),
            if index == items.len() - 1 { "]" } else { "," }
        );
        if filled && current.len() + 1 + piece.len() > LINE_WIDTH {
            out.push(std::mem::replace(&mut current, " ".repeat(indent) + &piece));
        } else {
            if filled {
                current.push(' ');
            }
            current.push_str(&piece);
        }
        filled = true;
    }
    out.push(current);
    if out.iter().any(|part| part.len() > LINE_WIDTH) {
        return None;
    }
    let lines = &document.lines;
    let mut text = lines[..first].to_vec();
    text.extend(out);
    text.extend(lines[(last + 1).min(lines.len())..].iter().cloned());
    Some(join(&text, document.newline))
}

/// Write a one-line flow mapping as a block mapping, member by member.
fn block_mapping(
    document: &Document,
    id: usize,
    column: usize,
    role: &str,
    line: usize,
) -> Option<String> {
    let pairs = document.pairs(id)?;
    if pairs.is_empty() {
        return None;
    }
    // `{literal: X}` is the canonical spelling of a literal; it stays a flow
    // mapping.
    if pairs.len() == 1 && document.scalar(pairs[0].0) == Some("literal") {
        return None;
    }
    let text = &document.lines[line];
    let members = pairs
        .iter()
        .map(|&(key, value)| {
            let written = document.source(value);
            if written.is_empty() {
                format!("{}:", document.source(key))
            } else {
                format!("{}: {written}", document.source(key))
            }
        })
        .collect::<Vec<_>>();
    let start = document.node(id).start.column;
    let out = if role == "value" {
        let indent = " ".repeat(column + 2);
        std::iter::once(rstrip(head(text, start)).to_owned())
            .chain(members.iter().map(|entry| format!("{indent}{entry}")))
            .collect()
    } else {
        let indent = " ".repeat(start);
        std::iter::once(format!("{}{}", head(text, start), members[0]))
            .chain(members[1..].iter().map(|entry| format!("{indent}{entry}")))
            .collect()
    };
    Some(replace(document, line, out))
}

/// Continue a quoted or plain scalar on following lines at single spaces.
/// YAML folds a line break inside a flow scalar to one space, so breaking at
/// a space between two other characters keeps the value; the proof still
/// decides whether the rewritten line is kept.
fn fold_scalar(document: &Document, id: usize, indent: usize, line: usize) -> Option<String> {
    let node = document.node(id);
    let text = &document.lines[line];
    let mut head_text = head(text, node.start.column).to_owned();
    let mut rest = document.source(id).to_owned();
    let mut out = Vec::new();
    let mut width = LINE_WIDTH as isize - head_text.len() as isize;
    let following = LINE_WIDTH as isize - indent as isize;
    let double = matches!(
        node.value,
        Value::Scalar {
            style: Style::Double,
            ..
        }
    );
    while rest.len() as isize > width {
        let bytes = rest.as_bytes();
        let limit = (width + 1).min(bytes.len() as isize - 1);
        let mut breaks = (1..limit.max(1))
            .map(|index| index as usize)
            .filter(|&index| {
                bytes[index] == b' '
                    && !matches!(bytes[index - 1], b' ' | b'\\')
                    && !matches!(bytes[index + 1], b' ' | b'#')
            })
            .collect::<Vec<_>>();
        if breaks.is_empty() {
            return None;
        }
        if double {
            // Keep a predicate's quoted text on one line when a break
            // outside it exists.
            let mut quoted = HashSet::new();
            let mut inside = false;
            for (index, &byte) in bytes.iter().enumerate() {
                if byte == b'\'' {
                    inside = !inside;
                } else if inside {
                    quoted.insert(index);
                }
            }
            let outside = breaks
                .iter()
                .copied()
                .filter(|index| !quoted.contains(index))
                .collect::<Vec<_>>();
            if !outside.is_empty() {
                breaks = outside;
            }
        }
        // Prefer a break after a comma or before a Boolean connective, so a
        // predicate continues at a clause rather than inside a comparison.
        let half = width.div_euclid(2);
        let clauses = breaks
            .iter()
            .copied()
            .filter(|&index| {
                index as isize >= half
                    && (bytes[index - 1] == b','
                        || rest[index + 1..].starts_with("AND ")
                        || rest[index + 1..].starts_with("OR "))
            })
            .collect::<Vec<_>>();
        if !clauses.is_empty() {
            breaks = clauses;
        }
        let mut cut = breaks[breaks.len() - 1];
        let remaining = |index: usize| rest.len() as isize - index as isize - 1;
        let closing = breaks
            .iter()
            .copied()
            .filter(|&index| remaining(index) <= following)
            .collect::<Vec<_>>();
        if !closing.is_empty() {
            // The rest fits on one more line: balance the two lines rather
            // than leave a word or two alone on the last one.
            let balance = |index: usize| (index as isize).max(remaining(index));
            let best = closing
                .iter()
                .map(|&index| balance(index))
                .min()
                .unwrap_or(0);
            cut = closing
                .into_iter()
                .find(|&index| balance(index) == best)
                .unwrap_or(cut);
        }
        out.push(format!("{head_text}{}", &rest[..cut]));
        rest = rest[cut + 1..].to_owned();
        head_text = " ".repeat(indent);
        width = following;
    }
    out.push(format!("{head_text}{rest}"));
    Some(replace(document, line, out))
}
