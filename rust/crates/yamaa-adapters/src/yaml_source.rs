//! Fixed-boundary YAML event scanning before document admission.
//!
//! The parser cannot represent escaped surrogates in a Rust string. For error
//! reporting only, scan a repaired copy, then compare it with a second copy
//! whose repairs use different code points. Equal positions remain original
//! text; differing positions recover the exact invalid code point. This does
//! not reserve a sentinel character, normalize text, or return repaired values.
//! Full syntax scanning precedes Unicode diagnostics, as in the reference
//! reader. Mapping admission must still check duplicates and forbidden YAML
//! constructs before reporting invalid text. These events are not documents.

use std::borrow::Cow;
use yamaa_core::limits::{LimitExceeded, Limits};

use yamaa_saphyr_parser::{Event, Marker, Parser, ScalarStyle, ScanError, Span, Tag};

/// An invalid string retains code points solely for key equality and diagnostics.
/// It cannot be mistaken for successfully decoded Unicode text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SourceText {
    Unicode(String),
    Invalid(Vec<u32>),
}

/// Source events retain all information needed by subsequent YAML admission.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SourceEvent {
    StreamStart,
    StreamEnd,
    DocumentStart(bool),
    DocumentEnd,
    Alias(usize),
    Scalar(SourceText, ScalarStyle, usize, Option<Tag>),
    SequenceStart(usize, Option<Tag>),
    SequenceEnd,
    MappingStart(usize, Option<Tag>),
    MappingEnd,
}

pub use crate::yaml_decode::{Encoding, SourcePosition};

/// Where the scanner's positions fall in the source's bytes. The scanner
/// counts characters; an ASCII source holds one byte per character, and a
/// UTF-8 source keeps the byte at which each character starts, with its
/// length last.
enum Offsets {
    Ascii,
    Utf8(Vec<usize>),
}

impl Offsets {
    fn byte(&self, character: usize) -> usize {
        match self {
            Self::Ascii => character,
            Self::Utf8(starts) => starts[character.min(starts.len() - 1)],
        }
    }

    fn position(&self, mark: Marker) -> SourcePosition {
        SourcePosition {
            offset: self.byte(mark.index()),
            line: mark.line(),
            // The pinned scanner stores zero-based columns despite its docs.
            column: mark.col() + 1,
        }
    }
}

/// Event locations always refer to the retained source, never a rewritten file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocatedEvent {
    pub event: SourceEvent,
    pub start: SourcePosition,
    pub end: SourcePosition,
}

/// Resource refusal is distinct from language errors and invalid Unicode.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ScanFailure {
    NonAscii(SourcePosition, Encoding),
    InvalidUtf8(SourcePosition),
    Syntax {
        position: SourcePosition,
        reason: String,
    },
    Limit(LimitExceeded),
    /// Fail closed if a parser upgrade violates diagnostic reconstruction.
    Reconstruction,
}

fn own(event: Event<'_>) -> Event<'static> {
    match event {
        Event::Scalar(text, style, anchor, tag) => Event::Scalar(
            Cow::Owned(text.into_owned()),
            style,
            anchor,
            tag.map(|tag| Cow::Owned(tag.into_owned())),
        ),
        Event::SequenceStart(anchor, tag) => {
            Event::SequenceStart(anchor, tag.map(|tag| Cow::Owned(tag.into_owned())))
        }
        Event::MappingStart(anchor, tag) => {
            Event::MappingStart(anchor, tag.map(|tag| Cow::Owned(tag.into_owned())))
        }
        Event::Nothing => Event::Nothing,
        Event::StreamStart => Event::StreamStart,
        Event::StreamEnd => Event::StreamEnd,
        Event::DocumentStart(explicit) => Event::DocumentStart(explicit),
        Event::DocumentEnd => Event::DocumentEnd,
        Event::Alias(anchor) => Event::Alias(anchor),
        Event::SequenceEnd => Event::SequenceEnd,
        Event::MappingEnd => Event::MappingEnd,
    }
}

enum PassFailure {
    Syntax(ScanError),
    Resource(ScanFailure),
}

fn parse_pass(
    source: &str,
    strict: bool,
    offsets: &Offsets,
) -> Result<Vec<(Event<'static>, Span)>, PassFailure> {
    let mut events = Vec::new();
    let mut depth = 0_usize;
    let mut documents = 0;
    let mut boundary = 0;
    for item in Parser::new_from_str(source) {
        let (event, span) = item.map_err(PassFailure::Syntax)?;
        if strict {
            let reason = match &event {
                Event::Alias(_) => Some("YAML aliases are not allowed"),
                Event::Scalar(_, _, anchor, _)
                | Event::SequenceStart(anchor, _)
                | Event::MappingStart(anchor, _)
                    if *anchor != 0 =>
                {
                    Some("YAML anchors are not allowed")
                }
                Event::Scalar(_, _, _, Some(_))
                | Event::SequenceStart(_, Some(_))
                | Event::MappingStart(_, Some(_)) => Some("explicit YAML tags are not allowed"),
                Event::DocumentStart(_) if documents != 0 => {
                    Some("expected a single document in the stream")
                }
                _ => None,
            };
            if let Some(reason) = reason {
                let position = if matches!(
                    &event,
                    Event::Scalar(..) | Event::SequenceStart(..) | Event::MappingStart(..)
                ) {
                    metadata_start(source, offsets, boundary, span.start)
                } else {
                    offsets.position(span.start)
                };
                return Err(PassFailure::Resource(ScanFailure::Syntax {
                    position,
                    reason: reason.into(),
                }));
            }
        }
        match &event {
            Event::DocumentStart(explicit) => {
                documents += 1;
                if *explicit {
                    boundary = span.end.index();
                }
            }
            Event::Scalar(..)
            | Event::SequenceStart(..)
            | Event::MappingStart(..)
            | Event::SequenceEnd
            | Event::MappingEnd => boundary = span.end.index(),
            _ => {}
        }
        match &event {
            Event::SequenceStart(..) | Event::MappingStart(..) => {
                depth += 1;
                Limits::yaml_depth(depth)
                    .map_err(|error| PassFailure::Resource(ScanFailure::Limit(error)))?;
            }
            Event::SequenceEnd | Event::MappingEnd => {
                depth = depth
                    .checked_sub(1)
                    .ok_or(PassFailure::Resource(ScanFailure::Reconstruction))?;
            }
            _ => {}
        }
        events.push((own(event), span));
    }
    Ok(events)
}

/// A node event starts after its tag/anchor prefix in the pinned parser. Between
/// the preceding node boundary and this event, only punctuation, whitespace,
/// comments and metadata can occur. Skip comments and retain the first prefix
/// marker (also when a tag precedes an anchor). Never search inside scalar text.
fn metadata_start(
    source: &str,
    offsets: &Offsets,
    boundary: usize,
    fallback: Marker,
) -> SourcePosition {
    let bytes = source.as_bytes();
    let mut comment = false;
    for (offset, &byte) in bytes
        .iter()
        .enumerate()
        .take(offsets.byte(fallback.index()).saturating_add(1))
        .skip(offsets.byte(boundary))
    {
        if byte == b'\n' || byte == b'\r' {
            comment = false;
        }
        if comment {
            continue;
        }
        if byte == b'#' {
            comment = true;
        }
        if byte == b'&' || byte == b'!' {
            return byte_position(bytes, offset);
        }
    }
    offsets.position(fallback)
}

#[derive(Clone, Copy)]
struct Repair {
    start: usize,
    digits: usize,
    original: u32,
}

fn replace_escape(source: &mut [u8], repair: Repair, base: u32) {
    let mut code = repair.original - 0xd800 + base;
    for offset in (0..repair.digits).rev() {
        source[repair.start + offset] = b"0123456789ABCDEF"[(code & 15) as usize];
        code >>= 4;
    }
}

/// Only the scanner-identified double-quoted token may be repaired. Skip paired
/// backslashes and escaped quotes; plain/single-quoted/block text is untouched.
fn repair_quoted(source: &mut [u8], start: usize, repairs: &mut Vec<Repair>) -> bool {
    if source.get(start) != Some(&b'"') {
        return false;
    }
    let before = repairs.len();
    let mut index = start + 1;
    while index < source.len() && source[index] != b'"' {
        if source[index] != b'\\' {
            index += 1;
            continue;
        }
        let digits = match source.get(index + 1) {
            Some(b'u') => 4,
            Some(b'U') => 8,
            _ => {
                index += 2;
                continue;
            }
        };
        let end = index + 2 + digits;
        let Some(hex) = source.get(index + 2..end) else {
            break;
        };
        let code = hex.iter().try_fold(0_u32, |code, byte| {
            (*byte as char)
                .to_digit(16)
                .map(|digit| (code << 4) + digit)
        });
        if let Some(original @ 0xd800..=0xdfff) = code {
            let repair = Repair {
                start: index + 2,
                digits,
                original,
            };
            replace_escape(source, repair, 0xe000);
            repairs.push(repair);
        }
        index = end;
    }
    repairs.len() != before
}

fn reconstruct(a: &str, b: &str, recovered: &mut usize) -> Result<SourceText, ScanFailure> {
    if a == b {
        return Ok(SourceText::Unicode(a.to_owned()));
    }
    let mut result = Vec::new();
    let mut other = b.chars();
    for left in a.chars() {
        let right = other.next().ok_or(ScanFailure::Reconstruction)?;
        let left = left as u32;
        let right = right as u32;
        result.push(if left == right {
            left
        } else if (0xe000..=0xe7ff).contains(&left) && right == left + 0x800 {
            *recovered += 1;
            left - 0xe000 + 0xd800
        } else {
            return Err(ScanFailure::Reconstruction);
        });
    }
    if other.next().is_some() {
        return Err(ScanFailure::Reconstruction);
    }
    Ok(SourceText::Invalid(result))
}

fn convert(event: Event<'static>, text: Option<SourceText>) -> Result<SourceEvent, ScanFailure> {
    Ok(match event {
        Event::Nothing => return Err(ScanFailure::Reconstruction),
        Event::StreamStart => SourceEvent::StreamStart,
        Event::StreamEnd => SourceEvent::StreamEnd,
        Event::DocumentStart(explicit) => SourceEvent::DocumentStart(explicit),
        Event::DocumentEnd => SourceEvent::DocumentEnd,
        Event::Alias(anchor) => SourceEvent::Alias(anchor),
        Event::SequenceStart(anchor, tag) => {
            SourceEvent::SequenceStart(anchor, tag.map(Cow::into_owned))
        }
        Event::MappingStart(anchor, tag) => {
            SourceEvent::MappingStart(anchor, tag.map(Cow::into_owned))
        }
        Event::SequenceEnd => SourceEvent::SequenceEnd,
        Event::MappingEnd => SourceEvent::MappingEnd,
        Event::Scalar(value, style, anchor, tag) => SourceEvent::Scalar(
            text.unwrap_or_else(|| SourceText::Unicode(value.into_owned())),
            style,
            anchor,
            tag.map(Cow::into_owned),
        ),
    })
}

/// Scan one retained ASCII or declared UTF-8 snapshot. No host decoder,
/// numeric conversion, mapping collapse, or lossy Unicode replacement
/// participates in this step.
/// Multiple documents and forbidden constructs are retained for admission.
#[cfg(test)]
pub fn scan(source: &[u8]) -> Result<Vec<LocatedEvent>, ScanFailure> {
    scan_impl(source, false)
}

/// Reject composition-time restrictions as their events arrive, before later
/// syntax/constructor errors. Mapping and Unicode admission follows scanning.
pub fn scan_strict(source: &[u8]) -> Result<Vec<LocatedEvent>, ScanFailure> {
    scan_impl(source, true)
}

/// The position of the byte at `offset`, whose prefix is UTF-8. Lines break
/// as the scanner breaks them, at a line feed, a carriage return or the two
/// together, and a column counts characters.
fn byte_position(source: &[u8], offset: usize) -> SourcePosition {
    let mut line = 1;
    let mut column = 1;
    let mut previous_cr = false;
    for &byte in &source[..offset] {
        match byte {
            b'\n' if previous_cr => {}
            b'\r' | b'\n' => {
                line += 1;
                column = 1;
            }
            // A UTF-8 continuation byte continues its character.
            _ if byte & 0xc0 == 0x80 => {}
            _ => column += 1,
        }
        previous_cr = byte == b'\r';
    }
    SourcePosition {
        offset,
        line,
        column,
    }
}

/// Whether the root mapping of the first document declares `encoding:
/// utf-8` (SPEC-8).
fn declares_utf8(text: &str) -> bool {
    header_encoding(text) == Some(true)
}

/// Whether the root mapping of the first document declares `encoding`, and
/// if so whether its value is `utf-8` (SPEC-8). A source the scanner cannot
/// read as far as that entry declares nothing, so it stays ASCII. A leading
/// byte-order mark does not hide the declaration, so the refusal of the
/// mark names the right remedy.
fn header_encoding(text: &str) -> Option<bool> {
    let text = text.strip_prefix('\u{FEFF}').unwrap_or(text);
    let mut depth = 0_usize;
    // Whether the next node at depth one is a key, and whether the value
    // being read belongs to the key `encoding`.
    let mut key = true;
    let mut encoding = false;
    for item in Parser::new_from_str(text) {
        let Ok((event, _)) = item else {
            return None;
        };
        match event {
            Event::MappingStart(..) if depth == 0 => depth = 1,
            Event::Scalar(..) | Event::SequenceStart(..) | Event::Alias(_) if depth == 0 => {
                return None
            }
            Event::Scalar(value, ..) if depth == 1 => {
                if key {
                    encoding = value == "encoding";
                } else if encoding {
                    return Some(value == "utf-8");
                }
                key = !key;
            }
            Event::Alias(_) if depth == 1 => {
                if !key && encoding {
                    return None;
                }
                encoding = false;
                key = !key;
            }
            Event::MappingStart(..) | Event::SequenceStart(..) => {
                if depth == 1 {
                    if !key && encoding {
                        return None;
                    }
                    encoding = false;
                }
                depth += 1;
            }
            Event::MappingEnd | Event::SequenceEnd => {
                depth -= 1;
                match depth {
                    0 => return None,
                    1 => key = !key,
                    _ => {}
                }
            }
            Event::DocumentEnd | Event::StreamEnd => return None,
            _ => {}
        }
    }
    None
}

/// A character a source writes only as an escape, even when it declares
/// UTF-8: a byte-order mark, and the Unicode `Bidi_Control` characters,
/// which can show a reader text in another order than the scanner reads it.
fn escaped_only(character: char) -> bool {
    matches!(
        character,
        '\u{FEFF}' | '\u{061C}' | '\u{200E}' | '\u{200F}' | '\u{202A}'..='\u{202E}'
            | '\u{2066}'..='\u{2069}'
    )
}

/// Admit the source's bytes under the encoding its header declares
/// (SPEC-8): ASCII, or UTF-8 after `encoding: utf-8`.
fn admit(source: &[u8]) -> Result<Offsets, ScanFailure> {
    let Some(first) = source.iter().position(|byte| !byte.is_ascii()) else {
        return Ok(Offsets::Ascii);
    };
    let ascii = ScanFailure::NonAscii(byte_position(source, first), Encoding::Ascii);
    let text = match std::str::from_utf8(source) {
        Ok(text) => text,
        Err(error) if declares_utf8(&String::from_utf8_lossy(source)) => {
            return Err(ScanFailure::InvalidUtf8(byte_position(
                source,
                error.valid_up_to(),
            )))
        }
        Err(_) => return Err(ascii),
    };
    match header_encoding(text) {
        Some(true) => {}
        Some(false) => {
            return Err(ScanFailure::NonAscii(
                byte_position(source, first),
                Encoding::Other,
            ))
        }
        None => return Err(ascii),
    }
    if let Some((offset, _)) = text.char_indices().find(|&(_, c)| escaped_only(c)) {
        return Err(ScanFailure::NonAscii(
            byte_position(source, offset),
            Encoding::Utf8,
        ));
    }
    Ok(Offsets::Utf8(
        text.char_indices()
            .map(|(offset, _)| offset)
            .chain([text.len()])
            .collect(),
    ))
}

fn scan_impl(source: &[u8], strict: bool) -> Result<Vec<LocatedEvent>, ScanFailure> {
    Limits::spec_bytes(source.len()).map_err(ScanFailure::Limit)?;
    let offsets = admit(source)?;
    let mut repaired = source.to_vec();
    let mut repairs = Vec::new();
    let first = loop {
        let text = std::str::from_utf8(&repaired).map_err(|_| ScanFailure::Reconstruction)?;
        match parse_pass(text, strict, &offsets) {
            Ok(events) => break events,
            Err(PassFailure::Resource(error)) => return Err(error),
            Err(PassFailure::Syntax(error)) => {
                if error.info()
                    == "while parsing a quoted scalar, found invalid Unicode character escape code"
                    && repair_quoted(
                        &mut repaired,
                        offsets.byte(error.marker().index()),
                        &mut repairs,
                    )
                {
                    continue;
                }
                return Err(ScanFailure::Syntax {
                    position: offsets.position(*error.marker()),
                    reason: error.info().to_owned(),
                });
            }
        }
    };
    let second = if repairs.is_empty() {
        None
    } else {
        for repair in &repairs {
            replace_escape(&mut repaired, *repair, 0xe800);
        }
        let text = std::str::from_utf8(&repaired).map_err(|_| ScanFailure::Reconstruction)?;
        let events = parse_pass(text, strict, &offsets).map_err(|error| match error {
            PassFailure::Resource(error) => error,
            PassFailure::Syntax(_) => ScanFailure::Reconstruction,
        })?;
        if events.len() != first.len() {
            return Err(ScanFailure::Reconstruction);
        }
        Some(events)
    };
    let mut result = Vec::with_capacity(first.len());
    let mut recovered = 0;
    for (index, (event, span)) in first.into_iter().enumerate() {
        let text = if let Some(second) = &second {
            let (other, other_span) = &second[index];
            if span != *other_span {
                return Err(ScanFailure::Reconstruction);
            }
            match (&event, other) {
                (
                    Event::Scalar(a, style_a, anchor_a, tag_a),
                    Event::Scalar(b, style_b, anchor_b, tag_b),
                ) if style_a == style_b && anchor_a == anchor_b && tag_a == tag_b => {
                    Some(reconstruct(a, b, &mut recovered)?)
                }
                _ if event == *other => None,
                _ => return Err(ScanFailure::Reconstruction),
            }
        } else {
            None
        };
        result.push(LocatedEvent {
            event: convert(event, text)?,
            start: offsets.position(span.start),
            end: offsets.position(span.end),
        });
    }
    if recovered != repairs.len() {
        return Err(ScanFailure::Reconstruction);
    }
    Ok(result)
}

#[cfg(test)]
mod tests;
