//! Trusted stage observations, separate from semantic reports and source authority.
//! Clocks, retained measurement storage and panic containment belong to adapters.
use crate::{
    dataset::ExecutionPhase,
    function_invocation::{
        Argument, FunctionPort, HostError, InvocationPlan, ProjectInvocationPlan,
    },
    project_activation::ActivationPort,
};
use core::cell::Cell;
use yamaa_core::{
    function_signature::{LogicalSignature, ProjectFunctionIdentity},
    project_environment::LockReference,
    project_function::Language,
    value::Value,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Phase {
    Preparation,
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

/// The actual graph node, borrowed only for the observer call. Ordinary requests
/// have no graph node; a retained adapter already owns the graph's identities.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Node<'a> {
    pub index: usize,
    pub identity: &'a str,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Event<'a> {
    Phase {
        node: Option<Node<'a>>,
        phase: Phase,
    },
    FunctionCall {
        node: Option<Node<'a>>,
        stage: CallStage,
    },
}

/// Observers have no failure result and must not mutate source/callback state,
/// invoke project code or publish artifacts. Shared access permits adapter-owned
/// interior-mutability clocks without aliasing mutable execution/function ports.
/// Returned failures retain the actual entered prefix; a panic may omit Finished.
pub trait Observer {
    fn observe(&self, event: Event<'_>);
}

pub struct Noop;
impl Observer for Noop {
    fn observe(&self, _: Event<'_>) {}
}

#[derive(Clone, Copy)]
pub struct Scope<'a> {
    pub observer: &'a dyn Observer,
    pub node: Option<Node<'a>>,
}
impl<'a> Scope<'a> {
    pub fn new(observer: &'a dyn Observer) -> Self {
        Self {
            observer,
            node: None,
        }
    }
    pub fn for_node(self, index: usize, identity: &'a str) -> Self {
        Self {
            node: Some(Node { index, identity }),
            ..self
        }
    }
    pub fn phase(self, phase: Phase) {
        self.observer.observe(Event::Phase {
            node: self.node,
            phase,
        });
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
            node: self.scope.node,
            stage: CallStage::Dataset(self.phase.get()),
        });
        self.port.call(slot, arguments)
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
            node: self.scope.node,
            stage: CallStage::Activation,
        });
        self.port.invoke(handle, arguments)
    }
}
