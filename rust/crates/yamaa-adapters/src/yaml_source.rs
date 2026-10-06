//! Bounded YAML event scanning before document admission.
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

use saphyr_parser::{Event, Marker, Parser, ScalarStyle, ScanError, Span, Tag};

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

/// Byte offset and one-based coordinates in the original ASCII snapshot.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub struct SourcePosition {
    pub offset: usize,
    pub line: usize,
    pub column: usize,
}

impl From<Marker> for SourcePosition {
    fn from(mark: Marker) -> Self {
        Self {
            offset: mark.index(),
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

/// Quotas bound retained events, tree depth and cumulative reparsing work.
#[derive(Clone, Copy, Debug)]
pub struct ScanLimits {
    pub source_bytes: usize,
    pub events: usize,
    pub depth: usize,
    pub decoded_bytes: usize,
    pub parse_bytes: usize,
}

impl Default for ScanLimits {
    fn default() -> Self {
        Self {
            source_bytes: 8_388_608,
            events: 262_144,
            depth: 64,
            decoded_bytes: 8_388_608,
            parse_bytes: 33_554_432,
        }
    }
}

/// Resource refusal is distinct from language errors and invalid Unicode.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ScanFailure {
    NonAscii(SourcePosition),
    Syntax {
        position: SourcePosition,
        reason: String,
    },
    Limit(&'static str),
    /// Fail closed if a parser upgrade violates diagnostic reconstruction.
    Reconstruction,
}

fn charge(
    used: &mut usize,
    amount: usize,
    limit: usize,
    name: &'static str,
) -> Result<(), ScanFailure> {
    *used = used.checked_add(amount).ok_or(ScanFailure::Limit(name))?;
    if *used > limit {
        return Err(ScanFailure::Limit(name));
    }
    Ok(())
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
    limits: ScanLimits,
    strict: bool,
) -> Result<Vec<(Event<'static>, Span)>, PassFailure> {
    let mut events = Vec::new();
    let mut depth = 0_usize;
    let mut decoded = 0;
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
                    metadata_start(source, boundary, span.start)
                } else {
                    span.start.into()
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
        if events.len() >= limits.events {
            return Err(PassFailure::Resource(ScanFailure::Limit("events")));
        }
        match &event {
            Event::SequenceStart(..) | Event::MappingStart(..) => {
                charge(&mut depth, 1, limits.depth, "depth").map_err(PassFailure::Resource)?;
            }
            Event::SequenceEnd | Event::MappingEnd => {
                depth = depth
                    .checked_sub(1)
                    .ok_or(PassFailure::Resource(ScanFailure::Reconstruction))?;
            }
            Event::Scalar(text, ..) => {
                charge(
                    &mut decoded,
                    text.len(),
                    limits.decoded_bytes,
                    "decoded_bytes",
                )
                .map_err(PassFailure::Resource)?;
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
fn metadata_start(source: &str, boundary: usize, fallback: Marker) -> SourcePosition {
    let bytes = source.as_bytes();
    let mut comment = false;
    for (offset, &byte) in bytes
        .iter()
        .enumerate()
        .take(fallback.index().saturating_add(1))
        .skip(boundary)
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
            let prefix = &bytes[..offset];
            // Markers follow YAML line-break handling, including standalone CR.
            let mut line = 1;
            let mut column = 1;
            let mut previous_cr = false;
            for &byte in prefix {
                match byte {
                    b'\n' if previous_cr => {}
                    b'\r' | b'\n' => {
                        line += 1;
                        column = 1;
                    }
                    _ => column += 1,
                }
                previous_cr = byte == b'\r';
            }
            return SourcePosition {
                offset,
                line,
                column,
            };
        }
    }
    fallback.into()
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

/// Scan one retained ASCII snapshot. No host decoder, numeric conversion,
/// mapping collapse, or lossy Unicode replacement participates in this step.
/// Multiple documents and forbidden constructs are retained for admission.
#[cfg(test)]
pub fn scan(source: &[u8], limits: ScanLimits) -> Result<Vec<LocatedEvent>, ScanFailure> {
    scan_impl(source, limits, false)
}

/// Reject composition-time restrictions as their events arrive, before later
/// syntax/constructor errors. Mapping and Unicode admission follows scanning.
pub fn scan_strict(source: &[u8], limits: ScanLimits) -> Result<Vec<LocatedEvent>, ScanFailure> {
    scan_impl(source, limits, true)
}

fn scan_impl(
    source: &[u8],
    limits: ScanLimits,
    strict: bool,
) -> Result<Vec<LocatedEvent>, ScanFailure> {
    if source.len() > limits.source_bytes {
        return Err(ScanFailure::Limit("source_bytes"));
    }
    if let Some(offset) = source.iter().position(|byte| !byte.is_ascii()) {
        let prefix = &source[..offset];
        return Err(ScanFailure::NonAscii(SourcePosition {
            offset,
            line: prefix.iter().filter(|byte| **byte == b'\n').count() + 1,
            column: prefix
                .iter()
                .rposition(|byte| *byte == b'\n')
                .map_or(offset + 1, |newline| offset - newline),
        }));
    }
    let mut repaired = source.to_vec();
    let mut repairs = Vec::new();
    let mut parse_bytes = 0;
    let first = loop {
        // Charge even an empty document; a zero-work budget permits no passes.
        charge(
            &mut parse_bytes,
            source.len().max(1),
            limits.parse_bytes,
            "parse_bytes",
        )?;
        let text = std::str::from_utf8(&repaired).map_err(|_| ScanFailure::Reconstruction)?;
        match parse_pass(text, limits, strict) {
            Ok(events) => break events,
            Err(PassFailure::Resource(error)) => return Err(error),
            Err(PassFailure::Syntax(error)) => {
                if error.info()
                    == "while parsing a quoted scalar, found invalid Unicode character escape code"
                    && repair_quoted(&mut repaired, error.marker().index(), &mut repairs)
                {
                    continue;
                }
                return Err(ScanFailure::Syntax {
                    position: (*error.marker()).into(),
                    reason: error.info().to_owned(),
                });
            }
        }
    };
    let second = if repairs.is_empty() {
        None
    } else {
        charge(
            &mut parse_bytes,
            source.len().max(1),
            limits.parse_bytes,
            "parse_bytes",
        )?;
        for repair in &repairs {
            replace_escape(&mut repaired, *repair, 0xe800);
        }
        let text = std::str::from_utf8(&repaired).map_err(|_| ScanFailure::Reconstruction)?;
        let events = parse_pass(text, limits, strict).map_err(|error| match error {
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
            start: span.start.into(),
            end: span.end.into(),
        });
    }
    if recovered != repairs.len() {
        return Err(ScanFailure::Reconstruction);
    }
    Ok(result)
}

#[cfg(test)]
mod tests;
