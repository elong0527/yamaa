//! Capacities admitted before report ownership grows. These are explicit
//! projection/work policies, not allocator or process-memory guarantees.
use crate::specification_report::Error;
use serde_json::Value;
use yamaa_core::{table::ValueRef, value::Value as Scalar};

pub(crate) struct Budget {
    bytes: usize,
    cells: usize,
    work: usize,
}
impl Budget {
    pub(crate) fn new(bytes: usize) -> Self {
        Self {
            bytes: bytes.min(16_777_216),
            cells: 1_048_576,
            work: 67_108_864,
        }
    }
    pub(crate) fn remaining_bytes(&self) -> usize {
        self.bytes
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
            _ => self.text("012345678901234567890123456789012345678901234567890123456789012345"),
        }
    }
    pub(crate) fn cell(&mut self, value: &ValueRef<'_>) -> Result<(), Error> {
        match value {
            ValueRef::Str(text) => self.text(text),
            _ => self.text("012345678901234567890123456789012345678901234567890123456789012345"),
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
        }
    }
    /// Immutable JSON is charged completely before a caller clones it. Depth
    /// and actual traversal work are bounded independently of retained bytes.
    pub(crate) fn json(&mut self, value: &Value) -> Result<(), Error> {
        self.json_at(value, 0)
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
