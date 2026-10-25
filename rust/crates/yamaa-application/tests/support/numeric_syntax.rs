//! Private typed observations of numeric text, read as text in an expression
//! field, for independent differential fixtures.
use super::syntax_spans as spans;
use yamaa_adapters::json;
use yamaa_core::expression::{CompiledExpression, Context, NodeKind, TemporalKind};
use yamaa_core::json::Value;
use yamaa_core::numeric::{BinaryOperator, UnaryOperator};

pub fn observe_with_spans(expression: &str) -> Value {
    let mut outcome = observe(expression);
    match spans::compiled(Context::Derivation, expression) {
        Ok(compiled) => annotate(
            &compiled,
            compiled.syntax().root(),
            &mut outcome["outcome"]["ast"],
        ),
        Err(error) => {
            if let Some(name) = spans::function_name(&error) {
                outcome["outcome"]["span"] = spans::span(expression, name);
            }
        }
    }
    outcome
}

pub(super) fn annotate(compiled: &CompiledExpression, id: usize, value: &mut Value) {
    let syntax = compiled.syntax();
    let node = &syntax.nodes()[id];
    let expression = syntax.expression();
    match &node.kind {
        NodeKind::Unary { operand, .. } => {
            annotate(compiled, *operand, &mut value["value"]);
            value["operator_span"] = spans::operator(
                expression,
                node.span.start,
                syntax.nodes()[*operand].span.start,
            );
        }
        NodeKind::Binary { left, right, .. } => {
            annotate(compiled, *left, &mut value["left"]);
            annotate(compiled, *right, &mut value["right"]);
            value["operator_span"] = spans::operator(
                expression,
                syntax.nodes()[*left].span.end,
                syntax.nodes()[*right].span.start,
            );
        }
        NodeKind::Call { name, arguments } => {
            value["name"] = json!(syntax.slice(*name));
            value["name_span"] = spans::span(expression, *name);
            for (index, child) in arguments.iter().enumerate() {
                annotate(compiled, *child, &mut value["arguments"][index]);
            }
        }
        NodeKind::Reduction { name, operand, .. } => {
            value["name"] = json!(syntax.slice(*name));
            value["name_span"] = spans::span(expression, *name);
            annotate(compiled, *operand, &mut value["argument"]);
        }
        NodeKind::Group(operand) => annotate(compiled, *operand, value),
        _ => {}
    }
    value["span"] = spans::span(expression, node.span);
}

pub fn observe(expression: &str) -> Value {
    let outcome = match spans::compiled(Context::Derivation, expression) {
        Ok(compiled) => json!({"status":"parsed", "ast": ast(&compiled, compiled.syntax().root()),
                "identifiers":compiled.identifiers().collect::<Vec<_>>() }),
        Err(error) => spans::refusal(&error, false),
    };
    json!({"outcome":outcome})
}

/// Serialize each tree occurrence once, eliding groups without changing association.
/// Numbers keep their written spelling, a sign written before one included.
pub(super) fn ast(compiled: &CompiledExpression, id: usize) -> Value {
    let syntax = compiled.syntax();
    let node = &syntax.nodes()[id];
    let written = syntax.text(id);
    match &node.kind {
        NodeKind::Number { fractional, text } => {
            json!({"kind":"number","type":if *fractional {"float"} else {"int"},"value":text})
        }
        NodeKind::Null => json!({"kind":"null"}),
        // An expression field admits quoted text and temporal literals.
        NodeKind::String(text) => json!({"kind":"string","value":text}),
        NodeKind::Temporal { kind, value } => {
            json!({"kind":"temporal","type":match kind {TemporalKind::Date=>"date",TemporalKind::DateTime=>"datetime"},"value":value})
        }
        NodeKind::Identifier => json!({"kind":"identifier","name":written}),
        NodeKind::Star => json!({"kind":"star","dataset":&written[..written.len()-2]}),
        NodeKind::Unary { operator, operand } => {
            json!({"kind":"unary","operator":match operator {UnaryOperator::Plus=>"+",UnaryOperator::Negate=>"-",UnaryOperator::Abs=>unreachable!("ABS parses as a call")},"value":ast(compiled,*operand)})
        }
        NodeKind::Binary {
            operator,
            left,
            right,
        } => {
            json!({"kind":"binary","operator":match operator {BinaryOperator::Add=>"+",BinaryOperator::Subtract=>"-",BinaryOperator::Multiply=>"*",BinaryOperator::Divide=>"/",BinaryOperator::Modulo=>unreachable!("MOD parses as a call")},"left":ast(compiled,*left),"right":ast(compiled,*right)})
        }
        NodeKind::Call { name, arguments } => {
            json!({"kind":"call","name":syntax.slice(*name).to_ascii_uppercase(),"arguments":arguments.iter().map(|&id|ast(compiled,id)).collect::<Vec<_>>()})
        }
        NodeKind::Reduction {
            reducer, operand, ..
        } => {
            json!({"kind":"reduction","name":reducer.name(),"argument":ast(compiled,*operand),"text":written})
        }
        NodeKind::Group(operand) => ast(compiled, *operand),
        other => unreachable!("an expression field or an aggregate admits no {other:?}"),
    }
}
