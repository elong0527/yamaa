//! Bounded parsing of the closed numeric grammar, without executing expressions.
//!
//! The arena preserves literal spelling and grouping spans. Parsing accepts every
//! grammar function; acceptance does not imply evaluator support. Resource limits
//! are parser policy, separate from language conditions and lifecycle handlers.

use std::collections::BTreeSet;

use crate::numeric::{BinaryOperator, UnaryOperator};

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

/// The fixed recursion boundary is distinct from grammar validation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParseResource {
    Depth,
}

/// Grammar-owned failure context, retaining the written function name as a span.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum GrammarFailure {
    #[error("numeric parser invalid expression")]
    InvalidExpression,
    #[error("numeric parser prohibited construct: construct={construct:?}")]
    ProhibitedConstruct { construct: &'static str },
    #[error(
        "numeric parser prohibited function: name={name:?}, argument_count={argument_count:?}"
    )]
    ProhibitedFunction {
        name: SourceSpan,
        argument_count: Option<usize>,
    },
}

impl GrammarFailure {
    /// The shared condition vocabulary entry for a validation failure.
    pub fn condition(&self) -> &'static str {
        self.code().definition().condition
    }

    /// The language requirement owning this failure.
    pub fn requirement(&self) -> &'static str {
        self.code()
            .definition()
            .requirement
            .expect("numeric grammar requirement")
    }
}

/// A grammar failure or a separate resource-policy outcome. No handler is applied.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ParseError {
    #[error("numeric parser grammar: position={position:?}, failure={failure:?}")]
    Grammar {
        position: SourcePosition,
        failure: GrammarFailure,
    },
    #[error("numeric parser limit: position={position:?}, resource={resource:?}, limit={limit:?}")]
    Limit {
        position: SourcePosition,
        resource: ParseResource,
        limit: usize,
    },
}

/// Closed function vocabulary; variant presence describes syntax, not execution.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NumericFunction {
    Abs,
    Ceil,
    Floor,
    Trunc,
    Sqrt,
    Power,
    Exp,
    Ln,
    Mod,
    NullIf,
}

impl NumericFunction {
    /// Canonical spelling from the shared grammar contract.
    pub fn name(self) -> &'static str {
        match self {
            Self::Abs => "ABS",
            Self::Ceil => "CEIL",
            Self::Floor => "FLOOR",
            Self::Trunc => "TRUNC",
            Self::Sqrt => "SQRT",
            Self::Power => "POWER",
            Self::Exp => "EXP",
            Self::Ln => "LN",
            Self::Mod => "MOD",
            Self::NullIf => "NULLIF",
        }
    }

    /// Look up a case-insensitive spelling without extending the vocabulary.
    pub(crate) fn lookup(name: &str) -> Option<Self> {
        [
            Self::Abs,
            Self::Ceil,
            Self::Floor,
            Self::Trunc,
            Self::Sqrt,
            Self::Power,
            Self::Exp,
            Self::Ln,
            Self::Mod,
            Self::NullIf,
        ]
        .into_iter()
        .find(|function| name.eq_ignore_ascii_case(function.name()))
    }

    /// Check the closed fixed arities after the whole call parses.
    pub(crate) fn accepts(self, count: usize) -> bool {
        match self {
            Self::Power | Self::Mod | Self::NullIf => count == 2,
            _ => count == 1,
        }
    }
}

/// An arena node. Child indices refer to earlier nodes in the same parsed arena.
/// Number and identifier spellings are the source slice at the node's span.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ParsedKind {
    Number {
        fractional: bool,
    },
    Null,
    Identifier,
    Unary {
        operator: UnaryOperator,
        operand: usize,
    },
    Binary {
        operator: BinaryOperator,
        left: usize,
        right: usize,
    },
    Call {
        function: NumericFunction,
        name: SourceSpan,
        arguments: Vec<usize>,
    },
    Group {
        operand: usize,
    },
}

/// A syntax node with its exact source extent, including grouping parentheses.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParsedNode {
    pub span: SourceSpan,
    pub kind: ParsedKind,
    depth: usize,
}

/// Immutable, validated arena with bounded depth and nonrecursive destruction.
/// Literals remain text: range errors belong to evaluation in written order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParsedNumeric {
    expression: String,
    nodes: Vec<ParsedNode>,
    root: usize,
    identifiers: Vec<usize>,
}

impl ParsedNumeric {
    /// Project an already bounded R013 subtree into the shared numeric syntax.
    /// Reductions become opaque resolver names at their original source spans.
    /// This private path preserves arithmetic association and never reparses text.
    pub(crate) fn aggregate_subtree(
        parsed: &crate::aggregate_parser::ParsedAggregate,
        root: usize,
    ) -> Self {
        use crate::aggregate_parser::ParsedKind as A;
        use std::collections::BTreeMap;
        let mut pending = std::vec![root];
        let mut selected = Vec::new();
        while let Some(id) = pending.pop() {
            selected.push(id);
            match &parsed.nodes()[id].kind {
                A::Unary { operand, .. } | A::Group { operand } => pending.push(*operand),
                A::Binary { left, right, .. } => pending.extend([*left, *right]),
                A::Call { arguments, .. } => pending.extend(arguments),
                _ => (),
            }
        }
        selected.sort_unstable();
        let mut indices: BTreeMap<usize, usize> = BTreeMap::new();
        let mut nodes: Vec<ParsedNode> = Vec::with_capacity(selected.len());
        let mut identifiers = Vec::new();
        let mut seen = BTreeSet::new();
        for id in selected {
            let node = &parsed.nodes()[id];
            let index = nodes.len();
            let mut depth = 1;
            let mut child = |id: &usize| {
                let index = indices[id];
                depth = depth.max(nodes[index].depth + 1);
                index
            };
            let kind = match &node.kind {
                A::Number { fractional } => ParsedKind::Number {
                    fractional: *fractional,
                },
                A::Null => ParsedKind::Null,
                A::Identifier | A::Reduction { .. } => ParsedKind::Identifier,
                A::Unary { operator, operand } => ParsedKind::Unary {
                    operator: *operator,
                    operand: child(operand),
                },
                A::Binary {
                    operator,
                    left,
                    right,
                } => ParsedKind::Binary {
                    operator: *operator,
                    left: child(left),
                    right: child(right),
                },
                A::Call {
                    function,
                    name,
                    arguments,
                } => ParsedKind::Call {
                    function: *function,
                    name: *name,
                    arguments: arguments.iter().map(child).collect(),
                },
                A::Group { operand } => ParsedKind::Group {
                    operand: child(operand),
                },
                A::Star => unreachable!("COUNT record stars bypass numeric evaluation"),
            };
            if kind == ParsedKind::Identifier
                && seen.insert(&parsed.expression()[node.span.start..node.span.end])
            {
                identifiers.push(index);
            }
            nodes.push(ParsedNode {
                span: node.span,
                kind,
                depth,
            });
            indices.insert(id, index);
        }
        identifiers.sort_by_key(|&id| nodes[id].span.start);
        Self {
            expression: parsed.expression().into(),
            root: indices[&root],
            nodes,
            identifiers,
        }
    }

    /// Original expression, retained without normalization.
    pub fn expression(&self) -> &str {
        &self.expression
    }
    /// Nodes in postorder, with children strictly before their parent.
    pub fn nodes(&self) -> &[ParsedNode] {
        &self.nodes
    }
    /// Index of the root in `nodes()`.
    pub fn root(&self) -> usize {
        self.root
    }
    /// Distinct case-sensitive names in first-occurrence order.
    pub fn identifiers(&self) -> impl Iterator<Item = &str> {
        self.identifiers.iter().map(|&id| {
            let span = self.nodes[id].span;
            &self.expression[span.start..span.end]
        })
    }
}

/// Lex the whole expression before grammar validation and retain exact spelling.
/// The fixed semantic depth guard applies before entering recursive productions.
pub fn parse_numeric(text: &str) -> Result<ParsedNumeric, ParseError> {
    let tokens = tokenize(text, false)?;
    let mut parser = Parser {
        text,
        tokens,
        cursor: 0,
        nodes: Vec::new(),
    };
    let root = parser.expression(1)?;
    if parser.token().kind != TokenKind::End {
        return Err(grammar_error(
            text,
            parser.token().span.start,
            GrammarFailure::InvalidExpression,
        ));
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
    Ok(ParsedNumeric {
        expression: text.into(),
        nodes: parser.nodes,
        root,
        identifiers,
    })
}

/// Translate an internal byte boundary to both public coordinate systems.
fn position(text: &str, byte: usize) -> SourcePosition {
    SourcePosition {
        byte,
        character: text[..byte].chars().count(),
    }
}

/// Construct a language error without copying or normalizing the input.
fn grammar_error(text: &str, byte: usize, failure: GrammarFailure) -> ParseError {
    ParseError::Grammar {
        position: position(text, byte),
        failure,
    }
}

/// Construct a budget outcome separately from the closed grammar vocabulary.
fn limit_error(text: &str, byte: usize, resource: ParseResource, limit: usize) -> ParseError {
    ParseError::Limit {
        position: position(text, byte),
        resource,
        limit,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TokenKind {
    QualifiedStar,
    Number,
    Name,
    Symbol(u8),
    End,
}
#[derive(Clone, Copy, Debug)]
pub(crate) struct Token {
    pub(crate) kind: TokenKind,
    pub(crate) span: SourceSpan,
}

/// ASCII name start from the shared predicate/name production.
fn name_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

/// Consume one ASCII name; callers have already checked its first byte.
fn name_end(bytes: &[u8], mut end: usize) -> usize {
    while end < bytes.len() && (name_start(bytes[end]) || bytes[end].is_ascii_digit()) {
        end += 1;
    }
    end
}

/// Reserved unqualified names map to the reference's construct context.
fn prohibited(name: &str) -> Option<&'static str> {
    for (words, construct) in [
        (&["AND", "FALSE", "NOT", "OR", "TRUE"][..], "boolean"),
        (&["BETWEEN", "IN", "IS", "LIKE"][..], "comparison"),
        (&["CASE", "ELSE", "END", "THEN", "WHEN"][..], "conditional"),
        (&["OVER"][..], "window"),
    ] {
        if words.iter().any(|word| name.eq_ignore_ascii_case(word)) {
            return Some(construct);
        }
    }
    None
}

/// Greedy number token; a fractional/exponent suffix requires at least one digit.
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

/// Tokenize all input before applying the closed grammar.
pub(crate) fn tokenize(text: &str, aggregate: bool) -> Result<Vec<Token>, ParseError> {
    let bytes = text.as_bytes();
    let mut tokens = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        let character = text[index..]
            .chars()
            .next()
            .expect("valid character boundary");
        // Python str.isspace also includes the four ASCII information separators.
        if character.is_whitespace() || matches!(character, '\u{1c}'..='\u{1f}') {
            index += character.len_utf8();
            continue;
        }

        let start = index;
        let kind = if bytes[index].is_ascii_digit() {
            index = number_end(bytes, index);
            TokenKind::Number
        } else if name_start(bytes[index]) {
            index = name_end(bytes, index);
            let qualified_star =
                aggregate && bytes.get(index) == Some(&b'.') && bytes.get(index + 1) == Some(&b'*');
            if qualified_star {
                index += 2;
            } else if bytes.get(index) == Some(&b'.') {
                if !bytes.get(index + 1).is_some_and(|&byte| name_start(byte)) {
                    return Err(grammar_error(
                        text,
                        start,
                        GrammarFailure::InvalidExpression,
                    ));
                }
                index = name_end(bytes, index + 1);
            }
            if let Some(construct) = prohibited(&text[start..index]) {
                return Err(grammar_error(
                    text,
                    start,
                    GrammarFailure::ProhibitedConstruct { construct },
                ));
            }
            if qualified_star {
                TokenKind::QualifiedStar
            } else {
                TokenKind::Name
            }
        } else if b"+-*/(),".contains(&bytes[index]) {
            index += 1;
            TokenKind::Symbol(bytes[start])
        } else {
            let failure = match character {
                '<' | '>' | '=' | '!' => GrammarFailure::ProhibitedConstruct {
                    construct: "comparison",
                },
                '\'' => GrammarFailure::ProhibitedConstruct {
                    construct: "string",
                },
                _ => GrammarFailure::InvalidExpression,
            };
            return Err(grammar_error(text, start, failure));
        };
        tokens.push(Token {
            kind,
            span: SourceSpan { start, end: index },
        });
    }
    tokens.push(Token {
        kind: TokenKind::End,
        span: SourceSpan {
            start: index,
            end: index,
        },
    });
    Ok(tokens)
}

struct Parser<'a> {
    text: &'a str,
    tokens: Vec<Token>,
    cursor: usize,
    nodes: Vec<ParsedNode>,
}

impl Parser<'_> {
    /// Peek without consuming EOF, which always remains in bounds.
    fn token(&self) -> Token {
        self.tokens[self.cursor]
    }
    /// Consume an already-checked non-EOF token.
    fn advance(&mut self) -> Token {
        let token = self.token();
        self.cursor += 1;
        token
    }
    /// Require and consume a punctuation token.
    fn expect(&mut self, symbol: u8) -> Result<Token, ParseError> {
        if self.token().kind == TokenKind::Symbol(symbol) {
            Ok(self.advance())
        } else {
            Err(grammar_error(
                self.text,
                self.token().span.start,
                GrammarFailure::InvalidExpression,
            ))
        }
    }
    /// Admit expression depth before retaining the node.
    fn push(
        &mut self,
        span: SourceSpan,
        kind: ParsedKind,
        depth: usize,
    ) -> Result<usize, ParseError> {
        if depth > crate::limits::Limits::SEMANTIC_DEPTH {
            return Err(limit_error(
                self.text,
                span.start,
                ParseResource::Depth,
                crate::limits::Limits::SEMANTIC_DEPTH,
            ));
        }

        let id = self.nodes.len();
        self.nodes.push(ParsedNode { span, kind, depth });
        Ok(id)
    }
    /// Parse the additive level and guard recursive descent through groups/calls.
    fn expression(&mut self, nesting: usize) -> Result<usize, ParseError> {
        if nesting > crate::limits::Limits::SEMANTIC_DEPTH {
            return Err(limit_error(
                self.text,
                self.token().span.start,
                ParseResource::Depth,
                crate::limits::Limits::SEMANTIC_DEPTH,
            ));
        }
        self.binary(nesting, false)
    }
    /// Parse either binary precedence level without changing written association.
    fn binary(&mut self, nesting: usize, product: bool) -> Result<usize, ParseError> {
        let mut left = if product {
            self.factor(nesting)?
        } else {
            self.binary(nesting, true)?
        };
        loop {
            let operator = match (product, self.token().kind) {
                (false, TokenKind::Symbol(b'+')) => BinaryOperator::Add,
                (false, TokenKind::Symbol(b'-')) => BinaryOperator::Subtract,
                (true, TokenKind::Symbol(b'*')) => BinaryOperator::Multiply,
                (true, TokenKind::Symbol(b'/')) => BinaryOperator::Divide,
                _ => break,
            };
            self.advance();
            let right = if product {
                self.factor(nesting)?
            } else {
                self.binary(nesting, true)?
            };
            let span = SourceSpan {
                start: self.nodes[left].span.start,
                end: self.nodes[right].span.end,
            };
            let depth = 1 + self.nodes[left].depth.max(self.nodes[right].depth);
            left = self.push(
                span,
                ParsedKind::Binary {
                    operator,
                    left,
                    right,
                },
                depth,
            )?;
        }
        Ok(left)
    }
    /// Permit at most one sign before a primary, as required by REQ-0414.
    fn factor(&mut self, nesting: usize) -> Result<usize, ParseError> {
        let operator = match self.token().kind {
            TokenKind::Symbol(b'+') => UnaryOperator::Plus,
            TokenKind::Symbol(b'-') => UnaryOperator::Negate,
            _ => return self.primary(nesting),
        };
        let start = self.advance().span.start;
        let operand = self.primary(nesting)?;
        let span = SourceSpan {
            start,
            end: self.nodes[operand].span.end,
        };
        self.push(
            span,
            ParsedKind::Unary { operator, operand },
            self.nodes[operand].depth + 1,
        )
    }
    /// Parse leaves, calls and groups without converting any literal to a value.
    fn primary(&mut self, nesting: usize) -> Result<usize, ParseError> {
        let token = self.token();
        match token.kind {
            TokenKind::Number => {
                self.advance();
                let fractional = self.text[token.span.start..token.span.end]
                    .bytes()
                    .any(|b| matches!(b, b'.' | b'e' | b'E'));
                self.push(token.span, ParsedKind::Number { fractional }, 1)
            }
            TokenKind::Name => {
                self.advance();
                if self.token().kind == TokenKind::Symbol(b'(') {
                    return self.call(token, nesting);
                }
                let kind =
                    if self.text[token.span.start..token.span.end].eq_ignore_ascii_case("NULL") {
                        ParsedKind::Null
                    } else {
                        ParsedKind::Identifier
                    };
                self.push(token.span, kind, 1)
            }
            TokenKind::Symbol(b'(') => {
                self.advance();
                let operand = self.expression(nesting + 1)?;
                let end = self.expect(b')')?.span.end;
                self.push(
                    SourceSpan {
                        start: token.span.start,
                        end,
                    },
                    ParsedKind::Group { operand },
                    self.nodes[operand].depth + 1,
                )
            }
            _ => Err(grammar_error(
                self.text,
                token.span.start,
                GrammarFailure::InvalidExpression,
            )),
        }
    }
    /// Parse the full call before reporting a prohibited name or invalid arity.
    fn call(&mut self, name: Token, nesting: usize) -> Result<usize, ParseError> {
        self.expect(b'(')?;
        let mut arguments = Vec::new();
        let mut depth = 1;
        if self.token().kind != TokenKind::Symbol(b')') {
            loop {
                let argument = self.expression(nesting + 1)?;
                depth = depth.max(self.nodes[argument].depth + 1);
                arguments.push(argument);
                if self.token().kind != TokenKind::Symbol(b',') {
                    break;
                }
                self.advance();
            }
        }
        let end = self.expect(b')')?.span.end;
        let function = NumericFunction::lookup(&self.text[name.span.start..name.span.end]);
        let argument_count = function.map(|_| arguments.len());
        let Some(function) = function.filter(|function| function.accepts(arguments.len())) else {
            return Err(grammar_error(
                self.text,
                name.span.start,
                GrammarFailure::ProhibitedFunction {
                    name: name.span,
                    argument_count,
                },
            ));
        };
        self.push(
            SourceSpan {
                start: name.span.start,
                end,
            },
            ParsedKind::Call {
                function,
                name: name.span,
                arguments,
            },
            depth,
        )
    }
}
