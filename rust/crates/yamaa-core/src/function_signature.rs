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
    InvalidHostMapping,
    InvalidName { parameter: usize },
    DuplicateName { parameter: usize },
    EmptyHostName { parameter: usize },
    DuplicateHostName { parameter: usize },
    InvalidDefault { parameter: usize },
}

/// One admitted logical signature, independent of legacy or package identity.
#[derive(Clone, Debug, PartialEq)]
pub struct LogicalSignature {
    parameters: Vec<Parameter>,
    returns: ColumnType,
    may_return_missing: bool,
}
impl LogicalSignature {
    pub fn new(
        parameters: Vec<Parameter>,
        returns: ColumnType,
        may_return_missing: bool,
    ) -> Result<Self, PlanError> {
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
            parameters,
            returns,
            may_return_missing,
        })
    }
    pub fn parameters(&self) -> &[Parameter] {
        &self.parameters
    }
    pub fn returns(&self) -> ColumnType {
        self.returns
    }
    pub fn may_return_missing(&self) -> bool {
        self.may_return_missing
    }
}

/// Immutable legacy plan. Its required versions and existing wire contract remain.
#[derive(Clone, Debug, PartialEq)]
pub struct InvocationPlan {
    identity: FunctionIdentity,
    signature: LogicalSignature,
}
impl InvocationPlan {
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
        Ok(Self {
            identity,
            signature: LogicalSignature::new(parameters, returns, may_return_missing)?,
        })
    }
    pub fn identity(&self) -> &FunctionIdentity {
        &self.identity
    }
    pub fn signature(&self) -> &LogicalSignature {
        &self.signature
    }
    pub fn parameters(&self) -> &[Parameter] {
        self.signature.parameters()
    }
    pub fn returns(&self) -> ColumnType {
        self.signature.returns()
    }
    pub fn may_return_missing(&self) -> bool {
        self.signature.may_return_missing()
    }
}

/// New package-based identity has no contract or implementation version fields.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectFunctionIdentity {
    pub name: String,
    pub call: String,
}
#[derive(Clone, Debug)]
pub struct ProjectInvocationPlan {
    identity: ProjectFunctionIdentity,
    signature: LogicalSignature,
}
impl PartialEq for ProjectInvocationPlan {
    /// Default metadata must preserve the bits that the activated callable was
    /// tested against. Runtime value equality continues to compare both zeros
    /// numerically; only versionless plan identity requires this extra check.
    fn eq(&self, other: &Self) -> bool {
        self.identity == other.identity
            && self.signature == other.signature
            && self
                .signature
                .parameters
                .iter()
                .zip(&other.signature.parameters)
                .all(|(left, right)| match (&left.presence, &right.presence) {
                    (
                        Presence::Optional(Value::Float(left)),
                        Presence::Optional(Value::Float(right)),
                    ) => left.get().to_bits() == right.get().to_bits(),
                    _ => true,
                })
    }
}
impl ProjectInvocationPlan {
    /// Host syntax, complete static coverage and activation precede this trusted
    /// typed construction; declaration names also name the host arguments.
    pub fn new(
        identity: ProjectFunctionIdentity,
        parameters: Vec<Parameter>,
        returns: ColumnType,
        may_return_missing: bool,
    ) -> Result<Self, PlanError> {
        if identity.name.is_empty() || identity.call.is_empty() {
            return Err(PlanError::EmptyIdentity);
        }
        if parameters.iter().any(|p| p.name != p.host_name) {
            return Err(PlanError::InvalidHostMapping);
        }
        Ok(Self {
            identity,
            signature: LogicalSignature::new(parameters, returns, may_return_missing)?,
        })
    }
    pub fn identity(&self) -> &ProjectFunctionIdentity {
        &self.identity
    }
    pub fn signature(&self) -> &LogicalSignature {
        &self.signature
    }
}
