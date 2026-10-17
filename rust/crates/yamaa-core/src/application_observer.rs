//! Trusted stage observations, separate from semantic reports and source authority.
//! Clocks, retained measurement storage and panic containment belong to adapters.
use crate::{
    dataset::ExecutionPhase,
    function_invocation::{Argument, FunctionPort, HostError},
    function_signature::{InvocationPlan, ProjectInvocationPlan},
    project_activation::ActivationPort,
};
use crate::{
    function_signature::{LogicalSignature, ProjectFunctionIdentity},
    project_environment::LockReference,
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

/// Every observed operation takes one scope; an unobserved caller passes
/// `Scope::default()`, which discards events.
#[derive(Clone, Copy)]
pub struct Scope<'a> {
    pub observer: &'a dyn Observer,
}
impl Default for Scope<'_> {
    fn default() -> Self {
        Self::new(&Noop)
    }
}
impl<'a> Scope<'a> {
    pub fn new(observer: &'a dyn Observer) -> Self {
        Self { observer }
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
    pub(crate) fn activation<P: ActivationPort>(self, port: &mut P) -> Activation<'_, 'a, P> {
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

pub(crate) struct Activation<'p, 'a, P> {
    port: &'p mut P,
    scope: Scope<'a>,
}
impl<P: ActivationPort> ActivationPort for Activation<'_, '_, P> {
    type Handle = P::Handle;
    type Error = P::Error;
    fn verify_lock(
        &mut self,
        language: Language,
        lock: &LockReference,
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
        self.scope.observer.observe(Event::FunctionCall {
            stage: CallStage::Activation,
        });
        let result = self.port.invoke(handle, arguments);
        self.scope.observer.observe(Event::FunctionReturn {
            stage: CallStage::Activation,
            outcome: Outcome::from_result(&result),
        });
        result
    }
}
