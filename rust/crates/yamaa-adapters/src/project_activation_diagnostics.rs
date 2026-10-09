//! Bounded versionless issues from retained binding and conformance failures.
//! Project code is never retried and unclassified host payloads remain borrowed.
use crate::{issue_rows::Issue, project_function_diagnostics as invocation};
use serde_json::{json, Map, Value as Json};
use std::collections::BTreeSet;
use yamaa_core::{
    project_function::Function, project_function_result::results_match, value::Value,
};
use yamaa_engine::{
    function_invocation::FailureKind,
    project_activation::{BindingFailure, TestFailure},
};

pub use invocation::{Error, HostDetails};

/// Definitions and authored paths are in the engine's selected activation order.
pub struct DefinitionSource<'a> {
    pub function: &'a Function,
    pub path: &'a str,
}
struct Budget(usize);
impl Budget {
    fn text<'a, E>(&mut self, text: &str) -> Result<(), Error<'a, E>> {
        let amount = text
            .len()
            .checked_mul(12)
            .and_then(|n| n.checked_add(128))
            .ok_or(Error::Limit)?;
        self.0 = self.0.checked_sub(amount).ok_or(Error::Limit)?;
        Ok(())
    }
    fn value<'a, E>(&mut self, value: &Value) -> Result<(), Error<'a, E>> {
        self.text(match value {
            Value::Str(text) => text,
            _ => "01234567890123456789012345678901",
        })
    }
    fn host<'a, E>(&mut self, host: Option<HostDetails<'_>>) -> Result<(), Error<'a, E>> {
        let Some(host) = host else { return Ok(()) };
        let fields = match host {
            HostDetails::Exception { class, message, .. } => [Some(class), Some(message)],
            HostDetails::Rejected { reason, returned } => [Some(reason), returned],
        };
        for text in fields.into_iter().flatten() {
            if text.len() > crate::function_transport::MAX_ERROR_BYTES {
                return Err(Error::Limit);
            }
            self.text(text)?;
        }
        Ok(())
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
fn exact(left: &Value, right: &Value) -> bool {
    match (left, right) {
        (Value::Float(a), Value::Float(b)) => a.get().to_bits() == b.get().to_bits(),
        _ => std::mem::discriminant(left) == std::mem::discriminant(right) && left == right,
    }
}
fn selection<'a, E>(
    definitions: &[DefinitionSource<'_>],
    budget: &mut Budget,
) -> Result<(), Error<'a, E>> {
    if definitions.len() > 1024 {
        return Err(Error::Limit);
    }
    let mut names = BTreeSet::new();
    for source in definitions {
        let definition = source.function.definition();
        for text in [source.path, &definition.name, &definition.function] {
            budget.text(text)?;
        }
        if source.path.is_empty() || !names.insert(definition.name.as_str()) {
            return Err(Error::InvalidContext);
        }
    }
    Ok(())
}
fn finish<'a, E>(issues: Vec<Issue>, maximum: usize) -> Result<Vec<Issue>, Error<'a, E>> {
    if !crate::issue_rows::within_limit(&issues, maximum) {
        return Err(Error::Limit);
    }
    Ok(issues)
}
fn host_context(context: &mut Map<String, Json>, host: HostDetails<'_>) {
    match host {
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
pub fn binding_issues<'a, E>(
    definitions: &[DefinitionSource<'_>],
    failures: &'a [BindingFailure<E>],
    details: &[Option<HostDetails<'_>>],
) -> Result<Vec<Issue>, Error<'a, E>> {
    if failures.len() > 1024 {
        return Err(Error::Limit);
    }
    if details.len() != failures.len() {
        return Err(Error::InvalidContext);
    }
    if failures.is_empty() {
        return Ok(Vec::new());
    }
    let maximum = crate::specification_check::MAX_ISSUE_BYTES;
    let mut budget = Budget(maximum);
    selection(definitions, &mut budget)?;
    let mut seen = BTreeSet::new();
    for (failure, host) in failures.iter().zip(details) {
        let source = definitions
            .get(failure.function)
            .ok_or(Error::InvalidContext)?;
        if !seen.insert(failure.function) {
            return Err(Error::InvalidContext);
        }
        if host.is_none() {
            return Err(Error::Opaque(&failure.error));
        }
        for text in [
            source.path,
            &source.function.definition().name,
            &source.function.definition().function,
        ] {
            budget.text(text)?;
        }
        budget.host(*host)?;
    }
    let mut issues = Vec::with_capacity(failures.len());
    for (failure, host) in failures.iter().zip(details) {
        let source = &definitions[failure.function];
        let definition = source.function.definition();
        let mut context = Map::new();
        context.insert("function".into(), json!(definition.name));
        context.insert("call".into(), json!(definition.function));
        host_context(&mut context, host.expect("preflighted ordinary facts"));
        issues.push(Issue::from_diagnostic(json!({
            "phase":"validation", "condition":"project_environment_invalid", "requirement":"REQ-0695",
            "spec_paths":[format!("{}.function",source.path)], "context":context,
        })).map_err(|_| Error::InvalidContext)?);
    }
    finish(issues, maximum)
}
pub fn test_issues<'a, E>(
    definitions: &[DefinitionSource<'_>],
    failures: &'a [TestFailure<E>],
    details: &[Option<HostDetails<'_>>],
) -> Result<Vec<Issue>, Error<'a, E>> {
    test_issues_with_limit(
        definitions,
        failures,
        details,
        crate::specification_check::MAX_ISSUE_BYTES,
    )
}
pub fn test_issues_with_limit<'a, E>(
    definitions: &[DefinitionSource<'_>],
    failures: &'a [TestFailure<E>],
    details: &[Option<HostDetails<'_>>],
    maximum: usize,
) -> Result<Vec<Issue>, Error<'a, E>> {
    if failures.len() > 65_536 {
        return Err(Error::Limit);
    }
    if details.len() != failures.len() {
        return Err(Error::InvalidContext);
    }
    if failures.is_empty() {
        return finish(Vec::new(), maximum);
    }
    let mut budget = Budget(maximum);
    selection(definitions, &mut budget)?;
    let mut seen = BTreeSet::new();
    for (failure, host) in failures.iter().zip(details) {
        let (function, case) = match failure {
            TestFailure::Invocation { function, case, .. }
            | TestFailure::Result { function, case, .. } => (*function, *case),
        };
        let source = definitions.get(function).ok_or(Error::InvalidContext)?;
        let definition = source.function.definition();
        let test = definition.tests.get(case).ok_or(Error::InvalidContext)?;
        if !seen.insert((function, case)) {
            return Err(Error::InvalidContext);
        }
        for text in [
            source.path,
            &definition.name,
            &definition.function,
            &test.id,
        ] {
            budget.text(text)?;
        }
        if test.args.len() > 65_536 {
            return Err(Error::Limit);
        }
        for (name, value) in &test.args {
            budget.text(name)?;
            budget.value(value)?;
        }
        budget.value(&test.result)?;
        budget.host(*host)?;
        match failure {
            TestFailure::Invocation { error, .. } => {
                for text in [&error.identity.name, &error.identity.call] {
                    budget.text(text)?;
                }
                if error.identity.name != definition.name
                    || error.identity.call != definition.function
                {
                    return Err(Error::InvalidContext);
                }
                // Inspect borrowed facts only: finish the whole aggregate budget
                // before constructing the first projected invocation or issue.
                match (&error.kind, host) {
                    (
                        FailureKind::CallFailed(payload) | FailureKind::InvalidHostResult(payload),
                        None,
                    ) => return Err(Error::Opaque(payload)),
                    (FailureKind::CallFailed(_) | FailureKind::InvalidHostResult(_), Some(_)) => (),
                    (_, Some(_)) => return Err(Error::InvalidContext),
                    (FailureKind::UnknownArguments(names), None) => {
                        if names.len() > 65_536 {
                            return Err(Error::Limit);
                        }
                        for name in names {
                            budget.text(name)?;
                        }
                    }
                    (
                        FailureKind::MissingRequired { parameter }
                        | FailureKind::ArgumentType { parameter, .. },
                        None,
                    ) => budget.text(parameter)?,
                    _ => (),
                }
            }
            TestFailure::Result {
                actual, expected, ..
            } => {
                budget.value(actual)?;
                budget.value(expected)?;
                if host.is_some()
                    || !exact(expected, &test.result)
                    || results_match(actual, expected, definition.comparison_decimals)
                {
                    return Err(Error::InvalidContext);
                }
            }
        }
    }
    let mut issues = Vec::with_capacity(failures.len());
    for (failure, host) in failures.iter().zip(details) {
        let (function, case) = match failure {
            TestFailure::Invocation { function, case, .. }
            | TestFailure::Result { function, case, .. } => (*function, *case),
        };
        let source = &definitions[function];
        let definition = source.function.definition();
        let test = &definition.tests[case];
        let path = format!("{}.tests[{case}]", source.path);
        let args: Map<_, _> = test
            .args
            .iter()
            .map(|(name, value)| (name.clone(), plain(value)))
            .collect();
        let mut context = json!({"function":definition.name,"call":definition.function,"case":test.id,"args":args,"expected":plain(&test.result),"comparison_decimals":definition.comparison_decimals});
        match failure {
            TestFailure::Invocation { error, .. } => {
                let held = invocation::invocation(&path, error, None, &[], *host)?;
                context["reason"] = json!("the case failed before it produced a result");
                context["failure"] = json!({"phase":held.phase,"condition":held.condition,"requirement":held.requirement,"context":serde_json::from_str::<Json>(&held.context).map_err(|_| Error::InvalidContext)?});
            }
            TestFailure::Result { actual, .. } => {
                context["actual"] = plain(actual);
                context["reason"] = json!("the case produced a different result");
            }
        }
        issues.push(Issue::from_diagnostic(json!({"phase":"validation","condition":"function_conformance_failed","requirement":"REQ-0703","spec_paths":[path],"context":context})).map_err(|_|Error::InvalidContext)?);
    }
    finish(issues, maximum)
}
