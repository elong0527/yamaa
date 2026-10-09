//! Project retained versionless invocation facts without consuming host failures.
//! Hosts classify ordinary errors explicitly; interrupts and boundaries stay opaque.
use crate::issue_rows::Issue;
use serde_json::{json, Map, Value as Json};
use std::collections::BTreeSet;
use yamaa_core::{
    function_signature::ProjectFunctionIdentity,
    value::{ColumnType, Value, ValueType},
};
use yamaa_engine::{
    dataset::RowIdentity,
    function_invocation::{FailureKind, InvocationFailure},
};

/// Supply these facts only for an already classified ordinary host failure.
/// Reading or formatting an opaque payload is the host's responsibility.
#[derive(Clone, Copy, Debug)]
pub enum HostDetails<'a> {
    Exception {
        class: &'a str,
        message: &'a str,
        truncated: bool,
    },
    Rejected {
        reason: &'a str,
        returned: Option<&'a str>,
    },
}
#[derive(Debug)]
pub enum Error<'a, E> {
    Opaque(&'a E),
    InvalidContext,
    Limit,
}
struct Budget(usize);
impl Budget {
    fn text<'a, E>(&mut self, text: &str) -> Result<(), Error<'a, E>> {
        // Context is JSON text inside the issue envelope. Bound both escaping
        // layers and field overhead before allocating any authored text copies.
        let charge = text
            .len()
            .checked_mul(12)
            .and_then(|n| n.checked_add(128))
            .ok_or(Error::Limit)?;
        self.0 = self.0.checked_sub(charge).ok_or(Error::Limit)?;
        Ok(())
    }
}
fn kind(value: ValueType) -> &'static str {
    match value {
        ValueType::Str => "str",
        ValueType::Int => "int",
        ValueType::Float => "float",
        ValueType::Bool => "bool",
        ValueType::Date => "date",
        ValueType::DateTime => "datetime",
    }
}
fn column(value: ColumnType) -> &'static str {
    match value {
        ColumnType::Str => "str",
        ColumnType::Int => "int",
        ColumnType::Float => "float",
        ColumnType::Date => "date",
        ColumnType::DateTime => "datetime",
    }
}
fn plain(value: &Value) -> Json {
    match value {
        Value::Missing => Json::Null,
        Value::Str(v) => json!(v),
        Value::Int(v) => json!(v),
        Value::Float(v) => json!(v.get()),
        Value::Bool(v) => json!(v),
        Value::Date(v) => json!(v.to_string()),
        Value::DateTime(v) => json!(v.to_string()),
    }
}

/// Preserve the original failure, identity and row. No callback is invoked, no
/// error is retried, and unavailable host facts return the same borrowed payload.
pub fn invocation<'a, E>(
    path: &str,
    failure: &'a InvocationFailure<E, ProjectFunctionIdentity>,
    row: Option<&RowIdentity>,
    key_names: &[&str],
    host: Option<HostDetails<'_>>,
) -> Result<Issue, Error<'a, E>> {
    match (&failure.kind, host) {
        (FailureKind::CallFailed(e) | FailureKind::InvalidHostResult(e), None) => {
            return Err(Error::Opaque(e));
        }
        (FailureKind::CallFailed(_) | FailureKind::InvalidHostResult(_), Some(_)) => (),
        (_, Some(_)) => return Err(Error::InvalidContext),
        _ => (),
    }
    if path.is_empty() || failure.identity.name.is_empty() || failure.identity.call.is_empty() {
        return Err(Error::InvalidContext);
    }
    if key_names.len() > 65_536 {
        return Err(Error::Limit);
    }
    let mut budget = Budget(crate::specification_check::MAX_ISSUE_BYTES);
    for text in [path, &failure.identity.name, &failure.identity.call] {
        budget.text(text)?;
    }
    if let Some(row) = row {
        if row.values.len() != key_names.len() {
            return Err(Error::InvalidContext);
        }
        let mut seen = BTreeSet::new();
        for (name, value) in key_names.iter().zip(&row.values) {
            if name.is_empty() || !seen.insert(*name) {
                return Err(Error::InvalidContext);
            }
            budget.text(name)?;
            budget.text(match value {
                Value::Str(text) => text,
                _ => "01234567890123456789012345678901",
            })?;
        }
    }
    match &failure.kind {
        FailureKind::UnknownArguments(names) => {
            if names.len() > 65_536 {
                return Err(Error::Limit);
            }
            for name in names {
                budget.text(name)?;
            }
        }
        FailureKind::MissingRequired { parameter }
        | FailureKind::ArgumentType { parameter, .. } => budget.text(parameter)?,
        _ => (),
    }
    if let Some(details) = host {
        let texts = match details {
            HostDetails::Exception { class, message, .. } => [Some(class), Some(message)],
            HostDetails::Rejected { reason, returned } => [Some(reason), returned],
        };
        for text in texts.into_iter().flatten() {
            if text.len() > crate::function_transport::MAX_ERROR_BYTES {
                return Err(Error::Limit);
            }
            budget.text(text)?;
        }
    }
    let mut context = Map::new();
    context.insert("function".into(), json!(failure.identity.name));
    context.insert("call".into(), json!(failure.identity.call));
    match &failure.kind {
        FailureKind::UnknownArguments(names) => {
            context.insert("unknown".into(), json!(names));
        }
        FailureKind::MissingRequired { parameter } => {
            context.insert("parameter".into(), json!(parameter));
            context.insert(
                "reason".into(),
                json!("a required argument was not supplied"),
            );
        }
        FailureKind::ArgumentType {
            parameter,
            expected,
            actual,
        } => {
            context.insert("parameter".into(), json!(parameter));
            context.insert("expected".into(), json!(kind(*expected)));
            context.insert("actual".into(), json!(kind(*actual)));
        }
        FailureKind::CallFailed(_) | FailureKind::InvalidHostResult(_) => {
            match host.expect("classified above") {
                HostDetails::Exception {
                    class,
                    message,
                    truncated,
                } => {
                    context.insert("host_error".into(), json!(class));
                    context.insert("host_message".into(), json!(message));
                    if truncated {
                        context.insert("host_details_truncated".into(), json!(true));
                    }
                }
                HostDetails::Rejected { reason, returned } => {
                    context.insert("reason".into(), json!(reason));
                    if let Some(returned) = returned {
                        context.insert("returned".into(), json!(returned));
                    }
                }
            }
        }
        FailureKind::BooleanResult => {
            context.insert("reason".into(), json!("a binding returned a Boolean"));
            context.insert("returned".into(), json!("bool"));
        }
        FailureKind::UndeclaredMissing => {
            context.insert(
                "reason".into(),
                json!("an invoked binding returned an undeclared missing"),
            );
        }
        FailureKind::ResultType { expected, actual } => {
            context.insert("expected".into(), json!(column(*expected)));
            context.insert("actual".into(), json!(kind(*actual)));
        }
    }
    if let Some(row) = row {
        let keys: Map<_, _> = key_names
            .iter()
            .zip(&row.values)
            .map(|(name, value)| ((*name).to_owned(), plain(value)))
            .collect();
        context.insert("keys".into(), json!([keys]));
    }
    let issue = Issue::from_diagnostic(json!({
        "phase":failure.phase(), "condition":failure.condition(),
        "requirement":failure.requirement(), "spec_paths":[path], "context":context,
    }))
    .map_err(|_| Error::InvalidContext)?;
    if !crate::issue_rows::within_limit(
        std::slice::from_ref(&issue),
        crate::specification_check::MAX_ISSUE_BYTES,
    ) {
        return Err(Error::Limit);
    }
    Ok(issue)
}
