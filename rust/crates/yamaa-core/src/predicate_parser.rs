//! Bounded R004 predicate syntax with portable regex literal admission.
//! No names are resolved and no predicate, callback or data source is evaluated.

pub use crate::numeric_parser::{ParseLimits, ParseResource, SourcePosition, SourceSpan};
use crate::{
    predicate::Comparison,
    regex,
    temporal::{Date, DateTime, TemporalError},
};
use alloc::{collections::BTreeSet, format, string::String, vec::Vec};

/// Temporal literal spelling is retained independently of its parsed civil value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TemporalKind {
    Date,
    DateTime,
}

/// Grammar-owned rejection; regex resource policies have a separate outcome.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GrammarFailure {
    InvalidExpression,
    InvalidEscape,
    InvalidTemporal {
        kind: TemporalKind,
        error: TemporalError,
    },
    InvalidRegex {
        byte: usize,
        reason: &'static str,
    },
}
impl GrammarFailure {
    /// The committed predicate grammar reports rejected temporal literals here too.
    /// REQ-0192's broader temporal diagnostic wording remains a recorded contract gap.
    pub fn condition(&self) -> &'static str {
        self.diagnostic_code().definition().condition
    }
    /// Keep regex and ESCAPE ownership separate from ordinary syntax errors.
    pub fn requirement(&self) -> &'static str {
        self.diagnostic_code()
            .definition()
            .requirement
            .expect("predicate grammar requirement")
    }
}

/// Resource and unsupported outcomes never become a grammar rejection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ParseError {
    Grammar {
        position: SourcePosition,
        failure: GrammarFailure,
        /// Production-owned explanation; hosts add the scalar source location.
        message: String,
    },
    Limit {
        position: SourcePosition,
        resource: ParseResource,
        limit: usize,
    },
    RegexLimit {
        position: SourcePosition,
        resource: regex::Resource,
        limit: usize,
    },
    UnsupportedRegex {
        position: SourcePosition,
        byte: usize,
        feature: &'static str,
    },
}

/// Flat postorder syntax; every child is an earlier node. Operands remain syntax.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ParsedKind {
    Number {
        fractional: bool,
    },
    String(String),
    Temporal {
        kind: TemporalKind,
        value: String,
    },
    Null,
    Identifier,
    Boolean(bool),
    Group(usize),
    Not(usize),
    And(usize, usize),
    Or(usize, usize),
    Compare {
        operator: Comparison,
        left: usize,
        right: usize,
    },
    IsNull {
        value: usize,
        negated: bool,
    },
    In {
        value: usize,
        items: Vec<usize>,
        negated: bool,
    },
    Between {
        value: usize,
        lower: usize,
        upper: usize,
        negated: bool,
    },
    Like {
        value: usize,
        pattern: usize,
        escape: Option<char>,
        negated: bool,
    },
    Contains {
        source: usize,
        pattern: usize,
    },
}

/// Half-open original UTF-8 extent, including grouping and literal delimiters.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParsedNode {
    pub span: SourceSpan,
    pub kind: ParsedKind,
    depth: usize,
}

/// Immutable syntax ownership with bounded recursion and nonrecursive destruction.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParsedPredicate {
    expression: String,
    nodes: Vec<ParsedNode>,
    root: usize,
    identifiers: Vec<usize>,
}
impl ParsedPredicate {
    /// Return the untouched input, including whitespace and number spelling.
    pub fn expression(&self) -> &str {
        &self.expression
    }
    /// Return the validated postorder arena without mutable access.
    pub fn nodes(&self) -> &[ParsedNode] {
        &self.nodes
    }
    /// Index of the one Boolean root in the arena.
    pub fn root(&self) -> usize {
        self.root
    }
    /// Unique names in first written occurrence order; no binding is attempted.
    pub fn identifiers(&self) -> impl Iterator<Item = &str> {
        self.identifiers.iter().map(|&id| {
            let span = self.nodes[id].span;
            &self.expression[span.start..span.end]
        })
    }
}

/// Compile the complete predicate before any data can be resolved.
/// Whole-input lexing precedes syntax/regex validation. Regex width-analysis work
/// and logical allocations share a request budget; each pattern also retains its
/// own limits. Parser byte/token/node policies bound total source and structure.
pub fn parse_predicate(text: &str, limits: ParseLimits) -> Result<ParsedPredicate, ParseError> {
    let tokens = tokenize(text, limits)?;
    let mut parser = Parser {
        text,
        tokens,
        cursor: 0,
        nodes: Vec::new(),
        regex_budget: regex::CompileBudget::new(regex::CompileLimits::default()),
        limits,
    };
    let root = parser.disjunction(1)?;
    if parser.token().kind != TokenKind::End {
        return Err(parser.invalid("unexpected trailing token"));
    }
    let mut seen = BTreeSet::new();
    let identifiers = parser
        .nodes
        .iter()
        .enumerate()
        .filter_map(|(id, node)| {
            if node.kind == ParsedKind::Identifier
                && seen.insert(&text[node.span.start..node.span.end])
            {
                Some(id)
            } else {
                None
            }
        })
        .collect();
    Ok(ParsedPredicate {
        expression: text.into(),
        nodes: parser.nodes,
        root,
        identifiers,
    })
}

/// Translate only valid original UTF-8 boundaries, never display-column offsets.
fn position(text: &str, byte: usize) -> SourcePosition {
    SourcePosition {
        byte,
        character: text[..byte].chars().count(),
    }
}
/// Construct a language rejection without rewriting source text.
fn grammar(
    text: &str,
    byte: usize,
    failure: GrammarFailure,
    message: impl Into<String>,
) -> ParseError {
    ParseError::Grammar {
        position: position(text, byte),
        failure,
        message: message.into(),
    }
}
/// Resource refusal carries the original site and the effective policy ceiling.
fn limit(text: &str, byte: usize, resource: ParseResource, ceiling: usize) -> ParseError {
    ParseError::Limit {
        position: position(text, byte),
        resource,
        limit: ceiling,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TokenKind {
    Name,
    Number,
    String,
    Compare(Comparison),
    Symbol(u8),
    End,
}
#[derive(Clone, Copy, Debug)]
struct Token {
    kind: TokenKind,
    span: SourceSpan,
}

/// Identifiers use the closed ASCII name production, independent of locale.
fn name_start(c: u8) -> bool {
    c.is_ascii_alphabetic() || c == b'_'
}
/// Continuations additionally admit ASCII digits.
fn name_continue(c: u8) -> bool {
    name_start(c) || c.is_ascii_digit()
}

/// Tokenize once, keeping strings as original spans until their operand is parsed.
fn tokenize(text: &str, limits: ParseLimits) -> Result<Vec<Token>, ParseError> {
    if text.len() > limits.bytes {
        return Err(limit(text, 0, ParseResource::Bytes, limits.bytes));
    }
    let mut tokens = Vec::new();
    let bytes = text.as_bytes();
    let mut at = 0;
    while at < bytes.len() {
        let character = text[at..].chars().next().expect("nonempty suffix");
        if character.is_whitespace() || matches!(character, '\u{1c}'..='\u{1f}') {
            at += character.len_utf8();
            continue;
        }
        if tokens.len() >= limits.tokens {
            return Err(limit(text, at, ParseResource::Tokens, limits.tokens));
        }
        let start = at;
        let kind = if bytes[at] == b'\'' {
            at += 1;
            loop {
                if at == bytes.len() {
                    return Err(grammar(
                        text,
                        start,
                        GrammarFailure::InvalidExpression,
                        "unterminated string literal",
                    ));
                }
                if bytes[at] == b'\'' {
                    at += 1;
                    if bytes.get(at) == Some(&b'\'') {
                        at += 1;
                    } else {
                        break;
                    }
                } else {
                    at += text[at..]
                        .chars()
                        .next()
                        .expect("nonempty suffix")
                        .len_utf8();
                }
            }
            TokenKind::String
        } else if bytes[at].is_ascii_digit()
            || (matches!(bytes[at], b'+' | b'-')
                && bytes.get(at + 1).is_some_and(u8::is_ascii_digit))
        {
            if matches!(bytes[at], b'+' | b'-') {
                at += 1;
            }
            while bytes.get(at).is_some_and(u8::is_ascii_digit) {
                at += 1;
            }
            if bytes.get(at) == Some(&b'.') && bytes.get(at + 1).is_some_and(u8::is_ascii_digit) {
                at += 1;
                while bytes.get(at).is_some_and(u8::is_ascii_digit) {
                    at += 1;
                }
            }
            if matches!(bytes.get(at), Some(b'e' | b'E')) {
                let mut end = at + 1;
                if matches!(bytes.get(end), Some(b'+' | b'-')) {
                    end += 1;
                }
                if bytes.get(end).is_some_and(u8::is_ascii_digit) {
                    at = end + 1;
                    while bytes.get(at).is_some_and(u8::is_ascii_digit) {
                        at += 1;
                    }
                }
            }
            TokenKind::Number
        } else if name_start(bytes[at]) {
            at += 1;
            while bytes.get(at).is_some_and(|&c| name_continue(c)) {
                at += 1;
            }
            if bytes.get(at) == Some(&b'.') {
                at += 1;
                if !bytes.get(at).is_some_and(|&c| name_start(c)) {
                    return Err(grammar(
                        text,
                        start,
                        GrammarFailure::InvalidExpression,
                        "invalid qualified identifier",
                    ));
                }
                at += 1;
                while bytes.get(at).is_some_and(|&c| name_continue(c)) {
                    at += 1;
                }
            }
            TokenKind::Name
        } else {
            at += 1;
            match bytes[start] {
                b'(' | b')' | b',' => TokenKind::Symbol(bytes[start]),
                b'=' => TokenKind::Compare(Comparison::Equal),
                b'<' => TokenKind::Compare(match bytes.get(at) {
                    Some(b'=') => {
                        at += 1;
                        Comparison::LessEqual
                    }
                    Some(b'>') => {
                        at += 1;
                        Comparison::NotEqual
                    }
                    _ => Comparison::Less,
                }),
                b'>' => TokenKind::Compare(if bytes.get(at) == Some(&b'=') {
                    at += 1;
                    Comparison::GreaterEqual
                } else {
                    Comparison::Greater
                }),
                _ => {
                    return Err(grammar(
                        text,
                        start,
                        GrammarFailure::InvalidExpression,
                        "unexpected character",
                    ))
                }
            }
        };
        tokens.push(Token {
            kind,
            span: SourceSpan { start, end: at },
        });
    }
    tokens.push(Token {
        kind: TokenKind::End,
        span: SourceSpan { start: at, end: at },
    });
    Ok(tokens)
}

struct Parser<'a> {
    text: &'a str,
    tokens: Vec<Token>,
    cursor: usize,
    nodes: Vec<ParsedNode>,
    regex_budget: regex::CompileBudget,
    limits: ParseLimits,
}
impl Parser<'_> {
    /// EOF is always present and is never consumed.
    fn token(&self) -> Token {
        self.tokens[self.cursor]
    }
    /// Reject at the current original token, including the end-of-input boundary.
    fn invalid(&self, message: &'static str) -> ParseError {
        grammar(
            self.text,
            self.token().span.start,
            GrammarFailure::InvalidExpression,
            message,
        )
    }
    /// Read one non-EOF token whose production has already been selected.
    fn advance(&mut self) -> Token {
        let token = self.token();
        self.cursor += 1;
        token
    }
    /// Keywords are case-insensitive while qualified and ordinary names stay exact.
    fn keyword(&self, name: &str) -> bool {
        let token = self.token();
        token.kind == TokenKind::Name
            && self.text[token.span.start..token.span.end].eq_ignore_ascii_case(name)
    }
    /// Consume a matching keyword, preserving its spelling in the original source.
    fn take(&mut self, name: &str) -> bool {
        if self.keyword(name) {
            self.advance();
            true
        } else {
            false
        }
    }
    /// Require the exact punctuation/string production without coercion.
    fn require(&mut self, kind: TokenKind, message: &'static str) -> Result<Token, ParseError> {
        if self.token().kind != kind {
            return Err(self.invalid(message));
        }
        Ok(self.advance())
    }
    /// Bound parser recursion independently of the eventual flat arena's depth.
    fn enter(&self, depth: usize) -> Result<(), ParseError> {
        let ceiling = self.limits.depth.min(64);
        if depth > ceiling {
            Err(limit(
                self.text,
                self.token().span.start,
                ParseResource::Depth,
                ceiling,
            ))
        } else {
            Ok(())
        }
    }
    /// Admit each immutable node and its expanded tree depth before retaining it.
    fn push(
        &mut self,
        kind: ParsedKind,
        start: usize,
        children: &[usize],
    ) -> Result<usize, ParseError> {
        let depth = children
            .iter()
            .map(|&id| self.nodes[id].depth)
            .max()
            .unwrap_or(0)
            + 1;
        let ceiling = self.limits.depth.min(64);
        if depth > ceiling {
            return Err(limit(self.text, start, ParseResource::Depth, ceiling));
        }
        if self.nodes.len() >= self.limits.nodes {
            return Err(limit(
                self.text,
                start,
                ParseResource::Nodes,
                self.limits.nodes,
            ));
        }
        let end = self.tokens[self.cursor - 1].span.end;
        let id = self.nodes.len();
        self.nodes.push(ParsedNode {
            span: SourceSpan { start, end },
            kind,
            depth,
        });
        Ok(id)
    }
    /// OR is left-associative and has the lowest Boolean precedence.
    fn disjunction(&mut self, depth: usize) -> Result<usize, ParseError> {
        self.enter(depth)?;
        let mut left = self.conjunction(depth)?;
        while self.take("OR") {
            let right = self.conjunction(depth)?;
            left = self.push(
                ParsedKind::Or(left, right),
                self.nodes[left].span.start,
                &[left, right],
            )?;
        }
        Ok(left)
    }
    /// AND binds more tightly than OR and preserves each written operand.
    fn conjunction(&mut self, depth: usize) -> Result<usize, ParseError> {
        let mut left = self.negation(depth)?;
        while self.take("AND") {
            let right = self.negation(depth)?;
            left = self.push(
                ParsedKind::And(left, right),
                self.nodes[left].span.start,
                &[left, right],
            )?;
        }
        Ok(left)
    }
    /// NOT applies to a Boolean expression and may repeat up to the depth policy.
    fn negation(&mut self, depth: usize) -> Result<usize, ParseError> {
        self.enter(depth)?;
        let start = self.token().span.start;
        if self.take("NOT") {
            let child = self.negation(depth + 1)?;
            self.push(ParsedKind::Not(child), start, &[child])
        } else {
            self.boolean(depth)
        }
    }
    /// Recognize exactly the Boolean productions; bare operand names are not truth.
    fn boolean(&mut self, depth: usize) -> Result<usize, ParseError> {
        let start = self.token().span.start;
        if self.token().kind == TokenKind::Symbol(b'(') {
            self.advance();
            let child = self.disjunction(depth + 1)?;
            self.require(TokenKind::Symbol(b')'), "expected ')' to close predicate")?;
            return self.push(ParsedKind::Group(child), start, &[child]);
        }
        for (name, value) in [("TRUE", true), ("FALSE", false)] {
            if self.take(name) {
                return self.push(ParsedKind::Boolean(value), start, &[]);
            }
        }
        if self.keyword("STR_CONTAINS")
            && self.tokens[self.cursor + 1].kind == TokenKind::Symbol(b'(')
        {
            return self.contains();
        }
        let value = self.operand()?;
        if let TokenKind::Compare(operator) = self.token().kind {
            self.advance();
            let right = self.operand()?;
            return self.push(
                ParsedKind::Compare {
                    operator,
                    left: value,
                    right,
                },
                start,
                &[value, right],
            );
        }
        if self.take("IS") {
            let negated = self.take("NOT");
            if !self.take("NULL") {
                return Err(self.invalid("expected NULL after IS"));
            }
            return self.push(ParsedKind::IsNull { value, negated }, start, &[value]);
        }
        let negated = self.take("NOT");
        if self.take("IN") {
            self.require(TokenKind::Symbol(b'('), "expected '(' after IN")?;
            let mut items = alloc::vec![self.operand()?];
            while self.token().kind == TokenKind::Symbol(b',') {
                self.advance();
                items.push(self.operand()?);
            }
            self.require(TokenKind::Symbol(b')'), "expected ')' after IN operands")?;
            let children: Vec<_> = core::iter::once(value)
                .chain(items.iter().copied())
                .collect();
            return self.push(
                ParsedKind::In {
                    value,
                    items,
                    negated,
                },
                start,
                &children,
            );
        }
        if self.take("BETWEEN") {
            let lower = self.operand()?;
            if !self.take("AND") {
                return Err(self.invalid("expected AND in BETWEEN predicate"));
            }
            let upper = self.operand()?;
            return self.push(
                ParsedKind::Between {
                    value,
                    lower,
                    upper,
                    negated,
                },
                start,
                &[value, lower, upper],
            );
        }
        if self.take("LIKE") {
            let pattern = self.operand()?;
            let escape = if self.take("ESCAPE") {
                let token = self.require(TokenKind::String, "ESCAPE requires a string literal")?;
                let value = self.string(token);
                let mut chars = value.chars();
                let character = chars.next();
                if character.is_none() || chars.next().is_some() {
                    return Err(grammar(
                        self.text,
                        token.span.start,
                        GrammarFailure::InvalidEscape,
                        "ESCAPE requires exactly one code point",
                    ));
                }
                character
            } else {
                None
            };
            if let (Some(escape), ParsedKind::String(pattern_text)) =
                (escape, &self.nodes[pattern].kind)
            {
                let mut escaped = false;
                for c in pattern_text.chars() {
                    escaped = !escaped && c == escape;
                }
                if escaped {
                    return Err(grammar(
                        self.text,
                        self.nodes[pattern].span.start,
                        GrammarFailure::InvalidEscape,
                        "LIKE pattern has a dangling escape",
                    ));
                }
            }
            return self.push(
                ParsedKind::Like {
                    value,
                    pattern,
                    escape,
                    negated,
                },
                start,
                &[value, pattern],
            );
        }
        Err(self.invalid(if negated {
            "NOT must precede IN, BETWEEN, or LIKE"
        } else {
            "operand must be followed by a Boolean operator"
        }))
    }
    /// Only doubled quotes are decoded; backslashes and Unicode scalars stay exact.
    fn string(&self, token: Token) -> String {
        self.text[token.span.start + 1..token.span.end - 1].replace("''", "'")
    }
    /// Admit normalized syntax without converting numeric literals to host numbers.
    fn operand(&mut self) -> Result<usize, ParseError> {
        let token = self.token();
        let start = token.span.start;
        let kind = match token.kind {
            TokenKind::Number => {
                self.advance();
                ParsedKind::Number {
                    fractional: self.text[start..token.span.end]
                        .bytes()
                        .any(|c| matches!(c, b'.' | b'e' | b'E')),
                }
            }
            TokenKind::String => {
                self.advance();
                ParsedKind::String(self.string(token))
            }
            TokenKind::Name if self.take("NULL") => ParsedKind::Null,
            TokenKind::Name if self.keyword("DATE") || self.keyword("DATETIME") => {
                let kind = if self.take("DATE") {
                    TemporalKind::Date
                } else {
                    self.advance();
                    TemporalKind::DateTime
                };
                let literal = self.require(
                    TokenKind::String,
                    match kind {
                        TemporalKind::Date => "DATE requires a string literal",
                        TemporalKind::DateTime => "DATETIME requires a string literal",
                    },
                )?;
                let value = self.string(literal);
                let parsed = match kind {
                    TemporalKind::Date => value.parse::<Date>().map(|_| ()),
                    TemporalKind::DateTime => value.parse::<DateTime>().map(|_| ()),
                };
                parsed.map_err(|error| {
                    grammar(
                        self.text,
                        literal.span.start,
                        GrammarFailure::InvalidTemporal { kind, error },
                        match kind {
                            TemporalKind::Date => "invalid date literal",
                            TemporalKind::DateTime => "invalid datetime literal",
                        },
                    )
                })?;
                ParsedKind::Temporal { kind, value }
            }
            TokenKind::Name => {
                if [
                    "AND", "BETWEEN", "DATE", "DATETIME", "ESCAPE", "FALSE", "IN", "IS", "LIKE",
                    "NOT", "NULL", "OR", "TRUE",
                ]
                .iter()
                .any(|name| self.keyword(name))
                {
                    return Err(self.invalid("expected operand"));
                }
                self.advance();
                ParsedKind::Identifier
            }
            _ => return Err(self.invalid("expected operand")),
        };
        self.push(kind, start, &[])
    }
    /// Validate the literal using R022 before requiring the call's closing token.
    fn contains(&mut self) -> Result<usize, ParseError> {
        let start = self.advance().span.start;
        self.require(TokenKind::Symbol(b'('), "expected '(' after str_contains")?;
        let source = self.operand()?;
        self.require(
            TokenKind::Symbol(b','),
            "expected ',' between str_contains source and pattern",
        )?;
        if self.token().kind != TokenKind::String {
            return Err(self.invalid("str_contains pattern must be a string literal"));
        }
        let pattern = self.operand()?;
        let ParsedKind::String(text) = &self.nodes[pattern].kind else {
            unreachable!()
        };
        let site = self.nodes[pattern].span.start;
        regex::Pattern::compile_with_budget(
            text,
            regex::CompileLimits::default(),
            &mut self.regex_budget,
        )
        .map_err(|error| match error {
            regex::CompileError::Invalid { byte, reason } => grammar(
                self.text,
                site,
                GrammarFailure::InvalidRegex { byte, reason },
                format!("invalid regex in str_contains pattern: {reason}"),
            ),
            regex::CompileError::Limit { resource, limit } => ParseError::RegexLimit {
                position: position(self.text, site),
                resource,
                limit,
            },
            regex::CompileError::Unsupported { byte, feature } => ParseError::UnsupportedRegex {
                position: position(self.text, site),
                byte,
                feature,
            },
        })?;
        self.require(
            TokenKind::Symbol(b')'),
            "expected ')' to close str_contains",
        )?;
        self.push(
            ParsedKind::Contains { source, pattern },
            start,
            &[source, pattern],
        )
    }
}
