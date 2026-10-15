//! Bounded R013 aggregate parsing; no relation selection, binding or evaluation.
//! Numeric lexemes, functions and arity rules are shared with R010. A separate
//! immutable arena keeps aggregate syntax out of the numeric compiler.

use crate::numeric::{BinaryOperator, UnaryOperator};
use crate::numeric_parser::{self, tokenize, NumericFunction, Token, TokenKind};
pub use crate::numeric_parser::{ParseLimits, ParseResource, SourcePosition, SourceSpan};
use alloc::{collections::BTreeSet, string::String, vec::Vec};

/// Closed reducer vocabulary; parsing does not advertise execution support.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reducer {
    Sum,
    Count,
    Min,
    Max,
    Mean,
    Only,
}
impl Reducer {
    /// Canonical spelling from the R013 grammar contract.
    pub fn name(self) -> &'static str {
        match self {
            Self::Sum => "SUM",
            Self::Count => "COUNT",
            Self::Min => "MIN",
            Self::Max => "MAX",
            Self::Mean => "MEAN",
            Self::Only => "ONLY",
        }
    }
    /// Recognize reducer calls without reserving identifier spellings.
    fn lookup(name: &str) -> Option<Self> {
        [
            Self::Sum,
            Self::Count,
            Self::Min,
            Self::Max,
            Self::Mean,
            Self::Only,
        ]
        .into_iter()
        .find(|reducer| name.eq_ignore_ascii_case(reducer.name()))
    }
}

/// Aggregate validation failure with only grammar-owned context.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GrammarFailure {
    InvalidExpression,
    ProhibitedConstruct {
        construct: &'static str,
    },
    ProhibitedFunction {
        name: SourceSpan,
        argument_count: Option<usize>,
    },
    NestedReduction {
        outer: Reducer,
        inner: Reducer,
    },
}
impl GrammarFailure {
    /// Stable language condition, independent of diagnostic rendering.
    pub fn condition(&self) -> &'static str {
        self.code().definition().condition
    }
    /// R013 owns aggregate diagnostics even for imported numeric productions.
    pub fn requirement(&self) -> &'static str {
        self.code()
            .definition()
            .requirement
            .expect("aggregate grammar requirement")
    }
}

/// Resource exhaustion remains distinct from language validation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ParseError {
    Grammar {
        position: SourcePosition,
        failure: GrammarFailure,
    },
    Limit {
        position: SourcePosition,
        resource: ParseResource,
        limit: usize,
    },
}
impl From<numeric_parser::ParseError> for ParseError {
    /// Translate shared lexer failures to the owning aggregate vocabulary.
    fn from(error: numeric_parser::ParseError) -> Self {
        match error {
            numeric_parser::ParseError::Limit {
                position,
                resource,
                limit,
            } => Self::Limit {
                position,
                resource,
                limit,
            },
            numeric_parser::ParseError::Grammar { position, failure } => Self::Grammar {
                position,
                failure: match failure {
                    numeric_parser::GrammarFailure::InvalidExpression => {
                        GrammarFailure::InvalidExpression
                    }
                    numeric_parser::GrammarFailure::ProhibitedConstruct { construct } => {
                        GrammarFailure::ProhibitedConstruct { construct }
                    }
                    numeric_parser::GrammarFailure::ProhibitedFunction {
                        name,
                        argument_count,
                    } => GrammarFailure::ProhibitedFunction {
                        name,
                        argument_count,
                    },
                },
            },
        }
    }
}

/// Postorder arena node; child indices always precede their parent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ParsedKind {
    Number {
        fractional: bool,
    },
    Null,
    Identifier,
    /// Qualified record star: the span includes the written `.*` suffix.
    Star,
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
    Reduction {
        reducer: Reducer,
        name: SourceSpan,
        operand: usize,
    },
    Group {
        operand: usize,
    },
}

/// Exact UTF-8 extent, retaining parentheses, case and literal spelling.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParsedNode {
    pub span: SourceSpan,
    pub kind: ParsedKind,
    depth: usize,
}

/// Parsed syntax with bounded depth and nonrecursive ownership/destruction.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParsedAggregate {
    expression: String,
    nodes: Vec<ParsedNode>,
    root: usize,
    identifiers: Vec<usize>,
    stars: Vec<usize>,
    ungrouped: Vec<usize>,
}
impl ParsedAggregate {
    /// The original input without normalization or literal conversion.
    pub fn expression(&self) -> &str {
        &self.expression
    }
    /// Immutable postorder arena; no evaluation or source access has occurred.
    pub fn nodes(&self) -> &[ParsedNode] {
        &self.nodes
    }
    /// Index of the expression root.
    pub fn root(&self) -> usize {
        self.root
    }
    /// Distinct case-sensitive identifiers in first written occurrence order.
    pub fn identifiers(&self) -> impl Iterator<Item = &str> {
        self.identifiers.iter().map(|&id| self.slice(id))
    }
    /// Distinct COUNT record relations, independent of field dependencies.
    pub fn star_datasets(&self) -> impl Iterator<Item = &str> {
        self.stars.iter().map(|&id| {
            let text = self.slice(id);
            &text[..text.len() - 2]
        })
    }
    /// References outside reductions that an enclosing group must supply.
    pub fn ungrouped_identifiers(&self) -> impl Iterator<Item = &str> {
        self.ungrouped.iter().map(|&id| self.slice(id))
    }
    /// Slice only spans created by the validated parser.
    fn slice(&self, id: usize) -> &str {
        let span = self.nodes[id].span;
        &self.expression[span.start..span.end]
    }
}

/// Parse the full closed language under explicit resource policy. Lexing finishes
/// before syntax checking. Names are not bound and numbers remain source text.
pub fn parse_aggregate(text: &str, mut limits: ParseLimits) -> Result<ParsedAggregate, ParseError> {
    limits.depth = limits.depth.min(64);
    if text.len() > limits.bytes {
        return Err(limit_error(text, 0, ParseResource::Bytes, limits.bytes));
    }
    let tokens = tokenize(text, limits.tokens, true)?;
    let mut parser = Parser {
        text,
        tokens,
        cursor: 0,
        nodes: Vec::new(),
        limits,
    };
    let root = parser.expression(1)?;
    if parser.token().kind != TokenKind::End {
        return Err(grammar_error(
            text,
            parser.token().span.start,
            GrammarFailure::InvalidExpression,
        ));
    }
    let mut identifiers = Vec::new();
    let mut stars = Vec::new();
    let mut ungrouped = Vec::new();
    let mut seen = BTreeSet::new();
    let mut seen_stars = BTreeSet::new();
    let mut seen_ungrouped = BTreeSet::new();
    let mut pending = alloc::vec![(root, false)];
    while let Some((id, reduced)) = pending.pop() {
        let node = &parser.nodes[id];
        let spelling = &text[node.span.start..node.span.end];
        match &node.kind {
            ParsedKind::Identifier => {
                if seen.insert(spelling) {
                    identifiers.push(id);
                }
                if !reduced && seen_ungrouped.insert(spelling) {
                    ungrouped.push(id);
                }
            }
            ParsedKind::Star => {
                if seen_stars.insert(spelling) {
                    stars.push(id);
                }
            }
            ParsedKind::Reduction { operand, .. } => pending.push((*operand, true)),
            ParsedKind::Unary { operand, .. } | ParsedKind::Group { operand } => {
                pending.push((*operand, reduced))
            }
            ParsedKind::Binary { left, right, .. } => {
                pending.push((*right, reduced));
                pending.push((*left, reduced));
            }
            ParsedKind::Call { arguments, .. } => {
                pending.extend(arguments.iter().rev().map(|&id| (id, reduced)))
            }
            _ => {}
        }
    }
    Ok(ParsedAggregate {
        expression: text.into(),
        nodes: parser.nodes,
        root,
        identifiers,
        stars,
        ungrouped,
    })
}

/// Locate an input boundary in both UTF-8 bytes and Unicode scalar characters.
fn position(text: &str, byte: usize) -> SourcePosition {
    SourcePosition {
        byte,
        character: text[..byte].chars().count(),
    }
}
/// Build a language failure without copying the supplied expression.
fn grammar_error(text: &str, byte: usize, failure: GrammarFailure) -> ParseError {
    ParseError::Grammar {
        position: position(text, byte),
        failure,
    }
}
/// Build a separate policy outcome; handlers never consume parser budgets.
fn limit_error(text: &str, byte: usize, resource: ParseResource, limit: usize) -> ParseError {
    ParseError::Limit {
        position: position(text, byte),
        resource,
        limit,
    }
}

struct Parser<'a> {
    text: &'a str,
    tokens: Vec<Token>,
    cursor: usize,
    nodes: Vec<ParsedNode>,
    limits: ParseLimits,
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
    /// Append a node only after its depth and arena budget have been checked.
    fn push(
        &mut self,
        span: SourceSpan,
        kind: ParsedKind,
        depth: usize,
    ) -> Result<usize, ParseError> {
        if depth > self.limits.depth {
            return Err(limit_error(
                self.text,
                span.start,
                ParseResource::Depth,
                self.limits.depth,
            ));
        }
        if self.nodes.len() == self.limits.nodes {
            return Err(limit_error(
                self.text,
                span.start,
                ParseResource::Nodes,
                self.limits.nodes,
            ));
        }
        let id = self.nodes.len();
        self.nodes.push(ParsedNode { span, kind, depth });
        Ok(id)
    }
    /// Parse the additive level and guard recursive descent through groups/calls.
    fn expression(&mut self, nesting: usize) -> Result<usize, ParseError> {
        if nesting > self.limits.depth {
            return Err(limit_error(
                self.text,
                self.token().span.start,
                ParseResource::Depth,
                self.limits.depth,
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
    /// Permit at most one sign before a primary, as required by REQ-0476.
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
                    if let Some(reducer) =
                        Reducer::lookup(&self.text[token.span.start..token.span.end])
                    {
                        return self.reduction(token, reducer, nesting);
                    }
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

    /// Parse exactly one reducer operand, then reject the first nested reduction.
    fn reduction(
        &mut self,
        name: Token,
        reducer: Reducer,
        nesting: usize,
    ) -> Result<usize, ParseError> {
        self.expect(b'(')?;
        let operand = if self.token().kind == TokenKind::QualifiedStar {
            let star = self.advance();
            if reducer != Reducer::Count {
                return Err(grammar_error(
                    self.text,
                    star.span.start,
                    GrammarFailure::InvalidExpression,
                ));
            }
            self.push(star.span, ParsedKind::Star, 1)?
        } else {
            self.expression(nesting + 1)?
        };
        let end = self.expect(b')')?.span.end;
        if let Some(inner) = self.first_reduction(operand) {
            return Err(grammar_error(
                self.text,
                name.span.start,
                GrammarFailure::NestedReduction {
                    outer: reducer,
                    inner,
                },
            ));
        }
        self.push(
            SourceSpan {
                start: name.span.start,
                end,
            },
            ParsedKind::Reduction {
                reducer,
                name: name.span,
                operand,
            },
            self.nodes[operand].depth + 1,
        )
    }
    /// Find the first reducer in written preorder; the arena is depth-bounded.
    fn first_reduction(&self, id: usize) -> Option<Reducer> {
        match &self.nodes[id].kind {
            ParsedKind::Reduction { reducer, .. } => Some(*reducer),
            ParsedKind::Unary { operand, .. } | ParsedKind::Group { operand } => {
                self.first_reduction(*operand)
            }
            ParsedKind::Binary { left, right, .. } => self
                .first_reduction(*left)
                .or_else(|| self.first_reduction(*right)),
            ParsedKind::Call { arguments, .. } => {
                arguments.iter().find_map(|&id| self.first_reduction(id))
            }
            _ => None,
        }
    }
}
