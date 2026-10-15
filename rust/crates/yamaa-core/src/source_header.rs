//! Producer header admission before physical source typing.
use {
    crate::diagnostic::Diagnostic, crate::producer_contract::Contract,
    crate::producer_contract::Error, crate::producer_contract::Limits,
};
pub fn validate_header(
    contract: &Contract,
    source: &str,
    header: &[&str],
    limits: Limits,
) -> Result<Option<Diagnostic>, Error> {
    contract.validate_header(source, header, limits)
}
