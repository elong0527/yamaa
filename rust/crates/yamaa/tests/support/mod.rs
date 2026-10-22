//! Factories and observers owned by the yamaa integration suite.

pub mod scratch;

pub use scratch::Scratch;

/// Project functions for specifications that call none: a call is a bug in
/// the test.
pub struct NoFunctions;
impl yamaa::ProjectFunctions for NoFunctions {
    fn verify_lock(
        &self,
        _: &yamaa::LockContext<'_>,
        _: &[yamaa::FunctionIdentity],
    ) -> Result<(), yamaa::FunctionError> {
        Ok(())
    }
    fn bind(
        &self,
        _: &yamaa::FunctionIdentity,
        _: &yamaa::Signature,
    ) -> Result<yamaa::FunctionHandle, yamaa::FunctionError> {
        unreachable!("the test calls no project function")
    }
    fn invoke(
        &self,
        _: &yamaa::FunctionHandle,
        _: &[yamaa::Argument<'_>],
    ) -> Result<yamaa::Scalar, yamaa::CallError> {
        unreachable!("the test calls no project function")
    }
}
