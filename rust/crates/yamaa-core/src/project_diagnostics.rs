//! Borrowed facts from retained project invocation and activation failures.
//! Admit the whole report before allocating paths or copying authored text.
use crate::{
    dataset::RowIdentity,
    diagnostic::ConditionCode,
    function_invocation::{FailureKind, InvocationFailure},
    function_signature::ProjectFunctionIdentity,
    project_activation::{BindingFailure, TestFailure},
    project_function::Function,
    project_function_result::results_match,
    value::{ColumnType, Value, ValueType},
};
use alloc::{
    collections::{BTreeMap, BTreeSet},
    format,
    string::String,
    vec,
    vec::Vec,
};

pub const MAX_DIAGNOSTIC_BYTES: usize = 16_777_216;
pub const MAX_HOST_DETAIL_BYTES: usize = 8_192;
const MAX_FUNCTIONS: usize = 1_024;
const MAX_CASES: usize = 65_536;

/// Host classification supplies ordinary facts; core never reads opaque errors.
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

/// A context tree borrows scalar values and text, including nested failures.
/// Adapters encode these facts but do not choose keys, reasons or causes.
#[derive(Clone, Debug, PartialEq)]
pub enum Fact<'a> {
    Text(&'a str),
    /// A computed diagnostic label, never a copied authored scalar.
    OwnedText(String),
    Scalar(&'a Value),
    Boolean(bool),
    Integer(i64),
    Unsigned(usize),
    /// A sealed canonical integer bound, kept exact beyond the machine range.
    Bound(&'a crate::dataset::RowCountBound),
    Null,
    Sequence(Vec<Fact<'a>>),
    Object(Context<'a>),
}
pub type Context<'a> = BTreeMap<&'a str, Fact<'a>>;
#[derive(Debug, PartialEq)]
pub struct Diagnostic<'a> {
    pub code: ConditionCode,
    pub spec_path: String,
    pub context: Context<'a>,
}

pub struct DefinitionSource<'a> {
    pub function: &'a Function,
    pub path: &'a str,
}
struct Budget(usize);
impl Budget {
    fn text<'a, E>(&mut self, text: &str) -> Result<(), Error<'a, E>> {
        // Context is JSON text inside the envelope: bound both escaping layers.
        let charge = text
            .len()
            .checked_mul(12)
            .and_then(|n| n.checked_add(128))
            .ok_or(Error::Limit)?;
        self.0 = self.0.checked_sub(charge).ok_or(Error::Limit)?;
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
            if text.len() > MAX_HOST_DETAIL_BYTES {
                return Err(Error::Limit);
            }
            self.text(text)?;
        }
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
fn host_context<'a>(context: &mut Context<'a>, host: HostDetails<'a>) {
    match host {
        HostDetails::Exception {
            class,
            message,
            truncated,
        } => {
            context.insert("host_error", Fact::Text(class));
            context.insert("host_message", Fact::Text(message));
            if truncated {
                context.insert("host_details_truncated", Fact::Boolean(true));
            }
        }
        HostDetails::Rejected { reason, returned } => {
            context.insert("reason", Fact::Text(reason));
            if let Some(returned) = returned {
                context.insert("returned", Fact::Text(returned));
            }
        }
    }
}
fn invocation_admission<'a, E>(
    kind: &'a FailureKind<E>,
    host: Option<HostDetails<'_>>,
) -> Result<(), Error<'a, E>> {
    match (kind, host) {
        (FailureKind::CallFailed(e) | FailureKind::InvalidHostResult(e), None) => {
            Err(Error::Opaque(e))
        }
        (FailureKind::CallFailed(_) | FailureKind::InvalidHostResult(_), Some(_)) => Ok(()),
        (_, Some(_)) => Err(Error::InvalidContext),
        _ => Ok(()),
    }
}
fn invocation_budget<'a, E>(
    kind: &FailureKind<E>,
    budget: &mut Budget,
) -> Result<(), Error<'a, E>> {
    match kind {
        FailureKind::UnknownArguments(names) => {
            if names.len() > MAX_CASES {
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
    Ok(())
}
pub fn invocation<'a, 'e: 'a, E>(
    path: &str,
    failure: &'e InvocationFailure<E, ProjectFunctionIdentity>,
    row: Option<&'a RowIdentity>,
    key_names: &[&'a str],
    host: Option<HostDetails<'a>>,
) -> Result<Diagnostic<'a>, Error<'e, E>> {
    let row = row.map(|row| row.values.as_slice());
    // Opaque failure precedence is preserved even for malformed identity facts.
    invocation_admission(&failure.kind, host)?;
    if path.is_empty() || failure.identity.name.is_empty() || failure.identity.call.is_empty() {
        return Err(Error::InvalidContext);
    }
    if key_names.len() > MAX_CASES {
        return Err(Error::Limit);
    }
    let mut budget = Budget(MAX_DIAGNOSTIC_BYTES);
    for text in [path, &failure.identity.name, &failure.identity.call] {
        budget.text(text)?;
    }
    if let Some(row) = row {
        if row.len() != key_names.len() {
            return Err(Error::InvalidContext);
        }
        let mut seen = BTreeSet::new();
        for (name, value) in key_names.iter().zip(row) {
            if name.is_empty() || !seen.insert(*name) {
                return Err(Error::InvalidContext);
            }
            budget.text(name)?;
            budget.value(value)?;
        }
    }
    invocation_budget(&failure.kind, &mut budget)?;
    budget.host(host)?;
    let mut context: Context<'a> = [
        ("function", Fact::Text(&failure.identity.name)),
        ("call", Fact::Text(&failure.identity.call)),
    ]
    .into();
    match &failure.kind {
        FailureKind::UnknownArguments(names) => {
            context.insert(
                "unknown",
                Fact::Sequence(names.iter().map(|name| Fact::Text(name)).collect()),
            );
        }
        FailureKind::MissingRequired { parameter } => {
            context.insert("parameter", Fact::Text(parameter));
            context.insert("reason", Fact::Text("a required argument was not supplied"));
        }
        FailureKind::ArgumentType {
            parameter,
            expected,
            actual,
        } => {
            context.insert("parameter", Fact::Text(parameter));
            context.insert("expected", Fact::Text(kind(*expected)));
            context.insert("actual", Fact::Text(kind(*actual)));
        }
        FailureKind::CallFailed(_) | FailureKind::InvalidHostResult(_) => {
            host_context(&mut context, host.expect("classified above"))
        }
        FailureKind::BooleanResult => {
            context.insert("reason", Fact::Text("a binding returned a Boolean"));
            context.insert("returned", Fact::Text("bool"));
        }
        FailureKind::UndeclaredMissing => {
            context.insert(
                "reason",
                Fact::Text("an invoked binding returned an undeclared missing"),
            );
        }
        FailureKind::ResultType { expected, actual } => {
            context.insert("expected", Fact::Text(column(*expected)));
            context.insert("actual", Fact::Text(kind(*actual)));
        }
    }
    if let Some(row) = row {
        let keys = key_names
            .iter()
            .zip(row)
            .map(|(name, value)| (*name, Fact::Scalar(value)))
            .collect();
        context.insert("keys", Fact::Sequence(vec![Fact::Object(keys)]));
    }
    Ok(Diagnostic {
        code: failure.kind.code(),
        spec_path: path.into(),
        context,
    })
}

fn selection<'a, E>(
    definitions: &[DefinitionSource<'_>],
    budget: &mut Budget,
) -> Result<(), Error<'a, E>> {
    if definitions.len() > MAX_FUNCTIONS {
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
pub fn binding_diagnostics<'a, 'e, E>(
    definitions: &[DefinitionSource<'a>],
    failures: &'e [BindingFailure<E>],
    details: &[Option<HostDetails<'a>>],
    maximum: usize,
) -> Result<Vec<Diagnostic<'a>>, Error<'e, E>> {
    if failures.len() > MAX_FUNCTIONS {
        return Err(Error::Limit);
    }
    if details.len() != failures.len() {
        return Err(Error::InvalidContext);
    }
    if failures.is_empty() {
        return Ok(Vec::new());
    }
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
    Ok(failures
        .iter()
        .zip(details)
        .map(|(failure, host)| {
            let source = &definitions[failure.function];
            let definition = source.function.definition();
            let mut context = [
                ("function", Fact::Text(&definition.name)),
                ("call", Fact::Text(&definition.function)),
            ]
            .into();
            host_context(&mut context, host.expect("preflighted ordinary facts"));
            Diagnostic {
                code: ConditionCode::ProjectEnvironmentInvalid,
                spec_path: format!("{}.function", source.path),
                context,
            }
        })
        .collect())
}
fn exact(left: &Value, right: &Value) -> bool {
    match (left, right) {
        (Value::Float(a), Value::Float(b)) => a.get().to_bits() == b.get().to_bits(),
        _ => core::mem::discriminant(left) == core::mem::discriminant(right) && left == right,
    }
}
pub fn test_diagnostics<'a, 'e: 'a, E>(
    definitions: &[DefinitionSource<'a>],
    failures: &'e [TestFailure<E>],
    details: &[Option<HostDetails<'a>>],
    maximum: usize,
) -> Result<Vec<Diagnostic<'a>>, Error<'e, E>> {
    if failures.len() > MAX_CASES {
        return Err(Error::Limit);
    }
    if details.len() != failures.len() {
        return Err(Error::InvalidContext);
    }
    if failures.is_empty() {
        return Ok(Vec::new());
    }
    let mut budget = Budget(maximum);
    selection(definitions, &mut budget)?;
    let mut seen = BTreeSet::new();
    for (failure, host) in failures.iter().zip(details) {
        let (function, case) = failure.position();
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
        if test.args.len() > MAX_CASES {
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
                invocation_admission(&error.kind, *host)?;
                invocation_budget(&error.kind, &mut budget)?;
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
    let mut diagnostics = Vec::with_capacity(failures.len());
    for (failure, host) in failures.iter().zip(details) {
        let (function, case) = failure.position();
        let source = &definitions[function];
        let definition = source.function.definition();
        let test = &definition.tests[case];
        let path = format!("{}.tests[{case}]", source.path);
        let args = test
            .args
            .iter()
            .map(|(name, value)| (name.as_str(), Fact::Scalar(value)))
            .collect();
        let mut context: Context<'a> = [
            ("function", Fact::Text(&definition.name)),
            ("call", Fact::Text(&definition.function)),
            ("case", Fact::Text(&test.id)),
            ("args", Fact::Object(args)),
            ("expected", Fact::Scalar(&test.result)),
            (
                "comparison_decimals",
                Fact::Integer(definition.comparison_decimals),
            ),
        ]
        .into();
        match failure {
            TestFailure::Invocation { error, .. } => {
                let held = invocation(&path, error, None, &[], *host)?;
                let mapping = held.code.definition();
                context.insert(
                    "reason",
                    Fact::Text("the case failed before it produced a result"),
                );
                context.insert(
                    "failure",
                    Fact::Object(
                        [
                            ("phase", Fact::Text(mapping.phase)),
                            ("condition", Fact::Text(mapping.condition)),
                            (
                                "requirement",
                                mapping.requirement.map_or(Fact::Null, Fact::Text),
                            ),
                            ("context", Fact::Object(held.context)),
                        ]
                        .into(),
                    ),
                );
            }
            TestFailure::Result { actual, .. } => {
                context.insert("actual", Fact::Scalar(actual));
                context.insert("reason", Fact::Text("the case produced a different result"));
            }
        }
        diagnostics.push(Diagnostic {
            code: ConditionCode::FunctionConformanceFailed,
            spec_path: path,
            context,
        });
    }
    Ok(diagnostics)
}
