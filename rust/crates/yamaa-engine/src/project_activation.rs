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
impl<P: ActivationPort> crate::dataset::FunctionBindings for Bindings<'_, P> {
    type Error = P::Error;
    fn signature(&self, _: usize) -> Option<&crate::function_invocation::InvocationPlan> {
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
}
impl<P: ActivationPort> FunctionPort for BoundPort<'_, P> {
    type Error = P::Error;
    fn call(&mut self, arguments: &[Argument<'_>]) -> Result<Value, HostError<Self::Error>> {
        self.calls += 1;
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
    port.verify_lock(language, lock, &identities)
        .map_err(Failure::Lock)?;
    let mut activated = Vec::with_capacity(plans.len());
    let mut binding_failures = Vec::new();
    for (function, plan) in plans.into_iter().enumerate() {
        match port.bind(plan.identity(), plan.signature()) {
            Ok(handle) => activated.push(ActivatedFunction {
                plan,
                handle,
                cases: Vec::new(),
            }),
            Err(error) if port.is_interrupt(&error) => {
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
            let mut bound = BoundPort {
                port,
                handle: &binding.handle,
                calls: 0,
            };
            let actual = match invoke_project(&binding.plan, &supplied, &mut bound) {
                Ok(actual) => actual,
                Err(error) => {
                    let interrupted = match &error.kind {
                        FailureKind::CallFailed(payload)
                        | FailureKind::InvalidHostResult(payload) => {
                            bound.port.is_interrupt(payload)
                        }
                        _ => false,
                    };
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
            if !results_match(
                &actual,
                &test.result,
                definition.definition().comparison_decimals,
            ) {
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
                invoked: bound.calls != 0,
            });
        }
    }
    if test_failures.is_empty() {
        Ok(activated)
    } else {
        Err(Failure::Tests(test_failures))
    }
}
