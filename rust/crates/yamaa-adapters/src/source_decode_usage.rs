//! Receipts for the quota models enforced by native source decoding.
//! Independent stages reuse the supplied allowance; their charge is the maximum,
//! while each stage records its own cumulative admitted prefix. These are logical
//! admission charges, not an allocator or process-memory measurement.
use crate::arrow_table::TableError;

#[derive(Default, Debug)]
pub(crate) struct Usage {
    pub(crate) storage_bytes: usize,
    pub(crate) work_bytes: usize,
    pub(crate) complete: bool,
}
impl Usage {
    pub(crate) fn storage(&mut self, charge: usize) {
        self.storage_bytes = self.storage_bytes.max(charge);
    }
    pub(crate) fn work(&mut self, charge: usize) {
        self.work_bytes = self.work_bytes.max(charge);
    }
    pub(crate) fn complete(&mut self) {
        self.complete = true;
    }
}

pub(crate) fn charge(
    used: &mut usize,
    amount: usize,
    limit: usize,
    resource: &'static str,
) -> Result<(), TableError> {
    let required = used.checked_add(amount);
    if required.is_none_or(|n| n > limit) {
        return Err(TableError::Limit {
            resource,
            limit,
            required,
        });
    }
    *used = required.expect("admitted charge");
    Ok(())
}

pub(crate) fn parse_csv(
    bytes: &[u8],
    csv_limits: crate::csv_source::Limits,
    limits: yamaa_engine::producer_build::DecodeLimits,
    usage: &mut Usage,
) -> Result<crate::csv_source::CsvSource, crate::specification_run::Error> {
    use crate::{csv_source::TextTableError, specification_run::Error};
    // Both byte allowances are checked before UTF-8/scanner ownership grows.
    for (resource, limit) in [
        ("storage_bytes", limits.storage_bytes),
        ("work_bytes", limits.work_bytes),
    ] {
        let mut used = 0;
        charge(&mut used, bytes.len(), limit, resource)
            .map_err(|error| Error::Source(TextTableError::Table(error)))?;
        if resource == "storage_bytes" {
            usage.storage(used);
        } else {
            usage.work(used);
        }
    }
    let mut used = 0;
    let mut refusal = None;
    let result = yamaa_core::csv_source::parse_with_admission(bytes, csv_limits, |amount| {
        match charge(&mut used, amount, limits.storage_bytes, "storage_bytes") {
            Ok(()) => {
                usage.storage(used);
                Ok(())
            }
            Err(error) => {
                refusal = Some(error);
                Err(crate::csv_source::Error::Limit {
                    resource: "storage_bytes",
                    limit: limits.storage_bytes,
                })
            }
        }
    });
    match (result, refusal) {
        (_, Some(error)) => Err(Error::Source(TextTableError::Table(error))),
        (result, None) => result.map_err(|error| Error::Source(TextTableError::Csv(error))),
    }
}

/// Admit owned canonical slots and copied text before declared conversion.
pub(crate) fn csv_conversion(
    parsed: &crate::csv_source::CsvSource,
    limit: usize,
    usage: &mut Usage,
) -> Result<(), TableError> {
    let mut used = 0;
    for name in &parsed.names {
        charge(&mut used, 32, limit, "storage_bytes")?;
        usage.storage(used);
        charge(&mut used, name.len(), limit, "storage_bytes")?;
        usage.storage(used);
    }
    for record in &parsed.records {
        charge(&mut used, 24, limit, "storage_bytes")?;
        usage.storage(used);
        for value in record {
            charge(&mut used, 32, limit, "storage_bytes")?;
            usage.storage(used);
            if let Some(value) = value {
                charge(&mut used, value.len(), limit, "storage_bytes")?;
                usage.storage(used);
            }
        }
    }
    Ok(())
}
