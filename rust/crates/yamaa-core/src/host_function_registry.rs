//! Caller-supplied registry of project functions, frozen per activation.
//!
//! Registration is deliberately separate from packaging-lock auditing: a
//! caller (a host embedding, a test harness, an unlocked synthetic setup)
//! may register any owned callable handle under a logical function
//! identity together with the exact `LogicalSignature` it was validated
//! against, whether or not any lock verification was requested. Lock
//! verification, when requested, stays with `project_activation` and the
//! host-functions port; this registry never imports code, reads a lock,
//! or resolves a namespace.
//!
//! Registration rules:
//! - Identities are keyed by their logical `call` string, the same string
//!   a project function definition names.
//! - Registering the same call twice is refused with
//!   `RegistryError::Duplicate`; silently replacing a binding would make
//!   activation depend on registration order. Replacement is possible only
//!   through the explicit `replace` operation, and only before freezing.
//! - A registration whose signature does not survive `LogicalSignature`
//!   validation is impossible by construction: callers supply an already
//!   validated `LogicalSignature`, and activation additionally requires
//!   the registered signature to equal the plan signature exactly
//!   (including float default bits, via the plan equality contract).
//! - `freeze` consumes the registry and produces an immutable snapshot.
//!   The frozen bindings are the only bindings an activation attempt can
//!   see; registering more functions afterwards requires a new registry
//!   and a new activation attempt. Handles are owned by the frozen
//!   registry and borrowed for exactly that attempt's lifetime.
//! - A called function with no frozen binding is refused with
//!   `RegistryActivationFailure::MissingBinding`; activation never falls
//!   back to namespace resolution.
//!
//! Activation through a frozen registry runs every authored conformance
//! case of every selected function before returning any handle, exactly
//! like scalar activation, but each case goes through the batch
//! invocation contract as a one-row batch so registered hosts exercise
//! the same code path study rows will use. A failed attempt retains no
//! state: the registry is immutable, activation evidence is local to the
//! call, and a later attempt re-runs every binding check and every case.
use crate::{
    function_batch::{invoke_batch, BatchFailure, BatchFailureKind},
    function_signature::{LogicalSignature, PlanError, ProjectFunctionIdentity},
    host_functions::HostFunctions,
    project_function::{Function, Language},
    project_function_result::results_match,
    value::Value,
};
use std::collections::BTreeMap;

/// One registered binding: the identity it was registered under, the
/// validated signature it was checked against, and the owned handle.
#[derive(Debug)]
pub struct RegisteredFunction<H> {
    identity: ProjectFunctionIdentity,
    signature: LogicalSignature,
    handle: H,
}
impl<H> RegisteredFunction<H> {
    pub fn identity(&self) -> &ProjectFunctionIdentity {
        &self.identity
    }
    pub fn signature(&self) -> &LogicalSignature {
        &self.signature
    }
    pub fn handle(&self) -> &H {
        &self.handle
    }
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum RegistryError {
    #[error("function registry duplicate registration: call={call:?}")]
    Duplicate { call: String },
    #[error("function registry replacement of unregistered function: call={call:?}")]
    NotRegistered { call: String },
}

/// A mutable registry under construction. Not usable for activation until
/// frozen; see the module contract for the registration rules.
#[derive(Debug, Default)]
pub struct FunctionRegistry<H> {
    entries: BTreeMap<String, RegisteredFunction<H>>,
}
impl<H> FunctionRegistry<H> {
    pub fn new() -> Self {
        Self {
            entries: BTreeMap::new(),
        }
    }
    /// Register one owned handle under its logical identity. Duplicate
    /// calls are refused; the existing binding is left untouched.
    pub fn register(
        &mut self,
        identity: ProjectFunctionIdentity,
        signature: LogicalSignature,
        handle: H,
    ) -> Result<(), RegistryError> {
        if self.entries.contains_key(&identity.call) {
            return Err(RegistryError::Duplicate {
                call: identity.call.clone(),
            });
        }
        self.entries.insert(
            identity.call.clone(),
            RegisteredFunction {
                identity,
                signature,
                handle,
            },
        );
        Ok(())
    }
    /// Explicitly replace a previously registered binding. Refused when
    /// no binding exists, so a typo cannot silently create a registration.
    pub fn replace(
        &mut self,
        identity: ProjectFunctionIdentity,
        signature: LogicalSignature,
        handle: H,
    ) -> Result<(), RegistryError> {
        if !self.entries.contains_key(&identity.call) {
            return Err(RegistryError::NotRegistered {
                call: identity.call.clone(),
            });
        }
        self.entries.insert(
            identity.call.clone(),
            RegisteredFunction {
                identity,
                signature,
                handle,
            },
        );
        Ok(())
    }
    pub fn contains(&self, call: &str) -> bool {
        self.entries.contains_key(call)
    }
    pub fn len(&self) -> usize {
        self.entries.len()
    }
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
    /// Freeze the selected bindings for one activation attempt. The
    /// mutable registry is consumed; no binding can change afterwards.
    pub fn freeze(self) -> FrozenRegistry<H> {
        FrozenRegistry {
            entries: self.entries,
        }
    }
}

/// Immutable bindings for a single activation attempt. Handles live exactly
/// as long as this value; activation borrows them and never clones them.
#[derive(Debug)]
pub struct FrozenRegistry<H> {
    entries: BTreeMap<String, RegisteredFunction<H>>,
}
impl<H> FrozenRegistry<H> {
    /// Resolve one logical identity or refuse the missing binding.
    pub fn resolve(
        &self,
        identity: &ProjectFunctionIdentity,
    ) -> Result<&RegisteredFunction<H>, MissingBinding> {
        self.entries
            .get(&identity.call)
            .filter(|entry| entry.identity == *identity)
            .ok_or_else(|| MissingBinding {
                call: identity.call.clone(),
            })
    }
    pub fn len(&self) -> usize {
        self.entries.len()
    }
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("function registry missing binding: call={call:?}")]
pub struct MissingBinding {
    pub call: String,
}

#[derive(Debug, PartialEq, thiserror::Error)]
pub enum RegistryActivationFailure<E> {
    #[error("registered activation function language mismatch: function={function:?}")]
    FunctionLanguageMismatch { function: usize },
    #[error("registered activation duplicate selection: function={function:?}")]
    DuplicateSelection { function: usize },
    #[error("registered activation plan: function={function:?}, error={error:?}")]
    Plan { function: usize, error: PlanError },
    #[error("registered activation missing binding: function={function:?}, call={call:?}")]
    MissingBinding { function: usize, call: String },
    #[error("registered activation signature mismatch: function={function:?}")]
    SignatureMismatch { function: usize },
    #[error("registered activation interrupted: function={function:?}, case={case:?}")]
    Interrupted { function: usize, case: usize },
    #[error("registered activation tests: {0:?}")]
    Tests(Vec<RegistryTestFailure<E>>),
}

#[derive(Debug, PartialEq, thiserror::Error)]
pub enum RegistryTestFailure<E> {
    #[error("registered activation batch: function={function:?}, case={case:?}, error={error:?}")]
    Batch {
        function: usize,
        case: usize,
        error: BatchFailure<E>,
    },
    #[error("registered activation result: function={function:?}, case={case:?}, actual={actual:?}, expected={expected:?}")]
    Result {
        function: usize,
        case: usize,
        actual: Value,
        expected: Value,
    },
}

/// A function whose binding was resolved from the frozen registry and
/// whose authored conformance cases all passed in this attempt.
#[derive(Debug)]
pub struct RegisteredActivatedFunction<'a, H> {
    pub plan: crate::function_signature::ProjectInvocationPlan,
    pub registered: &'a RegisteredFunction<H>,
    pub cases: Vec<crate::project_activation::CaseObservation>,
}

/// Activate selected functions from a frozen caller-supplied registry.
///
/// Selection order defines stable slots, mirroring scalar activation.
/// Every binding is resolved and signature-checked before any case runs;
/// missing bindings refuse the whole attempt. Every authored case then
/// runs through the batch contract (one row per case, short-circuited
/// cases included so their expected missing results are still checked
/// without entering the callback). Host interrupts abort immediately;
/// ordinary case failures collect in declaration order. No successful
/// evidence is retained anywhere, so a repeated attempt re-runs all of
/// it, and a failed attempt grants no study authority.
pub fn activate_registered<'a, P: HostFunctions>(
    language: Language,
    functions: &[&Function],
    registry: &'a FrozenRegistry<P::Handle>,
    port: &mut P,
) -> Result<Vec<RegisteredActivatedFunction<'a, P::Handle>>, RegistryActivationFailure<P::Error>> {
    use RegistryActivationFailure as Failure;
    if functions.is_empty() {
        return Ok(Vec::new());
    }
    let mut names = std::collections::BTreeSet::new();
    let mut plans = Vec::with_capacity(functions.len());
    for (function, definition) in functions.iter().enumerate() {
        if definition.language() != Some(language) {
            return Err(Failure::FunctionLanguageMismatch { function });
        }
        if !names.insert(definition.name()) {
            return Err(Failure::DuplicateSelection { function });
        }
        plans.push(
            definition
                .invocation_plan()
                .map_err(|error| Failure::Plan { function, error })?,
        );
    }
    let mut resolved = Vec::with_capacity(plans.len());
    for (function, plan) in plans.iter().enumerate() {
        let entry =
            registry
                .resolve(plan.identity())
                .map_err(|missing| Failure::MissingBinding {
                    function,
                    call: missing.call,
                })?;
        if entry.signature() != plan.signature() {
            return Err(Failure::SignatureMismatch { function });
        }
        resolved.push(entry);
    }
    let mut failures = Vec::new();
    let mut activated = Vec::with_capacity(plans.len());
    for (function, ((definition, plan), entry)) in
        functions.iter().zip(plans).zip(resolved).enumerate()
    {
        let rows: Vec<BTreeMap<String, Value>> = definition
            .definition()
            .tests
            .iter()
            .map(|test| {
                test.args
                    .iter()
                    .map(|(name, value)| (name.name.clone(), value.clone()))
                    .collect()
            })
            .collect();
        let mut cases = Vec::with_capacity(rows.len());
        if rows.is_empty() {
            activated.push(RegisteredActivatedFunction {
                plan,
                registered: entry,
                cases,
            });
            continue;
        }
        match invoke_batch(&plan, entry.handle(), &rows, port) {
            Ok(results) => {
                for (case, (test, actual)) in definition
                    .definition()
                    .tests
                    .iter()
                    .zip(results)
                    .enumerate()
                {
                    // A case is invoked unless scalar semantics would
                    // short-circuit it: an effective missing value for a
                    // parameter that rejects missing never enters the host.
                    let invoked = !plan.signature().parameters().iter().any(|parameter| {
                        let effective =
                            rows[case]
                                .get(&parameter.name)
                                .or(match &parameter.presence {
                                    crate::function_signature::Presence::Optional(default) => {
                                        Some(default)
                                    }
                                    crate::function_signature::Presence::Required => None,
                                });
                        effective == Some(&Value::Missing) && !parameter.accepts_missing
                    });
                    if results_match(
                        &actual,
                        &test.result,
                        definition.definition().comparison_decimals,
                    ) {
                        cases.push(crate::project_activation::CaseObservation {
                            id: test.id.text.clone(),
                            actual,
                            invoked,
                        });
                    } else {
                        failures.push(RegistryTestFailure::Result {
                            function,
                            case,
                            actual,
                            expected: test.result.clone(),
                        });
                    }
                }
            }
            Err(error) => {
                let interrupted = match &error.kind {
                    BatchFailureKind::Scalar(
                        crate::function_invocation::FailureKind::CallFailed(payload),
                    )
                    | BatchFailureKind::Scalar(
                        crate::function_invocation::FailureKind::InvalidHostResult(payload),
                    ) => port.is_interrupt(payload),
                    _ => false,
                };
                if interrupted {
                    return Err(Failure::Interrupted {
                        function,
                        case: error.row.unwrap_or(0),
                    });
                }
                // One batch covers every case of this function; attribute
                // the failure to its row when known, else to the first case.
                failures.push(RegistryTestFailure::Batch {
                    function,
                    case: error.row.unwrap_or(0),
                    error,
                });
            }
        }
        activated.push(RegisteredActivatedFunction {
            plan,
            registered: entry,
            cases,
        });
    }
    if failures.is_empty() {
        Ok(activated)
    } else {
        Err(Failure::Tests(failures))
    }
}
