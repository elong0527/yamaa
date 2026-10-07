//! Immutable function contracts admitted without activating or invoking host code.
use crate::value::{ColumnType, Value, ValueType};
use alloc::{collections::BTreeSet, string::String, vec::Vec};

/// Resolved identity retained on every fatal invocation failure (REQ-0704).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FunctionIdentity {
    pub name: String,
    pub contract_version: String,
    pub implementation_version: String,
    pub call: String,
}

/// Required parameters have no default; optional parameters always have one.
/// A missing default remains distinct from an absent default (REQ-0676).
#[derive(Clone, Debug, PartialEq)]
pub enum Presence {
    Required,
    Optional(Value),
}

/// One normalized parameter and its already-resolved host name.
#[derive(Clone, Debug, PartialEq)]
pub struct Parameter {
    pub name: String,
    pub host_name: String,
    pub kind: ValueType,
    pub accepts_missing: bool,
    pub presence: Presence,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlanError {
    EmptyIdentity,
    InvalidName { parameter: usize },
    DuplicateName { parameter: usize },
    EmptyHostName { parameter: usize },
    DuplicateHostName { parameter: usize },
    InvalidDefault { parameter: usize },
}

/// Immutable logical invocation plan. Admission budgets for materialized names,
/// values and callbacks belong to the caller; this is not an untrusted byte API.
#[derive(Clone, Debug, PartialEq)]
pub struct InvocationPlan {
    identity: FunctionIdentity,
    parameters: Vec<Parameter>,
    returns: ColumnType,
    may_return_missing: bool,
}

impl InvocationPlan {
    /// Validate the normalized closed signature without calling project code.
    /// Host identifier syntax, qualified callable resolution and activation are
    /// prerequisites owned by the selected host's environment compiler.
    pub fn new(
        identity: FunctionIdentity,
        parameters: Vec<Parameter>,
        returns: ColumnType,
        may_return_missing: bool,
    ) -> Result<Self, PlanError> {
        if [
            &identity.name,
            &identity.contract_version,
            &identity.implementation_version,
            &identity.call,
        ]
        .iter()
        .any(|s| s.is_empty())
        {
            return Err(PlanError::EmptyIdentity);
        }
        let mut names = BTreeSet::new();
        let mut host_names = BTreeSet::new();
        for (parameter, item) in parameters.iter().enumerate() {
            let mut bytes = item.name.bytes();
            if !bytes
                .next()
                .is_some_and(|b| b.is_ascii_alphabetic() || b == b'_')
                || !bytes.all(|b| b.is_ascii_alphanumeric() || b == b'_')
            {
                return Err(PlanError::InvalidName { parameter });
            }
            if !names.insert(&item.name) {
                return Err(PlanError::DuplicateName { parameter });
            }
            if item.host_name.is_empty() {
                return Err(PlanError::EmptyHostName { parameter });
            }
            if !host_names.insert(&item.host_name) {
                return Err(PlanError::DuplicateHostName { parameter });
            }
            if let Presence::Optional(default) = &item.presence {
                if default
                    .value_type()
                    .map_or(!item.accepts_missing, |t| t != item.kind)
                {
                    return Err(PlanError::InvalidDefault { parameter });
                }
            }
        }
        Ok(Self {
            identity,
            parameters,
            returns,
            may_return_missing,
        })
    }

    /// Retain immutable identity for diagnostics surrounding this application step.
    pub fn identity(&self) -> &FunctionIdentity {
        &self.identity
    }

    /// Inspect the immutable logical signature for static argument binding.
    pub fn parameters(&self) -> &[Parameter] {
        &self.parameters
    }

    /// Declared logical result type, before completed-result conversion.
    pub fn returns(&self) -> ColumnType {
        self.returns
    }

    /// Whether an explicit missing result belongs to the declared contract.
    pub fn may_return_missing(&self) -> bool {
        self.may_return_missing
    }
}
