//! One build's called-function activation: locks, bindings, then every test.
//! There is no activation cache and this service owns no study-data port.
use crate::function_invocation::{
    invoke_project, Argument, FailureKind, FunctionPort, HostError, InvocationFailure,
};
use crate::{
    function_signature::{
        LogicalSignature, PlanError, ProjectFunctionIdentity, ProjectInvocationPlan,
    },
    project_environment::{LockKind, LockReference},
    project_function::{Function, Language},
    project_function_result::results_match,
    value::Value,
};
use std::collections::{BTreeMap, BTreeSet};

/// Installed metadata, callable resolution and scalar host invocation are ports.
/// Lock checking covers yamaa and only packages of the selected called functions;
/// each host identifies its standard-library exemptions without importing code.
pub trait ActivationPort {
    type Handle;
    type Error;
    fn verify_lock(
        &mut self,
        language: Language,
        lock: &LockReference,
        functions: &[ProjectFunctionIdentity],
    ) -> Result<(), Self::Error>;
    fn bind(
        &mut self,
        identity: &ProjectFunctionIdentity,
        signature: &LogicalSignature,
    ) -> Result<Self::Handle, Self::Error>;
    /// Hosts classify user interrupts; their payload is propagated immediately.
    fn is_interrupt(&self, _error: &Self::Error) -> bool {
        false
    }
    fn invoke(
        &mut self,
        handle: &Self::Handle,
        arguments: &[Argument<'_>],
    ) -> Result<Value, HostError<Self::Error>>;
}
#[derive(Debug, PartialEq, thiserror::Error)]
pub enum Failure<E> {
    #[error("project activation lock kind mismatch: language={language:?}, actual={actual:?}")]
    LockKindMismatch {
        language: Language,
        actual: LockKind,
    },
    #[error("project activation function language mismatch: function={function:?}, declared={declared:?}, selected={selected:?}")]
    FunctionLanguageMismatch {
        function: usize,
        declared: Language,
        selected: Language,
    },
    #[error("project activation duplicate selection: function={function:?}")]
    DuplicateSelection { function: usize },
    #[error("project activation plan: function={function:?}, error={error:?}")]
    Plan { function: usize, error: PlanError },
    #[error("project activation lock: {0:?}")]
    Lock(E),
    #[error("project activation bindings: {0:?}")]
    Bindings(Vec<BindingFailure<E>>),
    #[error("project activation tests: {0:?}")]
    Tests(Vec<TestFailure<E>>),
    #[error("project activation interrupted binding: function={function:?}, error={error:?}")]
    InterruptedBinding { function: usize, error: E },
    #[error("project activation interrupted invocation: function={function:?}, case={case:?}, error={error:?}")]
    InterruptedInvocation {
        function: usize,
        case: usize,
        error: InvocationFailure<E, ProjectFunctionIdentity>,
    },
}
#[derive(Debug, PartialEq, thiserror::Error)]
#[error("project activation: function={function:?}, error={error:?}")]
pub struct BindingFailure<E> {
    pub function: usize,
    pub error: E,
}
#[derive(Debug, PartialEq, thiserror::Error)]
pub enum TestFailure<E> {
    #[error(
        "project activation invocation: function={function:?}, case={case:?}, error={error:?}"
    )]
    Invocation {
        function: usize,
        case: usize,
        error: InvocationFailure<E, ProjectFunctionIdentity>,
    },
    #[error("project activation result: function={function:?}, case={case:?}, actual={actual:?}, expected={expected:?}")]
    Result {
        function: usize,
        case: usize,
        actual: Value,
        expected: Value,
    },
}
impl<E> TestFailure<E> {
    pub(crate) fn position(&self) -> (usize, usize) {
        match self {
            Self::Invocation { function, case, .. } | Self::Result { function, case, .. } => {
                (*function, *case)
            }
        }
    }
}
#[derive(Debug, PartialEq)]
pub struct CaseObservation {
    pub id: String,
    pub actual: Value,
    pub invoked: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LockObservation {
    #[default]
    NotRequested,
    Attempted,
    Verified,
    Rejected,
    Interrupted,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BindingOutcome {
    Attempted,
    Bound,
    Rejected,
    Interrupted,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BindingObservation {
    pub function: usize,
    pub outcome: BindingOutcome,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TestOutcome {
    Attempted,
    Passed,
    ResultMismatch,
    InvocationFailed,
    Interrupted,
}
#[derive(Debug, PartialEq)]
pub struct TestObservation {
    pub function: usize,
    pub case: usize,
    pub outcome: TestOutcome,
    pub invoked: bool,
    /// A retained-text limit can prevent copying a returned value. The observed
    /// outcome and invocation remain recorded alongside the terminal limit.
    pub actual: Option<Value>,
}
/// Indices refer to the immutable selected definitions. No host errors are copied
/// or formatted, and unattempted bindings/cases never receive invented records.
#[derive(Debug, Default, PartialEq)]
pub struct Observations {
    pub lock: LockObservation,
    pub bindings: Vec<BindingObservation>,
    pub tests: Vec<TestObservation>,
}
#[derive(Debug)]
pub struct ActivatedFunction<H> {
    pub plan: ProjectInvocationPlan,
    pub handle: H,
    pub cases: Vec<CaseObservation>,
}
/// Borrow successful activation handles for this run. Dataset execution never
/// resolves namespaces, verifies versions or re-runs conformance cases here.
pub struct Bindings<'a, P: ActivationPort> {
    functions: &'a [ActivatedFunction<P::Handle>],
    port: &'a mut P,
}
impl<'a, P: ActivationPort> Bindings<'a, P> {
    pub fn new(functions: &'a [ActivatedFunction<P::Handle>], port: &'a mut P) -> Self {
        Self { functions, port }
    }
}
impl<P: ActivationPort> crate::function_invocation::FunctionPort for Bindings<'_, P> {
    type Error = P::Error;
    fn signature(&self, _: usize) -> Option<&crate::function_signature::InvocationPlan> {
        None
    }
    fn project_signature(&self, slot: usize) -> Option<&ProjectInvocationPlan> {
        self.functions.get(slot).map(|function| &function.plan)
    }
    fn call(
        &mut self,
        slot: usize,
        arguments: &[Argument<'_>],
    ) -> Result<Value, HostError<Self::Error>> {
        // The dataset service checks each complete signature and slot before
        // observing table cardinality or cells; slots are immutable thereafter.
        let function = self
            .functions
            .get(slot)
            .expect("preflighted activated slot");
        self.port.invoke(&function.handle, arguments)
    }
}
struct BoundPort<'a, P: ActivationPort> {
    port: &'a mut P,
    handle: &'a P::Handle,
    calls: usize,
    observed_invocation: Option<&'a mut bool>,
}
impl<P: ActivationPort> FunctionPort for BoundPort<'_, P> {
    type Error = P::Error;
    fn call(
        &mut self,
        _slot: usize,
        arguments: &[Argument<'_>],
    ) -> Result<Value, HostError<Self::Error>> {
        self.calls += 1;
        if let Some(invoked) = self.observed_invocation.as_deref_mut() {
            *invoked = true;
        }
        self.port.invoke(self.handle, arguments)
    }
}

/// The compiler supplies unique called functions in declaration order. Static
/// admission and argument binding precede this use case. All called bindings
/// resolve before any test invokes code. Ordinary binding/test failures collect
/// in declaration order; user interrupts abort immediately with original payloads.
/// No failure grants study authority, and every later build repeats all gates.
pub fn activate<P: ActivationPort>(
    language: Language,
    lock: &LockReference,
    functions: &[Function],
    port: &mut P,
) -> Result<Vec<ActivatedFunction<P::Handle>>, Failure<P::Error>> {
    let selected = functions.iter().collect::<Vec<_>>();
    activate_references(language, Some(lock), &selected, port)
}

/// Activate compiler-selected definitions borrowed from the one owned environment.
/// Selection order defines stable dataset slots; no test collection is recopied.
pub fn activate_references<P: ActivationPort>(
    language: Language,
    lock: Option<&LockReference>,
    functions: &[&Function],
    port: &mut P,
) -> Result<Vec<ActivatedFunction<P::Handle>>, Failure<P::Error>> {
    activate_observed(language, lock, functions, port, None)
}

/// Retain the attempted lock, every binding and every attempted case even when
/// later activation fails. Each call resets prior evidence before any host port.
pub fn activate_references_observed<P: ActivationPort>(
    language: Language,
    lock: Option<&LockReference>,
    functions: &[&Function],
    port: &mut P,
    observations: &mut Observations,
) -> Result<Vec<ActivatedFunction<P::Handle>>, Failure<P::Error>> {
    *observations = Observations::default();
    activate_observed(language, lock, functions, port, Some(observations))
}

fn activate_observed<P: ActivationPort>(
    language: Language,
    lock: Option<&LockReference>,
    functions: &[&Function],
    port: &mut P,
    mut observations: Option<&mut Observations>,
) -> Result<Vec<ActivatedFunction<P::Handle>>, Failure<P::Error>> {
    if functions.is_empty() {
        return Ok(Vec::new());
    }
    if let Some(lock) = lock {
        if !matches!(
            (language, lock.kind),
            (Language::Python, LockKind::Uv) | (Language::R, LockKind::Renv)
        ) {
            return Err(Failure::LockKindMismatch {
                language,
                actual: lock.kind,
            });
        }
    }
    let mut names = BTreeSet::new();
    let mut plans = Vec::with_capacity(functions.len());
    for (function, definition) in functions.iter().enumerate() {
        if definition.language() != language {
            return Err(Failure::FunctionLanguageMismatch {
                function,
                declared: definition.language(),
                selected: language,
            });
        }
        if !names.insert(&definition.definition().name) {
            return Err(Failure::DuplicateSelection { function });
        }
        plans.push(
            definition
                .invocation_plan()
                .map_err(|error| Failure::Plan { function, error })?,
        );
    }
    if let Some(lock) = lock {
        let identities = plans
            .iter()
            .map(|plan| plan.identity().clone())
            .collect::<Vec<_>>();
        if let Some(observations) = observations.as_deref_mut() {
            observations.lock = LockObservation::Attempted;
        }
        match port.verify_lock(language, lock, &identities) {
            Ok(()) => {
                if let Some(observations) = observations.as_deref_mut() {
                    observations.lock = LockObservation::Verified;
                }
            }
            Err(error) => {
                if let Some(observations) = observations.as_deref_mut() {
                    observations.lock = if port.is_interrupt(&error) {
                        LockObservation::Interrupted
                    } else {
                        LockObservation::Rejected
                    };
                }
                return Err(Failure::Lock(error));
            }
        }
    }
    let mut activated = Vec::with_capacity(plans.len());
    let mut binding_failures = Vec::new();
    for (function, plan) in plans.into_iter().enumerate() {
        if let Some(observations) = observations.as_deref_mut() {
            observations.bindings.push(BindingObservation {
                function,
                outcome: BindingOutcome::Attempted,
            });
        }
        let bound = port.bind(plan.identity(), plan.signature());
        let interrupted = matches!(&bound, Err(error) if port.is_interrupt(error));
        if let Some(observations) = observations.as_deref_mut() {
            observations
                .bindings
                .last_mut()
                .expect("recorded binding")
                .outcome = match &bound {
                Ok(_) => BindingOutcome::Bound,
                Err(_) if interrupted => BindingOutcome::Interrupted,
                Err(_) => BindingOutcome::Rejected,
            };
        }
        match bound {
            Ok(handle) => activated.push(ActivatedFunction {
                plan,
                handle,
                cases: Vec::new(),
            }),
            Err(error) if interrupted => {
                return Err(Failure::InterruptedBinding { function, error });
            }
            Err(error) => binding_failures.push(BindingFailure { function, error }),
        }
    }
    if !binding_failures.is_empty() {
        return Err(Failure::Bindings(binding_failures));
    }
    let mut test_failures = Vec::new();
    for (function, (definition, binding)) in functions.iter().zip(&mut activated).enumerate() {
        for (case, test) in definition.definition().tests.iter().enumerate() {
            let supplied: BTreeMap<_, _> = test.args.iter().cloned().collect();
            if let Some(observations) = observations.as_deref_mut() {
                observations.tests.push(TestObservation {
                    function,
                    case,
                    outcome: TestOutcome::Attempted,
                    invoked: false,
                    actual: None,
                });
            }
            let (returned, invoked) = {
                let mut bound = BoundPort {
                    port,
                    handle: &binding.handle,
                    calls: 0,
                    observed_invocation: observations
                        .as_deref_mut()
                        .and_then(|observations| observations.tests.last_mut())
                        .map(|observation| &mut observation.invoked),
                };
                let returned = invoke_project(&binding.plan, &supplied, &mut bound);
                (returned, bound.calls != 0)
            };
            let actual = match returned {
                Ok(actual) => actual,
                Err(error) => {
                    let interrupted = match &error.kind {
                        FailureKind::CallFailed(payload)
                        | FailureKind::InvalidHostResult(payload) => port.is_interrupt(payload),
                        _ => false,
                    };
                    if let Some(observations) = observations.as_deref_mut() {
                        observations
                            .tests
                            .last_mut()
                            .expect("recorded case")
                            .outcome = if interrupted {
                            TestOutcome::Interrupted
                        } else {
                            TestOutcome::InvocationFailed
                        };
                    }
                    if interrupted {
                        return Err(Failure::InterruptedInvocation {
                            function,
                            case,
                            error,
                        });
                    }
                    test_failures.push(TestFailure::Invocation {
                        function,
                        case,
                        error,
                    });
                    continue;
                }
            };
            let matched = results_match(
                &actual,
                &test.result,
                definition.definition().comparison_decimals,
            );
            if let Some(observations) = observations.as_deref_mut() {
                observations
                    .tests
                    .last_mut()
                    .expect("recorded case")
                    .outcome = if matched {
                    TestOutcome::Passed
                } else {
                    TestOutcome::ResultMismatch
                };
                observations.tests.last_mut().expect("recorded case").actual = Some(actual.clone());
            }
            if !matched {
                test_failures.push(TestFailure::Result {
                    function,
                    case,
                    actual,
                    expected: test.result.clone(),
                });
                continue;
            }
            binding.cases.push(CaseObservation {
                id: test.id.clone(),
                actual,
                invoked,
            });
        }
    }
    if test_failures.is_empty() {
        Ok(activated)
    } else {
        Err(Failure::Tests(test_failures))
    }
}
