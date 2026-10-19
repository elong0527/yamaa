//! Pure dataset declarations. No study data or host effects.
use super::*;
use crate::{
    bound_expression::Read,
    function_signature::{LogicalSignature, Presence, ProjectInvocationPlan},
};
use std::collections::BTreeSet;
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
pub struct BoundFunction<S> {
    slot: usize,
    signature: S,
    arguments: Vec<FunctionArgument>,
}

/// Versionless call to a package admitted from the explicit environment.
pub type BoundProjectFunction = BoundFunction<ProjectInvocationPlan>;
impl BoundProjectFunction {
    pub fn new_project(
        slot: usize,
        signature: ProjectInvocationPlan,
        arguments: Vec<FunctionArgument>,
    ) -> Result<Self, PlanError> {
        validate_arguments(signature.signature(), &arguments)?;
        Ok(Self {
            slot,
            signature,
            arguments,
        })
    }
}

fn validate_arguments(
    signature: &LogicalSignature,
    arguments: &[FunctionArgument],
) -> Result<(), PlanError> {
    let mut names = BTreeSet::new();
    for argument in arguments {
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
    Ok(())
}

impl<S> BoundFunction<S> {
    /// Identify the caller-owned callback slot without resolving project code.
    pub fn slot(&self) -> usize {
        self.slot
    }

    /// Match the entire immutable signature and identity against activated bindings.
    pub fn signature(&self) -> &S {
        &self.signature
    }

    /// Borrow supplied arguments in authored order without changing the admitted call.
    pub fn arguments(&self) -> &[FunctionArgument] {
        &self.arguments
    }
}
