//! Explicit source-callback authority for the shared inheritance application service.
use crate::schema_transport::{errors, wire::Tree, CompiledSchema};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    fmt, io,
    panic::{catch_unwind, AssertUnwindSafe},
};
use yamaa_core::schema::{Document, NormalizationBudget, NormalizationLimits};
use yamaa_engine::inheritance::{self as graph, SourceError, SourcePort};

pub const MAX_REQUEST_BYTES: usize = 8_388_608;
pub const MAX_REPLY_BYTES: usize = 8_388_608;
pub const MAX_RESPONSE_BYTES: usize = 16_777_216;
const PROTOCOL: &str = "inheritance/1";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransportError {
    RequestLimit,
    ResponseLimit,
    InvalidRequest,
    InvalidReply,
    UnsupportedProtocol,
    Internal,
}
impl fmt::Display for TransportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::RequestLimit => "inheritance request exceeds byte limit",
            Self::ResponseLimit => "inheritance response exceeds byte limit",
            Self::InvalidRequest => "invalid inheritance request",
            Self::InvalidReply => "invalid inheritance source reply",
            Self::UnsupportedProtocol => "unsupported inheritance protocol",
            Self::Internal => "internal inheritance failure",
        })
    }
}
impl std::error::Error for TransportError {}

/// Bindings restore the original host exception/condition rather than stringify it.
#[derive(Debug)]
pub enum Failure<E> {
    Transport(TransportError),
    Host(E),
}
impl<E> From<TransportError> for Failure<E> {
    fn from(error: TransportError) -> Self {
        Self::Transport(error)
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Source {
    identity: String,
    display_path: String,
}
impl From<Source> for graph::Source {
    fn from(source: Source) -> Self {
        Self {
            identity: source.identity,
            display_path: source.display_path,
        }
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    protocol: String,
    entry: Source,
    document: Tree,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Reply {
    protocol: String,
    outcome: ReplyValue,
}
#[derive(Deserialize)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
enum ReplyValue {
    Resolved {
        identity: String,
        display_path: String,
    },
    Document {
        document: Tree,
    },
    Unavailable {},
}
enum PortError<E> {
    Host(E),
    Transport(TransportError),
    Outcome(Value),
}
struct Port<F> {
    callback: F,
    remaining: usize,
}
fn limit(resource: &str, maximum: usize) -> Value {
    json!({"status":"resource_limit","phase":"inheritance","resource":resource,"limit":maximum})
}
impl<F> Port<F> {
    fn ask<E>(&mut self, message: Value) -> Result<ReplyValue, SourceError<PortError<E>>>
    where
        F: FnMut(&str, usize) -> Result<String, E>,
    {
        if self.remaining == 0 {
            return Err(SourceError::Raised(PortError::Outcome(limit(
                "source_reply_bytes",
                MAX_REPLY_BYTES,
            ))));
        }
        let request = serde_json::to_string(&message)
            .map_err(|_| SourceError::Raised(PortError::Transport(TransportError::Internal)))?;
        let response = (self.callback)(&request, self.remaining)
            .map_err(|e| SourceError::Raised(PortError::Host(e)))?;
        if response.len() > self.remaining {
            self.remaining = 0;
            return Err(SourceError::Raised(PortError::Outcome(limit(
                "source_reply_bytes",
                MAX_REPLY_BYTES,
            ))));
        }
        self.remaining -= response.len();
        let reply: Reply = serde_json::from_str(&response)
            .map_err(|_| SourceError::Raised(PortError::Transport(TransportError::InvalidReply)))?;
        if reply.protocol != PROTOCOL {
            return Err(SourceError::Raised(PortError::Transport(
                TransportError::InvalidReply,
            )));
        }
        if matches!(reply.outcome, ReplyValue::Unavailable {}) {
            Err(SourceError::Unavailable)
        } else {
            Ok(reply.outcome)
        }
    }
}
impl<F, E> SourcePort for Port<F>
where
    F: FnMut(&str, usize) -> Result<String, E>,
{
    type Error = PortError<E>;
    fn canonicalize(
        &mut self,
        declaring: &str,
        written: &str,
    ) -> Result<graph::Source, SourceError<Self::Error>> {
        match self.ask(json!({"protocol":PROTOCOL,"operation":"canonicalize","declaring":declaring,"path":written}))? {
            ReplyValue::Resolved{identity,display_path}=>Ok(graph::Source{identity,display_path}),
            _=>Err(SourceError::Raised(PortError::Transport(TransportError::InvalidReply))),
        }
    }
    fn read(&mut self, source: &graph::Source) -> Result<Document, SourceError<Self::Error>> {
        let ReplyValue::Document{document}=self.ask(json!({"protocol":PROTOCOL,"operation":"read","identity":source.identity,"display_path":source.display_path}))? else {
            return Err(SourceError::Raised(PortError::Transport(TransportError::InvalidReply)));
        };
        match document.admit() {
            Ok(Ok(document)) => Ok(document),
            Ok(Err(error)) => Err(SourceError::Raised(match errors::document(error) {
                Ok(outcome) => PortError::Outcome(outcome),
                Err(_) => PortError::Transport(TransportError::InvalidReply),
            })),
            Err(_) => Err(SourceError::Raised(PortError::Transport(
                TransportError::InvalidReply,
            ))),
        }
    }
}

struct Output(Vec<u8>);
impl io::Write for Output {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self.0.len().saturating_add(bytes.len()) > MAX_RESPONSE_BYTES {
            return Err(io::Error::other("inheritance response limit"));
        }
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
fn encode(outcome: Value) -> Result<String, TransportError> {
    let mut output = Output(Vec::new());
    serde_json::to_writer(&mut output, &json!({"protocol":PROTOCOL,"outcome":outcome})).map_err(
        |e| {
            if e.is_io() {
                TransportError::ResponseLimit
            } else {
                TransportError::Internal
            }
        },
    )?;
    String::from_utf8(output.0).map_err(|_| TransportError::Internal)
}
/// The legacy inheritance/1 transport retains its context-list compatibility
/// encoding. All vocabulary and provenance come from the common core finding.
fn legacy_diagnostic(
    mut finding: yamaa_core::diagnostic::Diagnostic,
) -> Result<Value, TransportError> {
    use yamaa_core::{diagnostic::ContextValue as C, value::Value as S};
    let definition = finding.definition();
    let [path]: [String; 1] = finding
        .spec_paths
        .try_into()
        .map_err(|_| TransportError::Internal)?;
    let mut outcome = json!({"status":"invalid"});
    for name in ["source", "entry", "parent"] {
        if let Some(value) = finding.context.remove(name) {
            let C::Scalar(S::Str(text)) = value else {
                return Err(TransportError::Internal);
            };
            outcome[name] = json!(text);
        }
    }
    let mut context = Vec::new();
    // Preserve the historical ordered list representation of these named facts.
    for name in [
        "reason",
        "path",
        "expected",
        "actual",
        "entry_version",
        "parent_version",
        "cycle",
    ] {
        let Some(value) = finding.context.remove(name) else {
            continue;
        };
        let value = match value {
            C::Scalar(S::Str(text)) => json!({"kind":"text","value":text}),
            C::Sequence(values) => {
                let texts = values
                    .into_iter()
                    .map(|value| match value {
                        C::Scalar(S::Str(text)) => Ok(text),
                        _ => Err(TransportError::Internal),
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                json!({"kind":"text_list","value":texts})
            }
            _ => return Err(TransportError::Internal),
        };
        context.push(json!({"name":name,"value":value}));
    }
    if !finding.context.is_empty() {
        return Err(TransportError::Internal);
    }
    outcome["diagnostics"] = json!([{
        "condition":definition.condition,"path":path,"requirement":definition.requirement,"context":context,
    }]);
    Ok(outcome)
}
fn outcome<E>(error: graph::Error<PortError<E>>) -> Result<Value, Failure<E>> {
    match traversal_outcome(error) {
        Ok(value) => Ok(value),
        Err(Failure::Transport(error)) => Err(Failure::Transport(error)),
        Err(Failure::Host(PortError::Host(error))) => Err(Failure::Host(error)),
        Err(Failure::Host(PortError::Transport(error))) => Err(Failure::Transport(error)),
        Err(Failure::Host(PortError::Outcome(value))) => Ok(value),
    }
}

/// Reuse typed traversal diagnostics independently of the JSON source transport.
pub(crate) fn traversal_outcome<E>(error: graph::Error<E>) -> Result<Value, Failure<E>> {
    match error.diagnostic_with_limit(MAX_RESPONSE_BYTES) {
        Ok(Some(finding)) => return legacy_diagnostic(finding).map_err(Into::into),
        Ok(None) => {}
        Err(yamaa_core::diagnostic::ProjectionError::Limit) => {
            return Err(TransportError::ResponseLimit.into())
        }
        Err(yamaa_core::diagnostic::ProjectionError::InvalidContext) => {
            return Err(TransportError::Internal.into())
        }
    }
    Ok(match error {
        graph::Error::Host(error) => return Err(Failure::Host(error)),
        graph::Error::Resource(error) => limit(
            match error.resource {
                graph::Resource::Layers => "layers",
                graph::Resource::ParentVisits => "parent_visits",
                graph::Resource::Depth => "depth",
                graph::Resource::PathBytes => "path_bytes",
                graph::Resource::InputNodes => "input_nodes",
                graph::Resource::InputTextBytes => "input_text_bytes",
                graph::Resource::Work => "work",
            },
            error.limit,
        ),
        graph::Error::InvalidIdentity => return Err(TransportError::InvalidReply.into()),
        graph::Error::UnsupportedControlField { source, field } => {
            json!({"status":"unsupported","feature":"inheritance_control_field","source":source,"field":field})
        }
        graph::Error::Layer {
            source,
            entry,
            input,
            error,
        } => {
            let mut value = errors::normalization(error).map_err(|_| TransportError::Internal)?;
            value["source"] = json!(source);
            value["entry"] = json!(entry);
            value["context_document"] = json!(Tree::from_core(&input));
            value
        }
        graph::Error::InvalidParent { .. }
        | graph::Error::Unavailable { .. }
        | graph::Error::Cycle { .. }
        | graph::Error::Version { .. } => return Err(TransportError::Internal.into()),
    })
}

/// Run against a captured schema. Replies are charged cumulatively before JSON decoding.
/// Callback failures retain their original payload and no successful partial graph is returned.
pub fn traverse<E>(
    schema: &CompiledSchema,
    request: &str,
    callback: impl FnMut(&str, usize) -> Result<String, E>,
) -> Result<String, Failure<E>> {
    catch_unwind(AssertUnwindSafe(|| {
        if request.len() > MAX_REQUEST_BYTES {
            return Err(TransportError::RequestLimit.into());
        }
        let request: Request =
            serde_json::from_str(request).map_err(|_| TransportError::InvalidRequest)?;
        if request.protocol != PROTOCOL {
            return Err(TransportError::UnsupportedProtocol.into());
        }
        if request.entry.identity.is_empty() || request.entry.display_path.is_empty() {
            return Err(TransportError::InvalidRequest.into());
        }
        let document = match request.document.admit() {
            Ok(Ok(document)) => document,
            Ok(Err(error)) => {
                return encode(errors::document(error).map_err(|_| TransportError::InvalidRequest)?)
                    .map_err(Into::into)
            }
            Err(_) => return Err(TransportError::InvalidRequest.into()),
        };
        let mut port = Port {
            callback,
            remaining: MAX_REPLY_BYTES,
        };
        let result = graph::traverse(
            &schema.schema,
            request.entry.into(),
            document,
            &mut port,
            &mut NormalizationBudget::new(NormalizationLimits::default()),
            &mut graph::Budget::new(graph::Limits::default()),
        );
        let value = match result {
            Ok(layers) => {
                json!({"status":"traversed","layers":layers.into_iter().map(|layer|json!({
                "identity":layer.source.identity,"display_path":layer.source.display_path,
                "document":Tree::from_core(&layer.document),
            })).collect::<Vec<_>>()})
            }
            Err(error) => outcome(error)?,
        };
        encode(value).map_err(Into::into)
    }))
    .unwrap_or_else(|_| Err(TransportError::Internal.into()))
}

/// Stateless host entry point. Compile the schema before granting callback authority.
pub fn interpret<E>(
    schema_request: &str,
    request: &str,
    callback: impl FnMut(&str, usize) -> Result<String, E>,
) -> Result<String, Failure<E>> {
    let (schema, response) =
        crate::schema_transport::compile_schema(schema_request).map_err(|error| {
            if error == crate::schema_transport::TransportError::Internal {
                TransportError::Internal
            } else {
                TransportError::InvalidRequest
            }
        })?;
    if let Some(schema) = schema {
        traverse(&schema, request, callback)
    } else {
        let schema_outcome: Value =
            serde_json::from_str(&response).map_err(|_| TransportError::Internal)?;
        encode(json!({"status":"invalid_schema","schema_outcome":schema_outcome["outcome"]}))
            .map_err(Into::into)
    }
}
