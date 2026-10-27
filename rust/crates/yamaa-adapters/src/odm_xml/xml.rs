//! A strict, namespace-aware XML event layer over the quick-xml tokenizer.
//!
//! quick-xml finds markup boundaries and checks UTF-8, end-tag names and
//! comment hyphens. This layer adds the XML 1.0 and Namespaces 1.0 rules the
//! libxml2 parser behind the Python reader enforced, so the reader is at least
//! as strict: one root element, valid characters, names, attribute syntax and
//! references, line-end and attribute-value normalization, bound prefixes,
//! unique attributes, valid namespace URIs, a well-formed XML declaration
//! naming UTF-8 or strictly checked US-ASCII, no DOCTYPE, and libxml2's
//! default size and depth bounds.
use super::Fault;
use quick_xml::events::{BytesPI, Event as Token};
use std::{
    collections::HashSet,
    io::{self, BufRead, Read},
    ops::Range,
    rc::Rc,
};
use yamaa_core::odm_items::{Error, Limit};

/// The largest text node, attribute value, comment or processing instruction,
/// in source bytes. libxml2 refuses 10,000,000 bytes by default.
pub(super) const MAX_TOKEN_BYTES: usize = 9_999_999;
/// The longest name component, the libxml2 default.
pub(super) const MAX_NAME_BYTES: usize = 50_000;
/// The deepest element nesting, the libxml2 default.
pub(super) const MAX_DEPTH: usize = 256;
/// In-scope namespace bindings; prefix lookup scans them, so they stay few.
pub(super) const MAX_NAMESPACES: usize = 4096;

const XML_NAMESPACE: &str = "http://www.w3.org/XML/1998/namespace";
const XMLNS_NAMESPACE: &str = "http://www.w3.org/2000/xmlns/";
const UTF8_BOM: [u8; 3] = [0xEF, 0xBB, 0xBF];
const CHUNK: usize = 64 * 1024;

/// One admitted XML event. Text arrives decoded and normalized, possibly in
/// several pieces per text node; comments, processing instructions, the
/// declaration and whitespace outside the root arrive as `Skip`.
pub(super) enum Event<'a> {
    Start(&'a Tag),
    End,
    Text(&'a str),
    Skip,
    Eof,
}

/// A start tag with its resolved namespaces. Namespace declarations are not
/// attributes; attribute values are normalized.
#[derive(Default)]
pub(super) struct Tag {
    arena: String,
    namespace: Option<Rc<str>>,
    local: Range<usize>,
    attributes: Vec<Attribute>,
}

struct Attribute {
    namespace: Option<Rc<str>>,
    local: Range<usize>,
    value: Range<usize>,
}

impl Tag {
    pub(super) fn namespace(&self) -> Option<&str> {
        self.namespace.as_deref()
    }

    pub(super) fn local(&self) -> &str {
        &self.arena[self.local.clone()]
    }

    /// Every attribute as (namespace, local name, value), in source order.
    pub(super) fn attributes(&self) -> impl Iterator<Item = (Option<&str>, &str, &str)> {
        self.attributes.iter().map(|attribute| {
            (
                attribute.namespace.as_deref(),
                &self.arena[attribute.local.clone()],
                &self.arena[attribute.value.clone()],
            )
        })
    }

    /// The value of the unprefixed attribute `name`.
    pub(super) fn plain(&self, name: &str) -> Option<&str> {
        self.attributes()
            .find(|(namespace, local, _)| namespace.is_none() && *local == name)
            .map(|(_, _, value)| value)
    }

    fn clear(&mut self) {
        self.arena.clear();
        self.namespace = None;
        self.local = 0..0;
        self.attributes.clear();
    }
}

/// A raw attribute: the qualified name and its normalized value in the arena.
struct RawAttribute {
    name: Range<usize>,
    colon: Option<usize>,
    value: Range<usize>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Root {
    Before,
    Inside,
    After,
}

/// In-scope namespace bindings; `None` is the default namespace and an empty
/// URI undeclares it.
#[derive(Default)]
struct Scopes {
    bindings: Vec<(Option<Box<str>>, Rc<str>)>,
    marks: Vec<usize>,
}

impl Scopes {
    fn open(&mut self) {
        self.marks.push(self.bindings.len());
    }

    fn close(&mut self) {
        if let Some(mark) = self.marks.pop() {
            self.bindings.truncate(mark);
        }
    }

    fn bind(&mut self, prefix: Option<&str>, uri: &str) -> Result<(), TagError> {
        if self.bindings.len() >= MAX_NAMESPACES {
            return Err(TagError::Limit(Error::Limit(Limit::Namespaces(
                MAX_NAMESPACES,
            ))));
        }
        self.bindings.push((prefix.map(Into::into), uri.into()));
        Ok(())
    }

    fn lookup(&self, prefix: Option<&str>) -> Option<&Rc<str>> {
        self.bindings
            .iter()
            .rev()
            .find(|(bound, _)| bound.as_deref() == prefix)
            .map(|(_, uri)| uri)
    }

    fn element(&self, prefix: Option<&str>) -> Result<Option<Rc<str>>, String> {
        match prefix {
            None => Ok(self.lookup(None).filter(|uri| !uri.is_empty()).cloned()),
            Some("xml") => Ok(Some(XML_NAMESPACE.into())),
            Some("xmlns") => Err("element names cannot use the reserved prefix xmlns".into()),
            Some(prefix) => self
                .lookup(Some(prefix))
                .cloned()
                .map(Some)
                .ok_or_else(|| format!("namespace prefix {prefix} is not defined")),
        }
    }

    fn attribute(&self, prefix: &str) -> Result<Rc<str>, String> {
        match prefix {
            "xml" => Ok(XML_NAMESPACE.into()),
            prefix => self
                .lookup(Some(prefix))
                .cloned()
                .ok_or_else(|| format!("namespace prefix {prefix} is not defined")),
        }
    }
}

/// Strict XML events from one UTF-8 byte stream.
pub(super) struct Xml<R> {
    reader: quick_xml::Reader<CheckedXml<R>>,
    buf: Vec<u8>,
    text: String,
    tag: Tag,
    raw: Vec<RawAttribute>,
    scopes: Scopes,
    open: usize,
    root: Root,
    first: bool,
    run: usize,
    depth_limit: usize,
}

impl<R: Read> Xml<R> {
    /// `limit` bounds the bytes read from `input`; `depth_limit` the nesting.
    pub(super) fn new(input: R, limit: Option<u64>, depth_limit: usize) -> Self {
        let mut reader = quick_xml::Reader::from_reader(CheckedXml::new(input, limit));
        let config = reader.config_mut();
        config.expand_empty_elements = true;
        config.check_end_names = true;
        config.check_comments = true;
        config.allow_dangling_amp = false;
        config.allow_unmatched_ends = false;
        config.trim_text(false);
        Self {
            reader,
            buf: Vec::new(),
            text: String::new(),
            tag: Tag::default(),
            raw: Vec::new(),
            scopes: Scopes::default(),
            open: 0,
            root: Root::Before,
            first: true,
            run: 0,
            depth_limit: depth_limit.min(MAX_DEPTH),
        }
    }

    /// Read the next admitted event.
    pub(super) fn next(&mut self) -> Result<Event<'_>, Error> {
        // The tokenizer's offsets leave out the UTF-8 byte order mark that it
        // skips, which the byte source finds on its first read, so the event's
        // source offset is known only once the event is read.
        let start = self.reader.buffer_position();
        self.buf.clear();
        let read = self.reader.read_event_into(&mut self.buf);
        let position = start + self.reader.get_ref().bom();
        let token = match read {
            Ok(token) => token,
            Err(error) => {
                // quick-xml checks end-tag names and comment hyphens once it
                // has read the markup's bytes. Decoding those bytes as UTF-8
                // comes first (`utf8`), so an invalid byte in them is reported
                // instead.
                if matches!(error, quick_xml::Error::IllFormed(_)) {
                    utf8(&self.buf, position)?;
                }
                let bom = self.reader.get_ref().bom();
                let at = match &error {
                    quick_xml::Error::Syntax(_) | quick_xml::Error::IllFormed(_) => {
                        self.reader.error_position() + bom
                    }
                    quick_xml::Error::Encoding(quick_xml::encoding::EncodingError::Utf8(error)) => {
                        position + error.valid_up_to() as u64
                    }
                    _ => position,
                };
                return Err(token_error(error, at));
            }
        };
        let first = std::mem::replace(&mut self.first, false);
        // Text that the event holds as read is returned from `self.buf`, at
        // this range, once the event no longer borrows it.
        let held = match token {
            Token::Decl(declaration) => {
                if !first {
                    utf8(&declaration, position + 2)?;
                    return Err(malformed(
                        position,
                        "XML declaration allowed only at the start of the document",
                    ));
                }
                // The tokenizer has consumed these bytes. Admit the encoding
                // before checking the suffix, including a non-UTF-8 suffix
                // after an otherwise readable encoding pseudo-attribute.
                let content = match std::str::from_utf8(&declaration) {
                    Ok(content) => content,
                    Err(error) => utf8(&declaration[..error.valid_up_to()], position + 2)?,
                };
                let declared = declaration_encoding(&content[3..]);
                if let Ok((encoding, _)) = &declared {
                    self.reader
                        .get_mut()
                        .set_encoding(*encoding, &declaration, position + 2)?;
                }
                utf8(&declaration, position + 2)?;
                let (_, rest) = declared.map_err(|error| error.at(position))?;
                check_declaration_end(rest).map_err(|error| error.at(position))?;
                return Ok(Event::Skip);
            }
            Token::DocType(doctype) => {
                // The name follows `<!DOCTYPE` and any whitespace; `>` ends it.
                let after = self.reader.buffer_position() + self.reader.get_ref().bom();
                let read = usize::try_from(after - position).unwrap_or(usize::MAX);
                let offset = read.saturating_sub(doctype.len() + 1);
                utf8(&doctype, position + offset as u64)?;
                return Err(Error::Doctype);
            }
            Token::PI(instruction) => {
                utf8(&instruction, position + 2)?;
                self.run = 0;
                check_instruction(&instruction).map_err(|error| error.at(position))?;
                return Ok(Event::Skip);
            }
            Token::Comment(comment) => {
                utf8(&comment, position + 4)?;
                self.run = 0;
                if comment.len() > MAX_TOKEN_BYTES {
                    return Err(token_limit("comment"));
                }
                return Ok(Event::Skip);
            }
            Token::Text(text) => {
                // Read up to markup, a reference or the end: the whole buffer.
                let content = utf8(&text, position)?;
                if self.root != Root::Inside {
                    if !is_spaces(content) {
                        return Err(malformed(
                            position,
                            "content is not allowed outside the root element",
                        ));
                    }
                    return Ok(Event::Skip);
                }
                if let Some(index) = content.find("]]>") {
                    return Err(malformed(
                        position + index as u64,
                        "sequence ']]>' is not allowed in content",
                    ));
                }
                self.run = self.run.saturating_add(content.len());
                if self.run > MAX_TOKEN_BYTES {
                    return Err(token_limit("text node"));
                }
                if content.contains('\r') {
                    self.text = normalize_line_ends(content);
                    return Ok(Event::Text(&self.text));
                }
                0..content.len()
            }
            Token::CData(data) => {
                // The data follows `<![CDATA[`.
                let content = utf8(&data, position + 9)?;
                if self.root != Root::Inside {
                    return Err(malformed(
                        position,
                        "CDATA is not allowed outside the root element",
                    ));
                }
                self.run = self.run.saturating_add(content.len());
                if self.run > MAX_TOKEN_BYTES {
                    return Err(token_limit("text node"));
                }
                if content.contains('\r') {
                    self.text = normalize_line_ends(content);
                    return Ok(Event::Text(&self.text));
                }
                9..9 + content.len()
            }
            Token::GeneralRef(reference) => {
                let reference = utf8(&reference, position + 1)?;
                if self.root != Root::Inside {
                    return Err(malformed(
                        position,
                        "references are not allowed outside the root element",
                    ));
                }
                let character =
                    resolve_reference(reference).map_err(|reason| malformed(position, reason))?;
                self.run = self.run.saturating_add(reference.len() + 2);
                if self.run > MAX_TOKEN_BYTES {
                    return Err(token_limit("text node"));
                }
                self.text.clear();
                self.text.push(character);
                return Ok(Event::Text(&self.text));
            }
            Token::Start(start) => {
                let content = utf8(&start, position + 1)?;
                if self.root == Root::After {
                    return Err(malformed(position, "extra content after the root element"));
                }
                self.scopes.open();
                parse_start(content, &mut self.tag, &mut self.raw, &mut self.scopes)
                    .map_err(|error| error.at(position))?;
                if self.open >= self.depth_limit {
                    return Err(Error::Limit(Limit::Depth(self.depth_limit)));
                }
                self.open += 1;
                self.root = Root::Inside;
                self.run = 0;
                return Ok(Event::Start(&self.tag));
            }
            Token::End(end) => {
                utf8(&end, position + 2)?;
                self.scopes.close();
                self.open = self.open.saturating_sub(1);
                if self.open == 0 {
                    self.root = Root::After;
                }
                self.run = 0;
                return Ok(Event::End);
            }
            Token::Empty(_) => {
                return Err(malformed(position, "unexpected unexpanded empty element"))
            }
            Token::Eof => {
                return if self.open > 0 {
                    Err(malformed(
                        position,
                        "premature end of data: an element is not closed",
                    ))
                } else if self.root == Root::Before {
                    Err(malformed(position, "document has no root element"))
                } else {
                    Ok(Event::Eof)
                };
            }
        };
        // Decoded above, so this cannot fail.
        Ok(Event::Text(utf8(&self.buf[held], position)?))
    }
}

/// The text of event bytes that begin `at` in the source. quick-xml hands
/// over each markup, text run and reference as read, and the reader decodes
/// it as UTF-8 before it reads anything inside it.
fn utf8(bytes: &[u8], at: u64) -> Result<&str, Error> {
    std::str::from_utf8(bytes).map_err(|error| {
        malformed(
            at + error.valid_up_to() as u64,
            "invalid UTF-8 byte sequence",
        )
    })
}

fn malformed(position: u64, reason: impl Into<String>) -> Error {
    Error::Malformed {
        position,
        reason: reason.into(),
    }
}

fn token_limit(token: &'static str) -> Error {
    Error::Limit(Limit::Token {
        token,
        limit: MAX_TOKEN_BYTES,
    })
}

/// A start-tag refusal before its source position is known.
enum TagError {
    Malformed(String),
    Limit(Error),
}

impl TagError {
    fn at(self, position: u64) -> Error {
        match self {
            Self::Malformed(reason) => malformed(position, reason),
            Self::Limit(error) => error,
        }
    }
}

impl From<String> for TagError {
    fn from(reason: String) -> Self {
        Self::Malformed(reason)
    }
}

impl From<&str> for TagError {
    fn from(reason: &str) -> Self {
        Self::Malformed(reason.into())
    }
}

/// Map a tokenizer failure, recovering refusals raised by the byte sources.
fn token_error(error: quick_xml::Error, position: u64) -> Error {
    match error {
        quick_xml::Error::Io(error) => Fault::from_io(&error),
        quick_xml::Error::Encoding(_) => malformed(position, "invalid UTF-8 byte sequence"),
        error => malformed(position, error.to_string()),
    }
}

fn is_space(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\n' | b'\r')
}

fn is_spaces(text: &str) -> bool {
    text.bytes().all(is_space)
}

/// Report each line break as one LF, as XML processors must.
fn normalize_line_ends(text: &str) -> String {
    let mut normalized = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(index) = rest.find('\r') {
        normalized.push_str(&rest[..index]);
        normalized.push('\n');
        rest = &rest[index + 1..];
        if let Some(stripped) = rest.strip_prefix('\n') {
            rest = stripped;
        }
    }
    normalized.push_str(rest);
    normalized
}

fn is_xml_char(character: char) -> bool {
    matches!(character,
        '\t' | '\n' | '\r'
        | '\u{20}'..='\u{D7FF}'
        | '\u{E000}'..='\u{FFFD}'
        | '\u{10000}'..='\u{10FFFF}')
}

fn is_name_start(character: char) -> bool {
    matches!(character,
        ':' | 'A'..='Z' | '_' | 'a'..='z'
        | '\u{C0}'..='\u{D6}'
        | '\u{D8}'..='\u{F6}'
        | '\u{F8}'..='\u{2FF}'
        | '\u{370}'..='\u{37D}'
        | '\u{37F}'..='\u{1FFF}'
        | '\u{200C}'..='\u{200D}'
        | '\u{2070}'..='\u{218F}'
        | '\u{2C00}'..='\u{2FEF}'
        | '\u{3001}'..='\u{D7FF}'
        | '\u{F900}'..='\u{FDCF}'
        | '\u{FDF0}'..='\u{FFFD}'
        | '\u{10000}'..='\u{EFFFF}')
}

fn is_name_char(character: char) -> bool {
    is_name_start(character)
        || matches!(character,
            '-' | '.' | '0'..='9' | '\u{B7}' | '\u{300}'..='\u{36F}' | '\u{203F}'..='\u{2040}')
}

/// An XML name without colons.
fn is_ncname(name: &str) -> bool {
    let mut characters = name.chars();
    characters
        .next()
        .is_some_and(|first| first != ':' && is_name_start(first))
        && characters.all(|character| character != ':' && is_name_char(character))
}

fn is_name(name: &str) -> bool {
    let mut characters = name.chars();
    characters.next().is_some_and(is_name_start) && characters.all(is_name_char)
}

/// Validate a qualified name and return the position of its colon.
fn check_qname(name: &str, what: &str) -> Result<Option<usize>, TagError> {
    if name.is_empty() {
        return Err(format!("{what} name expected").into());
    }
    let colon = name.find(':');
    let parts = match colon {
        Some(index) => [&name[..index], &name[index + 1..]],
        None => [name, ""],
    };
    for part in parts
        .iter()
        .filter(|part| !part.is_empty() || colon.is_some())
    {
        if part.len() > MAX_NAME_BYTES {
            return Err(TagError::Limit(Error::Limit(Limit::Token {
                token: "name",
                limit: MAX_NAME_BYTES,
            })));
        }
        if !is_ncname(part) {
            return Err(format!("invalid {what} name {name:?}").into());
        }
    }
    Ok(colon)
}

/// Resolve a predefined entity or character reference body.
fn resolve_reference(name: &str) -> Result<char, String> {
    let (digits, radix) = match name {
        "lt" => return Ok('<'),
        "gt" => return Ok('>'),
        "amp" => return Ok('&'),
        "apos" => return Ok('\''),
        "quot" => return Ok('"'),
        _ => {
            if let Some(digits) = name.strip_prefix("#x") {
                (digits, 16)
            } else if let Some(digits) = name.strip_prefix('#') {
                (digits, 10)
            } else if is_name(name) {
                return Err(format!(
                    "entity &{name}; is not defined; only the five predefined entities are supported"
                ));
            } else {
                return Err(format!("malformed reference &{name};"));
            }
        }
    };
    if digits.is_empty() || !digits.chars().all(|character| character.is_digit(radix)) {
        return Err(format!("malformed character reference &{name};"));
    }
    digits
        .chars()
        .try_fold(0_u32, |code, character| {
            code.checked_mul(radix)?
                .checked_add(character.to_digit(radix)?)
        })
        .and_then(char::from_u32)
        .filter(|character| is_xml_char(*character))
        .ok_or_else(|| format!("character reference &{name}; is not a valid XML character"))
}

/// Append a normalized attribute value: references resolve, each literal
/// whitespace character (a CR LF pair counting once) becomes one space.
fn normalize_attribute(raw: &str, output: &mut String) -> Result<(), TagError> {
    let mut rest = raw;
    while let Some(index) = rest.find(['<', '&', '\t', '\n', '\r']) {
        output.push_str(&rest[..index]);
        let marker = rest.as_bytes()[index];
        rest = &rest[index + 1..];
        match marker {
            b'<' => return Err("unescaped '<' is not allowed in attribute values".into()),
            b'&' => {
                let end = rest
                    .find(';')
                    .ok_or("a reference in an attribute value is not closed")?;
                output.push(resolve_reference(&rest[..end])?);
                rest = &rest[end + 1..];
            }
            b'\r' => {
                output.push(' ');
                if let Some(stripped) = rest.strip_prefix('\n') {
                    rest = stripped;
                }
            }
            _ => output.push(' '),
        }
    }
    output.push_str(rest);
    Ok(())
}

/// The first item equal to an earlier one, without quadratic work on large tags.
fn first_repeat<T: Eq + std::hash::Hash>(items: Vec<T>) -> Option<T> {
    let position = if items.len() <= 16 {
        (1..items.len()).find(|&index| items[..index].contains(&items[index]))
    } else {
        let mut seen = HashSet::with_capacity(items.len());
        items.iter().position(|item| !seen.insert(item))
    }?;
    items.into_iter().nth(position)
}

/// Parse one start tag into `tag`, binding its namespace declarations.
fn parse_start(
    content: &str,
    tag: &mut Tag,
    raw: &mut Vec<RawAttribute>,
    scopes: &mut Scopes,
) -> Result<(), TagError> {
    tag.clear();
    raw.clear();
    let bytes = content.as_bytes();
    let name_end = bytes
        .iter()
        .position(|&byte| is_space(byte))
        .unwrap_or(bytes.len());
    let element = &content[..name_end];
    let element_colon = check_qname(element, "element")?;
    tag.arena.push_str(element);
    let element_name = 0..element.len();
    let mut position = name_end;
    loop {
        let spaces = bytes[position..]
            .iter()
            .take_while(|&&byte| is_space(byte))
            .count();
        position += spaces;
        if position == bytes.len() {
            break;
        }
        if spaces == 0 {
            return Err(
                "attributes construct error: whitespace is required between attributes".into(),
            );
        }
        let name_start = position;
        while position < bytes.len() && !is_space(bytes[position]) && bytes[position] != b'=' {
            position += 1;
        }
        let name = &content[name_start..position];
        let colon = check_qname(name, "attribute")?;
        position += bytes[position..]
            .iter()
            .take_while(|&&byte| is_space(byte))
            .count();
        if bytes.get(position) != Some(&b'=') {
            return Err(format!("attribute {name} has no value").into());
        }
        position += 1;
        position += bytes[position..]
            .iter()
            .take_while(|&&byte| is_space(byte))
            .count();
        let quote = match bytes.get(position) {
            Some(&quote @ (b'"' | b'\'')) => quote,
            _ => return Err(format!("attribute {name} value must be quoted").into()),
        };
        position += 1;
        let length = bytes[position..]
            .iter()
            .position(|&byte| byte == quote)
            .ok_or_else(|| format!("attribute {name} value is not closed"))?;
        if length > MAX_TOKEN_BYTES {
            return Err(TagError::Limit(token_limit("attribute value")));
        }
        let start = tag.arena.len();
        tag.arena.push_str(name);
        let name_range = start..tag.arena.len();
        let value_start = tag.arena.len();
        normalize_attribute(&content[position..position + length], &mut tag.arena)?;
        raw.push(RawAttribute {
            name: name_range,
            colon,
            value: value_start..tag.arena.len(),
        });
        position += length + 1;
    }

    let arena = &tag.arena;
    if let Some(repeat) = first_repeat(
        raw.iter()
            .map(|attribute| &arena[attribute.name.clone()])
            .collect(),
    ) {
        return Err(format!("attribute {repeat} is repeated").into());
    }
    for attribute in raw.iter() {
        let name = &arena[attribute.name.clone()];
        let value = &arena[attribute.value.clone()];
        let prefix = match (name, attribute.colon) {
            ("xmlns", None) => None,
            (_, Some(colon)) if &name[..colon] == "xmlns" => Some(&name[colon + 1..]),
            _ => continue,
        };
        match prefix {
            Some("xml") if value != XML_NAMESPACE => {
                return Err("the xml prefix cannot be bound to another namespace".into())
            }
            Some("xml") => {}
            Some("xmlns") => return Err("the xmlns prefix cannot be declared".into()),
            Some(prefix) => {
                if value.is_empty() {
                    return Err(format!("namespace prefix {prefix} cannot be undeclared").into());
                }
                if value == XML_NAMESPACE || value == XMLNS_NAMESPACE {
                    return Err(
                        format!("namespace prefix {prefix} cannot be bound to {value}").into(),
                    );
                }
                if !is_uri_reference(value) {
                    return Err(format!("namespace {value:?} is not a valid URI").into());
                }
                scopes.bind(Some(prefix), value)?;
            }
            None => {
                if value == XML_NAMESPACE || value == XMLNS_NAMESPACE {
                    return Err(format!("the default namespace cannot be {value}").into());
                }
                if !value.is_empty() && !is_uri_reference(value) {
                    return Err(format!("namespace {value:?} is not a valid URI").into());
                }
                scopes.bind(None, value)?;
            }
        }
    }

    tag.namespace = scopes.element(element_colon.map(|colon| &element[..colon]))?;
    tag.local = match element_colon {
        Some(colon) => element_name.start + colon + 1..element_name.end,
        None => element_name,
    };
    for attribute in raw.iter() {
        let name = &arena[attribute.name.clone()];
        let namespace = match attribute.colon {
            None if name == "xmlns" => continue,
            None => None,
            Some(colon) if &name[..colon] == "xmlns" => continue,
            Some(colon) => Some(scopes.attribute(&name[..colon])?),
        };
        let local = match attribute.colon {
            Some(colon) => attribute.name.start + colon + 1..attribute.name.end,
            None => attribute.name.clone(),
        };
        tag.attributes.push(Attribute {
            namespace,
            local,
            value: attribute.value.clone(),
        });
    }
    let expanded = tag
        .attributes
        .iter()
        .map(|attribute| (attribute.namespace.clone(), &arena[attribute.local.clone()]))
        .collect::<Vec<_>>();
    if let Some((namespace, local)) = first_repeat(expanded) {
        return Err(format!(
            "attribute {local} in namespace {} is repeated",
            namespace.as_deref().unwrap_or("(none)")
        )
        .into());
    }
    Ok(())
}

/// Declaration failures: syntax, or a supported syntax naming another encoding.
enum Declaration {
    Malformed(&'static str),
    Encoding(String),
}

impl Declaration {
    fn at(self, position: u64) -> Error {
        match self {
            Self::Malformed(reason) => malformed(position, reason),
            Self::Encoding(encoding) => Error::Encoding { encoding },
        }
    }
}

/// Both encodings use the UTF-8 tokenizer; ASCII adds a source-byte bound.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Encoding {
    Utf8,
    Ascii,
}

fn trim_spaces(text: &str) -> &str {
    text.trim_start_matches([' ', '\t', '\n', '\r'])
}

/// Take one `name="value"` pseudo-attribute of the XML declaration.
fn pseudo_attribute<'a>(
    rest: &mut &'a str,
    name: &str,
    required: bool,
) -> Result<Option<&'a str>, Declaration> {
    let trimmed = trim_spaces(rest);
    let spaced = trimmed.len() != rest.len();
    let Some(after) = trimmed.strip_prefix(name) else {
        return if required {
            Err(Declaration::Malformed(
                "XML declaration must start with version",
            ))
        } else {
            Ok(None)
        };
    };
    if !spaced {
        return Err(Declaration::Malformed(
            "XML declaration needs whitespace between pseudo-attributes",
        ));
    }
    let Some(after) = trim_spaces(after).strip_prefix('=') else {
        return Err(Declaration::Malformed(
            "XML declaration pseudo-attribute needs '='",
        ));
    };
    let after = trim_spaces(after);
    let quote = match after.as_bytes().first() {
        Some(&quote @ (b'"' | b'\'')) => quote as char,
        _ => {
            return Err(Declaration::Malformed(
                "XML declaration value must be quoted",
            ))
        }
    };
    let after = &after[1..];
    let end = after.find(quote).ok_or(Declaration::Malformed(
        "XML declaration value is not closed",
    ))?;
    *rest = &after[end + 1..];
    Ok(Some(&after[..end]))
}

/// Read version 1.x and the optional encoding after `xml`. The caller checks
/// source bytes in that encoding before validating the remaining suffix.
fn declaration_encoding(content: &str) -> Result<(Encoding, &str), Declaration> {
    let mut rest = content;
    let mut admitted = Encoding::Utf8;
    let version = pseudo_attribute(&mut rest, "version", true)?.unwrap_or_default();
    let digits = version.strip_prefix("1.").unwrap_or_default();
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(Declaration::Malformed("XML version must be 1.x"));
    }
    if let Some(encoding) = pseudo_attribute(&mut rest, "encoding", false)? {
        let mut characters = encoding.chars();
        let valid = characters
            .next()
            .is_some_and(|first| first.is_ascii_alphabetic())
            && characters.all(|character| {
                character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-')
            });
        if !valid {
            return Err(Declaration::Malformed(
                "XML declaration encoding name is invalid",
            ));
        }
        if encoding.eq_ignore_ascii_case("US-ASCII") {
            admitted = Encoding::Ascii;
        } else if !encoding.eq_ignore_ascii_case("UTF-8") && !encoding.eq_ignore_ascii_case("UTF8")
        {
            return Err(Declaration::Encoding(encoding.into()));
        }
    }
    Ok((admitted, rest))
}

/// The declaration ends with an optional standalone yes or no and whitespace.
fn check_declaration_end(mut rest: &str) -> Result<(), Declaration> {
    if let Some(standalone) = pseudo_attribute(&mut rest, "standalone", false)? {
        if standalone != "yes" && standalone != "no" {
            return Err(Declaration::Malformed(
                "XML declaration standalone must be yes or no",
            ));
        }
    }
    if !rest.bytes().all(is_space) {
        return Err(Declaration::Malformed(
            "XML declaration contains unexpected content",
        ));
    }
    Ok(())
}

/// Processing instructions need a colon-free target other than `xml`. The
/// caller has decoded the instruction as UTF-8.
fn check_instruction(instruction: &BytesPI<'_>) -> Result<(), TagError> {
    if instruction.len() > MAX_TOKEN_BYTES {
        return Err(TagError::Limit(token_limit("processing instruction")));
    }
    let target = std::str::from_utf8(instruction.target())
        .map_err(|_| TagError::from("invalid UTF-8 byte sequence"))?;
    if target.eq_ignore_ascii_case("xml") {
        return Err("processing instruction target xml is reserved".into());
    }
    if target.len() > MAX_NAME_BYTES {
        return Err(TagError::Limit(Error::Limit(Limit::Token {
            token: "name",
            limit: MAX_NAME_BYTES,
        })));
    }
    if !is_ncname(target) {
        return Err(format!("invalid processing instruction target {target:?}").into());
    }
    Ok(())
}

fn is_unreserved(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~')
}

fn is_sub_delim(byte: u8) -> bool {
    matches!(
        byte,
        b'!' | b'$' | b'&' | b'\'' | b'(' | b')' | b'*' | b'+' | b',' | b';' | b'='
    )
}

/// Whether every byte is unreserved, a sub-delimiter, one of `extra` or part
/// of a percent-encoded octet.
fn uri_chars(bytes: &[u8], extra: &[u8]) -> bool {
    let mut index = 0;
    while index < bytes.len() {
        let byte = bytes[index];
        if byte == b'%' {
            if !(bytes.get(index + 1).is_some_and(u8::is_ascii_hexdigit)
                && bytes.get(index + 2).is_some_and(u8::is_ascii_hexdigit))
            {
                return false;
            }
            index += 3;
            continue;
        }
        if !(is_unreserved(byte) || is_sub_delim(byte) || extra.contains(&byte)) {
            return false;
        }
        index += 1;
    }
    true
}

/// An RFC 3986 URI reference, as libxml2 requires of namespace names. An IP
/// literal host is checked only for its bracketed character set.
fn is_uri_reference(uri: &str) -> bool {
    let bytes = uri.as_bytes();
    let (bytes, fragment) = match bytes.iter().position(|&byte| byte == b'#') {
        Some(index) => (&bytes[..index], Some(&bytes[index + 1..])),
        None => (bytes, None),
    };
    if fragment.is_some_and(|fragment| !uri_chars(fragment, b":@/?")) {
        return false;
    }
    let (bytes, query) = match bytes.iter().position(|&byte| byte == b'?') {
        Some(index) => (&bytes[..index], Some(&bytes[index + 1..])),
        None => (bytes, None),
    };
    if query.is_some_and(|query| !uri_chars(query, b":@/?")) {
        return false;
    }
    let colon = bytes.iter().position(|&byte| byte == b':');
    let slash = bytes.iter().position(|&byte| byte == b'/');
    let hierarchy = match colon {
        Some(colon) if slash.is_none_or(|slash| colon < slash) => {
            let scheme = &bytes[..colon];
            let valid_scheme = scheme.first().is_some_and(u8::is_ascii_alphabetic)
                && scheme.iter().all(|&byte| {
                    byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'-' | b'.')
                });
            if !valid_scheme {
                return false;
            }
            &bytes[colon + 1..]
        }
        _ => bytes,
    };
    let path = match hierarchy.strip_prefix(b"//") {
        Some(rest) => {
            let end = rest
                .iter()
                .position(|&byte| byte == b'/')
                .unwrap_or(rest.len());
            if !is_authority(&rest[..end]) {
                return false;
            }
            &rest[end..]
        }
        None => hierarchy,
    };
    uri_chars(path, b":@/")
}

fn is_authority(authority: &[u8]) -> bool {
    let (userinfo, host_port) = match authority.iter().rposition(|&byte| byte == b'@') {
        Some(index) => (Some(&authority[..index]), &authority[index + 1..]),
        None => (None, authority),
    };
    if userinfo.is_some_and(|userinfo| !uri_chars(userinfo, b":")) {
        return false;
    }
    let port = if host_port.first() == Some(&b'[') {
        let Some(close) = host_port.iter().position(|&byte| byte == b']') else {
            return false;
        };
        let literal = &host_port[1..close];
        if literal.is_empty() || !uri_chars(literal, b":") || literal.contains(&b'%') {
            return false;
        }
        match &host_port[close + 1..] {
            [] => None,
            [b':', port @ ..] => Some(port),
            _ => return false,
        }
    } else {
        let (host, port) = match host_port.iter().position(|&byte| byte == b':') {
            Some(index) => (&host_port[..index], Some(&host_port[index + 1..])),
            None => (host_port, None),
        };
        if !uri_chars(host, b"") {
            return false;
        }
        port
    };
    // libxml2 needs at least one digit after the colon and an `int` port.
    port.is_none_or(|port| {
        !port.is_empty()
            && port.iter().all(u8::is_ascii_digit)
            && std::str::from_utf8(port)
                .ok()
                .and_then(|port| port.parse::<i32>().ok())
                .is_some()
    })
}

/// A byte stream that refuses characters outside XML 1.0, a UTF-16 or
/// UTF-32 encoding and bytes beyond an optional limit. A refusal surfaces
/// only when the parser reaches its position, keeping refusals in document
/// order.
pub(super) struct CheckedXml<R> {
    inner: R,
    buffer: Box<[u8]>,
    start: usize,
    end: usize,
    held: usize,
    offset: u64,
    total: u64,
    limit: Option<u64>,
    pending: Option<Fault>,
    first: bool,
    bom: bool,
    encoding: Encoding,
}

impl<R: Read> CheckedXml<R> {
    pub(super) fn new(inner: R, limit: Option<u64>) -> Self {
        Self {
            inner,
            buffer: vec![0; CHUNK].into_boxed_slice(),
            start: 0,
            end: 0,
            held: 0,
            offset: 0,
            total: 0,
            limit,
            pending: None,
            first: true,
            bom: false,
            encoding: Encoding::Utf8,
        }
    }

    /// Check the consumed declaration, then recheck prefetched unread bytes,
    /// deferring their faults until the parser reaches them.
    fn set_encoding(
        &mut self,
        encoding: Encoding,
        declaration: &[u8],
        position: u64,
    ) -> Result<(), Error> {
        self.encoding = encoding;
        if encoding == Encoding::Ascii && self.bom {
            return Err(Fault::Ascii {
                position: 0,
                value: UTF8_BOM[0],
            }
            .into_error());
        }
        if let Some((_, fault)) = invalid_content(declaration, position, encoding) {
            return Err(fault.into_error());
        }
        // The first scan may already have held an XML character fault back.
        // Include its first byte so ASCII admission can judge that byte too,
        // with the same precedence as a later refill in the declared mode.
        let end = self.end + usize::from(matches!(&self.pending, Some(Fault::Character { .. })));
        if let Some((index, fault)) =
            invalid_content(&self.buffer[self.start..end], self.offset, self.encoding)
        {
            self.end = self.start + index;
            self.held = 0;
            self.pending = Some(fault);
        }
        Ok(())
    }

    /// Bytes of the UTF-8 byte order mark the tokenizer skips unreported.
    fn bom(&self) -> u64 {
        if self.bom {
            UTF8_BOM.len() as u64
        } else {
            0
        }
    }

    fn refill(&mut self) -> io::Result<()> {
        let held = self.held;
        self.buffer.copy_within(self.end..self.end + held, 0);
        self.start = 0;
        self.end = 0;
        self.held = 0;
        let mut filled = held;
        loop {
            let read = loop {
                match self.inner.read(&mut self.buffer[filled..]) {
                    Ok(read) => break read,
                    Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                    Err(error) => return Err(error),
                }
            };
            self.total = self.total.saturating_add(read as u64);
            if let Some(limit) = self.limit.filter(|&limit| self.total > limit) {
                let fault = Fault::Limit(Limit::XmlBytes(limit));
                self.pending = Some(fault.clone());
                return Err(fault.into_io());
            }
            filled += read;
            let eof = read == 0;
            if self.first {
                if filled < 4 && !eof {
                    continue;
                }
                self.first = false;
                self.bom = self.buffer[..filled].starts_with(&UTF8_BOM);
                if let Some(encoding) = foreign_encoding(&self.buffer[..filled]) {
                    let fault = Fault::Encoding(encoding);
                    self.pending = Some(fault.clone());
                    return Err(fault.into_io());
                }
            }
            let hold = if eof {
                0
            } else {
                incomplete_tail(&self.buffer[..filled])
            };
            let ready = filled - hold;
            if let Some((index, fault)) =
                invalid_content(&self.buffer[..ready], self.offset, self.encoding)
            {
                self.end = index;
                self.pending = Some(fault.clone());
                return if index == 0 {
                    Err(fault.into_io())
                } else {
                    Ok(())
                };
            }
            if ready > 0 || eof {
                self.end = ready;
                self.held = hold;
                return Ok(());
            }
        }
    }
}

impl<R: Read> Read for CheckedXml<R> {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        let available = self.fill_buf()?;
        let count = available.len().min(output.len());
        output[..count].copy_from_slice(&available[..count]);
        self.consume(count);
        Ok(count)
    }
}

impl<R: Read> BufRead for CheckedXml<R> {
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        if self.start == self.end {
            if let Some(fault) = &self.pending {
                return Err(fault.clone().into_io());
            }
            self.refill()?;
        }
        Ok(&self.buffer[self.start..self.end])
    }

    fn consume(&mut self, amount: usize) {
        let amount = amount.min(self.end - self.start);
        self.start += amount;
        self.offset += amount as u64;
    }
}

/// Bytes at the end that may begin U+FFFE or U+FFFF (EF BF BE / EF BF BF).
fn incomplete_tail(bytes: &[u8]) -> usize {
    if bytes.ends_with(&[0xEF, 0xBF]) {
        2
    } else if bytes.ends_with(&[0xEF]) {
        1
    } else {
        0
    }
}

/// The first source fault: an ASCII encoding violation or a character outside
/// XML 1.0 `Char`, in document order.
/// Input is UTF-8, so C0 controls are single bytes and U+FFFE/U+FFFF have
/// fixed three-byte forms.
fn invalid_content(bytes: &[u8], offset: u64, encoding: Encoding) -> Option<(usize, Fault)> {
    let mut index = 0;
    while index < bytes.len() {
        let byte = bytes[index];
        let position = offset + index as u64;
        if encoding == Encoding::Ascii && !byte.is_ascii() {
            return Some((
                index,
                Fault::Ascii {
                    position,
                    value: byte,
                },
            ));
        }
        if byte < 0x20 && !matches!(byte, b'\t' | b'\n' | b'\r') {
            return Some((
                index,
                Fault::Character {
                    position,
                    value: byte.into(),
                },
            ));
        }
        if byte == 0xEF
            && bytes.get(index + 1) == Some(&0xBF)
            && matches!(bytes.get(index + 2), Some(0xBE | 0xBF))
        {
            return Some((
                index,
                Fault::Character {
                    position,
                    value: 0xFFFE + u32::from(bytes[index + 2] - 0xBE),
                },
            ));
        }
        index += 1;
    }
    None
}

/// Name a UTF-16 or UTF-32 byte order mark or `<` encoding.
fn foreign_encoding(head: &[u8]) -> Option<&'static str> {
    const UTF32: [&[u8]; 4] = [
        &[0x00, 0x00, 0xFE, 0xFF],
        &[0xFF, 0xFE, 0x00, 0x00],
        &[0x00, 0x00, 0x00, 0x3C],
        &[0x3C, 0x00, 0x00, 0x00],
    ];
    const UTF16: [&[u8]; 4] = [&[0xFE, 0xFF], &[0xFF, 0xFE], &[0x00, 0x3C], &[0x3C, 0x00]];
    if UTF32.iter().any(|prefix| head.starts_with(prefix)) {
        Some("UTF-32")
    } else if UTF16.iter().any(|prefix| head.starts_with(prefix)) {
        Some("UTF-16")
    } else {
        None
    }
}
