//! Private typed aggregate observations for independent differential fixtures.
use super::numeric_syntax::{annotate, ast};
use super::syntax_spans as spans;
use yamaa_adapters::json;
use yamaa_core::expression::Context;
use yamaa_core::json::Value;

pub fn observe_with_spans(expression: &str) -> Value {
    let mut outcome = observe(expression);
    match spans::compiled(Context::Aggregate, expression) {
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

pub fn observe(expression: &str) -> Value {
    let outcome = match spans::compiled(Context::Aggregate, expression) {
        Ok(compiled) => json!({"status":"parsed", "ast": ast(&compiled, compiled.syntax().root()),
                "identifiers":compiled.identifiers().collect::<Vec<_>>(),
                "star_datasets":compiled.record_relations().collect::<Vec<_>>(),
                "ungrouped_identifiers":compiled.ungrouped_identifiers().collect::<Vec<_>>() }),
        Err(error) => spans::refusal(&error, false),
    };
    json!({"outcome":outcome})
}
