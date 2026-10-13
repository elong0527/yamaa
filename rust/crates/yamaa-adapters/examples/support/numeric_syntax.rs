//! Private typed parser observations for independent differential fixtures.
use serde_json::{json, Value};
use yamaa_core::{
    numeric::{BinaryOperator, UnaryOperator},
    numeric_parser::{
        parse_numeric, GrammarFailure, ParseError, ParseLimits, ParseResource, ParsedKind,
        ParsedNumeric,
    },
};
pub fn observe(expression: &str) -> Value {
    let outcome = match parse_numeric(expression, ParseLimits::default()) {
        Ok(parsed) => json!({"status":"parsed", "ast": ast(&parsed, parsed.root()),
                "identifiers":parsed.identifiers().collect::<Vec<_>>() }),
        Err(ParseError::Grammar { position, failure }) => {
            let context = match &failure {
                GrammarFailure::InvalidExpression => json!({}),
                GrammarFailure::ProhibitedConstruct { construct } => json!({"construct":construct}),
                GrammarFailure::ProhibitedFunction {
                    name,
                    argument_count,
                } => {
                    let mut context = json!({"function": &expression[name.start..name.end]});
                    if let Some(count) = argument_count {
                        context["argument_count"] = json!(count);
                    }
                    context
                }
            };
            json!({"status":"invalid", "condition":failure.condition(),"requirement":failure.requirement(),
                    "position":{"byte":position.byte,"character":position.character},"context":context})
        }
        Err(ParseError::Limit {
            position,
            resource,
            limit,
        }) => {
            let resource = match resource {
                ParseResource::Bytes => "bytes",
                ParseResource::Tokens => "tokens",
                ParseResource::Nodes => "nodes",
                ParseResource::Depth => "depth",
            };
            json!({"status":"resource_limit","resource":resource,"limit":limit,
                    "position":{"byte":position.byte,"character":position.character}})
        }
    };
    json!({"outcome":outcome})
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
