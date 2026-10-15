//! Producer header admission before physical source typing.
use crate::{
    diagnostic::Diagnostic,
    producer_contract::{Contract, Error, Limits},
};
pub fn validate_header(
    contract: &Contract,
    source: &str,
    header: &[&str],
    limits: Limits,
) -> Result<Option<Diagnostic>, Error> {
    contract.validate_header(source, header, limits)
}
