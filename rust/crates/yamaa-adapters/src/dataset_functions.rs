//! Bounded dataset callback admission and adaptation of immutable Arrow snapshots.
use super::{CallbackError, Error};
use crate::{function_signature, scalar_transport::ScalarValue};
use serde::{Deserialize, Serialize};
use yamaa_core::table::{CellError, TableAccess, TableSchema, ValueRef};
use yamaa_engine::{
    dataset::{ExpressionDeclaration, FunctionArgument, FunctionInput},
    dataset_predicate::Read,
    function_invocation::InvocationFailure,
};

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

impl Call {
    /// Decode logical arguments without binding a signature or copying its metadata.
    pub fn decode(
        self,
        compiler: &mut yamaa_core::dataset::DatasetCompiler,
    ) -> Result<ExpressionDeclaration, Error> {
        let call = self;
        if call.arguments.len() > function_signature::MAX_PARAMETERS {
            return Err(Error::RequestLimit);
        }
        compiler
            .reserve_function(call.slot)
            .map_err(super::compile_error)?;
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
        Ok(ExpressionDeclaration::Function {
            slot: call.slot,
            arguments,
        })
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
