//! Private typed parser observations for independent differential fixtures.
use serde_json::{json, Value};
use yamaa_core::{
    aggregate_parser::{
        parse_aggregate, GrammarFailure, ParseError, ParseLimits, ParseResource, ParsedAggregate,
        ParsedKind,
    },
    numeric::{BinaryOperator, UnaryOperator},
};
pub fn observe(expression: &str) -> Value {
    let outcome = match parse_aggregate(expression, ParseLimits::default()) {
        Ok(parsed) => json!({"status":"parsed", "ast": ast(&parsed, parsed.root()),
                "identifiers":parsed.identifiers().collect::<Vec<_>>(),
                "star_datasets":parsed.star_datasets().collect::<Vec<_>>(),
                "ungrouped_identifiers":parsed.ungrouped_identifiers().collect::<Vec<_>>() }),
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
                GrammarFailure::NestedReduction { outer, inner } => {
                    json!({"outer":outer.name(),"inner":inner.name()})
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

/// Serialize each tree occurrence once, eliding groups but retaining reducer text.
/// The validated arena caps recursion at 64 and retains numbers as exact strings.
fn ast(parsed: &ParsedAggregate, id: usize) -> Value {
    let node = &parsed.nodes()[id];
    let written = &parsed.expression()[node.span.start..node.span.end];
    match &node.kind {
        ParsedKind::Number { fractional } => {
            json!({"kind":"number","type":if *fractional {"float"} else {"int"},"value":written})
        }
        ParsedKind::Null => json!({"kind":"null"}),
        ParsedKind::Identifier => json!({"kind":"identifier","name":written}),
        ParsedKind::Star => json!({"kind":"star","dataset":&written[..written.len()-2]}),
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
        ParsedKind::Reduction {
            reducer, operand, ..
        } => {
            json!({"kind":"reduction","name":reducer.name(),"argument":ast(parsed,*operand),"text":written})
        }
        ParsedKind::Group { operand } => ast(parsed, *operand),
    }
}
