//! Installed R metadata and namespace capabilities on the calling R thread.
//! Original condition objects are rooted in owned error payloads; no longjmp
//! crosses Rust because installed host helpers return closed reply envelopes.
use extendr_api::prelude::*;
use yamaa_adapters::{
    project_lock::{self, Finding, Reason},
    scalar_bytes,
};
use yamaa_core::{
    function_signature::{LogicalSignature, ProjectFunctionIdentity},
    project_environment::{LockKind, LockReference},
    project_function::Language,
    table::ValueRef,
    value::Value,
};
use yamaa_engine::{
    function_invocation::{Argument, HostError},
    project_activation::ActivationPort,
};
#[derive(Debug)]
pub enum Error {
    Host(Robj),
    Interrupt(Robj),
    Representation(Robj),
    Scalar(&'static str),
    LockSyntax(project_lock::Error),
    Lock(Vec<Finding>),
    Boundary(extendr_api::Error),
    Protocol,
}
impl Error {
    /// Mechanical SDK facts, distinct from projected semantic issues. Original
    /// R conditions remain rooted objects; no host condition is stringified.
    pub fn facts(&self) -> Robj {
        match self {
            Self::Host(condition) => {
                list!(kind = "host", condition = condition.clone()).into_robj()
            }
            Self::Interrupt(condition) => {
                list!(kind = "interrupt", condition = condition.clone()).into_robj()
            }
            Self::Representation(condition) => {
                list!(kind = "representation", condition = condition.clone()).into_robj()
            }
            Self::Scalar(reason) => {
                list!(kind = "scalar", reason = Raw::from_bytes(reason.as_bytes())).into_robj()
            }
            Self::Lock(findings) => list!(
                kind = "lock",
                findings = List::from_values(findings.iter().map(|finding| {
                    list!(
                        package = Raw::from_bytes(finding.package.as_bytes()),
                        reason = finding.reason.as_str(),
                        expected = List::from_values(
                            finding
                                .expected
                                .iter()
                                .map(|value| Raw::from_bytes(value.as_bytes()))
                        ),
                        actual = finding
                            .actual
                            .as_ref()
                            .map(|value| Raw::from_bytes(value.as_bytes()))
                    )
                }))
            )
            .into_robj(),
            Self::LockSyntax(error) => match error {
                project_lock::Error::Limit(resource) => {
                    list!(kind = "lock_limit", resource = *resource).into_robj()
                }
                project_lock::Error::Shape(field) => {
                    list!(kind = "lock_shape", field = *field).into_robj()
                }
                project_lock::Error::Json(error) => list!(
                    kind = "lock_json",
                    line = error.line() as f64,
                    column = error.column() as f64
                )
                .into_robj(),
                project_lock::Error::Document(error) => list!(
                    kind = "lock_document",
                    resource_refusal =
                        matches!(error, yamaa_adapters::yaml_decode::DecodeFailure::Limit(_))
                )
                .into_robj(),
            },
            Self::Boundary(error) => {
                let condition = match error {
                    extendr_api::Error::EvalError(value) | extendr_api::Error::Panic(value) => {
                        value.clone()
                    }
                    _ => ().into_robj(),
                };
                list!(kind = "sdk_boundary", condition = condition).into_robj()
            }
            Self::Protocol => list!(kind = "protocol").into_robj(),
        }
    }
}
/// Capabilities are installed package helpers. Constructing this host port does
/// not query versions, load project namespaces or invoke any project function.
pub struct Port<'lock> {
    lock: &'lock [u8],
    verify: Function,
    resolve: Function,
}
impl<'lock> Port<'lock> {
    pub fn new(lock: &'lock [u8], verify: Function, resolve: Function) -> Self {
        Self {
            lock,
            verify,
            resolve,
        }
    }
}
fn reply(function: &Function, arguments: Pairlist) -> std::result::Result<Robj, Error> {
    let result = function.call(arguments).map_err(Error::Boundary)?;
    let result = result
        .as_list()
        .filter(|list| list.len() == 2)
        .ok_or(Error::Protocol)?;
    let status = result
        .elt(0)
        .map_err(Error::Boundary)?
        .as_integer()
        .ok_or(Error::Protocol)?;
    let payload = result.elt(1).map_err(Error::Boundary)?;
    match status {
        0 => Ok(payload),
        1 => Err(Error::Host(payload)),
        2 => Err(Error::Representation(payload)),
        3 => Err(Error::Interrupt(payload)),
        _ => Err(Error::Protocol),
    }
}
fn text(value: &Robj) -> std::result::Result<String, Error> {
    let raw = value
        .as_raw()
        .filter(|raw| raw.len() <= 2048)
        .ok_or(Error::Protocol)?;
    std::str::from_utf8(raw.as_slice())
        .map(str::to_owned)
        .map_err(|_| Error::Protocol)
}
fn optional_text(value: &Robj) -> std::result::Result<Option<String>, Error> {
    if value.is_null() {
        Ok(None)
    } else {
        text(value).map(Some)
    }
}
impl ActivationPort for Port<'_> {
    type Handle = Function;
    type Error = Error;
    fn verify_lock(
        &mut self,
        language: Language,
        lock: &LockReference,
        functions: &[ProjectFunctionIdentity],
    ) -> std::result::Result<(), Error> {
        if language != Language::R
            || lock.kind != LockKind::Renv
            || self.lock.len() > 16_777_216
            || functions.len() > 1024
        {
            return Err(Error::Protocol);
        }
        let locked = project_lock::decode_renv(self.lock).map_err(Error::LockSyntax)?;
        let versions = List::from_pairs(locked.packages.iter().map(|(name, version)| {
            (
                name.as_str(),
                Robj::from(Raw::from_bytes(version.as_bytes())),
            )
        }));
        let calls = List::from_values(
            functions
                .iter()
                .map(|function| Raw::from_bytes(function.call.as_bytes())),
        );
        let result = reply(&self.verify, pairlist!(versions = versions, calls = calls))?;
        let rows = result
            .as_list()
            .filter(|rows| rows.len() <= 1025)
            .ok_or(Error::Protocol)?;
        let mut findings = Vec::with_capacity(rows.len());
        for (_, row) in rows.iter() {
            let row = row
                .as_list()
                .filter(|row| row.len() == 4)
                .ok_or(Error::Protocol)?;
            findings.push(Finding {
                package: text(&row.elt(0).map_err(Error::Boundary)?)?,
                reason: Reason::parse(&text(&row.elt(1).map_err(Error::Boundary)?)?)
                    .ok_or(Error::Protocol)?,
                expected: optional_text(&row.elt(2).map_err(Error::Boundary)?)?
                    .into_iter()
                    .collect(),
                actual: optional_text(&row.elt(3).map_err(Error::Boundary)?)?,
            });
        }
        if findings.is_empty() {
            Ok(())
        } else {
            Err(Error::Lock(findings))
        }
    }
    fn bind(
        &mut self,
        identity: &ProjectFunctionIdentity,
        signature: &LogicalSignature,
    ) -> std::result::Result<Function, Error> {
        let names = List::from_values(
            signature
                .parameters()
                .iter()
                .map(|parameter| Raw::from_bytes(parameter.name.as_bytes())),
        );
        reply(
            &self.resolve,
            pairlist!(
                call = Raw::from_bytes(identity.call.as_bytes()),
                parameters = names
            ),
        )?
        .as_function()
        .ok_or(Error::Protocol)
    }
    fn is_interrupt(&self, error: &Error) -> bool {
        matches!(error, Error::Interrupt(_))
    }
    fn invoke(
        &mut self,
        handle: &Function,
        arguments: &[Argument<'_>],
    ) -> std::result::Result<Value, HostError<Error>> {
        let mut encoded = Vec::with_capacity(arguments.len());
        for argument in arguments {
            let value = match argument.value {
                ValueRef::Missing => Value::Missing,
                ValueRef::Int(value) => Value::Int(value),
                ValueRef::Float(value) => Value::Float(value),
                ValueRef::Str(value) => Value::Str(value.into()),
                ValueRef::Bool(value) => Value::Bool(value),
                ValueRef::Date(value) => Value::Date(value),
                ValueRef::DateTime(value) => Value::DateTime(value),
            };
            let value =
                scalar_bytes::encode(value).map_err(|_| HostError::Raised(Error::Protocol))?;
            encoded.push((
                argument.name,
                Robj::from(list!(
                    tag = value.tag,
                    payload = Raw::from_bytes(&value.payload)
                )),
            ));
        }
        let result =
            reply(handle, pairlist!(encoded = List::from_pairs(encoded))).map_err(|error| {
                match error {
                    Error::Representation(_) => HostError::InvalidResult(error),
                    _ => HostError::Raised(error),
                }
            })?;
        let result = result
            .as_list()
            .filter(|list| list.len() == 2)
            .ok_or(HostError::InvalidResult(Error::Protocol))?;
        let tag = result
            .elt(0)
            .map_err(|error| HostError::InvalidResult(Error::Boundary(error)))?
            .as_integer()
            .ok_or(HostError::InvalidResult(Error::Protocol))?;
        let payload = result
            .elt(1)
            .map_err(|error| HostError::InvalidResult(Error::Boundary(error)))?
            .as_raw()
            .ok_or(HostError::InvalidResult(Error::Protocol))?;
        scalar_bytes::decode(tag, payload.as_slice())
            .map_err(|error| HostError::InvalidResult(Error::Scalar(error)))
    }
}
