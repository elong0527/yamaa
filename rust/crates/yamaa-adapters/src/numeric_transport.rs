//! Bounded numeric application protocol; no tables, callbacks or implicit fallback.
//! Serialization composes existing core compilation and engine lifecycle services.

use crate::scalar_transport::{ScalarValue, MAX_REQUEST_BYTES};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, convert::Infallible, fmt, panic::catch_unwind};
use yamaa_core::{
    conversion::ConversionError,
    evaluation::NumericResolver,
    numeric_compiler::{compile_numeric_with_policy, CompileError, CompileLimits, MathPolicy},
    numeric_parser::{GrammarFailure, ParseError, ParseResource, SourcePosition, SourceSpan},
    value::{ColumnType, Selection, Value},
};
use yamaa_engine::numeric_lifecycle::{
    HandlerCounter, LiteralHandler, NumericDerivation, NumericLifecycleError,
};

const PROTOCOL: &str = "numeric/1";
const MAX_BINDINGS: usize = 4096;

/// Malformed transport is separate from normative failed/unsupported evaluation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NumericTransportError {
    RequestLimit,
    InvalidRequest,
    UnsupportedProtocol,
    InvalidScalar,
    Internal,
}

impl fmt::Display for NumericTransportError {
    /// Return a stable boundary message without echoing the supplied request.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::RequestLimit => "numeric transport request exceeds resource limit",
            Self::InvalidRequest => "invalid numeric transport request",
            Self::UnsupportedProtocol => "unsupported numeric transport protocol",
            Self::InvalidScalar => "invalid numeric transport scalar",
            Self::Internal => "internal numeric transport failure",
        })
    }
}
impl std::error::Error for NumericTransportError {}

#[derive(Deserialize, Serialize, Clone, Copy, Default)]
#[serde(rename_all = "snake_case")]
enum Policy {
    #[default]
    ReferenceSubset,
    PortableLibmV1,
}

#[derive(Deserialize, Clone, Copy)]
#[serde(rename_all = "lowercase")]
enum Target {
    Str,
    Int,
    Float,
    Date,
    Datetime,
}

impl Target {
    /// Decode the closed column vocabulary, which deliberately excludes bool.
    fn core(self) -> ColumnType {
        match self {
            Self::Str => ColumnType::Str,
            Self::Int => ColumnType::Int,
            Self::Float => ColumnType::Float,
            Self::Date => ColumnType::Date,
            Self::Datetime => ColumnType::DateTime,
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Binding {
    name: String,
    value: ScalarValue,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Replacement {
    value: ScalarValue,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    protocol: String,
    expression: String,
    column_path: String,
    target: Target,
    bindings: Vec<Binding>,
    #[serde(default)]
    math_policy: Policy,
    // None is absent; an explicit null handler is {"value":{"missing":null}}.
    #[serde(default, deserialize_with = "present_replacement")]
    unconvertible: Option<Replacement>,
}

/// Missing fields mean absence; a present handler must use the explicit value wrapper.
fn present_replacement<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<Replacement>, D::Error> {
    Replacement::deserialize(deserializer).map(Some)
}

#[derive(Serialize)]
struct Response {
    protocol: &'static str,
    math_policy: Policy,
    outcome: Outcome,
    handler_counts: Vec<Count>,
    resolutions: Vec<String>,
}

#[derive(Serialize)]
struct Count {
    spec_path: String,
    handler: &'static str,
    count: String,
}

#[derive(Serialize)]
struct Span {
    start: String,
    end: String,
}
impl From<SourceSpan> for Span {
    /// Preserve half-open UTF-8 offsets without host integer narrowing.
    fn from(span: SourceSpan) -> Self {
        Self {
            start: span.start.to_string(),
            end: span.end.to_string(),
        }
    }
}

#[derive(Serialize)]
struct Position {
    byte: String,
    character: String,
}
impl From<SourcePosition> for Position {
    /// Preserve byte and Unicode-scalar positions as distinct coordinates.
    fn from(position: SourcePosition) -> Self {
        Self {
            byte: position.byte.to_string(),
            character: position.character.to_string(),
        }
    }
}

#[derive(Serialize)]
#[serde(untagged)]
enum ContextValue {
    Scalar(ScalarValue),
    // This diagnostic-only integer may lie outside the valid runtime i64 range.
    Integer { integer: String },
    Sequence(Vec<ContextValue>),
    Object(BTreeMap<String, ContextValue>),
}

type Context = BTreeMap<String, ContextValue>;

/// Encode ordinary context values through the shared scalar transport.
fn scalar(value: Value) -> ContextValue {
    ContextValue::Scalar(ScalarValue::from_core(value))
}
/// Preserve text context without conflating it with a diagnostic integer.
fn text(value: impl Into<String>) -> ContextValue {
    scalar(Value::Str(value.into()))
}

#[derive(Serialize)]
pub(crate) struct Diagnostic {
    phase: &'static str,
    condition: &'static str,
    requirement: Option<&'static str>,
    spec_paths: Vec<String>,
    context: Context,
    #[serde(skip_serializing_if = "Option::is_none")]
    source_span: Option<Span>,
    #[serde(skip_serializing_if = "Option::is_none")]
    operand_route: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    position: Option<Position>,
}

impl From<yamaa_core::diagnostic::ContextValue> for ContextValue {
    fn from(value: yamaa_core::diagnostic::ContextValue) -> Self {
        use yamaa_core::diagnostic::ContextValue as C;
        match value {
            C::Scalar(value) => scalar(value),
            C::Integer(integer) => Self::Integer { integer },
            C::Sequence(values) => Self::Sequence(values.into_iter().map(Into::into).collect()),
            C::Object(values) => Self::Object(
                values
                    .into_iter()
                    .map(|(key, value)| (key, value.into()))
                    .collect(),
            ),
        }
    }
}

impl From<yamaa_core::diagnostic::Diagnostic> for Diagnostic {
    /// Translate representation only; core owns vocabulary, context and geometry.
    fn from(diagnostic: yamaa_core::diagnostic::Diagnostic) -> Self {
        let definition = diagnostic.definition();
        Self {
            phase: definition.phase,
            condition: definition.condition,
            requirement: definition.requirement,
            spec_paths: diagnostic.spec_paths,
            context: diagnostic
                .context
                .into_iter()
                .map(|(key, value)| (key, value.into()))
                .collect(),
            source_span: diagnostic.source_span.map(Into::into),
            operand_route: diagnostic.operand_route,
            position: None,
        }
    }
}

#[derive(Serialize)]
struct Unsupported {
    function: &'static str,
    name_span: Span,
}

#[derive(Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
enum Outcome {
    Value {
        value: ScalarValue,
    },
    Failure {
        diagnostic: Box<Diagnostic>,
        #[serde(skip_serializing_if = "Option::is_none")]
        original: Option<Box<Diagnostic>>,
    },
    Unsupported {
        spec_path: String,
        expression: String,
        functions: Vec<Unsupported>,
    },
    Limit {
        spec_path: String,
        expression: String,
        resource: &'static str,
        limit: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        required: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        position: Option<Position>,
    },
}

/// Static normalized bindings are the only resolution authority in this prototype.
struct Resolver {
    values: BTreeMap<String, Value>,
    trace: Vec<String>,
}
impl NumericResolver for Resolver {
    type Error = Infallible;
    /// Record each reached occurrence and distinguish absence from present missing.
    fn resolve(&mut self, name: &str) -> Result<Selection, Infallible> {
        self.trace.push(name.into());
        Ok(self
            .values
            .get(name)
            .cloned()
            .map_or(Selection::Absent, Selection::Present))
    }
}

/// Retain native predicate provenance; host reports may project site-specific fields.
pub(crate) fn predicate<E>(
    error: &yamaa_core::predicate::EvaluationError<yamaa_core::table::CellError<E>>,
) -> Result<Box<Diagnostic>, crate::dataset_transport::DatasetTransportError> {
    use yamaa_core::diagnostic::PredicateDiagnosticError;
    error
        .diagnostic()
        .map(|d| Box::new(d.into()))
        .map_err(|e| match e {
            PredicateDiagnosticError::OutputLimit => {
                crate::dataset_transport::DatasetTransportError::OutputLimit
            }
            PredicateDiagnosticError::Resolution(_) | PredicateDiagnosticError::Resource(_) => {
                crate::dataset_transport::DatasetTransportError::Internal
            }
        })
}
/// Serialize core-owned completed-result diagnostics, including wide integers.
pub(crate) fn conversion(error: ConversionError, path: String) -> Box<Diagnostic> {
    Box::new(error.into_diagnostic(path).into())
}

/// Serialize the same core diagnostic used for bound numeric reductions.
pub(crate) fn arithmetic(
    error: yamaa_core::numeric::ArithmeticError,
    path: String,
) -> Box<Diagnostic> {
    Box::new(error.into_diagnostic(path).into())
}

/// Separate invalid grammar, unsupported functions and compile resource policy.
fn compile_outcome(error: CompileError, expression: &str, path: String) -> Outcome {
    match error {
        CompileError::Unsupported { functions } => Outcome::Unsupported {
            spec_path: path,
            expression: expression.into(),
            functions: functions
                .into_iter()
                .map(|f| Unsupported {
                    function: f.function.name(),
                    name_span: f.name.into(),
                })
                .collect(),
        },
        CompileError::ResolutionLimit { limit, required } => Outcome::Limit {
            spec_path: path,
            expression: expression.into(),
            resource: "resolutions",
            limit: limit.to_string(),
            required: Some(required.to_string()),
            position: None,
        },
        CompileError::Parse(ParseError::Limit {
            position,
            resource,
            limit,
        }) => Outcome::Limit {
            spec_path: path,
            expression: expression.into(),
            resource: match resource {
                ParseResource::Bytes => "bytes",
                ParseResource::Tokens => "tokens",
                ParseResource::Nodes => "nodes",
                ParseResource::Depth => "depth",
            },
            limit: limit.to_string(),
            required: None,
            position: Some(position.into()),
        },
        CompileError::Parse(ParseError::Grammar { position, failure }) => {
            let mut context = Context::from([("expr".into(), text(expression))]);
            match &failure {
                GrammarFailure::ProhibitedConstruct { construct } => {
                    context.insert("construct".into(), text(*construct));
                }
                GrammarFailure::ProhibitedFunction {
                    name,
                    argument_count,
                } => {
                    context.insert("function".into(), text(&expression[name.start..name.end]));
                    if let Some(count) = argument_count {
                        context.insert("argument_count".into(), scalar(Value::Int(*count as i64)));
                    }
                }
                GrammarFailure::InvalidExpression => {}
            }
            Outcome::Failure {
                diagnostic: Box::new(Diagnostic {
                    phase: "validation",
                    condition: failure.condition(),
                    requirement: Some(failure.requirement()),
                    spec_paths: vec![format!("{path}.expr")],
                    context,
                    source_span: None,
                    operand_route: None,
                    position: Some(position.into()),
                }),
                original: None,
            }
        }
    }
}

/// Serialize core diagnostics without reclassifying opaque resolver failures.
pub(crate) fn numeric(
    error: yamaa_core::numeric_compiler::CompiledEvaluationError<Infallible>,
) -> Result<Box<Diagnostic>, NumericTransportError> {
    error
        .diagnostic()
        .map(|diagnostic| Box::new(diagnostic.into()))
        .ok_or(NumericTransportError::Internal)
}

/// Preserve lifecycle ownership, source geometry and both replacement failures.
fn lifecycle_outcome(
    result: Result<Value, Box<NumericLifecycleError<Infallible>>>,
) -> Result<Outcome, NumericTransportError> {
    Ok(match result {
        Ok(value) => Outcome::Value {
            value: ScalarValue::from_core(value),
        },
        Err(error) => match *error {
            NumericLifecycleError::Evaluation(error) => Outcome::Failure {
                diagnostic: numeric(error)?,
                original: None,
            },
            NumericLifecycleError::Conversion { spec_path, error } => Outcome::Failure {
                diagnostic: conversion(error, spec_path),
                original: None,
            },
            NumericLifecycleError::HandlerConversion {
                spec_path,
                column_path,
                original,
                replacement,
            } => Outcome::Failure {
                diagnostic: conversion(replacement, spec_path),
                original: Some(conversion(original, column_path)),
            },
            // A fresh counter with one possible firing cannot overflow. Never
            // publish a misleading normative condition if this invariant changes.
            NumericLifecycleError::Accounting { .. } => {
                return Err(NumericTransportError::Internal)
            }
        },
    })
}

/// Run one normalized numeric derivation with no callbacks, tables or publication.
/// Invalid envelopes raise host errors; language, unsupported and compile-limit
/// outcomes return structured JSON with no fallback or reference-engine execution.
pub fn evaluate_numeric(request: &str) -> Result<String, NumericTransportError> {
    catch_unwind(|| execute(request)).unwrap_or(Err(NumericTransportError::Internal))
}

/// Decode before compilation, then evaluate one immutable plan with fresh accounting.
fn execute(request: &str) -> Result<String, NumericTransportError> {
    if request.len() > MAX_REQUEST_BYTES {
        return Err(NumericTransportError::RequestLimit);
    }
    let request: Request =
        serde_json::from_str(request).map_err(|_| NumericTransportError::InvalidRequest)?;
    if request.protocol != PROTOCOL {
        return Err(NumericTransportError::UnsupportedProtocol);
    }
    if request.bindings.len() > MAX_BINDINGS {
        return Err(NumericTransportError::RequestLimit);
    }
    if request.column_path.is_empty() {
        return Err(NumericTransportError::InvalidRequest);
    }
    let mut values = BTreeMap::new();
    for binding in request.bindings {
        if binding.name.is_empty() || values.contains_key(&binding.name) {
            return Err(NumericTransportError::InvalidRequest);
        }
        values.insert(
            binding.name,
            binding
                .value
                .into_core()
                .map_err(|_| NumericTransportError::InvalidScalar)?,
        );
    }
    let handler = request
        .unconvertible
        .map(|replacement| {
            replacement
                .value
                .into_core()
                .map_err(|_| NumericTransportError::InvalidScalar)
        })
        .transpose()?;
    if matches!(handler, Some(Value::Date(_) | Value::DateTime(_))) {
        // Normalized source values can be temporal, but local handlers are
        // specification literals. Temporal replacements must use text literals.
        return Err(NumericTransportError::InvalidRequest);
    }
    let expression_path = format!("{}.derivation.value.compute", request.column_path);
    let policy = match request.math_policy {
        Policy::ReferenceSubset => MathPolicy::ReferenceSubset,
        Policy::PortableLibmV1 => MathPolicy::PortableLibmV1,
    };
    let compiled = compile_numeric_with_policy(
        &request.expression,
        &expression_path,
        CompileLimits::default(),
        policy,
    );
    let mut resolver = Resolver {
        values,
        trace: Vec::new(),
    };
    let mut counter = HandlerCounter::default();
    let outcome = match compiled {
        Err(error) => compile_outcome(error, &request.expression, expression_path),
        Ok(compiled) => {
            let plan = NumericDerivation::new(
                compiled,
                request.target.core(),
                &request.column_path,
                handler.map(|value| LiteralHandler {
                    spec_path: format!("{}.derivation.unconvertible", request.column_path),
                    value,
                }),
            );
            lifecycle_outcome(plan.evaluate(&mut resolver, &mut counter))?
        }
    };
    let response = Response {
        protocol: PROTOCOL,
        math_policy: request.math_policy,
        outcome,
        handler_counts: counter
            .snapshot()
            .iter()
            .map(|c| Count {
                spec_path: c.spec_path.clone(),
                handler: c.handler.name(),
                count: c.count.to_string(),
            })
            .collect(),
        resolutions: resolver.trace,
    };
    serde_json::to_string(&response).map_err(|_| NumericTransportError::Internal)
}

/// A compiler finding reached in the engine's verification sequence.
pub(crate) fn declaration(
    path: String,
    condition: &'static str,
    requirement: &'static str,
    reason: String,
) -> Box<Diagnostic> {
    Box::new(Diagnostic {
        phase: "validation",
        condition,
        requirement: Some(requirement),
        spec_paths: vec![path],
        context: [("reason".into(), text(reason))].into_iter().collect(),
        source_span: None,
        operand_route: None,
        position: None,
    })
}
