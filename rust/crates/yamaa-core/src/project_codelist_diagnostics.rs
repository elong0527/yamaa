//! Complete borrowed codelist observations, independent of retained output.
use crate::{
    dataset::Check,
    diagnostic::ConditionCode,
    project_diagnostics::{Context, Diagnostic, Fact},
    specification::PreparedSpecification,
    value::Value,
};
use alloc::{collections::BTreeSet, vec::Vec};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    InvalidObservation,
    Limit,
    Projection,
}
/// A retained row identity; projection neither reads a table nor reruns a check.
pub trait RowObservation {
    fn position(&self) -> usize;
    fn values(&self) -> &[Value];
}
#[derive(Clone, Copy)]
pub struct CodelistObservation<'a> {
    pub id: &'a str,
    pub values: &'a [Value],
}
#[derive(Clone, Copy)]
pub struct RowCountObservation<'a> {
    pub counts: &'a [usize],
    pub denominators: Option<&'a [usize]>,
}
pub struct CheckObservation<'a, R> {
    pub path: &'a str,
    pub code: ConditionCode,
    pub evaluated_count: usize,
    pub failed_count: usize,
    pub output_rows: usize,
    pub offending_rows: &'a [R],
    pub codelist: Option<CodelistObservation<'a>>,
    pub inventory: Option<&'a [Value]>,
    pub row_count: Option<RowCountObservation<'a>>,
}
struct Budget(usize);
impl Budget {
    fn text(&mut self, text: &str) -> Result<(), Error> {
        let charge = text
            .len()
            .checked_mul(12)
            .and_then(|n| n.checked_add(128))
            .ok_or(Error::Limit)?;
        self.0 = self.0.checked_sub(charge).ok_or(Error::Limit)?;
        Ok(())
    }
    fn value(&mut self, value: &Value) -> Result<(), Error> {
        self.text(match value {
            Value::Str(text) => text,
            _ => "01234567890123456789012345678901",
        })
    }
}
/// Admit all observation geometry and cumulative escaping cost before allocating
/// a diagnostic path or context tree. Other check families are left untouched.
pub fn diagnostics<'a, R: RowObservation>(
    compiled: &'a PreparedSpecification,
    records: &[CheckObservation<'a, R>],
    maximum: usize,
) -> Result<Vec<Diagnostic<'a>>, Error> {
    if records.len() > 65_536 {
        return Err(Error::Limit);
    }
    let mut budget = Budget(maximum);
    if compiled.key_names().count() > 65_536 {
        return Err(Error::Limit);
    }
    let names: Vec<_> = compiled.key_names().collect();
    let mut unique = BTreeSet::new();
    for name in &names {
        budget.text(name)?;
        if name.is_empty() || !unique.insert(*name) {
            return Err(Error::InvalidObservation);
        }
    }
    let mut paths = BTreeSet::new();
    for record in records {
        budget.text(record.path)?;
        let check = compiled.column_check(record.path);
        let Some(observation) = record.codelist else {
            if matches!(
                check,
                Some(Check::Codelist { .. } | Check::ValueCodelists { .. })
            ) || record.code == ConditionCode::VerificationCodelistFailed
            {
                return Err(Error::InvalidObservation);
            }
            continue;
        };
        budget.text(observation.id)?;
        let Some((id, _)) = check.and_then(|check| check.codelist(observation.id)) else {
            return Err(Error::InvalidObservation);
        };
        if record.offending_rows.len() > 65_536 || observation.values.len() > 65_536 {
            return Err(Error::Limit);
        }
        if id != observation.id
            || record.code != ConditionCode::VerificationCodelistFailed
            || record.failed_count != observation.values.len()
            || record.failed_count != record.offending_rows.len()
            || record.failed_count > record.evaluated_count
            || record.failed_count > record.output_rows
            || !paths.insert((record.path, observation.id))
        {
            return Err(Error::InvalidObservation);
        }
        if record.failed_count == 0 {
            continue;
        }
        let column = compiled
            .verification_target(record.path)
            .ok_or(Error::InvalidObservation)?;
        budget.text(column)?;
        let mut previous = None;
        for (row, value) in record.offending_rows.iter().zip(observation.values) {
            if row.values().len() != names.len()
                || row.position() >= record.output_rows
                || previous.is_some_and(|position| row.position() <= position)
                || matches!(value, Value::Missing)
                || row
                    .values()
                    .iter()
                    .any(|value| matches!(value, Value::Missing))
            {
                return Err(Error::InvalidObservation);
            }
            previous = Some(row.position());
            for (name, key) in names.iter().zip(row.values()) {
                budget.text(name)?;
                budget.value(key)?;
            }
            budget.value(value)?;
        }
    }
    let mut diagnostics = Vec::new();
    for record in records {
        let Some(observation) = record.codelist else {
            continue;
        };
        if record.failed_count == 0 {
            continue;
        }
        let column = compiled
            .verification_target(record.path)
            .ok_or(Error::InvalidObservation)?;
        let Some((id, _)) = compiled
            .column_check(record.path)
            .and_then(|check| check.codelist(observation.id))
        else {
            return Err(Error::Projection);
        };
        let keys = record
            .offending_rows
            .iter()
            .map(|row| {
                Fact::Object(
                    names
                        .iter()
                        .zip(row.values())
                        .map(|(name, value)| (*name, Fact::Scalar(value)))
                        .collect(),
                )
            })
            .collect();
        let values = observation.values.iter().map(Fact::Scalar).collect();
        let context: Context<'a> = [
            ("column", Fact::Text(column)),
            ("codelist", Fact::Text(id)),
            (
                "failure_count",
                Fact::Integer(i64::try_from(record.failed_count).expect("bounded observations")),
            ),
            ("keys", Fact::Sequence(keys)),
            ("values", Fact::Sequence(values)),
        ]
        .into();
        diagnostics.push(Diagnostic {
            code: ConditionCode::VerificationCodelistFailed,
            spec_path: record.path.into(),
            context,
        });
    }
    Ok(diagnostics)
}
