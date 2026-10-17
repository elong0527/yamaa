//! Private typed predicate observations for independent differential fixtures.
use super::syntax_spans as spans;
use yamaa_adapters::json;
use yamaa_core::expression::{Comparison, CompiledExpression, Context, NodeKind, TemporalKind};
use yamaa_core::json::Value;
use yamaa_core::numeric::{BinaryOperator, UnaryOperator};

pub fn observe(expression: &str) -> Value {
    let outcome = match spans::compiled(Context::Predicate, expression) {
        Ok(compiled) => {
            json!({"status":"parsed","ast":ast(&compiled,compiled.syntax().root()),"identifiers":compiled.identifiers().collect::<Vec<_>>()})
        }
        Err(error) => spans::refusal(&error, true),
    };
    json!({"outcome":outcome})
}

/// Temporal spelling is syntax metadata, not a conversion or precision policy.
fn temporal_name(kind: TemporalKind) -> &'static str {
    match kind {
        TemporalKind::Date => "date",
        TemporalKind::DateTime => "datetime",
    }
}

/// Emit the existing portable AST vocabulary, retaining operand/call character sites.
/// Parser-built children are unique occurrences, not a caller-controlled DAG.
fn ast(compiled: &CompiledExpression, id: usize) -> Value {
    let syntax = compiled.syntax();
    let node = &syntax.nodes()[id];
    let text = syntax.text(id);
    let position = syntax.expression()[..node.span.start].chars().count();
    let child = |id| ast(compiled, id);
    match &node.kind {
        NodeKind::Number { fractional, text } => {
            json!({"kind":"literal","type":if *fractional {"float"} else {"int"},"value":text,"position":position})
        }
        NodeKind::String(text) => {
            json!({"kind":"literal","type":"str","value":text,"position":position})
        }
        NodeKind::Temporal { kind, value } => {
            json!({"kind":"literal","type":temporal_name(*kind),"value":value,"position":position})
        }
        NodeKind::Null => json!({"kind":"literal","type":null,"value":null,"position":position}),
        NodeKind::Identifier => json!({"kind":"identifier","name":text,"position":position}),
        NodeKind::Boolean(value) => json!({"kind":"boolean","value":value}),
        NodeKind::Group(inner) => child(*inner),
        NodeKind::Not(inner) => json!({"kind":"not","value":child(*inner)}),
        NodeKind::And(left, right) | NodeKind::Or(left, right) => {
            json!({"kind":if matches!(node.kind,NodeKind::And(..)) {"and"} else {"or"},"left":child(*left),"right":child(*right)})
        }
        NodeKind::Compare {
            operator,
            left,
            right,
        } => {
            json!({"kind":"comparison","operator":match operator {Comparison::Equal=>"=",Comparison::NotEqual=>"<>",Comparison::Less=>"<",Comparison::LessEqual=>"<=",Comparison::Greater=>">",Comparison::GreaterEqual=>">="},"left":child(*left),"right":child(*right)})
        }
        NodeKind::IsNull { value, negated } => {
            json!({"kind":"null_test","value":child(*value),"negated":negated})
        }
        NodeKind::In {
            value,
            items,
            negated,
        } => {
            json!({"kind":"in","value":child(*value),"values":items.iter().map(|&id|child(id)).collect::<Vec<_>>(),"negated":negated})
        }
        NodeKind::Between {
            value,
            lower,
            upper,
            negated,
        } => {
            json!({"kind":"between","value":child(*value),"lower":child(*lower),"upper":child(*upper),"negated":negated})
        }
        NodeKind::Like {
            value,
            pattern,
            escape,
            negated,
        } => {
            json!({"kind":"like","value":child(*value),"pattern":child(*pattern),"escape":escape.map(|c|c.to_string()),"negated":negated})
        }
        NodeKind::Call { name, arguments }
            if syntax.slice(*name).eq_ignore_ascii_case("str_contains") =>
        {
            let NodeKind::String(pattern) = &syntax.nodes()[arguments[1]].kind else {
                unreachable!("admitted string literal")
            };
            json!({"kind":"call","function":"str_contains","source":child(arguments[0]),"pattern":pattern,"position":position})
        }
        NodeKind::Call { name, arguments } => {
            json!({"kind":"call","function":syntax.slice(*name).to_ascii_uppercase(),"arguments":arguments.iter().map(|&id|child(id)).collect::<Vec<_>>(),"position":position})
        }
        NodeKind::Unary { operator, operand } => {
            json!({"kind":"unary","operator":match operator {UnaryOperator::Plus=>"+",UnaryOperator::Negate=>"-",UnaryOperator::Abs=>unreachable!("ABS parses as a call")},"value":child(*operand)})
        }
        NodeKind::Binary {
            operator,
            left,
            right,
        } => {
            json!({"kind":"binary","operator":match operator {BinaryOperator::Add=>"+",BinaryOperator::Subtract=>"-",BinaryOperator::Multiply=>"*",BinaryOperator::Divide=>"/",BinaryOperator::Modulo=>unreachable!("MOD parses as a call")},"left":child(*left),"right":child(*right)})
        }
        other => unreachable!("a predicate admits no {other:?}"),
    }
}
