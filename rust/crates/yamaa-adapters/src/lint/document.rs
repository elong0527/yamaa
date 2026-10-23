//! One specification file as written: its lines, its nodes, its comment lines
//! and its suppressions.
//!
//! The lint judges the text a person reads, so this keeps what document
//! admission discards: each node's start and end, each collection's flow or
//! block form, each scalar's written style, the comment lines, and the lines
//! that belong to a multi-line scalar or flow collection and are therefore
//! value text rather than layout. The nodes come from the shared scanner's
//! located events. A position is where a reader sees the node: a scalar from
//! its first character (a block scalar from its `|` or `>` indicator, a
//! quoted one from its opening quote) to just after its last (a quoted one's
//! closing quote; a block scalar's end follows its trailing line breaks), a
//! collection from its first key, `-`, `[` or `{`, and a value left empty from
//! just after the `:` or `-` that introduces it.

use std::collections::HashMap;

use crate::yaml_decode::{decode_yaml, DecodeFailure, DecodedYaml};
use crate::yaml_source::{scan_strict, LocatedEvent, SourceEvent, SourceText};
use yamaa_saphyr_parser::ScalarStyle;

/// The comment that suppresses findings at one site (P-LINT-1).
pub(super) const SUPPRESSION: &str = "yamaa-lint:";

/// A zero-based position; findings report lines and columns one-based.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Mark {
    pub line: usize,
    pub column: usize,
    pub index: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Style {
    Plain,
    Single,
    Double,
    Literal,
    Folded,
}

#[derive(Clone, Debug)]
pub(super) enum Value {
    Scalar {
        text: String,
        style: Style,
    },
    Sequence {
        flow: bool,
        items: Vec<usize>,
    },
    Mapping {
        flow: bool,
        pairs: Vec<(usize, usize)>,
    },
}

#[derive(Clone, Debug)]
pub(super) struct Node {
    pub value: Value,
    pub start: Mark,
    pub end: Mark,
}

/// One `# yamaa-lint: allow` comment and the lines it covers.
#[derive(Clone, Debug)]
pub(super) struct Suppression {
    pub line: usize,
    pub names: Vec<String>,
    pub reason: String,
    pub first: usize,
    pub last: usize,
}

/// A parsed yamaa file. Line numbers are zero-based here.
pub(super) struct Document {
    pub text: String,
    pub lines: Vec<String>,
    pub nodes: Vec<Node>,
    pub root: Option<usize>,
    content: Vec<bool>,
    pub suppressions: Vec<Suppression>,
    pub newline: &'static str,
    /// Each node's spec path, in the notation findings write (P-CLR-2); a
    /// mapping key has its value's.
    paths: Vec<String>,
    /// The values document admission decoded, which the schema findings
    /// read; none for the directory scan's structure alone.
    pub decoded: Option<DecodedYaml>,
    /// Another specification of the same lint call names this file in
    /// `include`: it is a fragment, which composes on the layers before it
    /// (LAYER-3).
    pub included: bool,
}

// --- Text as Python's str methods read it ---------------------------------------

/// The ASCII characters Python's `str.isspace` accepts.
pub(super) fn is_space(byte: u8) -> bool {
    matches!(
        byte,
        b'\t' | b'\n' | 0x0b | 0x0c | b'\r' | 0x1c..=0x1f | b' '
    )
}

pub(super) fn strip(text: &str) -> &str {
    lstrip(rstrip(text))
}

pub(super) fn lstrip(text: &str) -> &str {
    let start = text
        .bytes()
        .position(|byte| !is_space(byte))
        .unwrap_or(text.len());
    &text[start..]
}

pub(super) fn rstrip(text: &str) -> &str {
    let end = text
        .bytes()
        .rposition(|byte| !is_space(byte))
        .map_or(0, |index| index + 1);
    &text[..end]
}

/// `text[..end]`, clamped as a Python slice clamps.
pub(super) fn head(text: &str, end: usize) -> &str {
    &text[..end.min(text.len())]
}

/// `text[start..]`, clamped as a Python slice clamps.
pub(super) fn tail(text: &str, start: usize) -> &str {
    &text[start.min(text.len())..]
}

/// Lines split at every ending YAML reads: CRLF, lone CR and LF. A final
/// ending closes the last line rather than opening an empty one.
pub(super) fn split_lines(text: &str) -> Vec<String> {
    let mut lines = Vec::new();
    let bytes = text.as_bytes();
    let mut start = 0;
    let mut index = 0;
    while index < bytes.len() {
        let width = match bytes[index] {
            b'\r' if bytes.get(index + 1) == Some(&b'\n') => 2,
            b'\r' | b'\n' => 1,
            _ => {
                index += 1;
                continue;
            }
        };
        lines.push(text[start..index].to_owned());
        index += width;
        start = index;
    }
    lines.push(text[start..].to_owned());
    if lines.last().is_some_and(String::is_empty) {
        lines.pop();
    }
    lines
}

/// How many line endings `text` holds, CRLF counted once.
pub(super) fn count_endings(text: &str) -> usize {
    let bytes = text.as_bytes();
    let mut count = 0;
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'\r' if bytes.get(index + 1) == Some(&b'\n') => {
                count += 1;
                index += 2;
            }
            b'\r' | b'\n' => {
                count += 1;
                index += 1;
            }
            _ => index += 1,
        }
    }
    count
}

/// The ending a fix writes back: the file's dominant one. A single CRLF in an
/// LF file must not let a one-line fix rewrite every ending; ties keep the
/// order CRLF, CR, LF, so a uniformly written file keeps its own ending.
fn dominant_ending(text: &str) -> &'static str {
    let bytes = text.as_bytes();
    let (mut crlf, mut cr, mut lf) = (0, 0, 0);
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'\r' if bytes.get(index + 1) == Some(&b'\n') => {
                crlf += 1;
                index += 2;
            }
            b'\r' => {
                cr += 1;
                index += 1;
            }
            b'\n' => {
                lf += 1;
                index += 1;
            }
            _ => index += 1,
        }
    }
    let mut newline = "\n";
    let mut dominant = 0;
    for (candidate, count) in [("\r\n", crlf), ("\r", cr), ("\n", lf)] {
        if count > dominant {
            newline = candidate;
            dominant = count;
        }
    }
    newline
}

// --- Positions ----------------------------------------------------------------

struct Lines {
    starts: Vec<usize>,
}

impl Lines {
    fn new(bytes: &[u8]) -> Self {
        let mut starts = vec![0];
        let mut index = 0;
        while index < bytes.len() {
            match bytes[index] {
                b'\r' if bytes.get(index + 1) == Some(&b'\n') => {
                    index += 2;
                    starts.push(index);
                }
                b'\r' | b'\n' => {
                    index += 1;
                    starts.push(index);
                }
                _ => index += 1,
            }
        }
        Self { starts }
    }

    fn line(&self, offset: usize) -> usize {
        self.starts.partition_point(|&start| start <= offset) - 1
    }

    fn start(&self, offset: usize) -> usize {
        self.starts[self.line(offset)]
    }

    fn mark(&self, offset: usize) -> Mark {
        let line = self.line(offset);
        Mark {
            line,
            column: offset - self.starts[line],
            index: offset,
        }
    }
}

/// Just after a quoted scalar's closing quote. The scanner's own span also
/// covers the blanks and comment that follow it on the line.
fn quoted_end(bytes: &[u8], start: usize, limit: usize) -> usize {
    let quote = bytes[start];
    let mut index = start + 1;
    while index < limit {
        match bytes[index] {
            b'\\' if quote == b'"' => index += 2,
            byte if byte == quote => {
                if quote == b'\'' && bytes.get(index + 1) == Some(&b'\'') {
                    index += 2;
                } else {
                    return index + 1;
                }
            }
            _ => index += 1,
        }
    }
    limit
}

/// A block scalar's `|` or `>`: the first outside a comment after the event
/// before it. The scanner starts the scalar's own span at its first content
/// line.
fn block_indicator(bytes: &[u8], from: usize, limit: usize) -> usize {
    let mut comment = false;
    let last = limit.min(bytes.len().saturating_sub(1));
    for (index, &byte) in bytes.iter().enumerate().take(last + 1).skip(from) {
        match byte {
            b'\n' | b'\r' => comment = false,
            _ if comment => {}
            b'#' => comment = true,
            b'|' | b'>' => return index,
            _ => {}
        }
    }
    limit
}

/// A block scalar ends at the start of the line after its last line break;
/// the scanner's own end also covers that line's indentation.
fn block_end(bytes: &[u8], lines: &Lines, end: usize) -> usize {
    let start = lines.start(end);
    if bytes[start..end].iter().all(|&byte| byte == b' ') {
        start
    } else {
        end
    }
}

/// Just after the `-` of an empty sequence entry. The scanner marks the entry
/// after the blanks and comment that follow the `-` on its line.
fn dash_end(bytes: &[u8], lines: &Lines, at: usize) -> Option<usize> {
    let start = lines.start(at);
    let line = &bytes[start..at];
    let comment = (0..line.len())
        .find(|&index| {
            line[index] == b'#' && (index == 0 || matches!(line[index - 1], b' ' | b'\t'))
        })
        .unwrap_or(line.len());
    let mut end = comment;
    while end > 0 && matches!(line[end - 1], b' ' | b'\t') {
        end -= 1;
    }
    (end > 0 && line[end - 1] == b'-').then_some(start + end)
}

/// The `-` that opens a block sequence written as a mapping value: the first
/// character of its line. An indentless sequence's own event is marked at its
/// first entry, after the `-`.
fn dash_start(bytes: &[u8], lines: &Lines, at: usize) -> usize {
    let mut index = lines.start(at);
    while index < at && bytes[index] == b' ' {
        index += 1;
    }
    if bytes.get(index) == Some(&b'-') {
        index
    } else {
        at
    }
}

struct Frame {
    node: usize,
    mapping: bool,
    flow: bool,
    children: Vec<usize>,
}

impl Frame {
    /// The next child of this mapping is a value.
    fn awaits_value(&self) -> bool {
        self.mapping && self.children.len() % 2 == 1
    }
}

/// Where a value the text leaves empty sits.
fn empty_position(bytes: &[u8], lines: &Lines, at: usize, parent: Option<&Frame>) -> usize {
    if bytes.get(at) == Some(&b':') {
        return at + 1;
    }
    match parent {
        Some(frame) if !frame.mapping && !frame.flow => dash_end(bytes, lines, at).unwrap_or(at),
        Some(frame) if frame.awaits_value() => {
            let mut index = at;
            while index > 0 && matches!(bytes[index - 1], b' ' | b'\t' | b'\r' | b'\n') {
                index -= 1;
            }
            if index > 0 && bytes[index - 1] == b':' {
                index
            } else {
                at
            }
        }
        _ => at,
    }
}

// --- The document ---------------------------------------------------------------

impl Document {
    /// Admit `source` through the shared document admission, then keep its
    /// nodes, lines, comment lines and suppressions.
    pub(super) fn parse(source: &[u8]) -> Result<Self, DecodeFailure> {
        let decoded = decode_yaml(source)?;
        let mut document = Self::structure(source)?;
        document.decoded = Some(decoded);
        Ok(document)
    }

    /// The nodes and lines without document admission, as the directory scan
    /// reads a file's root keys.
    pub(super) fn structure(source: &[u8]) -> Result<Self, DecodeFailure> {
        let text = std::str::from_utf8(source).map_err(|_| DecodeFailure::Internal)?;
        let events = scan_strict(source).map_err(DecodeFailure::from)?;
        let (nodes, root) = build(text, &events)?;
        let lines = split_lines(text);
        let mut document = Self {
            text: text.to_owned(),
            content: vec![false; lines.len()],
            lines,
            paths: vec![String::new(); nodes.len()],
            nodes,
            root,
            suppressions: Vec::new(),
            newline: dominant_ending(text),
            decoded: None,
            included: false,
        };
        if let Some(root) = document.root {
            let mut content = std::mem::take(&mut document.content);
            document.mark_content(root, &mut content);
            document.content = content;
            let mut paths = std::mem::take(&mut document.paths);
            document.mark_paths(root, "$".to_owned(), &mut paths);
            document.paths = paths;
        }
        document.suppressions = document.find_suppressions();
        Ok(document)
    }

    pub(super) fn node(&self, id: usize) -> &Node {
        &self.nodes[id]
    }

    /// Whether the file is a layer that composes on others, so a value it
    /// writes may replace one it inherits: it declares `parent` or `include`,
    /// or another file of the call includes it (LAYER-3). A null `parent`
    /// names no parent.
    pub(super) fn layered(&self) -> bool {
        use yamaa_core::schema::DocumentNode as N;
        let declares = |field| match &self.decoded {
            Some(decoded) => {
                let values = &decoded.document;
                values
                    .field(values.root(), field)
                    .is_some_and(|node| !matches!(values.nodes()[node], N::Null))
            }
            None => self.member(self.root, field).is_some(),
        };
        self.included || declares("parent") || declares("include")
    }

    /// The text of a scalar node.
    pub(super) fn scalar(&self, id: usize) -> Option<&str> {
        match &self.nodes[id].value {
            Value::Scalar { text, .. } => Some(text),
            _ => None,
        }
    }

    pub(super) fn pairs(&self, id: usize) -> Option<&[(usize, usize)]> {
        match &self.nodes[id].value {
            Value::Mapping { pairs, .. } => Some(pairs),
            _ => None,
        }
    }

    pub(super) fn items(&self, id: usize) -> Option<&[usize]> {
        match &self.nodes[id].value {
            Value::Sequence { items, .. } => Some(items),
            _ => None,
        }
    }

    pub(super) fn is_flow(&self, id: usize) -> bool {
        matches!(
            self.nodes[id].value,
            Value::Mapping { flow: true, .. } | Value::Sequence { flow: true, .. }
        )
    }

    /// The text a node is written as, from its start to its end.
    pub(super) fn source(&self, id: usize) -> &str {
        let node = &self.nodes[id];
        &self.text[node.start.index..node.end.index.max(node.start.index)]
    }

    pub(super) fn is_blank(&self, line: usize) -> bool {
        !self.content[line] && strip(&self.lines[line]).is_empty()
    }

    pub(super) fn is_comment(&self, line: usize) -> bool {
        !self.content[line] && lstrip(&self.lines[line]).starts_with('#')
    }

    /// First line of the comment block written directly above `line`.
    pub(super) fn lead_start(&self, line: usize) -> usize {
        let mut start = line;
        while start > 0 && self.is_comment(start - 1) {
            start -= 1;
        }
        start
    }

    /// The value of mapping `node` under `name`, if it has one.
    pub(super) fn member(&self, node: Option<usize>, name: &str) -> Option<usize> {
        self.pairs(node?)?
            .iter()
            .find(|&&(key, _)| self.scalar(key) == Some(name))
            .map(|&(_, value)| value)
    }

    /// The key node of mapping `node` named `name`.
    pub(super) fn key(&self, node: Option<usize>, name: &str) -> Option<usize> {
        self.pairs(node?)?
            .iter()
            .find(|&&(key, _)| self.scalar(key) == Some(name))
            .map(|&(key, _)| key)
    }

    /// Zero-based line holding the last character of `node`.
    pub(super) fn last_line(&self, id: usize) -> usize {
        let node = &self.nodes[id];
        match &node.value {
            Value::Mapping { flow: false, pairs } if !pairs.is_empty() => {
                return self.last_line(pairs[pairs.len() - 1].1)
            }
            Value::Sequence { flow: false, items } if !items.is_empty() => {
                return self.last_line(items[items.len() - 1])
            }
            _ => {}
        }
        let end = node.end;
        if let Value::Scalar {
            style: Style::Literal | Style::Folded,
            ..
        } = node.value
        {
            // A block scalar's end follows its trailing blank lines; it ends
            // on its last written line.
            let written = rstrip(&self.text[..end.index]);
            return node.start.line.max(count_endings(written));
        }
        if end.column == 0 && end.line > node.start.line {
            return end.line - 1;
        }
        end.line
    }

    /// Every node with its spec path, the root first, then each value or
    /// entry depth first.
    pub(super) fn walk(&self) -> Vec<(String, usize)> {
        let mut nodes = Vec::new();
        if let Some(root) = self.root {
            self.walk_from(root, &mut nodes);
        }
        nodes
    }

    fn walk_from(&self, id: usize, nodes: &mut Vec<(String, usize)>) {
        nodes.push((self.paths[id].clone(), id));
        match &self.nodes[id].value {
            Value::Mapping { pairs, .. } => {
                for &(_, value) in pairs {
                    self.walk_from(value, nodes);
                }
            }
            Value::Sequence { items, .. } => {
                for &item in items {
                    self.walk_from(item, nodes);
                }
            }
            Value::Scalar { .. } => {}
        }
    }

    /// The spec path of a node, `$` for the root; a mapping key has its
    /// value's. It is written in the notation findings use (P-CLR-1,
    /// P-CLR-2): a list entry that writes `name`, or else `id`, as text is
    /// named by it (`columns.AGE.name`), any other by its index, and a key
    /// that is not a bare segment is quoted (`['NOT DONE']`).
    pub(super) fn path(&self, id: usize) -> &str {
        &self.paths[id]
    }

    fn mark_paths(&self, id: usize, path: String, paths: &mut [String]) {
        match &self.nodes[id].value {
            Value::Mapping { pairs, .. } => {
                for &(key, value) in pairs {
                    let step = self.key_path(&path, key);
                    paths[key] = step.clone();
                    self.mark_paths(value, step, paths);
                }
            }
            Value::Sequence { items, .. } => {
                for (index, &item) in items.iter().enumerate() {
                    self.mark_paths(item, self.item_path(&path, index, item), paths);
                }
            }
            Value::Scalar { .. } => {}
        }
        paths[id] = path;
    }

    /// `path` followed by the step that names mapping key `key`.
    fn key_path(&self, path: &str, key: usize) -> String {
        let name = self.scalar(key).unwrap_or("");
        let parent = if path == "$" { "" } else { path };
        match self.text_scalar(key) {
            Some(name) => spec_join(parent, name),
            // An integer, Boolean or null key takes the bracketed spelling.
            None => format!("{parent}[{name}]"),
        }
    }

    /// `path` followed by the step that names entry `index` of a list.
    fn item_path(&self, path: &str, index: usize, item: usize) -> String {
        // The engine takes `name` when the entry writes it, else `id`, and
        // names the entry by it only when it is text.
        let identity = self
            .member(Some(item), "name")
            .or_else(|| self.member(Some(item), "id"))
            .and_then(|node| self.text_scalar(node));
        if let Some(identity) = identity {
            return spec_join(if path == "$" { "" } else { path }, identity);
        }
        format!("{path}[{index}]")
    }

    /// The text of a scalar that reads as text: quoted or a block scalar, or
    /// a plain scalar the YAML 1.2 core schema reads as no null, Boolean or
    /// number.
    pub(super) fn text_scalar(&self, id: usize) -> Option<&str> {
        match &self.nodes[id].value {
            Value::Scalar {
                text,
                style: Style::Plain,
            } => super::authoring::plain_text(text).then_some(text.as_str()),
            Value::Scalar { text, .. } => Some(text),
            _ => None,
        }
    }

    /// Lines after the first of every multi-line scalar or flow collection.
    fn mark_content(&self, id: usize, content: &mut [bool]) {
        let first = self.nodes[id].start.line;
        let last = self.last_line(id);
        match &self.nodes[id].value {
            Value::Mapping { flow: false, pairs } => {
                for &(key, value) in pairs {
                    self.mark_content(key, content);
                    self.mark_content(value, content);
                }
            }
            Value::Sequence { flow: false, items } => {
                for &item in items {
                    self.mark_content(item, content);
                }
            }
            _ => {
                for line in first + 1..=last {
                    if let Some(slot) = content.get_mut(line) {
                        *slot = true;
                    }
                }
            }
        }
    }

    /// Each line an entry or field starts on, with the last line it covers.
    fn spans(&self) -> HashMap<usize, usize> {
        let mut spans = HashMap::new();
        let mut record = |start: usize, end: usize| {
            let covered = spans.get(&start).copied().unwrap_or(start);
            spans.insert(start, end.max(covered));
        };
        for (_, id) in self.walk() {
            match &self.nodes[id].value {
                Value::Mapping { pairs, .. } => {
                    for &(key, value) in pairs {
                        record(self.nodes[key].start.line, self.last_line(value));
                    }
                }
                Value::Sequence { items, .. } => {
                    for &item in items {
                        record(self.nodes[item].start.line, self.last_line(item));
                    }
                }
                Value::Scalar { .. } => {}
            }
        }
        spans
    }

    fn find_suppressions(&self) -> Vec<Suppression> {
        let spans = self.spans();
        let mut suppressions = Vec::new();
        for (line, text) in self.lines.iter().enumerate() {
            if !self.is_comment(line) {
                continue;
            }
            let Some((names, reason)) = suppression(strip(text)) else {
                if text.contains(SUPPRESSION) {
                    suppressions.push(Suppression {
                        line,
                        names: Vec::new(),
                        reason: String::new(),
                        first: line,
                        last: line,
                    });
                }
                continue;
            };
            let names = names
                .split(',')
                .map(strip)
                .filter(|name| !name.is_empty())
                .map(str::to_owned)
                .collect();
            let mut target = line + 1;
            while target < self.lines.len() && self.is_comment(target) {
                target += 1;
            }
            let last = spans.get(&target).copied().unwrap_or(target);
            suppressions.push(Suppression {
                line,
                names,
                reason: strip(reason).to_owned(),
                first: target,
                last,
            });
        }
        suppressions
    }
}

/// `path` followed by the step naming `key` in the 1.x spec path notation,
/// as the engine writes it: `.key` for a bare segment of ASCII letters,
/// digits, `_` and `-`, else `['key']` with `'` written twice; a bare key
/// starts a path at the root.
fn spec_join(path: &str, key: &str) -> String {
    let plain = !key.is_empty()
        && key
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'));
    match (path.is_empty(), plain) {
        (true, true) => key.to_owned(),
        (false, true) => format!("{path}.{key}"),
        (_, false) => format!("{path}['{}']", key.replace('\'', "''")),
    }
}

/// The names and reason of `# yamaa-lint: allow <names> -- <reason>`,
/// matched as `^#\s*yamaa-lint:\s*allow\s+([a-z_, ]+?)\s*(?:--\s*(.*))?$`
/// matches it, or `None` when it does not.
fn suppression(comment: &str) -> Option<(&str, &str)> {
    let rest = lstrip(comment.strip_prefix('#')?);
    let rest = lstrip(rest.strip_prefix(SUPPRESSION)?);
    let rest = rest.strip_prefix("allow")?;
    let blanks = rest.len() - lstrip(rest).len();
    // The regular expression tries the longest run of blanks first and the
    // shortest list of names, giving blanks back to the names as needed.
    for taken in (1..=blanks).rev() {
        let candidate = &rest[taken..];
        for (index, byte) in candidate.bytes().enumerate() {
            if !matches!(byte, b'a'..=b'z' | b'_' | b',' | b' ') {
                break;
            }
            let after = lstrip(&candidate[index + 1..]);
            if after.is_empty() {
                return Some((&candidate[..=index], ""));
            }
            if let Some(reason) = after.strip_prefix("--") {
                return Some((&candidate[..=index], lstrip(reason)));
            }
        }
    }
    None
}

/// The nodes in event order and the root, with the positions a reader sees.
fn build(text: &str, events: &[LocatedEvent]) -> Result<(Vec<Node>, Option<usize>), DecodeFailure> {
    let bytes = text.as_bytes();
    let lines = Lines::new(bytes);
    let mut nodes: Vec<Node> = Vec::new();
    let mut stack: Vec<Frame> = Vec::new();
    let mut root = None;
    // Where the event before the current one ended in the scanner's spans.
    let mut previous = 0;
    for located in events {
        let (start, end) = (located.start.offset, located.end.offset);
        let id = match &located.event {
            SourceEvent::StreamStart | SourceEvent::StreamEnd | SourceEvent::DocumentEnd => {
                continue
            }
            SourceEvent::DocumentStart(explicit) => {
                previous = if *explicit { end } else { 0 };
                continue;
            }
            SourceEvent::Alias(_) => return Err(DecodeFailure::Internal),
            SourceEvent::Scalar(value, style, _, _) => {
                let SourceText::Unicode(value) = value else {
                    return Err(DecodeFailure::Internal);
                };
                let (first, last, style) = match style {
                    ScalarStyle::Plain if value.is_empty() => {
                        let at = empty_position(bytes, &lines, start, stack.last());
                        (at, at, Style::Plain)
                    }
                    ScalarStyle::Plain => (start, end, Style::Plain),
                    ScalarStyle::SingleQuoted => {
                        (start, quoted_end(bytes, start, end), Style::Single)
                    }
                    ScalarStyle::DoubleQuoted => {
                        (start, quoted_end(bytes, start, end), Style::Double)
                    }
                    ScalarStyle::Literal | ScalarStyle::Folded => (
                        block_indicator(bytes, previous, start),
                        block_end(bytes, &lines, end),
                        if *style == ScalarStyle::Literal {
                            Style::Literal
                        } else {
                            Style::Folded
                        },
                    ),
                };
                nodes.push(Node {
                    value: Value::Scalar {
                        text: value.clone(),
                        style,
                    },
                    start: lines.mark(first),
                    end: lines.mark(last),
                });
                nodes.len() - 1
            }
            SourceEvent::SequenceStart(..) | SourceEvent::MappingStart(..) => {
                let mapping = matches!(located.event, SourceEvent::MappingStart(..));
                // An explicit flow collection's event spans its bracket; a
                // block collection's, and an implicit pair's in a flow
                // sequence, is empty.
                let flow = start != end || stack.last().is_some_and(|frame| frame.flow);
                let first = if !flow
                    && !mapping
                    && stack
                        .last()
                        .is_some_and(|frame| !frame.flow && frame.awaits_value())
                {
                    dash_start(bytes, &lines, start)
                } else {
                    start
                };
                let value = if mapping {
                    Value::Mapping {
                        flow,
                        pairs: Vec::new(),
                    }
                } else {
                    Value::Sequence {
                        flow,
                        items: Vec::new(),
                    }
                };
                nodes.push(Node {
                    value,
                    start: lines.mark(first),
                    end: lines.mark(first),
                });
                let id = nodes.len() - 1;
                attach(&mut stack, &mut root, id)?;
                stack.push(Frame {
                    node: id,
                    mapping,
                    flow,
                    children: Vec::new(),
                });
                previous = end;
                continue;
            }
            SourceEvent::SequenceEnd | SourceEvent::MappingEnd => {
                let frame = stack.pop().ok_or(DecodeFailure::Internal)?;
                let last = if frame.flow && matches!(bytes.get(start), Some(b']' | b'}')) {
                    start + 1
                } else {
                    start
                };
                let node = &mut nodes[frame.node];
                node.end = lines.mark(last);
                node.value = if frame.mapping {
                    let pairs = frame.children.chunks_exact(2);
                    if !pairs.remainder().is_empty() {
                        return Err(DecodeFailure::Internal);
                    }
                    Value::Mapping {
                        flow: frame.flow,
                        pairs: pairs.map(|pair| (pair[0], pair[1])).collect(),
                    }
                } else {
                    Value::Sequence {
                        flow: frame.flow,
                        items: frame.children,
                    }
                };
                previous = end;
                continue;
            }
        };
        attach(&mut stack, &mut root, id)?;
        previous = end;
    }
    if !stack.is_empty() {
        return Err(DecodeFailure::Internal);
    }
    Ok((nodes, root))
}

fn attach(stack: &mut [Frame], root: &mut Option<usize>, id: usize) -> Result<(), DecodeFailure> {
    match stack.last_mut() {
        Some(frame) => frame.children.push(id),
        None if root.is_none() => *root = Some(id),
        None => return Err(DecodeFailure::Internal),
    }
    Ok(())
}
