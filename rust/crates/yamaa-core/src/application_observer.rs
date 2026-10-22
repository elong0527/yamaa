//! The observer port: an operation's phase and stage events, separate from
//! semantic reports and source authority. It is internal (#3030 D72): the
//! adapters' measurements, which own clocks, retained storage and panic
//! containment, observe each build the facade runs, its compilation and its
//! run, and [`Noop`] observes an operation nothing measures.
use crate::{
    dataset::ExecutionPhase,
    function_invocation::{Argument, FunctionPort, HostError},
    function_signature::{InvocationPlan, ProjectInvocationPlan},
    host_functions::{HostFunctions, Lock},
};
use crate::{
    function_batch::BatchArgument,
    function_signature::{LogicalSignature, ProjectFunctionIdentity},
    project_function::Language,
    value::Value,
};
use std::cell::Cell;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Phase {
    Preparation,
    Build,
    Activation,
    Input,
    Dataset(ExecutionPhase),
    Output,
    Finished,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CallStage {
    Activation,
    Dataset(ExecutionPhase),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Outcome {
    Succeeded,
    Failed,
}
impl Outcome {
    pub fn from_result<T, E>(result: &Result<T, E>) -> Self {
        if result.is_ok() {
            Self::Succeeded
        } else {
            Self::Failed
        }
    }
}

/// Only the classified native limit payload; no opaque-error or cap inference.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Refusal {
    pub resource: crate::dataset::Resource,
    pub limit: usize,
    pub required: Option<usize>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Event {
    Phase { phase: Phase },
    FunctionCall { stage: CallStage },
    Exit { phase: Phase, outcome: Outcome },
    FunctionReturn { stage: CallStage, outcome: Outcome },
    Refusal { phase: Phase, refusal: Refusal },
}

/// Observers have no failure result and must not mutate source/callback state,
/// invoke project code or publish artifacts. Shared access permits adapter-owned
/// interior-mutability clocks without aliasing mutable execution/function ports.
/// Returned failures retain the actual entered prefix; a panic may omit Finished.
pub trait Observer {
    fn observe(&self, event: Event);
}

pub struct Noop;
impl Observer for Noop {
    fn observe(&self, _: Event) {}
}

/// Every observed operation takes one scope: the observer of its events and
/// the probe of its cancellation. An unobserved caller passes
/// `Scope::default()`, which discards events and is never cancelled.
#[derive(Clone, Copy)]
pub struct Scope<'a> {
    pub observer: &'a dyn Observer,
    cancelled: &'a dyn Fn() -> bool,
}
impl Default for Scope<'_> {
    fn default() -> Self {
        Self::new(&Noop)
    }
}
/// The probe of an operation nothing cancels.
fn never() -> bool {
    false
}
impl<'a> Scope<'a> {
    pub fn new(observer: &'a dyn Observer) -> Self {
        Self {
            observer,
            cancelled: &never,
        }
    }
    /// The same scope, cancelled once `cancelled` answers true (#2822). A run
    /// polls it at every checkpoint of its schedule and before each batch of
    /// project-function calls, and a cancelled run ends with
    /// `ExecutionError::Cancelled`, keeping no output.
    pub fn cancellable(self, cancelled: &'a dyn Fn() -> bool) -> Self {
        Self { cancelled, ..self }
    }
    /// Whether the operation has been cancelled.
    pub fn cancelled(self) -> bool {
        (self.cancelled)()
    }
    pub fn phase(self, phase: Phase) {
        self.observer.observe(Event::Phase { phase });
    }
    pub fn exit(self, phase: Phase, outcome: Outcome) {
        self.observer.observe(Event::Exit { phase, outcome });
    }
    pub fn refusal(self, phase: Phase, refusal: Refusal) {
        self.observer.observe(Event::Refusal { phase, refusal });
    }
    pub(crate) fn activation<P: HostFunctions>(self, port: &mut P) -> Activation<'_, 'a, P> {
        Activation { port, scope: self }
    }
    pub(crate) fn functions<'p, E>(
        self,
        port: &'p mut dyn FunctionPort<Error = E>,
        phase: &'p Cell<ExecutionPhase>,
    ) -> Functions<'p, 'a, E> {
        Functions {
            port,
            scope: self,
            phase,
        }
    }
}

pub(crate) struct Functions<'p, 'a, E> {
    port: &'p mut dyn FunctionPort<Error = E>,
    scope: Scope<'a>,
    phase: &'p Cell<ExecutionPhase>,
}
impl<E> FunctionPort for Functions<'_, '_, E> {
    type Error = E;
    fn signature(&self, slot: usize) -> Option<&InvocationPlan> {
        self.port.signature(slot)
    }
    fn project_signature(&self, slot: usize) -> Option<&ProjectInvocationPlan> {
        self.port.project_signature(slot)
    }
    fn call(&mut self, slot: usize, arguments: &[Argument<'_>]) -> Result<Value, HostError<E>> {
        self.scope.observer.observe(Event::FunctionCall {
            stage: CallStage::Dataset(self.phase.get()),
        });
        let result = self.port.call(slot, arguments);
        self.scope.observer.observe(Event::FunctionReturn {
            stage: CallStage::Dataset(self.phase.get()),
            outcome: Outcome::from_result(&result),
        });
        result
    }
}

/// The host-functions port of one activation, whose calls the scope observes.
pub(crate) struct Activation<'p, 'a, P> {
    port: &'p mut P,
    scope: Scope<'a>,
}
impl<P: HostFunctions> Activation<'_, '_, P> {
    /// One observed call into the host.
    fn observed<T>(
        &mut self,
        call: impl FnOnce(&mut P) -> Result<T, HostError<P::Error>>,
    ) -> Result<T, HostError<P::Error>> {
        self.scope.observer.observe(Event::FunctionCall {
            stage: CallStage::Activation,
        });
        let result = call(&mut *self.port);
        self.scope.observer.observe(Event::FunctionReturn {
            stage: CallStage::Activation,
            outcome: Outcome::from_result(&result),
        });
        result
    }
}
impl<P: HostFunctions> HostFunctions for Activation<'_, '_, P> {
    type Handle = P::Handle;
    type Error = P::Error;
    fn is_empty(&self) -> bool {
        self.port.is_empty()
    }
    fn verify_lock(
        &mut self,
        language: Language,
        lock: Lock<'_>,
        functions: &[ProjectFunctionIdentity],
    ) -> Result<(), P::Error> {
        self.port.verify_lock(language, lock, functions)
    }
    fn bind(
        &mut self,
        identity: &ProjectFunctionIdentity,
        signature: &LogicalSignature,
    ) -> Result<P::Handle, P::Error> {
        self.port.bind(identity, signature)
    }
    fn is_interrupt(&self, error: &P::Error) -> bool {
        self.port.is_interrupt(error)
    }
    fn invoke(
        &mut self,
        handle: &P::Handle,
        arguments: &[Argument<'_>],
    ) -> Result<Value, HostError<P::Error>> {
        self.observed(|port| port.invoke(handle, arguments))
    }
    fn call_batch(
        &mut self,
        handle: &P::Handle,
        rows: usize,
        arguments: &[BatchArgument],
    ) -> Result<Vec<Value>, HostError<P::Error>> {
        self.observed(|port| port.call_batch(handle, rows, arguments))
    }
}
