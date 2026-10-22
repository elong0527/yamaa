//! The host-functions port (#2845 section 2, "Five ports"): the one way a
//! build reaches a host's project functions. It verifies the declared lock,
//! binds each called function, invokes it, also in batches, and classifies
//! the host's interrupts. Core calls out through it across the dependency
//! rule (#3030 section 2.10): the facade implements it over an embedder's
//! `ProjectFunctions`, which the Python and R hosts and the memory route
//! supply, and [`Empty`] is the port of an engine without project functions,
//! such as the command line's or a bare Rust embedder's.
//!
//! Every method runs on the caller's thread. A host failure is the port's
//! own `Error`, which the engine retains unchanged; an interrupt the port
//! classifies ends activation at once, and the facade answers it with
//! `Error::Cancelled` (#3030 D46, #3260).
use crate::{
    function_batch::BatchArgument,
    function_invocation::{Argument, HostError},
    function_signature::{LogicalSignature, ProjectFunctionIdentity},
    project_environment::LockReference,
    project_function::Language,
    table::ValueRef,
    value::Value,
};

/// A declared packaging lock, as one activation verifies it: its declaration
/// and the exact bytes captured with the project. A host verifies installed
/// versions from these facts without importing code.
#[derive(Clone, Copy, Debug)]
pub struct Lock<'a> {
    pub reference: &'a LockReference,
    pub bytes: &'a [u8],
}

/// Installed metadata, callable resolution and invocation of one host's
/// project functions. Lock verification covers yamaa and only the packages
/// of the selected called functions; each host identifies its
/// standard-library exemptions without importing code.
pub trait HostFunctions {
    type Handle;
    type Error;
    /// Whether this is the empty port, which keeps no project functions
    /// (#3030 D110, D123). Activation refuses each build that calls one
    /// through it before anything is bound, so no other method runs.
    fn is_empty(&self) -> bool {
        false
    }
    fn verify_lock(
        &mut self,
        language: Language,
        lock: Lock<'_>,
        functions: &[ProjectFunctionIdentity],
    ) -> Result<(), Self::Error>;
    /// Resolve the logical call to a host callable with exactly the logical
    /// parameters, without invoking project code.
    fn bind(
        &mut self,
        identity: &ProjectFunctionIdentity,
        signature: &LogicalSignature,
    ) -> Result<Self::Handle, Self::Error>;
    /// Hosts classify user interrupts; their payload is propagated immediately.
    fn is_interrupt(&self, _error: &Self::Error) -> bool {
        false
    }
    /// Invoke a bound function once with declaration-order arguments.
    fn invoke(
        &mut self,
        handle: &Self::Handle,
        arguments: &[Argument<'_>],
    ) -> Result<Value, HostError<Self::Error>>;
    /// Invoke a bound function once for a batch of `rows` rows, explicit
    /// because a function with no parameters has no column to count. The
    /// port returns one owned value per row, in row order, or fails the whole
    /// call, and the engine checks the length and each result's exact type
    /// regardless. Rows the signature short-circuits never reach the port:
    /// every row given must reach the callable, a parameter that accepts
    /// missing receiving an explicit missing value. The provided method
    /// invokes row by row with scalar semantics, the reference a host's
    /// native vectorized callable must match.
    fn call_batch(
        &mut self,
        handle: &Self::Handle,
        rows: usize,
        arguments: &[BatchArgument],
    ) -> Result<Vec<Value>, HostError<Self::Error>> {
        let mut results = Vec::with_capacity(rows);
        for row in 0..rows {
            let borrowed = arguments
                .iter()
                .map(|argument| Argument {
                    name: &argument.name,
                    value: ValueRef::from(&argument.values[row]),
                })
                .collect::<Vec<_>>();
            results.push(self.invoke(handle, &borrowed)?);
        }
        Ok(results)
    }
}

/// The empty port: the host-functions port of an engine without project
/// functions, the command line's or a bare Rust embedder's (#3030 D110,
/// D123). It keeps no function, so activation refuses the functions a build
/// calls before binding any (`FunctionLanguageMismatch`, with no selected
/// language), and the build reports that refusal before it reads any input
/// (D110). Only [`HostFunctions::is_empty`] is ever asked.
pub struct Empty<E>(std::marker::PhantomData<fn() -> E>);

impl<E> Empty<E> {
    pub const fn new() -> Self {
        Self(std::marker::PhantomData)
    }
}

impl<E> Default for Empty<E> {
    fn default() -> Self {
        Self::new()
    }
}

impl<E> HostFunctions for Empty<E> {
    /// No function is ever bound.
    type Handle = std::convert::Infallible;
    type Error = E;
    fn is_empty(&self) -> bool {
        true
    }
    fn verify_lock(
        &mut self,
        _: Language,
        _: Lock<'_>,
        _: &[ProjectFunctionIdentity],
    ) -> Result<(), E> {
        unreachable!("activation verifies no lock through the empty port")
    }
    fn bind(
        &mut self,
        _: &ProjectFunctionIdentity,
        _: &LogicalSignature,
    ) -> Result<Self::Handle, E> {
        unreachable!("activation binds nothing through the empty port")
    }
    fn invoke(&mut self, handle: &Self::Handle, _: &[Argument<'_>]) -> Result<Value, HostError<E>> {
        match *handle {}
    }
}
