//! Capacities admitted before report ownership grows. These are explicit
//! projection/work policies, not allocator or process-memory guarantees.
use crate::specification_report::Error;
use serde_json::Value;
use yamaa_core::{table::ValueRef, value::Value as Scalar};

pub(crate) struct Budget {
    bytes: usize,
    cells: usize,
    work: usize,
    initial_bytes: usize,
    initial_work: usize,
}
impl Budget {
    pub(crate) fn new(bytes: usize) -> Self {
        let bytes = bytes
            .min(yamaa_engine::capacity::REPORT_ENCODED_BYTES)
            .saturating_mul(yamaa_engine::capacity::REPORT_RETAINED_FACTOR)
            .min(yamaa_engine::capacity::REPORT_RETAINED_BYTES);
        Self {
            bytes,
            cells: 1_048_576,
            work: 67_108_864,
            initial_bytes: bytes,
            initial_work: 67_108_864,
        }
    }
    pub(crate) fn remaining_bytes(&self) -> usize {
        self.bytes / yamaa_engine::capacity::REPORT_RETAINED_FACTOR
    }
    pub(crate) fn work(&mut self, count: usize) -> Result<(), Error> {
        self.work = self.work.checked_sub(count).ok_or(Error::OutputLimit)?;
        Ok(())
    }
    pub(crate) fn entries(&mut self, count: usize) -> Result<(), Error> {
        self.work(count)?;
        self.bytes = self
            .bytes
            .checked_sub(count.checked_mul(128).ok_or(Error::OutputLimit)?)
            .ok_or(Error::OutputLimit)?;
        Ok(())
    }
    pub(crate) fn retained_bytes(&mut self, count: usize) -> Result<(), Error> {
        self.bytes = self.bytes.checked_sub(count).ok_or(Error::OutputLimit)?;
        Ok(())
    }
    pub(crate) fn row_shape(&mut self, rows: usize, columns: usize) -> Result<(), Error> {
        let cells = rows.checked_mul(columns).ok_or(Error::OutputLimit)?;
        self.cells = self.cells.checked_sub(cells).ok_or(Error::OutputLimit)?;
        self.entries(columns.checked_add(1).ok_or(Error::OutputLimit)?)?;
        self.work(cells)
    }
    pub(crate) fn shared_report(
        &mut self,
        report: &crate::report_storage::StoredReport,
    ) -> Result<(), Error> {
        self.json(report)?;
        self.entries(report.row_blocks())
    }
    pub(crate) fn text(&mut self, text: &str) -> Result<(), Error> {
        self.work(text.len().checked_mul(2).ok_or(Error::OutputLimit)?)?;
        self.bytes = self
            .bytes
            .checked_sub(
                text.len()
                    .checked_mul(12)
                    .and_then(|n| n.checked_add(128))
                    .ok_or(Error::OutputLimit)?,
            )
            .ok_or(Error::OutputLimit)?;
        Ok(())
    }
    pub(crate) fn shape(&mut self, rows: usize, columns: usize) -> Result<(), Error> {
        let cells = rows.checked_mul(columns).ok_or(Error::OutputLimit)?;
        self.cells = self.cells.checked_sub(cells).ok_or(Error::OutputLimit)?;
        self.entries(rows)?;
        self.entries(columns)?;
        self.entries(cells)?;
        self.work(cells)?;
        Ok(())
    }
    pub(crate) fn scalar(&mut self, value: &Scalar) -> Result<(), Error> {
        match value {
            Scalar::Str(text) => self.text(text),
            Scalar::Int(value) => self.text(&value.to_string()),
            Scalar::Float(_) => self.text("0123456789012345"),
            Scalar::Date(value) => self.text(&value.to_string()),
            Scalar::DateTime(value) => self.text(&value.to_string()),
            Scalar::Missing | Scalar::Bool(_) => self.entries(1),
        }
    }
    pub(crate) fn cell(&mut self, value: &ValueRef<'_>) -> Result<(), Error> {
        // The wire cell owns a two-entry type/value map, its key/type strings
        // and the reserved map node. Shape already charged its parent slot.
        // Admit that fixed structure before allocating the actual payload.
        self.entries(4)?;
        match value {
            ValueRef::Str(text) => self.text(text),
            ValueRef::Int(value) => self.text(&value.to_string()),
            ValueRef::Float(_) => self.text("0123456789012345"),
            ValueRef::Date(value) => self.text(&value.to_string()),
            ValueRef::DateTime(value) => self.text(&value.to_string()),
            ValueRef::Missing | ValueRef::Bool(_) => self.entries(1),
        }
    }
    pub(crate) fn check(&mut self, check: &yamaa_core::dataset::Check) -> Result<(), Error> {
        use yamaa_core::dataset::Check;
        self.entries(32)?;
        for _ in 0..8 {
            match check {
                Check::AllowedValues(values) | Check::Codelist { values, .. } => {
                    self.entries(values.len())?;
                    for value in values {
                        self.scalar(value)?;
                    }
                }
                Check::Range { min, max } => {
                    for value in min.iter().chain(max) {
                        self.scalar(value)?;
                    }
                }
                Check::InvalidDiagnostic(value) => self.diagnostic(value)?,
                Check::InvalidDeclaration { reason, .. } => self.text(reason)?,
                _ => {}
            }
        }
        Ok(())
    }
    pub(crate) fn diagnostic(
        &mut self,
        value: &yamaa_core::diagnostic::Diagnostic,
    ) -> Result<(), Error> {
        self.entries(16)?;
        self.entries(value.spec_paths.len())?;
        for path in &value.spec_paths {
            self.text(path)?;
        }
        self.entries(value.context.len())?;
        for (key, value) in &value.context {
            self.text(key)?;
            self.context(value, 0)?;
        }
        if let Some(route) = &value.operand_route {
            self.entries(route.len())?;
            for path in route {
                self.text(path)?;
            }
        }
        Ok(())
    }
    fn context(
        &mut self,
        value: &yamaa_core::diagnostic::ContextValue,
        depth: usize,
    ) -> Result<(), Error> {
        use yamaa_core::diagnostic::ContextValue;
        if depth > 64 {
            return Err(Error::OutputLimit);
        }
        self.entries(1)?;
        match value {
            ContextValue::Scalar(value) => self.scalar(value),
            ContextValue::Integer(value) => self.text(value),
            ContextValue::Sequence(values) => {
                self.entries(values.len())?;
                for value in values {
                    self.context(value, depth + 1)?;
                }
                Ok(())
            }
            ContextValue::Object(values) => {
                self.entries(values.len())?;
                for (key, value) in values {
                    self.text(key)?;
                    self.context(value, depth + 1)?;
                }
                Ok(())
            }
        }
    }
    /// Immutable JSON is charged completely before a caller clones it. Depth
    /// and actual traversal work are bounded independently of retained bytes.
    pub(crate) fn json(&mut self, value: &Value) -> Result<(), Error> {
        self.json_at(value, 0)
    }
    pub(crate) fn json_object_without(
        &mut self,
        value: &Value,
        omitted: &str,
    ) -> Result<(), Error> {
        let values = value.as_object().ok_or(Error::InvalidObservation)?;
        self.entries(1)?;
        self.entries(
            values
                .keys()
                .filter(|name| name.as_str() != omitted)
                .count(),
        )?;
        for (name, value) in values.iter().filter(|(name, _)| name.as_str() != omitted) {
            self.text(name)?;
            self.json_at(value, 1)?;
        }
        Ok(())
    }
    fn json_at(&mut self, value: &Value, depth: usize) -> Result<(), Error> {
        if depth > 64 {
            return Err(Error::OutputLimit);
        }
        self.entries(1)?;
        match value {
            Value::String(text) => self.text(text),
            Value::Array(values) => {
                self.entries(values.len())?;
                for value in values {
                    self.json_at(value, depth + 1)?;
                }
                Ok(())
            }
            Value::Object(values) => {
                self.entries(values.len())?;
                for (name, value) in values {
                    self.text(name)?;
                    self.json_at(value, depth + 1)?;
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }
}

impl Drop for Budget {
    fn drop(&mut self) {
        if std::env::var_os("YAMAA_REPORT_CAPACITY_TRACE").is_some() {
            use std::io::Write;
            let _ = writeln!(
                std::io::stderr(),
                "report-capacity retained_limit={} retained_charged={} cells_charged={} work_charged={}",
                self.initial_bytes,
                self.initial_bytes - self.bytes,
                1_048_576 - self.cells,
                self.initial_work - self.work
            );
        }
    }
}

/// Count the actual compact encoded representation without first allocating
/// an unbounded temporary String. The writer stops at the admitted ceiling.
pub(crate) fn encoded_within_limit(
    value: &impl serde::Serialize,
    maximum: usize,
) -> Result<(), Error> {
    struct Counter {
        remaining: usize,
        exceeded: bool,
    }
    impl std::io::Write for Counter {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            match self.remaining.checked_sub(bytes.len()) {
                Some(remaining) => {
                    self.remaining = remaining;
                    Ok(bytes.len())
                }
                None => {
                    self.exceeded = true;
                    Err(std::io::Error::other("encoded report capacity"))
                }
            }
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut counter = Counter {
        remaining: maximum,
        exceeded: false,
    };
    match serde_json::to_writer(&mut counter, value) {
        Ok(()) => Ok(()),
        Err(_) if counter.exceeded => Err(Error::OutputLimit),
        Err(_) => Err(Error::InvalidObservation),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encoded_ceiling_counts_unicode_escaping_and_container_syntax_exactly() {
        let value = serde_json::json!({"value": ["é\n\"", null, true, -17]});
        let exact = serde_json::to_vec(&value).unwrap().len();
        assert!(encoded_within_limit(&value, exact).is_ok());
        assert!(matches!(
            encoded_within_limit(&value, exact - 1),
            Err(Error::OutputLimit)
        ));
    }

    #[test]
    fn encoded_ceiling_stops_a_large_value_without_a_temporary_encoded_copy() {
        let value = Value::Array(vec![Value::String("x".repeat(4096)); 100]);
        assert!(matches!(
            encoded_within_limit(&value, 4096),
            Err(Error::OutputLimit)
        ));
    }
}
