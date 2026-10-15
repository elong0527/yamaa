//! Producer header admission before physical source typing.
use yamaa_core::{
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
