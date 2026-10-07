//! Resolve authored scalar arguments before the shared exact-type invocation lifecycle.
use super::*;
use crate::{
    dataset_predicate::Read,
    function_invocation::{Argument, FunctionPort, HostError, InvocationPlan, Presence},
};
use alloc::collections::BTreeSet;

/// Already admitted literal or statically bound current-candidate value.
#[derive(Clone, Debug, PartialEq)]
pub enum FunctionInput {
    Literal(Value),
    Read(Read),
    /// Distinct present readings across every record feeding a key-grain output.
    Collect {
        column: usize,
        identifier: String,
    },
}

/// One supplied logical argument, retained in authored order (not signature order).
#[derive(Clone, Debug, PartialEq)]
pub struct FunctionArgument {
    pub name: String,
    pub input: FunctionInput,
}

/// Immutable invocation plus source/output bindings; activation remains an outer gate.
#[derive(Clone, Debug, PartialEq)]
pub struct BoundFunction {
    slot: usize,
    signature: InvocationPlan,
    arguments: Vec<FunctionArgument>,
}

impl BoundFunction {
    /// Reject duplicate/unknown names and absent required declarations before execution.
    /// Metadata and literal admission budgets belong to the compiler, as for InvocationPlan.
    pub fn new(
        slot: usize,
        signature: InvocationPlan,
        arguments: Vec<FunctionArgument>,
    ) -> Result<Self, PlanError> {
        let mut names = BTreeSet::new();
        for argument in &arguments {
            if !names.insert(argument.name.as_str())
                || !signature
                    .parameters()
                    .iter()
                    .any(|p| p.name == argument.name)
            {
                return Err(PlanError::InvalidFunction);
            }
        }
        if signature
            .parameters()
            .iter()
            .any(|p| matches!(p.presence, Presence::Required) && !names.contains(p.name.as_str()))
        {
            return Err(PlanError::InvalidFunction);
        }
        Ok(Self {
            slot,
            signature,
            arguments,
        })
    }

    /// Identify the caller-owned callback slot without resolving project code.
    pub fn slot(&self) -> usize {
        self.slot
    }

    /// Match the entire immutable signature and identity against activated bindings.
    pub fn signature(&self) -> &InvocationPlan {
        &self.signature
    }

    /// Key-grain non-key calls must not choose a single feeding source row.
    pub(super) fn reads_source(&self) -> bool {
        self.arguments
            .iter()
            .any(|a| matches!(a.input, FunctionInput::Read(Read::Source(_))))
    }

    /// Validate completed outputs and record/group source scope without reading cells.
    pub(super) fn validate(
        &self,
        source: &TableSchema,
        available: &[bool],
        mode: &RowMode,
    ) -> Result<(), PlanError> {
        for argument in &self.arguments {
            match &argument.input {
                FunctionInput::Read(Read::Source(column)) => {
                    if *column >= source.columns().len() {
                        return Err(PlanError::InvalidSource);
                    }
                    if matches!(mode, RowMode::Groups(keys) if !keys.contains(column)) {
                        return Err(PlanError::NonGroupSource);
                    }
                }
                FunctionInput::Read(Read::Column(column))
                    if !available.get(*column).copied().unwrap_or(false) =>
                {
                    return Err(PlanError::UnavailableColumn);
                }
                FunctionInput::Collect { column, identifier } => {
                    if *column >= source.columns().len() {
                        return Err(PlanError::InvalidSource);
                    }
                    if !matches!(mode, RowMode::Keys) || identifier.is_empty() {
                        return Err(PlanError::InvalidKeyMode);
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }
}

/// Caller-owned activated functions. Metadata inspection must not execute project code.
/// Signatures and slot mappings stay stable throughout a run. Callbacks use the same
/// opaque error type as table ports (an adapter may use an enum for both payloads).
/// No Send/Sync or thread migration is required. Host allocation, interruption and
/// exception containment remain the adapter's responsibility, as for FunctionPort.
pub trait FunctionBindings {
    type Error;
    /// Borrow immutable resolved metadata; return None for an unavailable slot.
    fn signature(&self, slot: usize) -> Option<&InvocationPlan>;
    /// Invoke the selected already-bound callback with declaration-order host arguments.
    fn call(
        &mut self,
        slot: usize,
        arguments: &[Argument<'_>],
    ) -> Result<Value, HostError<Self::Error>>;
}

pub(super) struct UnavailableFunctions<E>(pub core::marker::PhantomData<E>);
impl<E> FunctionBindings for UnavailableFunctions<E> {
    type Error = E;
    /// Legacy execution has no implicit callback resolution.
    fn signature(&self, _: usize) -> Option<&InvocationPlan> {
        None
    }
    /// Signature admission prevents any callback attempt through the legacy entrypoint.
    fn call(&mut self, _: usize, _: &[Argument<'_>]) -> Result<Value, HostError<E>> {
        unreachable!("callback signatures were admitted before table access")
    }
}

struct Selected<'a, E> {
    bindings: &'a mut dyn FunctionBindings<Error = E>,
    slot: usize,
}
impl<E> FunctionPort for Selected<'_, E> {
    type Error = E;
    /// Forward exactly one invocation to its stable activated slot.
    fn call(&mut self, arguments: &[Argument<'_>]) -> Result<Value, HostError<E>> {
        self.bindings.call(self.slot, arguments)
    }
}

pub(super) struct Context<'a> {
    pub plan: &'a DatasetPlan,
    pub row: usize,
    pub assignment: &'a Assignment,
}

/// Resolve every supplied argument in authored order, then apply the shared signature rules.
pub(super) fn evaluate<T: TableAccess + ?Sized>(
    function: &BoundFunction,
    table: &T,
    candidate: &Candidate,
    context: Context<'_>,
    budget: &mut Budget,
    handlers: &mut HandlerCounter,
    bindings: &mut dyn FunctionBindings<Error = T::Error>,
) -> Result<Value, Box<ExecutionError<T::Error>>> {
    let mut supplied = BTreeMap::new();
    for argument in &function.arguments {
        budget.work(1, 1)?;
        if let FunctionInput::Collect { column, identifier } = &argument.input {
            budget.work(candidate.members.len(), 1)?;
            let value = key_grain::collect_bound(
                table,
                key_grain::Collection {
                    column: *column,
                    identifier,
                    filter: None,
                    selection: None,
                },
                key_grain::CollectionContext {
                    assignment: context.assignment,
                    candidate,
                    plan: context.plan,
                    row: context.row,
                },
                budget,
                handlers,
            )?;
            supplied.insert(argument.name.clone(), value);
            continue;
        }
        let value = match &argument.input {
            FunctionInput::Collect { .. } => unreachable!("collected input was resolved above"),
            FunctionInput::Literal(value) => ValueRef::from(value),
            FunctionInput::Read(Read::Column(column)) => ValueRef::from(&candidate.values[*column]),
            FunctionInput::Read(Read::Source(column)) => {
                let source_row = candidate.members[0];
                table.cell(source_row, *column).map_err(|error| {
                    Box::new(ExecutionError::Cell {
                        path: context.assignment.path.clone(),
                        source_row,
                        error,
                    })
                })?
            }
        };
        if let ValueRef::Str(text) = value {
            budget.scalar_text(text.len())?;
        }
        supplied.insert(argument.name.clone(), own(value));
    }
    // This charges a potential call; missing-value short circuit never executes host code.
    budget.work(1, 1)?;
    let result = crate::function_invocation::invoke(
        &function.signature,
        &supplied,
        &mut Selected {
            bindings,
            slot: function.slot,
        },
    );
    match result {
        Ok(value) => {
            if let Value::Str(text) = &value {
                budget.scalar_text(text.len())?;
            }
            Ok(value)
        }
        Err(error) => Err(Box::new(ExecutionError::Function {
            path: context.assignment.path.clone(),
            identity: failure_identity(candidate, &context.plan.keys, context.row, budget)?,
            error,
        })),
    }
}
