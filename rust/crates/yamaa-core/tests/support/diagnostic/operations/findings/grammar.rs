//! The refusals of an expression's text in an expression field, an
//! aggregate or a predicate (FORMULA-1, FORMULA-7, FORMULA-8, FORMULA-13,
//! FORMULA-21, FORMULA-27, VALUE-5).
use super::text;
use crate::support::diagnostic::json::Json;
use yamaa_core::{
    diagnostic::Diagnostic,
    expression::{compile, CompileError, Context},
};

pub(super) fn run(operation: &str, input: &Json) -> Option<Vec<Diagnostic>> {
    Some(match operation {
        "expression_refusal" => expression_refusal(input),
        _ => return None,
    })
}

/// The refusal of `expression` written at `path` in a `field` of the kind
/// named, `{field, expression, path}`: `derivation`, `aggregate` or
/// `predicate`.
fn expression_refusal(input: &Json) -> Vec<Diagnostic> {
    let input = input.closed(&["field", "expression", "path"]);
    let field = match text(input, "field") {
        "derivation" => Context::Derivation,
        "aggregate" => Context::Aggregate,
        "predicate" => Context::Predicate,
        other => panic!("no expression field {other:?}"),
    };
    let (expression, path) = (text(input, "expression"), text(input, "path"));
    match compile(expression, field, path) {
        Ok(_) => Vec::new(),
        Err(CompileError::Invalid {
            position, failure, ..
        }) => vec![failure.diagnostic(field, path, expression, position.character)],
        Err(other) => panic!("a refusal with no language finding: {other:?}"),
    }
}
