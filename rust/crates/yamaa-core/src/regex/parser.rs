//! Pattern grammar and static validation, independent of a matching host library.
use super::*;
use alloc::collections::BTreeMap;

/// Build an arena, resolve all backreferences, then qualify fixed-width lookbehind.
pub(super) fn compile(source: &str, limits: CompileLimits) -> Result<Pattern, CompileError> {
    if source.len() > limits.bytes {
        return Err(CompileError::Limit {
            resource: Resource::PatternBytes,
            limit: limits.bytes,
        });
    }
    let mut parser = Parser {
        source,
        chars: source.char_indices().collect(),
        at: 0,
        nodes: Vec::new(),
        groups: 0,
        names: BTreeMap::new(),
        limits,
    };
    let root = parser.disjunction(0)?;
    if parser.peek().is_some() {
        return Err(parser.invalid("unexpected closing delimiter"));
    }
    for node in &mut parser.nodes {
        if let Kind::Backreference(reference) = &mut node.kind {
            let number = match reference {
                Reference::Number(number) => *number,
                Reference::Name(name) => *parser.names.get(name).ok_or(CompileError::Invalid {
                    byte: node.byte,
                    reason: "unknown named backreference",
                })?,
            };
            if number == 0 || number > parser.groups {
                return Err(CompileError::Invalid {
                    byte: node.byte,
                    reason: "unknown backreference",
                });
            }
            *reference = Reference::Number(number);
        }
    }
    for node in &parser.nodes {
        if let Kind::Look {
            child,
            behind: true,
            ..
        } = node.kind
        {
            match parser.nodes[child].width {
                Width::Fixed(_) => {}
                Width::Backref { .. } => {
                    return Err(CompileError::Unsupported {
                        byte: node.byte,
                        feature: "lookbehind width depending on a backreference",
                    });
                }
                Width::Variable => {
                    return Err(CompileError::Invalid {
                        byte: node.byte,
                        reason: "variable-length lookbehind",
                    });
                }
            }
        }
    }
    Ok(Pattern {
        nodes: parser.nodes,
        root,
        groups: parser.groups,
    })
}

struct Parser<'s> {
    source: &'s str,
    chars: Vec<(usize, char)>,
    at: usize,
    nodes: Vec<Node>,
    groups: usize,
    names: BTreeMap<String, usize>,
    limits: CompileLimits,
}
enum Escape {
    Scalar(u32),
    Set(SetItem),
    Boundary(bool),
    Reference(Reference),
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
    /// Keep syntax diagnostics independent of resource and capability outcomes.
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
    /// Preserve the resource name and selected policy limit.
    fn limit(&self, resource: Resource, limit: usize) -> CompileError {
        CompileError::Limit { resource, limit }
    }
    /// Bound arena growth and fixed width before retaining a postorder node.
    fn push(
        &mut self,
        kind: Kind,
        byte: usize,
        width: Width,
        captures: core::ops::Range<usize>,
    ) -> Result<usize, CompileError> {
        if self.nodes.len() >= self.limits.nodes {
            return Err(self.limit(Resource::Nodes, self.limits.nodes));
        }
        if matches!(width, Width::Fixed(value) if value > self.limits.width) {
            return Err(self.limit(Resource::Width, self.limits.width));
        }
        let id = self.nodes.len();
        self.nodes.push(Node {
            kind,
            byte,
            width,
            captures,
        });
        Ok(id)
    }
    /// Preserve alternative order and capture numbering while computing fixed width.
    fn disjunction(&mut self, depth: usize) -> Result<usize, CompileError> {
        let byte = self.byte();
        let first = self.groups + 1;
        let mut children = vec![self.sequence(depth)?];
        while self.take('|') {
            children.push(self.sequence(depth)?);
        }
        if children.len() == 1 {
            return Ok(children[0]);
        }
        let mut fixed = None;
        let mut nonempty = true;
        let mut backref = false;
        let mut variable = false;
        for &id in &children {
            match self.nodes[id].width {
                Width::Fixed(value) => {
                    variable |= fixed.is_some_and(|known| known != value);
                    fixed = Some(value);
                    nonempty &= value > 0;
                }
                Width::Backref { nonempty: value } => {
                    backref = true;
                    nonempty &= value;
                }
                Width::Variable => variable = true,
            }
        }
        let width = if variable {
            Width::Variable
        } else if backref {
            Width::Backref { nonempty }
        } else {
            Width::Fixed(fixed.expect("nonempty alternatives contain a fixed width"))
        };
        self.push(
            Kind::Alternative(children),
            byte,
            width,
            first..self.groups + 1,
        )
    }
    /// Flatten concatenation; matching uses an explicit task stack in either direction.
    fn sequence(&mut self, depth: usize) -> Result<usize, CompileError> {
        let byte = self.byte();
        let first = self.groups + 1;
        let mut children = Vec::new();
        while !matches!(self.peek(), None | Some(')' | '|')) {
            children.push(self.term(depth)?);
        }
        if children.len() == 1 {
            return Ok(children[0]);
        }
        let mut width = Width::Fixed(0);
        for &id in &children {
            width = match (width, self.nodes[id].width) {
                (Width::Variable, _) | (_, Width::Variable) => Width::Variable,
                (Width::Fixed(a), Width::Fixed(b)) => Width::Fixed(
                    a.checked_add(b)
                        .ok_or_else(|| self.limit(Resource::Width, self.limits.width))?,
                ),
                (Width::Backref { nonempty }, Width::Fixed(value))
                | (Width::Fixed(value), Width::Backref { nonempty }) => Width::Backref {
                    nonempty: nonempty || value > 0,
                },
                (Width::Backref { nonempty: a }, Width::Backref { nonempty: b }) => {
                    Width::Backref { nonempty: a || b }
                }
            };
        }
        self.push(
            Kind::Sequence(children),
            byte,
            width,
            first..self.groups + 1,
        )
    }
    /// Parse one quantified atom without expanding its repetition count.
    fn term(&mut self, depth: usize) -> Result<usize, CompileError> {
        let byte = self.byte();
        let first = self.groups + 1;
        let child = self.atom(depth)?;
        let bounds = match self.peek() {
            Some('*') => {
                self.at += 1;
                Some((0, None))
            }
            Some('+') => {
                self.at += 1;
                Some((1, None))
            }
            Some('?') => {
                self.at += 1;
                Some((0, Some(1)))
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
                Some((min, max))
            }
            _ => None,
        };
        let Some((min, max)) = bounds else {
            return Ok(child);
        };
        if matches!(
            self.nodes[child].kind,
            Kind::Start | Kind::End | Kind::Boundary(_) | Kind::Look { .. }
        ) {
            return Err(self.invalid("assertion cannot be quantified"));
        }
        let greedy = !self.take('?');
        let width = match (self.nodes[child].width, max) {
            (_, Some(0)) | (Width::Fixed(0), _) => Width::Fixed(0),
            (Width::Fixed(width), Some(max)) if max == min => Width::Fixed(
                width
                    .checked_mul(min)
                    .ok_or_else(|| self.limit(Resource::Width, self.limits.width))?,
            ),
            (Width::Backref { nonempty }, max) => {
                // Variable repetition of a definitely consuming child is variable.
                // An unresolved reference may instead be empty; retain that gap.
                if nonempty && max != Some(min) {
                    Width::Variable
                } else {
                    Width::Backref { nonempty }
                }
            }
            _ => Width::Variable,
        };
        self.push(
            Kind::Repeat {
                child,
                min,
                max,
                greedy,
            },
            byte,
            width,
            first..self.groups + 1,
        )
    }
    /// Read a repetition count with checked arithmetic and a separate policy bound.
    fn decimal(&mut self) -> Result<usize, CompileError> {
        if !self.peek().is_some_and(|c| c.is_ascii_digit()) {
            return Err(self.invalid("expected quantifier digit"));
        }
        let mut number = 0usize;
        while let Some(c) = self.peek().filter(|c| c.is_ascii_digit()) {
            number = number
                .checked_mul(10)
                .and_then(|n| n.checked_add((c as u8 - b'0') as usize))
                .filter(|n| *n <= self.limits.repetition)
                .ok_or_else(|| self.limit(Resource::Repetition, self.limits.repetition))?;
            self.at += 1;
        }
        Ok(number)
    }
    /// Distinguish consuming atoms, zero-width assertions and unresolved backreferences.
    fn atom(&mut self, depth: usize) -> Result<usize, CompileError> {
        let byte = self.byte();
        let first = self.groups + 1;
        let c = self.peek().ok_or_else(|| self.invalid("missing atom"))?;
        self.at += 1;
        let (kind, width) = match c {
            '(' => return self.group(depth, byte, first),
            '[' => (self.class()?, Width::Fixed(1)),
            '.' => (Kind::Dot, Width::Fixed(1)),
            '^' => (Kind::Start, Width::Fixed(0)),
            '$' => (Kind::End, Width::Fixed(0)),
            '\\' => match self.escape(false)? {
                Escape::Scalar(c) => (Kind::Literal(c), Width::Fixed(1)),
                Escape::Set(item) => (
                    Kind::Class {
                        items: vec![item],
                        negative: false,
                    },
                    Width::Fixed(1),
                ),
                Escape::Boundary(positive) => (Kind::Boundary(positive), Width::Fixed(0)),
                Escape::Reference(reference) => (
                    Kind::Backreference(reference),
                    Width::Backref { nonempty: false },
                ),
            },
            '*' | '+' | '?' | '{' | '}' | ']' | ')' | '|' => {
                return Err(CompileError::Invalid {
                    byte,
                    reason: "unexpected syntax character",
                })
            }
            c => (Kind::Literal(c as u32), Width::Fixed(1)),
        };
        self.push(kind, byte, width, first..first)
    }
    /// Number captures at their opening delimiter and cap recursive group nesting.
    fn group(&mut self, depth: usize, byte: usize, first: usize) -> Result<usize, CompileError> {
        let depth_limit = self.limits.depth.min(64);
        if depth >= depth_limit {
            return Err(self.limit(Resource::Depth, depth_limit));
        }
        let mut capture = true;
        let mut look = None;
        let mut name = None;
        if self.take('?') {
            if self.take(':') {
                capture = false;
            } else if self.take('=') {
                capture = false;
                look = Some((false, true));
            } else if self.take('!') {
                capture = false;
                look = Some((false, false));
            } else if self.take('<') {
                if self.take('=') {
                    capture = false;
                    look = Some((true, true));
                } else if self.take('!') {
                    capture = false;
                    look = Some((true, false));
                } else {
                    name = Some(self.name()?);
                }
            } else {
                return Err(self.invalid("group extension outside portable contract"));
            }
        }
        let number = if capture {
            if self.groups >= self.limits.groups {
                return Err(self.limit(Resource::Groups, self.limits.groups));
            }
            self.groups += 1;
            if let Some(name) = name {
                if self.names.insert(name, self.groups).is_some() {
                    return Err(self.invalid("duplicate group name"));
                }
            }
            Some(self.groups)
        } else {
            None
        };
        let child = self.disjunction(depth + 1)?;
        self.require(')')?;
        let (kind, width) = if let Some((behind, positive)) = look {
            (
                Kind::Look {
                    child,
                    behind,
                    positive,
                },
                Width::Fixed(0),
            )
        } else {
            (Kind::Group { child, number }, self.nodes[child].width)
        };
        self.push(kind, byte, width, first..self.groups + 1)
    }
    /// Admit ASCII group names; defer Unicode identifier tables as an explicit gap.
    fn name(&mut self) -> Result<String, CompileError> {
        let begin = self.byte();
        let mut first = true;
        while let Some(c) = self.peek() {
            if c == '>' {
                break;
            }
            if !c.is_ascii() || c == '\\' {
                return Err(CompileError::Unsupported {
                    byte: self.byte(),
                    feature: "Unicode or escaped capture-group identifiers",
                });
            }
            if !(c.is_ascii_alphabetic() || c == '_' || c == '$' || (!first && c.is_ascii_digit()))
            {
                return Err(self.invalid("invalid group name"));
            }
            first = false;
            self.at += 1;
        }
        if first {
            return Err(self.invalid("empty group name"));
        }
        let end = self.byte();
        self.require('>')?;
        Ok(String::from(&self.source[begin..end]))
    }
    /// Parse scalar ranges and complemented sets, including empty character classes.
    fn class(&mut self) -> Result<Kind, CompileError> {
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
                let (SetItem::Range(a, a2), SetItem::Range(b, b2)) = (&start, &end) else {
                    return Err(self.invalid("class range endpoint is not a scalar"));
                };
                if a != a2 || b != b2 || a > b {
                    return Err(self.invalid("invalid character range"));
                }
                items.push(SetItem::Range(*a, *b));
            } else {
                items.push(start);
            }
        }
        self.at += 1;
        Ok(Kind::Class { items, negative })
    }
    /// Preserve class escapes as either singleton scalars or semantic character sets.
    fn class_item(&mut self) -> Result<SetItem, CompileError> {
        let c = self
            .peek()
            .ok_or_else(|| self.invalid("unclosed character class"))?;
        self.at += 1;
        if c == '\\' {
            match self.escape(true)? {
                Escape::Scalar(c) => Ok(SetItem::Range(c, c)),
                Escape::Set(item) => Ok(item),
                _ => Err(self.invalid("invalid character class escape")),
            }
        } else {
            Ok(SetItem::Range(c as u32, c as u32))
        }
    }
    /// Interpret Unicode-mode escapes without inheriting Python octal or identity rules.
    fn escape(&mut self, in_class: bool) -> Result<Escape, CompileError> {
        let c = self
            .peek()
            .ok_or_else(|| self.invalid("trailing backslash"))?;
        self.at += 1;
        Ok(match c {
            'd' | 'D' => Escape::Set(SetItem::Digit(c == 'd')),
            'w' | 'W' => Escape::Set(SetItem::Word(c == 'w')),
            's' | 'S' => Escape::Set(SetItem::Space(c == 's')),
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
            '1'..='9' if !in_class => {
                let mut n = (c as u8 - b'0') as usize;
                while let Some(c) = self.peek().filter(|c| c.is_ascii_digit()) {
                    n = n
                        .checked_mul(10)
                        .and_then(|n| n.checked_add((c as u8 - b'0') as usize))
                        .ok_or_else(|| self.invalid("unknown backreference"))?;
                    self.at += 1;
                }
                Escape::Reference(Reference::Number(n))
            }
            'k' if !in_class => {
                self.require('<')?;
                Escape::Reference(Reference::Name(self.name()?))
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
            'u' => {
                let value = if self.take('{') {
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
                    // REQ-0825 expands braced escapes into REQ-0022 scalar values.
                    // Surrogate code points cannot cross that normalization boundary.
                    if char::from_u32(value).is_none() {
                        return Err(self.invalid("code point escape is not a Unicode scalar"));
                    }
                    value
                } else {
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
                    value
                };
                Escape::Scalar(value)
            }
            '^' | '$' | '\\' | '.' | '*' | '+' | '?' | '(' | ')' | '[' | ']' | '{' | '}' | '|'
            | '/' => Escape::Scalar(c as u32),
            '-' if in_class => Escape::Scalar(c as u32),
            _ => return Err(self.invalid("escape outside portable Unicode grammar")),
        })
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
}
