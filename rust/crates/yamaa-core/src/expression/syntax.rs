//! The one lexer and parser of the closed expression grammar.
//!
//! The whole text is lexed before it is parsed. The arena keeps every literal
//! and name as written; nothing is resolved or evaluated here. A fixed depth
//! guard bounds recursion and the retained tree without counting source size.

use std::collections::BTreeSet;

use super::{CompileError, Failure, Reducer, Resource};
use crate::limits::Limits;
use crate::numeric::{BinaryOperator, UnaryOperator};
use crate::temporal::{Date, DateTime};

/// Half-open UTF-8 byte offsets into the original expression.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourceSpan {
    pub start: usize,
    pub end: usize,
}

/// Zero-based byte and Unicode scalar offsets; neither counts display columns.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourcePosition {
    pub byte: usize,
    pub character: usize,
}

/// A temporal literal's keyword, retained apart from its parsed civil value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TemporalKind {
    Date,
    DateTime,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Comparison {
    Equal,
    NotEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
}

/// An arena node. Child indices always refer to earlier nodes of the arena.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NodeKind {
    /// A number; a sign written before it is folded into its text, so `- 1`
    /// is the literal `-1`.
    Number {
        fractional: bool,
        text: String,
    },
    /// A string literal with its doubled quotes decoded.
    String(String),
    Temporal {
        kind: TemporalKind,
        value: String,
    },
    Boolean(bool),
    Null,
    Identifier,
    /// The record star `D.*`, written only as the argument of `COUNT`.
    Star,
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
    Unary {
        operator: UnaryOperator,
        operand: usize,
    },
    Binary {
        operator: BinaryOperator,
        left: usize,
        right: usize,
    },
    /// A call of a name the binder resolves against the function vocabulary.
    Call {
        name: SourceSpan,
        arguments: Vec<usize>,
    },
    Reduction {
        reducer: Reducer,
        name: SourceSpan,
        operand: usize,
    },
}

/// A node with its source extent, including grouping and literal delimiters.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Node {
    pub span: SourceSpan,
    pub kind: NodeKind,
    /// The byte a diagnostic cites for the node as a construct: its operator
    /// or keyword, or its start.
    pub(super) site: usize,
    /// The byte of the token that follows the node, or the text length.
    pub(super) follow: usize,
    depth: usize,
}

/// Immutable syntax with bounded depth; no name is bound or value computed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Parsed {
    expression: String,
    nodes: Vec<Node>,
    root: usize,
}

impl Parsed {
    /// The original text, including whitespace and literal spelling.
    pub fn expression(&self) -> &str {
        &self.expression
    }
    /// The arena in postorder: every child precedes its parent.
    pub fn nodes(&self) -> &[Node] {
        &self.nodes
    }
    /// The index of the root node.
    pub fn root(&self) -> usize {
        self.root
    }
    /// The written text of one node.
    pub fn text(&self, node: usize) -> &str {
        let span = self.nodes[node].span;
        &self.expression[span.start..span.end]
    }
    /// The written text of a span inside this expression.
    pub fn slice(&self, span: SourceSpan) -> &str {
        &self.expression[span.start..span.end]
    }
    /// Strip grouping parentheses, which leave no trace in the parse.
    pub fn ungroup(&self, mut node: usize) -> usize {
        while let NodeKind::Group(inner) = self.nodes[node].kind {
            node = inner;
        }
        node
    }
    /// Distinct case-sensitive names in first written occurrence order.
    pub fn identifiers(&self) -> impl Iterator<Item = &str> {
        let mut seen = BTreeSet::new();
        self.nodes
            .iter()
            .enumerate()
            .filter(|(_, node)| node.kind == NodeKind::Identifier)
            .map(|(id, _)| self.text(id))
            .filter(move |name| seen.insert(*name))
    }
    /// Distinct `COUNT` record relations, apart from field dependencies.
    pub fn star_datasets(&self) -> impl Iterator<Item = &str> {
        let mut seen = BTreeSet::new();
        self.nodes
            .iter()
            .enumerate()
            .filter(|(_, node)| node.kind == NodeKind::Star)
            .map(|(id, _)| {
                let text = self.text(id);
                &text[..text.len() - 2]
            })
            .filter(move |name| seen.insert(*name))
    }
    /// The node's byte position in both coordinate systems.
    pub(super) fn position(&self, byte: usize) -> SourcePosition {
        position(&self.expression, byte)
    }
}

/// Translate a byte boundary to both public coordinate systems.
pub(super) fn position(text: &str, byte: usize) -> SourcePosition {
    SourcePosition {
        byte,
        character: text[..byte].chars().count(),
    }
}

fn invalid(text: &str, byte: usize, failure: Failure, message: impl Into<String>) -> CompileError {
    CompileError::Invalid {
        position: position(text, byte),
        failure,
        message: message.into(),
    }
}

fn depth_limit(text: &str, byte: usize) -> CompileError {
    CompileError::Limit {
        position: position(text, byte),
        resource: Resource::Depth,
        limit: Limits::SEMANTIC_DEPTH,
    }
}

/// Parse the complete text. Lexing precedes syntax checking, and the integer
/// carrier is checked after the whole grammar, so a later syntax error is
/// never hidden by an earlier overflowing literal.
pub fn parse(text: &str) -> Result<Parsed, CompileError> {
    let tokens = tokenize(text)?;
    let mut parser = Parser {
        text,
        tokens,
        cursor: 0,
        nodes: Vec::new(),
    };
    let root = parser.expression(1)?;
    if parser.token().kind != TokenKind::End {
        return Err(parser.invalid("unexpected trailing token"));
    }
    for node in &parser.nodes {
        if let NodeKind::Number {
            fractional: false,
            text: written,
        } = &node.kind
        {
            if written.parse::<i64>().is_err() {
                return Err(invalid(
                    text,
                    node.span.start,
                    Failure::IntegerOverflow {
                        literal: written.clone(),
                    },
                    "integer literal is outside the signed 64-bit range",
                ));
            }
        }
    }
    Ok(Parsed {
        expression: text.into(),
        nodes: parser.nodes,
        root,
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TokenKind {
    Name,
    Star,
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

/// Names use the closed ASCII production, independent of locale.
fn name_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

fn name_end(bytes: &[u8], mut end: usize) -> usize {
    while end < bytes.len() && (name_start(bytes[end]) || bytes[end].is_ascii_digit()) {
        end += 1;
    }
    end
}

/// Digits, then a fraction and an exponent that each need at least one digit.
fn number_end(bytes: &[u8], start: usize) -> usize {
    let mut end = start;
    while bytes.get(end).is_some_and(u8::is_ascii_digit) {
        end += 1;
    }
    if bytes.get(end) == Some(&b'.') && bytes.get(end + 1).is_some_and(u8::is_ascii_digit) {
        end += 2;
        while bytes.get(end).is_some_and(u8::is_ascii_digit) {
            end += 1;
        }
    }
    if matches!(bytes.get(end), Some(b'e' | b'E')) {
        let mut exponent = end + 1;
        if matches!(bytes.get(exponent), Some(b'+' | b'-')) {
            exponent += 1;
        }
        let digits = exponent;
        while bytes.get(exponent).is_some_and(u8::is_ascii_digit) {
            exponent += 1;
        }
        if exponent > digits {
            end = exponent;
        }
    }
    end
}

/// Lex the whole text once, keeping strings as spans until they are parsed.
fn tokenize(text: &str) -> Result<Vec<Token>, CompileError> {
    let bytes = text.as_bytes();
    let mut tokens = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        let character = text[at..].chars().next().expect("character boundary");
        // Python str.isspace also includes the four ASCII information separators.
        if character.is_whitespace() || matches!(character, '\u{1c}'..='\u{1f}') {
            at += character.len_utf8();
            continue;
        }
        let start = at;
        let kind = if bytes[at] == b'\'' {
            at += 1;
            loop {
                if at == bytes.len() {
                    return Err(invalid(
                        text,
                        start,
                        Failure::Syntax,
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
                    at += text[at..].chars().next().expect("character").len_utf8();
                }
            }
            TokenKind::String
        } else if bytes[at].is_ascii_digit() {
            at = number_end(bytes, at);
            TokenKind::Number
        } else if name_start(bytes[at]) {
            at = name_end(bytes, at);
            if bytes.get(at) == Some(&b'.') {
                if bytes.get(at + 1) == Some(&b'*') {
                    at += 2;
                    tokens.push(Token {
                        kind: TokenKind::Star,
                        span: SourceSpan { start, end: at },
                    });
                    continue;
                }
                if !bytes.get(at + 1).is_some_and(|&byte| name_start(byte)) {
                    return Err(invalid(
                        text,
                        start,
                        Failure::Syntax,
                        "invalid qualified identifier",
                    ));
                }
                at = name_end(bytes, at + 1);
            }
            TokenKind::Name
        } else {
            at += 1;
            match bytes[start] {
                b'(' | b')' | b',' | b'+' | b'-' | b'*' | b'/' => TokenKind::Symbol(bytes[start]),
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
                    return Err(invalid(
                        text,
                        start,
                        Failure::Syntax,
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

/// The words a bare name may not spell; a qualified field may still use one.
pub const RESERVED: [&str; 10] = [
    "AND", "BETWEEN", "FALSE", "IN", "IS", "LIKE", "NOT", "NULL", "OR", "TRUE",
];

struct Parser<'a> {
    text: &'a str,
    tokens: Vec<Token>,
    cursor: usize,
    nodes: Vec<Node>,
}

impl<'a> Parser<'a> {
    /// The end token is always present and never consumed.
    fn token(&self) -> Token {
        self.tokens[self.cursor]
    }
    fn next(&self) -> Token {
        self.tokens[(self.cursor + 1).min(self.tokens.len() - 1)]
    }
    fn advance(&mut self) -> Token {
        let token = self.token();
        self.cursor += 1;
        token
    }
    fn spelling(&self, token: Token) -> &'a str {
        &self.text[token.span.start..token.span.end]
    }
    /// Reject at the current token, including the end-of-text boundary.
    fn invalid(&self, message: &'static str) -> CompileError {
        invalid(self.text, self.token().span.start, Failure::Syntax, message)
    }
    /// Keywords are case-insensitive; a qualified name never spells one.
    fn keyword(&self, word: &str) -> bool {
        let token = self.token();
        token.kind == TokenKind::Name && self.spelling(token).eq_ignore_ascii_case(word)
    }
    fn take(&mut self, word: &str) -> bool {
        let found = self.keyword(word);
        if found {
            self.advance();
        }
        found
    }
    fn symbol(&self, symbol: u8) -> bool {
        self.token().kind == TokenKind::Symbol(symbol)
    }
    fn require(&mut self, symbol: u8, message: &'static str) -> Result<Token, CompileError> {
        if self.symbol(symbol) {
            Ok(self.advance())
        } else {
            Err(self.invalid(message))
        }
    }
    /// Bound recursion before any node of the nested production exists.
    fn enter(&self, nesting: usize) -> Result<(), CompileError> {
        if nesting > Limits::SEMANTIC_DEPTH {
            Err(depth_limit(self.text, self.token().span.start))
        } else {
            Ok(())
        }
    }
    /// Admit a node and its tree depth; the node ends at the last consumed token.
    fn push(
        &mut self,
        kind: NodeKind,
        start: usize,
        site: usize,
        children: &[usize],
    ) -> Result<usize, CompileError> {
        let depth = children
            .iter()
            .map(|&id| self.nodes[id].depth)
            .max()
            .unwrap_or(0)
            + 1;
        if depth > Limits::SEMANTIC_DEPTH {
            return Err(depth_limit(self.text, start));
        }
        let end = self.tokens[self.cursor - 1].span.end;
        let id = self.nodes.len();
        self.nodes.push(Node {
            span: SourceSpan { start, end },
            kind,
            site,
            follow: self.token().span.start,
            depth,
        });
        Ok(id)
    }
    fn start(&self, node: usize) -> usize {
        self.nodes[node].span.start
    }

    /// `OR` is left-associative and binds most loosely.
    fn expression(&mut self, nesting: usize) -> Result<usize, CompileError> {
        self.enter(nesting)?;
        let mut left = self.conjunction(nesting)?;
        while self.keyword("OR") {
            let site = self.advance().span.start;
            let right = self.conjunction(nesting)?;
            left = self.push(
                NodeKind::Or(left, right),
                self.start(left),
                site,
                &[left, right],
            )?;
        }
        Ok(left)
    }
    fn conjunction(&mut self, nesting: usize) -> Result<usize, CompileError> {
        let mut left = self.negation(nesting)?;
        while self.keyword("AND") {
            let site = self.advance().span.start;
            let right = self.negation(nesting)?;
            left = self.push(
                NodeKind::And(left, right),
                self.start(left),
                site,
                &[left, right],
            )?;
        }
        Ok(left)
    }
    /// `NOT` may repeat up to the depth policy.
    fn negation(&mut self, nesting: usize) -> Result<usize, CompileError> {
        self.enter(nesting)?;
        if self.keyword("NOT") {
            let start = self.advance().span.start;
            let operand = self.negation(nesting + 1)?;
            return self.push(NodeKind::Not(operand), start, start, &[operand]);
        }
        self.comparison(nesting)
    }
    /// At most one comparison: comparison operators do not associate.
    fn comparison(&mut self, nesting: usize) -> Result<usize, CompileError> {
        let value = self.sum(nesting)?;
        let start = self.start(value);
        if let TokenKind::Compare(operator) = self.token().kind {
            let site = self.advance().span.start;
            let right = self.sum(nesting)?;
            return self.push(
                NodeKind::Compare {
                    operator,
                    left: value,
                    right,
                },
                start,
                site,
                &[value, right],
            );
        }
        let site = self.token().span.start;
        if self.take("IS") {
            let negated = self.take("NOT");
            if !self.take("NULL") {
                return Err(self.invalid("expected NULL after IS"));
            }
            return self.push(NodeKind::IsNull { value, negated }, start, site, &[value]);
        }
        let negated = self.take("NOT");
        if self.take("IN") {
            self.require(b'(', "expected '(' after IN")?;
            let mut items = vec![self.expression(nesting + 1)?];
            while self.symbol(b',') {
                self.advance();
                items.push(self.expression(nesting + 1)?);
            }
            self.require(b')', "expected ')' after IN operands")?;
            let children: Vec<_> = std::iter::once(value)
                .chain(items.iter().copied())
                .collect();
            return self.push(
                NodeKind::In {
                    value,
                    items,
                    negated,
                },
                start,
                site,
                &children,
            );
        }
        if self.take("BETWEEN") {
            let lower = self.sum(nesting)?;
            if !self.take("AND") {
                return Err(self.invalid("expected AND in BETWEEN"));
            }
            let upper = self.sum(nesting)?;
            return self.push(
                NodeKind::Between {
                    value,
                    lower,
                    upper,
                    negated,
                },
                start,
                site,
                &[value, lower, upper],
            );
        }
        if self.take("LIKE") {
            let pattern = self.sum(nesting)?;
            let escape = if self.take("ESCAPE") {
                let token = self.token();
                if token.kind != TokenKind::String {
                    return Err(self.invalid("ESCAPE requires a string literal"));
                }
                self.advance();
                let literal = self.string(token);
                let mut characters = literal.chars();
                let escape = characters.next();
                if escape.is_none() || characters.next().is_some() {
                    return Err(invalid(
                        self.text,
                        token.span.start,
                        Failure::InvalidEscape,
                        "ESCAPE requires exactly one code point",
                    ));
                }
                escape
            } else {
                None
            };
            if let (Some(escape), NodeKind::String(pattern_text)) =
                (escape, &self.nodes[pattern].kind)
            {
                let mut escaped = false;
                for character in pattern_text.chars() {
                    escaped = !escaped && character == escape;
                }
                if escaped {
                    return Err(invalid(
                        self.text,
                        self.start(pattern),
                        Failure::InvalidEscape,
                        "LIKE pattern has a dangling escape",
                    ));
                }
            }
            return self.push(
                NodeKind::Like {
                    value,
                    pattern,
                    escape,
                    negated,
                },
                start,
                site,
                &[value, pattern],
            );
        }
        if negated {
            return Err(self.invalid("NOT must precede IN, BETWEEN, or LIKE"));
        }
        Ok(value)
    }
    /// Binary `+` and `-` are left-associative.
    fn sum(&mut self, nesting: usize) -> Result<usize, CompileError> {
        let mut left = self.product(nesting)?;
        loop {
            let operator = match self.token().kind {
                TokenKind::Symbol(b'+') => BinaryOperator::Add,
                TokenKind::Symbol(b'-') => BinaryOperator::Subtract,
                _ => return Ok(left),
            };
            let site = self.advance().span.start;
            let right = self.product(nesting)?;
            left = self.push(
                NodeKind::Binary {
                    operator,
                    left,
                    right,
                },
                self.start(left),
                site,
                &[left, right],
            )?;
        }
    }
    /// `*` and `/` bind more tightly than the sums.
    fn product(&mut self, nesting: usize) -> Result<usize, CompileError> {
        let mut left = self.factor(nesting)?;
        loop {
            let operator = match self.token().kind {
                TokenKind::Symbol(b'*') => BinaryOperator::Multiply,
                TokenKind::Symbol(b'/') => BinaryOperator::Divide,
                _ => return Ok(left),
            };
            let site = self.advance().span.start;
            let right = self.factor(nesting)?;
            left = self.push(
                NodeKind::Binary {
                    operator,
                    left,
                    right,
                },
                self.start(left),
                site,
                &[left, right],
            )?;
        }
    }
    /// At most one sign. A sign written before a number is that number's own,
    /// so the least `int` is written `-9223372036854775808`.
    fn factor(&mut self, nesting: usize) -> Result<usize, CompileError> {
        let (operator, sign) = match self.token().kind {
            TokenKind::Symbol(b'+') => (UnaryOperator::Plus, '+'),
            TokenKind::Symbol(b'-') => (UnaryOperator::Negate, '-'),
            _ => return self.primary(nesting),
        };
        let start = self.advance().span.start;
        let token = self.token();
        if token.kind == TokenKind::Number {
            self.advance();
            let digits = self.spelling(token);
            let kind = NodeKind::Number {
                fractional: digits
                    .bytes()
                    .any(|byte| matches!(byte, b'.' | b'e' | b'E')),
                text: format!("{sign}{digits}"),
            };
            return self.push(kind, start, start, &[]);
        }
        let operand = self.primary(nesting)?;
        self.push(
            NodeKind::Unary { operator, operand },
            start,
            start,
            &[operand],
        )
    }
    /// Literals, names, calls, reductions and parenthesized expressions.
    fn primary(&mut self, nesting: usize) -> Result<usize, CompileError> {
        let token = self.token();
        let start = token.span.start;
        match token.kind {
            TokenKind::Number => {
                self.advance();
                let digits = self.spelling(token);
                let kind = NodeKind::Number {
                    fractional: digits
                        .bytes()
                        .any(|byte| matches!(byte, b'.' | b'e' | b'E')),
                    text: digits.into(),
                };
                self.push(kind, start, start, &[])
            }
            TokenKind::String => {
                self.advance();
                let kind = NodeKind::String(self.string(token));
                self.push(kind, start, start, &[])
            }
            TokenKind::Symbol(b'(') => {
                self.advance();
                let inner = self.expression(nesting + 1)?;
                self.require(b')', "expected ')' to close a group")?;
                self.push(NodeKind::Group(inner), start, start, &[inner])
            }
            TokenKind::Name => self.name(nesting),
            TokenKind::Star => Err(self.invalid("a record star is only the argument of COUNT")),
            _ => Err(self.invalid("expected an operand")),
        }
    }
    /// A name is a keyword literal, a temporal literal, a call, a reduction or
    /// an identifier, in that order of precedence.
    fn name(&mut self, nesting: usize) -> Result<usize, CompileError> {
        let token = self.token();
        let start = token.span.start;
        for (word, kind) in [
            ("TRUE", NodeKind::Boolean(true)),
            ("FALSE", NodeKind::Boolean(false)),
            ("NULL", NodeKind::Null),
        ] {
            if self.take(word) {
                return self.push(kind, start, start, &[]);
            }
        }
        if RESERVED.iter().any(|word| self.keyword(word)) {
            return Err(self.invalid("expected an operand"));
        }
        let spelling = self.spelling(token);
        let temporal = if spelling.eq_ignore_ascii_case("DATE") {
            Some(TemporalKind::Date)
        } else if spelling.eq_ignore_ascii_case("DATETIME") {
            Some(TemporalKind::DateTime)
        } else {
            None
        };
        if let Some(kind) = temporal.filter(|_| self.next().kind == TokenKind::String) {
            self.advance();
            let literal = self.advance();
            let value = self.string(literal);
            let parsed = match kind {
                TemporalKind::Date => value.parse::<Date>().map(|_| ()),
                TemporalKind::DateTime => value.parse::<DateTime>().map(|_| ()),
            };
            parsed.map_err(|error| {
                invalid(
                    self.text,
                    literal.span.start,
                    Failure::InvalidTemporal { kind, error },
                    match kind {
                        TemporalKind::Date => "invalid date literal",
                        TemporalKind::DateTime => "invalid datetime literal",
                    },
                )
            })?;
            return self.push(NodeKind::Temporal { kind, value }, start, start, &[]);
        }
        self.advance();
        if !self.symbol(b'(') {
            return self.push(NodeKind::Identifier, start, start, &[]);
        }
        self.advance();
        if let Some(reducer) = Reducer::lookup(spelling) {
            let operand = if self.token().kind == TokenKind::Star {
                if reducer != Reducer::Count {
                    return Err(self.invalid("a record star is only the argument of COUNT"));
                }
                let star = self.advance().span.start;
                self.push(NodeKind::Star, star, star, &[])?
            } else {
                self.expression(nesting + 1)?
            };
            self.require(b')', "a reducer takes exactly one argument")?;
            return self.push(
                NodeKind::Reduction {
                    reducer,
                    name: token.span,
                    operand,
                },
                start,
                start,
                &[operand],
            );
        }
        let mut arguments = Vec::new();
        if !self.symbol(b')') {
            loop {
                arguments.push(self.expression(nesting + 1)?);
                if !self.symbol(b',') {
                    break;
                }
                self.advance();
            }
        }
        self.require(b')', "expected ')' to close a call")?;
        let children = arguments.clone();
        self.push(
            NodeKind::Call {
                name: token.span,
                arguments,
            },
            start,
            start,
            &children,
        )
    }
    /// Only doubled quotes are decoded; backslashes and scalars stay exact.
    fn string(&self, token: Token) -> String {
        self.text[token.span.start + 1..token.span.end - 1].replace("''", "'")
    }
}
