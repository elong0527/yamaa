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
    Phase {
        phase: Phase,
    },
    FunctionCall {
        stage: CallStage,
    },
    Exit {
        phase: Phase,
        outcome: Outcome,
    },
    FunctionReturn {
        stage: CallStage,
        outcome: Outcome,
    },
    Refusal {
        phase: Phase,
        refusal: Refusal,
    },
    /// How a run divided its driver input, with the most records its
    /// partitions hold when it ran divided (#3794).
    Divided {
        division: crate::dataset::Division,
        rows: Option<std::num::NonZeroUsize>,
    },
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

/// Every observed operation takes one scope: the observer of its events, the
/// probe of its cancellation and how its runs divide their driver input. An
/// unobserved caller passes `Scope::default()`, which discards events, is
/// never cancelled and runs every dataset over its whole input.
#[derive(Clone, Copy)]
pub struct Scope<'a> {
    pub observer: &'a dyn Observer,
    cancelled: &'a dyn Fn() -> bool,
    partitioning: Option<crate::dataset::Partitioning>,
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
            partitioning: None,
        }
    }
    /// The same scope, whose runs divide their driver input as
    /// `partitioning` asks, or run it whole for `None` (#3697).
    pub fn partitioned(self, partitioning: Option<crate::dataset::Partitioning>) -> Self {
        Self {
            partitioning,
            ..self
        }
    }
    /// How the operation's runs divide their driver input.
    pub fn partitioning(self) -> Option<crate::dataset::Partitioning> {
        self.partitioning
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
    /// Report how a run divided its driver input (#3794), under the
    /// scope's partitions.
    pub fn divided(self, division: crate::dataset::Division) {
        let rows = match division {
            crate::dataset::Division::Whole => None,
            crate::dataset::Division::Partitions { .. } => {
                self.partitioning.map(|partitioning| partitioning.rows)
            }
        };
        self.observer.observe(Event::Divided { division, rows });
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    /// The events an operation emits, in order.
    #[derive(Default)]
    struct Recorded(RefCell<Vec<Event>>);
    impl Observer for Recorded {
        fn observe(&self, event: Event) {
            self.0.borrow_mut().push(event);
        }
    }

    /// A port that answers each batch whole with its `x` values, or raises.
    struct Batches {
        raise: bool,
    }
    impl HostFunctions for Batches {
        type Handle = ();
        type Error = ();
        fn verify_lock(
            &mut self,
            _: Language,
            _: Lock<'_>,
            _: &[ProjectFunctionIdentity],
        ) -> Result<(), ()> {
            unreachable!("a batch verifies no lock")
        }
        fn bind(&mut self, _: &ProjectFunctionIdentity, _: &LogicalSignature) -> Result<(), ()> {
            unreachable!("a batch is given its handle")
        }
        fn invoke(&mut self, _: &(), _: &[Argument<'_>]) -> Result<Value, HostError<()>> {
            unreachable!("this port answers a batch whole")
        }
        fn call_batch(
            &mut self,
            _: &(),
            _: usize,
            arguments: &[BatchArgument],
        ) -> Result<Vec<Value>, HostError<()>> {
            if self.raise {
                return Err(HostError::Raised(()));
            }
            Ok(arguments[0].values.clone())
        }
    }

    /// An activation's scope observes one call and one return per batch,
    /// with the batch's outcome, however many rows the batch holds.
    #[test]
    fn an_observed_batch_is_one_call_and_one_return() {
        let rows = vec![Value::Int(1), Value::Int(2), Value::Int(3)];
        let arguments = [BatchArgument {
            name: "x".into(),
            values: rows.clone(),
        }];
        for (raise, expected, outcome, label) in [
            (false, Ok(rows), Outcome::Succeeded, "a batch answered"),
            (
                true,
                Err(HostError::Raised(())),
                Outcome::Failed,
                "a batch raised",
            ),
        ] {
            let recorded = Recorded::default();
            let mut port = Batches { raise };
            let returned =
                Scope::new(&recorded)
                    .activation(&mut port)
                    .call_batch(&(), 3, &arguments);
            assert_eq!(returned, expected, "{label}");
            assert_eq!(
                *recorded.0.borrow(),
                [
                    Event::FunctionCall {
                        stage: CallStage::Activation
                    },
                    Event::FunctionReturn {
                        stage: CallStage::Activation,
                        outcome
                    },
                ],
                "{label}"
            );
        }
    }
}
