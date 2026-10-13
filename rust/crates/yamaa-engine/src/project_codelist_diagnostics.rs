//! Borrow complete retained codelist records without table access or retries.
use crate::dataset::{CheckRecord, RowIdentity};
use alloc::vec::Vec;
use yamaa_core::{
    project_codelist_diagnostics::{self as core, Error},
    project_diagnostics::Diagnostic,
    specification::PreparedSpecification,
    value::Value,
};
impl core::RowObservation for RowIdentity {
    fn position(&self) -> usize {
        self.position
    }
    fn values(&self) -> &[Value] {
        &self.values
    }
}
pub fn diagnostics<'a>(
    compiled: &'a PreparedSpecification,
    records: &'a [CheckRecord],
    maximum: usize,
) -> Result<Vec<Diagnostic<'a>>, Error> {
    if records.len() > 65_536 {
        return Err(Error::Limit);
    }
    let facts = observations(records);
    core::diagnostics(compiled, &facts, maximum)
}

pub(crate) fn observations(
    records: &[CheckRecord],
) -> Vec<core::CheckObservation<'_, RowIdentity>> {
    records
        .iter()
        .map(|record| core::CheckObservation {
            path: &record.path,
            code: record.code,
            evaluated_count: record.evaluated_count,
            failed_count: record.failed_count,
            output_rows: record.output_rows,
            offending_rows: &record.offending_rows,
            inventory: record.inventory.as_deref(),
            codelist: record
                .codelist
                .as_ref()
                .map(|observation| core::CodelistObservation {
                    id: &observation.id,
                    values: &observation.values,
                }),
        })
        .collect()
}
