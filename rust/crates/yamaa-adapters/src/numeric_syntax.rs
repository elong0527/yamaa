//! Strict numeric-syntax/1 transport. Syntax never grants execution capability.

use serde::Deserialize;
use serde_json::{json, Value};
use std::{fmt, panic::catch_unwind};
use yamaa_core::{
    numeric::{BinaryOperator, UnaryOperator},
    numeric_parser::{
        parse_numeric, GrammarFailure, ParseError, ParseLimits, ParseResource, ParsedKind,
        ParsedNumeric,
    },
};

/// Bound decoding before allocations; expression budgets are separately lower.
pub const MAX_REQUEST_BYTES: usize = 1_048_576;
const PROTOCOL: &str = "numeric-syntax/1";

/// Transport defects are distinct from grammar failures and resource outcomes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransportError {
    RequestLimit,
    InvalidRequest,
    UnsupportedProtocol,
    Internal,
}
impl fmt::Display for TransportError {
    /// Never echo user-controlled input or panic payloads in host exceptions.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::RequestLimit => "numeric syntax request exceeds byte limit",
            Self::InvalidRequest => "invalid numeric syntax request",
            Self::UnsupportedProtocol => "unsupported numeric syntax protocol",
            Self::Internal => "internal numeric syntax failure",
        })
    }
}
impl std::error::Error for TransportError {}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    protocol: String,
    expression: String,
}

/// Decode bounded metadata and return an owned AST or portable parser outcome.
/// No resolvers, host functions, data sources or evaluator policies are involved.
pub fn analyze_numeric(request: &str) -> Result<String, TransportError> {
    if request.len() > MAX_REQUEST_BYTES {
        return Err(TransportError::RequestLimit);
    }
    catch_unwind(|| {
        let request: Request = serde_json::from_str(request).map_err(|_| TransportError::InvalidRequest)?;
        if request.protocol != PROTOCOL { return Err(TransportError::UnsupportedProtocol); }
        let outcome = match parse_numeric(&request.expression, ParseLimits::default()) {
            Ok(parsed) => json!({"status":"parsed", "ast": ast(&parsed, parsed.root()),
                "identifiers":parsed.identifiers().collect::<Vec<_>>() }),
            Err(ParseError::Grammar { position, failure }) => {
                let context = match &failure {
                    GrammarFailure::InvalidExpression => json!({}),
                    GrammarFailure::ProhibitedConstruct { construct } => json!({"construct":construct}),
                    GrammarFailure::ProhibitedFunction { name, argument_count } => {
                        let mut context = json!({"function": &request.expression[name.start..name.end]});
                        if let Some(count) = argument_count { context["argument_count"] = json!(count); }
                        context
                    }
                };
                json!({"status":"invalid", "condition":failure.condition(),"requirement":failure.requirement(),
                    "position":{"byte":position.byte,"character":position.character},"context":context})
            }
            Err(ParseError::Limit { position, resource, limit }) => {
                let resource = match resource { ParseResource::Bytes=>"bytes",ParseResource::Tokens=>"tokens",ParseResource::Nodes=>"nodes",ParseResource::Depth=>"depth" };
                json!({"status":"resource_limit","resource":resource,"limit":limit,
                    "position":{"byte":position.byte,"character":position.character}})
            }
        };
        serde_json::to_string(&json!({"protocol":PROTOCOL,"outcome":outcome})).map_err(|_|TransportError::Internal)
    }).map_err(|_|TransportError::Internal)?
}

/// Serialize each tree occurrence once, eliding groups without changing association.
/// The validated arena caps recursion at 64 and retains numbers as exact strings.
fn ast(parsed: &ParsedNumeric, id: usize) -> Value {
    let node = &parsed.nodes()[id];
    let written = &parsed.expression()[node.span.start..node.span.end];
    match &node.kind {
        ParsedKind::Number { fractional } => {
            json!({"kind":"number","type":if *fractional {"float"} else {"int"},"value":written})
        }
        ParsedKind::Null => json!({"kind":"null"}),
        ParsedKind::Identifier => json!({"kind":"identifier","name":written}),
        ParsedKind::Unary { operator, operand } => {
            json!({"kind":"unary","operator":match operator {UnaryOperator::Plus=>"+",UnaryOperator::Negate=>"-",UnaryOperator::Abs=>unreachable!("ABS parses as a call")},"value":ast(parsed,*operand)})
        }
        ParsedKind::Binary {
            operator,
            left,
            right,
        } => {
            json!({"kind":"binary","operator":match operator {BinaryOperator::Add=>"+",BinaryOperator::Subtract=>"-",BinaryOperator::Multiply=>"*",BinaryOperator::Divide=>"/",BinaryOperator::Modulo=>unreachable!("MOD parses as a call")},"left":ast(parsed,*left),"right":ast(parsed,*right)})
        }
        ParsedKind::Call {
            function,
            arguments,
            ..
        } => {
            json!({"kind":"call","name":function.name(),"arguments":arguments.iter().map(|&id|ast(parsed,id)).collect::<Vec<_>>()})
        }
        ParsedKind::Group { operand } => ast(parsed, *operand),
    }
}
