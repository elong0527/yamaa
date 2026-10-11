//! Borrow retained verification records; no table, host call, or check is repeated.
use crate::dataset::CheckRecord;
pub use core::{Error, Phase, Projection};
use yamaa_core::{specification::PreparedSpecification, verification_diagnostics as core};

pub fn project<'a>(
    compiled: &'a PreparedSpecification,
    records: &'a [CheckRecord],
    phase: Phase,
    specification: &'a str,
) -> Result<Projection<'a>, Error> {
    core::project(
        compiled,
        &crate::project_codelist_diagnostics::observations(records),
        phase,
        specification,
    )
}
