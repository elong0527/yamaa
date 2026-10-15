//! Fixed governed verification tables from completed check records, with no IO.
use crate::{
    conversion::float_text,
    dataset::VerificationSeverity,
    limits::{LimitExceeded, Limits},
    table::{CellError, Column, TableAccess, TableSchema, ValueRef},
    value::{ColumnType, FiniteFloat, Value},
};
use std::convert::Infallible;
use std::{
    collections::{BTreeMap, BTreeSet},
    vec,
};

use crate::json::Value as Json;

#[derive(Clone, Debug)]
pub struct Failure {
    pub condition: String,
    pub count: usize,
    pub keys: Vec<Json>,
    pub details: BTreeMap<String, Json>,
}
#[derive(Clone, Debug)]
pub struct Record {
    pub path: String,
    pub id: Option<String>,
    pub check: String,
    pub target: Option<String>,
    pub requirement: String,
    pub severity: VerificationSeverity,
    pub evaluated: usize,
    pub failure: Option<Failure>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    #[error("verification logs invalid record")]
    InvalidRecord,
    #[error("verification logs: {0}")]
    Limit(#[from] LimitExceeded),
}

pub(crate) const VERIFICATION_COLUMNS: &[&str] = &[
    "REPORT_VERSION",
    "ARTIFACT",
    "SPEC_PATH",
    "VERIFICATION_ID",
    "CHECK",
    "TARGET",
    "REQUIREMENT",
    "SEVERITY",
    "OUTCOME",
    "CONDITION",
    "EVALUATED_COUNT",
    "FAILURE_COUNT",
    "DETAILS",
];
pub(crate) const WARNING_COLUMNS: &[&str] = &[
    "LOG_VERSION",
    "ARTIFACT",
    "SEVERITY",
    "CONDITION",
    "REQUIREMENT",
    "SPEC_PATH",
    "VERIFICATION_ID",
    "FAILURE_COUNT",
    "OFFENDING_KEYS",
    "DETAILS",
];
#[derive(Debug)]
pub struct LogTable {
    schema: TableSchema,
    rows: Vec<Vec<Value>>,
}
impl TableAccess for LogTable {
    type Error = Infallible;
    fn schema(&self) -> &TableSchema {
        &self.schema
    }
    fn row_count(&self) -> usize {
        self.rows.len()
    }
    fn cell(&self, row: usize, column: usize) -> Result<ValueRef<'_>, CellError<Infallible>> {
        self.rows
            .get(row)
            .and_then(|values| values.get(column))
            .map(ValueRef::from)
            .ok_or(CellError::OutOfBounds { row, column })
    }
}
pub struct Logs {
    pub verification: Option<LogTable>,
    pub warning: Option<LogTable>,
}

fn validate_json(value: &Json, depth: usize) -> Result<(), Error> {
    Limits::semantic_depth(depth)?;
    match value {
        Json::Number(number) => {
            let text = number.as_str();
            if !number.is_integer() {
                if number.as_f64().is_none() {
                    return Err(Error::InvalidRecord);
                }
            } else if text == "-0" {
                return Err(Error::InvalidRecord);
            }
        }
        Json::Array(values) => {
            for value in values {
                validate_json(value, depth + 1)?;
            }
        }
        Json::Object(values) => {
            for value in values.values() {
                validate_json(value, depth + 1)?;
            }
        }
        _ => (),
    }
    Ok(())
}
#[derive(Default)]
struct Writer {
    output: String,
}
impl Writer {
    fn write(&mut self, text: &str) {
        self.output.push_str(text);
    }
    fn string(&mut self, text: &str) {
        self.write("\"");
        for character in text.chars() {
            match character {
                '"' => self.write("\\\""),
                '\\' => self.write("\\\\"),
                '\u{8}' => self.write("\\b"),
                '\u{c}' => self.write("\\f"),
                '\n' => self.write("\\n"),
                '\r' => self.write("\\r"),
                '\t' => self.write("\\t"),
                c if (' '..='~').contains(&c) => {
                    self.write(std::str::from_utf8(&[c as u8]).expect("ASCII"))
                }
                c => {
                    for unit in c.encode_utf16(&mut [0; 2]) {
                        self.write(&std::format!("\\u{unit:04x}"));
                    }
                }
            }
        }
        self.write("\"")
    }
    fn json(&mut self, value: &Json) -> Result<(), Error> {
        match value {
            Json::Null => self.write("null"),
            Json::Bool(value) => self.write(if *value { "true" } else { "false" }),
            Json::String(value) => self.string(value),
            Json::Number(number) if !number.is_integer() => {
                let value = number
                    .as_f64()
                    .and_then(FiniteFloat::new)
                    .ok_or(Error::InvalidRecord)?;
                self.write(&float_text(value))
            }
            Json::Number(number) => self.write(number.as_str()),
            Json::Array(values) => return self.sequence(values),
            Json::Object(values) => return self.object(values),
        }
        Ok(())
    }
    fn sequence(&mut self, values: &[Json]) -> Result<(), Error> {
        self.write("[");
        for (index, value) in values.iter().enumerate() {
            if index > 0 {
                self.write(",");
            }
            self.json(value)?;
        }
        self.write("]");
        Ok(())
    }
    fn object<'a>(
        &mut self,
        values: impl IntoIterator<Item = (&'a String, &'a Json)>,
    ) -> Result<(), Error> {
        self.write("{");
        for (index, (name, value)) in values.into_iter().enumerate() {
            if index > 0 {
                self.write(",");
            }
            self.string(name);
            self.write(":");
            self.json(value)?;
        }
        self.write("}");
        Ok(())
    }
}
fn encoded(write: impl FnOnce(&mut Writer) -> Result<(), Error>) -> Result<String, Error> {
    let mut writer = Writer::default();
    write(&mut writer)?;
    Ok(writer.output)
}
fn table(names: &[&str], integers: &[usize], rows: Vec<Vec<Value>>) -> LogTable {
    LogTable {
        schema: TableSchema::new(
            names
                .iter()
                .enumerate()
                .map(|(index, name)| Column {
                    name: (*name).into(),
                    kind: if integers.contains(&index) {
                        ColumnType::Int
                    } else {
                        ColumnType::Str
                    },
                })
                .collect(),
        )
        .expect("fixed unique governed columns"),
        rows,
    }
}
fn text(value: &str) -> Value {
    Value::Str(value.into())
}
fn optional(value: Option<&str>) -> Value {
    value.map_or(Value::Missing, text)
}
fn count(value: usize) -> Result<Value, Error> {
    i64::try_from(value)
        .map(Value::Int)
        .map_err(|_| Error::InvalidRecord)
}

/// Validate the complete record set before allocating row copies. Failed runs
/// retain verification rows only; declared empty logs retain the fixed schema.
pub fn build(
    records: &[Record],
    artifact: &str,
    verification: bool,
    warning: bool,
    success: bool,
) -> Result<Logs, Error> {
    let warnings = records
        .iter()
        .filter(|record| {
            record.severity == VerificationSeverity::Warning && record.failure.is_some()
        })
        .count();
    if verification {
        Limits::table(records.len(), VERIFICATION_COLUMNS.len())?;
    }
    if warning && success {
        Limits::table(warnings, WARNING_COLUMNS.len())?;
    }
    let mut paths = BTreeSet::new();
    for record in records {
        if record.path.is_empty()
            || record.check.is_empty()
            || record.requirement.is_empty()
            || !(paths.insert(record.path.as_str())
                || record.check == "allowed_values"
                    && Some(record.requirement.as_str())
                        == crate::diagnostic::ConditionCode::VerificationCodelistFailed
                            .definition()
                            .requirement
                    && record.path.starts_with("columns.")
                    && record.path.ends_with(".submission.codelist"))
        {
            return Err(Error::InvalidRecord);
        }
        count(record.evaluated)?;
        if let Some(failure) = &record.failure {
            if failure.condition.is_empty()
                || failure.count == 0
                || failure.count > record.evaluated
            {
                return Err(Error::InvalidRecord);
            }
            count(failure.count)?;
            for key in &failure.keys {
                if !matches!(key, Json::Object(_)) {
                    return Err(Error::InvalidRecord);
                }
                validate_json(key, 0)?;
            }
            for (key, value) in &failure.details {
                if matches!(key.as_str(), "failure_count" | "keys" | "verification_id") {
                    return Err(Error::InvalidRecord);
                }
                validate_json(value, 0)?;
            }
        }
    }
    let mut verification_rows = Vec::new();
    let mut warning_rows = Vec::new();
    for record in records {
        let emits_warning = warning
            && success
            && record.severity == VerificationSeverity::Warning
            && record.failure.is_some();
        if !verification && !emits_warning {
            continue;
        }
        let (condition, failed, details) = match &record.failure {
            Some(failure) => (
                Some(failure.condition.as_str()),
                failure.count,
                encoded(|writer| writer.object(&failure.details))?,
            ),
            None => (None, 0, "{}".into()),
        };
        if verification {
            verification_rows.push(vec![
                text("1.0"),
                text(artifact),
                text(&record.path),
                optional(record.id.as_deref()),
                text(&record.check),
                optional(record.target.as_deref()),
                text(&record.requirement),
                text(record.severity.name()),
                text(if condition.is_some() {
                    "violated"
                } else {
                    "held"
                }),
                optional(condition),
                count(record.evaluated)?,
                count(failed)?,
                text(&details),
            ]);
        }
        if emits_warning {
            if let Some(failure) = &record.failure {
                let keys = encoded(|writer| writer.sequence(&failure.keys))?;
                warning_rows.push(vec![
                    text("1.0"),
                    text(artifact),
                    text("warning"),
                    text(&failure.condition),
                    text(&record.requirement),
                    text(&record.path),
                    optional(record.id.as_deref()),
                    count(failed)?,
                    text(&keys),
                    text(&details),
                ]);
            }
        }
    }
    Ok(Logs {
        verification: verification
            .then(|| table(VERIFICATION_COLUMNS, &[10, 11], verification_rows)),
        warning: (warning && success).then(|| table(WARNING_COLUMNS, &[7], warning_rows)),
    })
}
