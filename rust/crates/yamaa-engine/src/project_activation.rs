//! One build's called-function activation: locks, bindings, then every test.
//! There is no activation cache and this service owns no study-data port.
use crate::function_invocation::{
    invoke_project, Argument, FailureKind, FunctionPort, HostError, InvocationFailure,
};
use alloc::{
    collections::{BTreeMap, BTreeSet},
    string::String,
    vec::Vec,
};
use yamaa_core::{
    function_signature::{
        LogicalSignature, PlanError, ProjectFunctionIdentity, ProjectInvocationPlan,
    },
    project_environment::{LockKind, LockReference},
    project_function::{Function, Language},
    project_function_result::results_match,
    value::Value,
};

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
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub functions: usize,
    pub parameters: usize,
    pub cases: usize,
    pub metadata_text_bytes: usize,
    pub retained_text_bytes: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            functions: 1_024,
            parameters: 65_536,
            cases: 65_536,
            metadata_text_bytes: 16_777_216,
            retained_text_bytes: 16_777_216,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Resource {
    Functions,
    Parameters,
    Cases,
    MetadataText,
    RetainedText,
}
fn text_size(value: &Value) -> usize {
    match value {
        Value::Str(value) => value.len(),
        _ => 0,
    }
}
fn retain(held: &mut usize, amount: usize, maximum: usize) -> Result<(), Resource> {
    *held = held
        .checked_add(amount)
        .filter(|&total| total <= maximum)
        .ok_or(Resource::RetainedText)?;
    Ok(())
}
#[derive(Debug, PartialEq)]
pub enum Failure<E> {
    Limit(Resource),
    LockKindMismatch {
        language: Language,
        actual: LockKind,
    },
    FunctionLanguageMismatch {
        function: usize,
        declared: Language,
        selected: Language,
    },
    DuplicateSelection {
        function: usize,
    },
    Plan {
        function: usize,
        error: PlanError,
    },
    Lock(E),
    Bindings(Vec<BindingFailure<E>>),
    Tests(Vec<TestFailure<E>>),
    InterruptedBinding {
        function: usize,
        error: E,
    },
    InterruptedInvocation {
        function: usize,
        case: usize,
        error: InvocationFailure<E, ProjectFunctionIdentity>,
    },
}
#[derive(Debug, PartialEq)]
pub struct BindingFailure<E> {
    pub function: usize,
    pub error: E,
}
#[derive(Debug, PartialEq)]
pub enum TestFailure<E> {
    Invocation {
        function: usize,
        case: usize,
        error: InvocationFailure<E, ProjectFunctionIdentity>,
    },
    Result {
        function: usize,
        case: usize,
        actual: Value,
        expected: Value,
    },
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
    projection: Option<&'a [usize]>,
    port: &'a mut P,
}
impl<'a, P: ActivationPort> Bindings<'a, P> {
    pub fn new(functions: &'a [ActivatedFunction<P::Handle>], port: &'a mut P) -> Self {
        Self {
            functions,
            projection: None,
            port,
        }
    }
    /// Borrow one node's local slots from a complete graph's activated union.
    /// Validate every full identity/signature before accepting the projection;
    /// construction never invokes the host or observes study data.
    pub fn projected(
        functions: &'a [ActivatedFunction<P::Handle>],
        projection: &'a [usize],
        plans: &[ProjectInvocationPlan],
        port: &'a mut P,
    ) -> Result<Self, usize> {
        if projection.len() != plans.len() {
            return Err(core::cmp::min(projection.len(), plans.len()));
        }
        for (local, (global, expected)) in projection.iter().zip(plans).enumerate() {
            if functions.get(*global).map(|function| &function.plan) != Some(expected) {
                return Err(local);
            }
        }
        Ok(Self {
            functions,
            projection: Some(projection),
            port,
        })
    }
    fn slot(&self, local: usize) -> Option<usize> {
        match self.projection {
            Some(projection) => projection.get(local).copied(),
            None => Some(local),
        }
    }
}
impl<P: ActivationPort> crate::dataset::FunctionBindings for Bindings<'_, P> {
    type Error = P::Error;
    fn signature(&self, _: usize) -> Option<&crate::function_invocation::InvocationPlan> {
        None
    }
    fn project_signature(&self, slot: usize) -> Option<&ProjectInvocationPlan> {
        self.functions
            .get(self.slot(slot)?)
            .map(|function| &function.plan)
    }
    fn call(
        &mut self,
        slot: usize,
        arguments: &[Argument<'_>],
    ) -> Result<Value, HostError<Self::Error>> {
        // The dataset service checks each complete signature and slot before
        // observing table cardinality or cells; slots are immutable thereafter.
        let global = self.slot(slot).expect("preflighted local activated slot");
        let function = self
            .functions
            .get(global)
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
    fn call(&mut self, arguments: &[Argument<'_>]) -> Result<Value, HostError<Self::Error>> {
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
    activate_with_limits(language, lock, functions, port, Limits::default())
}

/// Count selected work before ports, then bound copied test observations and
/// failure identities. Exhaustion rejects the whole activation rather than
/// returning truncated case findings. Hosts bound their opaque error payloads.
pub fn activate_with_limits<P: ActivationPort>(
    language: Language,
    lock: &LockReference,
    functions: &[Function],
    port: &mut P,
    limits: Limits,
) -> Result<Vec<ActivatedFunction<P::Handle>>, Failure<P::Error>> {
    if functions.len() > limits.functions {
        return Err(Failure::Limit(Resource::Functions));
    }
    let selected = functions.iter().collect::<Vec<_>>();
    activate_references_with_limits(language, lock, &selected, port, limits)
}

/// Activate compiler-selected definitions borrowed from the one owned environment.
/// Selection order defines stable dataset slots; no test collection is recopied.
pub fn activate_references<P: ActivationPort>(
    language: Language,
    lock: &LockReference,
    functions: &[&Function],
    port: &mut P,
) -> Result<Vec<ActivatedFunction<P::Handle>>, Failure<P::Error>> {
    activate_references_with_limits(language, lock, functions, port, Limits::default())
}

/// Apply the same cumulative activation policy to borrowed selected definitions.
pub fn activate_references_with_limits<P: ActivationPort>(
    language: Language,
    lock: &LockReference,
    functions: &[&Function],
    port: &mut P,
    limits: Limits,
) -> Result<Vec<ActivatedFunction<P::Handle>>, Failure<P::Error>> {
    activate_observed(language, lock, functions, port, limits, None)
}

/// Retain the attempted lock, every binding and every attempted case even when
/// later activation fails. Each call resets prior evidence before any host port.
pub fn activate_references_observed<P: ActivationPort>(
    language: Language,
    lock: &LockReference,
    functions: &[&Function],
    port: &mut P,
    limits: Limits,
    observations: &mut Observations,
) -> Result<Vec<ActivatedFunction<P::Handle>>, Failure<P::Error>> {
    *observations = Observations::default();
    activate_observed(language, lock, functions, port, limits, Some(observations))
}

fn activate_observed<P: ActivationPort>(
    language: Language,
    lock: &LockReference,
    functions: &[&Function],
    port: &mut P,
    limits: Limits,
    mut observations: Option<&mut Observations>,
) -> Result<Vec<ActivatedFunction<P::Handle>>, Failure<P::Error>> {
    if functions.is_empty() {
        return Ok(Vec::new());
    }
    if functions.len() > limits.functions {
        return Err(Failure::Limit(Resource::Functions));
    }
    let mut cases = 0usize;
    let mut parameters = 0usize;
    let mut metadata_text = 0usize;
    for function in functions {
        let definition = function.definition();
        parameters = parameters
            .checked_add(definition.params.len())
            .filter(|&total| total <= limits.parameters)
            .ok_or(Failure::Limit(Resource::Parameters))?;
        cases = cases
            .checked_add(definition.tests.len())
            .filter(|&total| total <= limits.cases)
            .ok_or(Failure::Limit(Resource::Cases))?;
        // Plans own both logical/host names and defaults; the lock port's
        // identity collection also owns name/call copies. Bound the aggregate
        // before constructing the first plan, independently of case results.
        let texts = [&definition.name, &definition.function]
            .into_iter()
            .flat_map(|text| [text.as_str(), text.as_str()])
            .chain(definition.params.iter().flat_map(|parameter| {
                let default = match &parameter.default {
                    Some(Value::Str(text)) => text.as_str(),
                    _ => "",
                };
                [parameter.name.as_str(), parameter.name.as_str(), default]
            }));
        for text in texts {
            metadata_text = metadata_text
                .checked_add(text.len())
                .filter(|&total| total <= limits.metadata_text_bytes)
                .ok_or(Failure::Limit(Resource::MetadataText))?;
        }
    }
    let mut retained_text = 0usize;
    if !matches!(
        (language, lock.kind),
        (Language::Python, LockKind::Uv) | (Language::R, LockKind::Renv)
    ) {
        return Err(Failure::LockKindMismatch {
            language,
            actual: lock.kind,
        });
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
                return Err(Failure::InterruptedBinding { function, error })
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
                    retain(
                        &mut retained_text,
                        error.identity.name.len(),
                        limits.retained_text_bytes,
                    )
                    .map_err(Failure::Limit)?;
                    retain(
                        &mut retained_text,
                        error.identity.call.len(),
                        limits.retained_text_bytes,
                    )
                    .map_err(Failure::Limit)?;
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
                retain(
                    &mut retained_text,
                    text_size(&actual),
                    limits.retained_text_bytes,
                )
                .map_err(Failure::Limit)?;
                observations.tests.last_mut().expect("recorded case").actual = Some(actual.clone());
            }
            if !matched {
                retain(
                    &mut retained_text,
                    text_size(&actual),
                    limits.retained_text_bytes,
                )
                .map_err(Failure::Limit)?;
                retain(
                    &mut retained_text,
                    text_size(&test.result),
                    limits.retained_text_bytes,
                )
                .map_err(Failure::Limit)?;
                test_failures.push(TestFailure::Result {
                    function,
                    case,
                    actual,
                    expected: test.result.clone(),
                });
                continue;
            }
            retain(
                &mut retained_text,
                test.id.len(),
                limits.retained_text_bytes,
            )
            .map_err(Failure::Limit)?;
            retain(
                &mut retained_text,
                text_size(&actual),
                limits.retained_text_bytes,
            )
            .map_err(Failure::Limit)?;
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
