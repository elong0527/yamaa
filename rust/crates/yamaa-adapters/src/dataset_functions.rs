//! Bounded dataset callback admission and adaptation of immutable Arrow snapshots.
use super::{CallbackError, Error};
use crate::{function_signature, scalar_transport::ScalarValue};
use serde::{Deserialize, Serialize};
use yamaa_core::{
    table::{CellError, TableAccess, TableSchema, ValueRef},
    value::Value,
};
use yamaa_engine::{
    dataset::{BoundFunction, FunctionArgument, FunctionBindings, FunctionInput},
    dataset_predicate::Read,
    function_invocation::{Argument, HostError, InvocationFailure, InvocationPlan, Presence},
};

const MAX_FUNCTIONS: usize = 64;
const MAX_BOUND_PARAMETERS: usize = 16_384;
const MAX_BOUND_TEXT: usize = 1_048_576;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Call {
    slot: usize,
    arguments: Vec<Binding>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Binding {
    name: String,
    input: Input,
}
#[derive(Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
enum Input {
    Literal(ScalarValue),
    Source(usize),
    Column(usize),
    Collect { column: usize, identifier: String },
}

pub(super) struct Admission {
    pub signatures: Vec<InvocationPlan>,
    parameters_left: usize,
    text_left: usize,
}
impl Admission {
    /// Bound the declaration collection before preparing any reusable signature.
    pub fn new(signatures: Vec<function_signature::Signature>) -> Result<Self, Error> {
        if signatures.len() > MAX_FUNCTIONS {
            return Err(Error::RequestLimit);
        }
        Ok(Self {
            signatures: signatures
                .into_iter()
                .map(|s| s.prepare().map_err(Error::Function))
                .collect::<Result<_, _>>()?,
            parameters_left: MAX_BOUND_PARAMETERS,
            text_left: MAX_BOUND_TEXT,
        })
    }
    /// Charge expanded metadata before cloning a shared signature into each bound call.
    pub fn bind(&mut self, call: Call) -> Result<BoundFunction, Error> {
        if call.arguments.len() > function_signature::MAX_PARAMETERS {
            return Err(Error::RequestLimit);
        }
        let signature = self.signatures.get(call.slot).ok_or(Error::InvalidPlan)?;
        self.parameters_left = self
            .parameters_left
            .checked_sub(signature.parameters().len())
            .ok_or(Error::RequestLimit)?;
        let identity = signature.identity();
        for text in [
            &identity.name,
            &identity.contract_version,
            &identity.implementation_version,
            &identity.call,
        ] {
            self.text_left = self
                .text_left
                .checked_sub(text.len())
                .ok_or(Error::RequestLimit)?;
        }
        for parameter in signature.parameters() {
            for text in [&parameter.name, &parameter.host_name] {
                self.text_left = self
                    .text_left
                    .checked_sub(text.len())
                    .ok_or(Error::RequestLimit)?;
            }
            if let Presence::Optional(Value::Str(text)) = &parameter.presence {
                self.text_left = self
                    .text_left
                    .checked_sub(text.len())
                    .ok_or(Error::RequestLimit)?;
            }
        }
        let arguments = call
            .arguments
            .into_iter()
            .map(|binding| {
                function_signature::check_name(&binding.name).map_err(Error::Function)?;
                Ok(FunctionArgument {
                    name: binding.name,
                    input: match binding.input {
                        Input::Literal(value) => FunctionInput::Literal(
                            value.into_core().map_err(|_| Error::InvalidScalar)?,
                        ),
                        Input::Source(column) => FunctionInput::Read(Read::Source(column)),
                        Input::Column(column) => FunctionInput::Read(Read::Column(column)),
                        Input::Collect { column, identifier } => {
                            function_signature::check_name(&identifier).map_err(Error::Function)?;
                            FunctionInput::Collect { column, identifier }
                        }
                    },
                })
            })
            .collect::<Result<_, Error>>()?;
        BoundFunction::new(call.slot, signature.clone(), arguments).map_err(|_| Error::InvalidPlan)
    }
}

pub(super) struct Unavailable;
impl FunctionBindings for Unavailable {
    type Error = CallbackError;
    /// Legacy entrypoints provide no implicit callable authority.
    fn signature(&self, _: usize) -> Option<&InvocationPlan> {
        None
    }
    /// Binding admission rejects all function-bearing plans before this point.
    fn call(&mut self, _: usize, _: &[Argument<'_>]) -> Result<Value, HostError<CallbackError>> {
        unreachable!("legacy execution admitted no callback bindings")
    }
}

/// Keep source access infallible while allowing an opaque callback failure in the same run.
pub(crate) struct Snapshot<T = crate::arrow_table::ArrowTable>(pub T);
impl<T: std::borrow::Borrow<crate::arrow_table::ArrowTable>> TableAccess for Snapshot<T> {
    type Error = CallbackError;
    /// Borrow the same admitted schema without copying or invoking host code.
    fn schema(&self) -> &TableSchema {
        self.0.borrow().schema()
    }
    /// Preserve snapshot row count exactly.
    fn row_count(&self) -> usize {
        self.0.borrow().row_count()
    }
    /// Preserve cell values and bounds failures without introducing host access.
    fn cell(&self, row: usize, column: usize) -> Result<ValueRef<'_>, CellError<Self::Error>> {
        self.0
            .borrow()
            .cell(row, column)
            .map_err(|error| match error {
                CellError::OutOfBounds { row, column } => CellError::OutOfBounds { row, column },
                CellError::Access(never) => match never {},
            })
    }
}

#[derive(Serialize)]
pub(super) struct Diagnostic {
    spec_paths: Vec<String>,
    #[serde(flatten)]
    detail: crate::function_transport::Diagnostic,
}
impl Diagnostic {
    /// Reuse the bounded scalar callback diagnostic with dataset assignment provenance.
    pub fn new(path: String, error: InvocationFailure<CallbackError>) -> Result<Self, Error> {
        Ok(Self {
            spec_paths: vec![path],
            detail: crate::function_transport::diagnostic(error).map_err(Error::Function)?,
        })
    }
}
