//! Project retained fixed-codelist facts without accessing discarded output or sources.
use crate::issue_rows::Issue;
use serde_json::{json, Map, Value as Json};
use std::collections::BTreeSet;
use yamaa_core::{dataset::Check, specification::PreparedSpecification, value::Value};
use yamaa_engine::dataset::CheckRecord;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    InvalidObservation,
    Limit,
    Projection,
}
struct Budget(usize);
impl Budget {
    fn text(&mut self, text: &str) -> Result<(), Error> {
        let bytes = text
            .len()
            .checked_mul(12)
            .and_then(|n| n.checked_add(128))
            .ok_or(Error::Limit)?;
        self.0 = self.0.checked_sub(bytes).ok_or(Error::Limit)?;
        Ok(())
    }
    fn value(&mut self, value: &Value) -> Result<(), Error> {
        self.text(match value {
            Value::Str(text) => text,
            _ => "01234567890123456789012345678901",
        })
    }
}
pub fn codelist_issues(
    compiled: &PreparedSpecification,
    records: &[CheckRecord],
) -> Result<Vec<Issue>, Error> {
    codelist_issues_with_limit(
        compiled,
        records,
        crate::specification_check::MAX_ISSUE_BYTES,
    )
}
/// Check the complete observation geometry and budget before copying issue facts.
/// Other check kinds retain their own formatter; no check or callback is rerun.
pub fn codelist_issues_with_limit(
    compiled: &PreparedSpecification,
    records: &[CheckRecord],
    maximum: usize,
) -> Result<Vec<Issue>, Error> {
    if records.len() > 65_536 {
        return Err(Error::Limit);
    }
    let mut budget = Budget(maximum);
    if compiled.key_names().count() > 65_536 {
        return Err(Error::Limit);
    }
    let names = compiled.key_names().collect::<Vec<_>>();
    let mut unique = BTreeSet::new();
    for name in &names {
        budget.text(name)?;
        if name.is_empty() || !unique.insert(*name) {
            return Err(Error::InvalidObservation);
        }
    }
    let mut paths = BTreeSet::new();
    for record in records {
        budget.text(&record.path)?;
        let check = compiled.column_check(&record.path);
        let Some(observation) = &record.codelist else {
            if matches!(check, Some(Check::Codelist { .. })) || record.requirement == "REQ-0957" {
                return Err(Error::InvalidObservation);
            }
            continue;
        };
        budget.text(&observation.id)?;
        let Some(Check::Codelist { id, .. }) = check else {
            return Err(Error::InvalidObservation);
        };
        if record.offending_rows.len() > 65_536 || observation.values.len() > 65_536 {
            return Err(Error::Limit);
        }
        if id != &observation.id
            || record.condition != "allowed_values_failed"
            || record.requirement != "REQ-0957"
            || record.failed_count != observation.values.len()
            || record.failed_count != record.offending_rows.len()
            || record.failed_count > record.evaluated_count
            || record.failed_count > record.output_rows
            || !paths.insert(record.path.as_str())
        {
            return Err(Error::InvalidObservation);
        }
        if record.failed_count == 0 {
            continue;
        }
        let column = compiled
            .verification_target(&record.path)
            .ok_or(Error::InvalidObservation)?;
        budget.text(column)?;
        let mut previous = None;
        for (row, value) in record.offending_rows.iter().zip(&observation.values) {
            if row.values.len() != names.len()
                || row.position >= record.output_rows
                || previous.is_some_and(|position| row.position <= position)
                || matches!(value, Value::Missing)
                || row
                    .values
                    .iter()
                    .any(|value| matches!(value, Value::Missing))
            {
                return Err(Error::InvalidObservation);
            }
            previous = Some(row.position);
            for (name, key) in names.iter().zip(&row.values) {
                budget.text(name)?;
                budget.value(key)?;
            }
            budget.value(value)?;
        }
    }
    let mut issues = Vec::new();
    for record in records {
        let Some(observation) = &record.codelist else {
            continue;
        };
        if record.failed_count == 0 {
            continue;
        }
        let column = compiled
            .verification_target(&record.path)
            .ok_or(Error::InvalidObservation)?;
        let keys = record
            .offending_rows
            .iter()
            .map(|row| {
                Json::Object(
                    names
                        .iter()
                        .zip(&row.values)
                        .map(|(name, value)| (String::from(*name), plain(value)))
                        .collect::<Map<_, _>>(),
                )
            })
            .collect::<Vec<_>>();
        let check = compiled
            .column_check(&record.path)
            .ok_or(Error::InvalidObservation)?;
        let diagnostic = yamaa_core::dataset_checks::column_diagnostic(
            check,
            record.path.clone(),
            column.into(),
            record.failed_count,
        )
        .ok_or(Error::Projection)?;
        let mut diagnostic = crate::specification_diagnostics::portable_diagnostic(diagnostic)
            .ok_or(Error::Projection)?;
        let context = diagnostic["context"]
            .as_object_mut()
            .ok_or(Error::Projection)?;
        context.insert("keys".into(), Json::Array(keys));
        context.insert(
            "values".into(),
            Json::Array(observation.values.iter().map(plain).collect()),
        );
        issues.push(Issue::from_diagnostic(diagnostic).map_err(|_| Error::Projection)?);
    }
    if !crate::issue_rows::within_limit(&issues, maximum) {
        return Err(Error::Limit);
    }
    Ok(issues)
}

fn plain(value: &Value) -> Json {
    match value {
        Value::Missing => Json::Null,
        Value::Str(v) => json!(v),
        Value::Int(v) => json!(v),
        Value::Float(v) => json!(v.get()),
        Value::Bool(v) => json!(v),
        Value::Date(v) => json!(v.to_string()),
        Value::DateTime(v) => json!(v.to_string()),
    }
}
