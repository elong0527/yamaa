//! Shared closed function-signature admission, independent of supplied call arguments.
use crate::{function_transport::FunctionTransportError as Error, scalar_transport::ScalarValue};
use serde::Deserialize;
use yamaa_core::value::{ColumnType, ValueType};
use yamaa_engine::function_invocation::{FunctionIdentity, InvocationPlan, Parameter, Presence};

pub(crate) const MAX_PARAMETERS: usize = 256;
const MAX_NAME_BYTES: usize = 1024;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Identity {
    name: String,
    contract_version: String,
    implementation_version: String,
    call: String,
}
#[derive(Deserialize, Clone, Copy)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Kind {
    Str,
    Int,
    Float,
    Bool,
    Date,
    Datetime,
}
impl Kind {
    /// Closed parameter vocabulary includes Boolean only at the function boundary.
    fn core(self) -> ValueType {
        match self {
            Self::Str => ValueType::Str,
            Self::Int => ValueType::Int,
            Self::Float => ValueType::Float,
            Self::Bool => ValueType::Bool,
            Self::Date => ValueType::Date,
            Self::Datetime => ValueType::DateTime,
        }
    }
    /// Column/result vocabulary deliberately excludes Boolean.
    fn column(self) -> Result<ColumnType, Error> {
        Ok(match self {
            Self::Str => ColumnType::Str,
            Self::Int => ColumnType::Int,
            Self::Float => ColumnType::Float,
            Self::Bool => return Err(Error::InvalidSignature),
            Self::Date => ColumnType::Date,
            Self::Datetime => ColumnType::DateTime,
        })
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WireParameter {
    name: String,
    host_name: String,
    #[serde(rename = "type")]
    kind: Kind,
    accepts_missing: bool,
    presence: WirePresence,
}
#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
enum WirePresence {
    Required(()),
    Optional(ScalarValue),
}
/// Wire signature reused by scalar invocation and dataset callback admission.
/// Host identifier syntax and callable activation remain outer adapter gates.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Signature {
    pub identity: Identity,
    pub parameters: Vec<WireParameter>,
    pub returns: Kind,
    pub may_return_missing: bool,
}
impl Signature {
    /// Bound all signature metadata and defaults before constructing the trusted plan.
    pub(crate) fn prepare(self) -> Result<InvocationPlan, Error> {
        if self.parameters.len() > MAX_PARAMETERS {
            return Err(Error::RequestLimit);
        }
        for text in [
            &self.identity.name,
            &self.identity.contract_version,
            &self.identity.implementation_version,
            &self.identity.call,
        ] {
            check_name(text)?;
        }
        let identity = FunctionIdentity {
            name: self.identity.name,
            contract_version: self.identity.contract_version,
            implementation_version: self.identity.implementation_version,
            call: self.identity.call,
        };
        let mut parameters = Vec::new();
        for p in self.parameters {
            check_name(&p.name)?;
            check_name(&p.host_name)?;
            parameters.push(Parameter {
                name: p.name,
                host_name: p.host_name,
                kind: p.kind.core(),
                accepts_missing: p.accepts_missing,
                presence: match p.presence {
                    WirePresence::Required(()) => Presence::Required,
                    WirePresence::Optional(s) => {
                        Presence::Optional(s.into_core().map_err(|_| Error::InvalidScalar)?)
                    }
                },
            });
        }
        InvocationPlan::new(
            identity,
            parameters,
            self.returns.column()?,
            self.may_return_missing,
        )
        .map_err(|_| Error::InvalidSignature)
    }
}
/// Bound every name before cloning, sorting or echoing diagnostic context.
pub(crate) fn check_name(name: &str) -> Result<(), Error> {
    if name.len() > MAX_NAME_BYTES {
        Err(Error::RequestLimit)
    } else if name.is_empty() {
        Err(Error::InvalidRequest)
    } else {
        Ok(())
    }
}
