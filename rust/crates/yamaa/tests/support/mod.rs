//! Factories and observers owned by the yamaa integration suite.

pub mod corpus;
pub mod scratch;

pub use scratch::Scratch;

/// Project functions for specifications that call none: a call is a bug in
/// the test.
pub struct NoFunctions;
impl yamaa::ProjectFunctions for NoFunctions {
    fn verify_lock(
        &self,
        _: &yamaa::host::functions::LockContext<'_>,
        _: &[yamaa::host::functions::FunctionIdentity],
    ) -> Result<(), yamaa::host::functions::FunctionError> {
        Ok(())
    }
    fn bind(
        &self,
        _: &yamaa::host::functions::FunctionIdentity,
        _: &yamaa::host::functions::Signature,
    ) -> Result<yamaa::host::functions::FunctionHandle, yamaa::host::functions::FunctionError> {
        unreachable!("the test calls no project function")
    }
    fn invoke(
        &self,
        _: &yamaa::host::functions::FunctionHandle,
        _: &[yamaa::host::functions::Argument<'_>],
    ) -> Result<yamaa::scalar::Scalar, yamaa::host::functions::CallError> {
        unreachable!("the test calls no project function")
    }
}
