//! Source-site projection for typed development observations, with no parsing.
use yamaa_adapters::json;
use yamaa_core::expression::{
    compile, CompileError, CompiledExpression, Context, Failure, Resource, SourceSpan,
};
use yamaa_core::json::Value;

pub fn span(text: &str, site: SourceSpan) -> Value {
    json!([
        text[..site.start].chars().count(),
        text[..site.end].chars().count()
    ])
}

pub fn operator(text: &str, start: usize, end: usize) -> Value {
    let offset = text[start..end]
        .char_indices()
        .find(|(_, character)| !character.is_whitespace())
        .expect("typed operator gap")
        .0;
    span(
        text,
        SourceSpan {
            start: start + offset,
            end: start + offset + 1,
        },
    )
}

/// Compile one family's text in its context; observations never bind names.
pub fn compiled(context: Context, text: &str) -> Result<CompiledExpression, CompileError> {
    compile(text, context, "columns.RESULT.derivation.expr")
}

/// The shared outcome of a refused text, in the family's established keys.
pub fn refusal(error: &CompileError, message: bool) -> Value {
    match error {
        CompileError::Invalid {
            position,
            failure,
            message: explanation,
        } => {
            let issue = failure.syntax_diagnostic();
            let context = yamaa_core::json::Value::Object(issue.context.clone());
            let mut outcome = json!({"status":"invalid","condition":failure.condition(),"rule":failure.rule(),"position":{"byte":position.byte,"character":position.character},"context":context});
            if message {
                outcome["message"] = json!(explanation);
            }
            outcome
        }
        CompileError::Limit {
            position,
            resource,
            limit,
        } => {
            let (phase, resource) = match resource {
                Resource::Depth => ("parse", "depth"),
                Resource::Regex(yamaa_core::portable_regex::Resource::Depth) => {
                    ("regex_compile", "depth")
                }
                Resource::Regex(yamaa_core::portable_regex::Resource::Size) => {
                    ("regex_compile", "size")
                }
            };
            let mut outcome = json!({"status":"resource_limit","resource":resource,"limit":limit,"position":{"byte":position.byte,"character":position.character}});
            if message {
                outcome["phase"] = json!(phase);
            }
            outcome
        }
    }
}

/// A prohibited function's written name, which starts at the finding.
pub fn function_name(error: &CompileError) -> Option<SourceSpan> {
    match error {
        CompileError::Invalid {
            position,
            failure: Failure::ProhibitedFunction { function, .. },
            ..
        } => Some(SourceSpan {
            start: position.byte,
            end: position.byte + function.len(),
        }),
        _ => None,
    }
}
