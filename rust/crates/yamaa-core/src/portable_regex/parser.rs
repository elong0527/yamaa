//! The portable pattern grammar: validation and lowering to `regex` crate syntax.
//!
//! The grammar is ECMA-262 `Pattern` syntax with the Unicode flag, without
//! lookaround, backreferences, property escapes or flags; group names are ASCII
//! identifiers. The lowered pattern escapes every literal scalar and spells the
//! shorthand classes, `.` and `\b` as explicit sets, so neither the crate's
//! Unicode tables nor its defaults supply any meaning.
//!
//! Parsing also measures the lowered pattern's syntax nesting the way the
//! crate's parser counts it: each group, alternation, concatenation,
//! repetition and bracketed class (with a nested class or a union of members)
//! adds a level. The crate compiles recursively, so this nesting, not group
//! depth alone, is what the semantic depth bounds.
use super::{CompileError, Resource, SPACE};
use crate::limits::Limits;
use std::{borrow::Cow, collections::BTreeSet, fmt::Write};

const DIGIT: &str = "0-9";
const WORD: &str = "0-9A-Za-z_";
/// Every scalar except the line terminators U+000A, U+000D, U+2028 and U+2029.
const DOT: &str = r"[^\x{A}\x{D}\x{2028}\x{2029}]";
const NOTHING: &str = r"[^\x{0}-\x{10FFFF}]";
const ANYTHING: &str = r"[\x{0}-\x{10FFFF}]";
/// The members of `\s`, each range of [`SPACE`] spelled as the crate reads it.
fn space() -> String {
    let mut members = String::new();
    for (low, high) in SPACE {
        let written = if low == high {
            write!(members, r"\x{{{low:X}}}")
        } else {
            write!(members, r"\x{{{low:X}}}-\x{{{high:X}}}")
        };
        written.expect("writing to a String");
    }
    members
}
/// A bracketed class and the union of its members, as the crate nests them.
const CLASS_NESTING: usize = 2;

/// A validated pattern in crate syntax, with its capture count and an upper
/// bound on its crate syntax nesting.
pub(super) struct Lowered {
    pub(super) pattern: String,
    pub(super) groups: usize,
    pub(super) nesting: usize,
}

/// Validate the whole source before anything is compiled.
pub(super) fn lower(source: &str) -> Result<Lowered, CompileError> {
    let mut parser = Parser {
        source,
        chars: source.char_indices().collect(),
        at: 0,
        groups: 0,
        names: BTreeSet::new(),
        output: String::with_capacity(source.len()),
    };
    let nesting = parser.disjunction(0)?;
    if parser.peek().is_some() {
        return Err(parser.invalid("unexpected closing delimiter"));
    }
    if nesting > Limits::SEMANTIC_DEPTH {
        return Err(CompileError::Limit {
            resource: Resource::Depth,
            limit: Limits::SEMANTIC_DEPTH,
        });
    }
    Ok(Lowered {
        pattern: parser.output,
        groups: parser.groups,
        nesting,
    })
}

struct Parser<'s> {
    source: &'s str,
    chars: Vec<(usize, char)>,
    at: usize,
    groups: usize,
    names: BTreeSet<String>,
    output: String,
}

#[derive(Clone, Copy)]
enum Item {
    Range(u32, u32),
    Digit(bool),
    Word(bool),
    Space(bool),
}

enum Escape {
    Scalar(u32),
    Set(Item),
    Boundary(bool),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Atom {
    Consuming,
    Assertion,
}

impl Parser<'_> {
    /// Inspect the next Unicode scalar without advancing the source coordinate.
    fn peek(&self) -> Option<char> {
        self.chars.get(self.at).map(|(_, c)| *c)
    }
    /// Preserve UTF-8 source coordinates, including the end-of-input position.
    fn byte(&self) -> usize {
        self.chars
            .get(self.at)
            .map_or(self.source.len(), |(i, _)| *i)
    }
    /// Consume one exact syntax scalar when present.
    fn take(&mut self, c: char) -> bool {
        if self.peek() == Some(c) {
            self.at += 1;
            true
        } else {
            false
        }
    }
    fn invalid(&self, reason: &'static str) -> CompileError {
        CompileError::Invalid {
            byte: self.byte(),
            reason,
        }
    }
    /// Consume a required closing/opening delimiter or return a grammar failure.
    fn require(&mut self, c: char) -> Result<(), CompileError> {
        if self.take(c) {
            Ok(())
        } else {
            Err(self.invalid("missing delimiter"))
        }
    }
    /// Alternatives keep their order; leftmost-first matching prefers earlier ones.
    /// `depth` counts enclosing groups, which bounds this parser's recursion.
    fn disjunction(&mut self, depth: usize) -> Result<usize, CompileError> {
        let mut nesting = self.sequence(depth)?;
        let mut alternatives = 1;
        while self.take('|') {
            self.output.push('|');
            nesting = nesting.max(self.sequence(depth)?);
            alternatives += 1;
        }
        Ok(nesting + usize::from(alternatives > 1))
    }
    fn sequence(&mut self, depth: usize) -> Result<usize, CompileError> {
        let mut nesting = 0;
        let mut terms = 0;
        while !matches!(self.peek(), None | Some(')' | '|')) {
            nesting = nesting.max(self.term(depth)?);
            terms += 1;
        }
        Ok(nesting + usize::from(terms > 1))
    }
    /// One atom and its quantifier. Repetition counts are not expanded here;
    /// the compiled size limit bounds them.
    fn term(&mut self, depth: usize) -> Result<usize, CompileError> {
        let (atom, nesting) = self.atom(depth)?;
        let (min, max) = match self.peek() {
            Some('*') => {
                self.at += 1;
                (0, None)
            }
            Some('+') => {
                self.at += 1;
                (1, None)
            }
            Some('?') => {
                self.at += 1;
                (0, Some(1))
            }
            Some('{') => {
                self.at += 1;
                let min = self.decimal()?;
                let max = if self.take(',') {
                    if self.peek() == Some('}') {
                        None
                    } else {
                        Some(self.decimal()?)
                    }
                } else {
                    Some(min)
                };
                self.require('}')?;
                if max.is_some_and(|max| max < min) {
                    return Err(self.invalid("reversed quantifier bounds"));
                }
                (min, max)
            }
            _ => return Ok(nesting),
        };
        if atom == Atom::Assertion {
            return Err(self.invalid("assertion cannot be quantified"));
        }
        let written = match max {
            None => write!(self.output, "{{{min},}}"),
            Some(max) if max == min => write!(self.output, "{{{min}}}"),
            Some(max) => write!(self.output, "{{{min},{max}}}"),
        };
        written.expect("writing to a String");
        if self.take('?') {
            self.output.push('?');
        }
        Ok(nesting + 1)
    }
    /// A count the compiled-size limit would refuse anyway is refused here,
    /// without constructing it.
    fn decimal(&mut self) -> Result<u32, CompileError> {
        if !self.peek().is_some_and(|c| c.is_ascii_digit()) {
            return Err(self.invalid("expected quantifier digit"));
        }
        let mut number = 0u32;
        while let Some(c) = self.peek().filter(|c| c.is_ascii_digit()) {
            number = number
                .checked_mul(10)
                .and_then(|n| n.checked_add(u32::from(c as u8 - b'0')))
                .ok_or(CompileError::Limit {
                    resource: Resource::Size,
                    limit: Limits::REGEX_SIZE,
                })?;
            self.at += 1;
        }
        Ok(number)
    }
    /// The atom's kind and the nesting of its lowered syntax.
    fn atom(&mut self, depth: usize) -> Result<(Atom, usize), CompileError> {
        let byte = self.byte();
        let c = self.peek().ok_or_else(|| self.invalid("missing atom"))?;
        self.at += 1;
        let nesting = match c {
            '(' => self.group(depth, byte)?,
            '[' => self.class()?,
            '.' => {
                self.output.push_str(DOT);
                CLASS_NESTING
            }
            '^' | '$' => {
                // Without flags the crate anchors these at the subject's ends only.
                self.output.push(c);
                return Ok((Atom::Assertion, 0));
            }
            '\\' => match self.escape(false, byte)? {
                Escape::Scalar(c) => self.literal(c),
                Escape::Set(item) => self.class_body(false, &[item]),
                Escape::Boundary(positive) => {
                    // The flag group is one level.
                    self.output
                        .push_str(if positive { r"(?-u:\b)" } else { r"(?-u:\B)" });
                    return Ok((Atom::Assertion, 1));
                }
            },
            '*' | '+' | '?' | '{' | '}' | ']' | ')' | '|' => {
                return Err(CompileError::Invalid {
                    byte,
                    reason: "unexpected syntax character",
                })
            }
            c => self.literal(c as u32),
        };
        Ok((Atom::Consuming, nesting))
    }
    /// Number captures at their opening delimiter and cap group nesting.
    fn group(&mut self, depth: usize, byte: usize) -> Result<usize, CompileError> {
        if depth >= Limits::SEMANTIC_DEPTH {
            return Err(CompileError::Limit {
                resource: Resource::Depth,
                limit: Limits::SEMANTIC_DEPTH,
            });
        }
        let mut capture = true;
        if self.take('?') {
            if self.take(':') {
                capture = false;
            } else if self.take('=') || self.take('!') {
                return Err(CompileError::Invalid {
                    byte,
                    reason: "lookahead outside the portable subset",
                });
            } else if self.take('<') {
                if self.take('=') || self.take('!') {
                    return Err(CompileError::Invalid {
                        byte,
                        reason: "lookbehind outside the portable subset",
                    });
                }
                let name = self.name()?;
                if !self.names.insert(name) {
                    return Err(self.invalid("duplicate group name"));
                }
            } else {
                return Err(self.invalid("group extension outside portable contract"));
            }
        }
        if capture {
            // Names only label numbered groups; nothing refers to a group by name.
            self.groups += 1;
            self.output.push('(');
        } else {
            self.output.push_str("(?:");
        }
        let nesting = self.disjunction(depth + 1)?;
        self.require(')')?;
        self.output.push(')');
        Ok(nesting + 1)
    }
    /// An ASCII identifier: a letter or underscore, then letters, digits or underscores.
    fn name(&mut self) -> Result<String, CompileError> {
        let mut name = String::new();
        while let Some(c) = self.peek().filter(|c| *c != '>') {
            let admitted = if name.is_empty() {
                c.is_ascii_alphabetic() || c == '_'
            } else {
                c.is_ascii_alphanumeric() || c == '_'
            };
            if !admitted {
                return Err(self.invalid("invalid group name"));
            }
            self.at += 1;
            name.push(c);
        }
        if name.is_empty() {
            return Err(self.invalid("empty group name"));
        }
        self.require('>')?;
        Ok(name)
    }
    /// Parse scalar ranges and complemented sets, including empty classes.
    fn class(&mut self) -> Result<usize, CompileError> {
        let negative = self.take('^');
        let mut items = Vec::new();
        while self.peek() != Some(']') {
            if self.peek().is_none() {
                return Err(self.invalid("unclosed character class"));
            }
            let start = self.class_item()?;
            if self.peek() == Some('-')
                && self.chars.get(self.at + 1).is_some_and(|(_, c)| *c != ']')
            {
                self.at += 1;
                let end = self.class_item()?;
                let (Item::Range(a, a2), Item::Range(b, b2)) = (start, end) else {
                    return Err(self.invalid("class range endpoint is not a scalar"));
                };
                if a != a2 || b != b2 || a > b {
                    return Err(self.invalid("invalid character range"));
                }
                items.push(Item::Range(a, b));
            } else {
                items.push(start);
            }
        }
        self.at += 1;
        Ok(self.class_body(negative, &items))
    }
    /// Class escapes are singleton scalars or the closed shorthand sets.
    fn class_item(&mut self) -> Result<Item, CompileError> {
        let byte = self.byte();
        let c = self
            .peek()
            .ok_or_else(|| self.invalid("unclosed character class"))?;
        self.at += 1;
        if c != '\\' {
            return Ok(Item::Range(c as u32, c as u32));
        }
        match self.escape(true, byte)? {
            Escape::Scalar(c) => Ok(Item::Range(c, c)),
            Escape::Set(item) => Ok(item),
            Escape::Boundary(_) => unreachable!("a class escape is never an assertion"),
        }
    }
    /// Interpret Unicode-mode escapes without Python octal or identity rules.
    fn escape(&mut self, in_class: bool, byte: usize) -> Result<Escape, CompileError> {
        let c = self
            .peek()
            .ok_or_else(|| self.invalid("trailing backslash"))?;
        self.at += 1;
        Ok(match c {
            'd' | 'D' => Escape::Set(Item::Digit(c == 'd')),
            'w' | 'W' => Escape::Set(Item::Word(c == 'w')),
            's' | 'S' => Escape::Set(Item::Space(c == 's')),
            'b' if in_class => Escape::Scalar(8),
            'b' | 'B' if !in_class => Escape::Boundary(c == 'b'),
            'n' => Escape::Scalar(10),
            'r' => Escape::Scalar(13),
            't' => Escape::Scalar(9),
            'v' => Escape::Scalar(11),
            'f' => Escape::Scalar(12),
            '0' => {
                if self.peek().is_some_and(|c| c.is_ascii_digit()) {
                    return Err(self.invalid("legacy octal escape"));
                }
                Escape::Scalar(0)
            }
            '1'..='9' | 'k' if !in_class => {
                return Err(CompileError::Invalid {
                    byte,
                    reason: "backreference outside the portable subset",
                })
            }
            'c' => {
                let c = self
                    .peek()
                    .filter(|c| c.is_ascii_alphabetic())
                    .ok_or_else(|| self.invalid("invalid control escape"))?;
                self.at += 1;
                Escape::Scalar(c as u32 % 32)
            }
            'x' => Escape::Scalar(self.hex(2)?),
            'u' => Escape::Scalar(self.unicode()?),
            '^' | '$' | '\\' | '.' | '*' | '+' | '?' | '(' | ')' | '[' | ']' | '{' | '}' | '|'
            | '/' => Escape::Scalar(c as u32),
            '-' if in_class => Escape::Scalar(c as u32),
            _ => return Err(self.invalid("escape outside portable Unicode grammar")),
        })
    }
    /// `\uHHHH`, a surrogate pair of them, or a braced scalar `\u{H...}`.
    fn unicode(&mut self) -> Result<u32, CompileError> {
        if !self.take('{') {
            let mut value = self.hex(4)?;
            if (0xd800..=0xdbff).contains(&value) {
                let saved = self.at;
                if self.take('\\') && self.take('u') {
                    match self.hex(4) {
                        Ok(low) if (0xdc00..=0xdfff).contains(&low) => {
                            value = 0x10000 + ((value - 0xd800) << 10) + (low - 0xdc00);
                        }
                        _ => self.at = saved,
                    }
                } else {
                    self.at = saved;
                }
            }
            return Ok(value);
        }
        let mut value = 0u32;
        let mut any = false;
        while let Some(d) = self.peek().and_then(|c| c.to_digit(16)) {
            any = true;
            value = value
                .checked_mul(16)
                .and_then(|v| v.checked_add(d))
                .filter(|v| *v <= 0x10ffff)
                .ok_or_else(|| self.invalid("invalid code point escape"))?;
            self.at += 1;
        }
        if !any {
            return Err(self.invalid("empty code point escape"));
        }
        self.require('}')?;
        // TEXT-8 expands braced escapes into VALUE-11 scalar values.
        // Surrogate code points cannot cross that normalization boundary.
        if char::from_u32(value).is_none() {
            return Err(self.invalid("code point escape is not a Unicode scalar"));
        }
        Ok(value)
    }
    /// Read a fixed number of hexadecimal digits without consuming unrelated syntax.
    fn hex(&mut self, count: usize) -> Result<u32, CompileError> {
        let mut n = 0;
        for _ in 0..count {
            let digit = self
                .peek()
                .and_then(|c| c.to_digit(16))
                .ok_or_else(|| self.invalid("malformed hex escape"))?;
            n = n * 16 + digit;
            self.at += 1;
        }
        Ok(n)
    }
    /// Escape every scalar but ASCII letters and digits. An unpaired surrogate
    /// escape names no scalar, so nothing matches it.
    fn literal(&mut self, value: u32) -> usize {
        match char::from_u32(value) {
            Some(c) if c.is_ascii_alphanumeric() => self.output.push(c),
            Some(_) => write!(self.output, r"\x{{{value:X}}}").expect("writing to a String"),
            None => {
                self.output.push_str(NOTHING);
                return CLASS_NESTING;
            }
        }
        0
    }
    /// Spell one class with explicit members. Shorthand complements nest as
    /// classes, so `[^a\D]` keeps the ECMA union-then-complement meaning.
    /// Returns the class's nesting: the bracket and its member union, plus the
    /// same again for a nested complemented shorthand.
    fn class_body(&mut self, negative: bool, items: &[Item]) -> usize {
        let mut body = String::new();
        let mut nesting = CLASS_NESTING;
        for item in items {
            let (positive, members) = match *item {
                Item::Range(a, b) => {
                    // Unpaired surrogate endpoints contribute only the interval's scalars.
                    for (low, high) in [(a, b.min(0xd7ff)), (a.max(0xe000), b)] {
                        let written = match low.cmp(&high) {
                            std::cmp::Ordering::Less => {
                                write!(body, r"\x{{{low:X}}}-\x{{{high:X}}}")
                            }
                            std::cmp::Ordering::Equal => write!(body, r"\x{{{low:X}}}"),
                            std::cmp::Ordering::Greater => Ok(()),
                        };
                        written.expect("writing to a String");
                    }
                    continue;
                }
                Item::Digit(positive) => (positive, Cow::Borrowed(DIGIT)),
                Item::Word(positive) => (positive, Cow::Borrowed(WORD)),
                Item::Space(positive) => (positive, Cow::Owned(space())),
            };
            if positive {
                body.push_str(&members);
            } else if items.len() == 1 && !negative {
                // A lone complemented shorthand is the complemented class itself.
                write!(self.output, "[^{members}]").expect("writing to a String");
                return CLASS_NESTING;
            } else {
                write!(body, "[^{members}]").expect("writing to a String");
                nesting = 2 * CLASS_NESTING;
            }
        }
        if body.is_empty() {
            self.output
                .push_str(if negative { ANYTHING } else { NOTHING });
        } else {
            self.output.push('[');
            if negative {
                self.output.push('^');
            }
            self.output.push_str(&body);
            self.output.push(']');
        }
        nesting
    }
}
