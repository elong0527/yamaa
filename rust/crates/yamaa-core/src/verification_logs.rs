//! Fixed governed verification tables from completed check records, with no IO.
use crate::{
    conversion::float_text,
    dataset::VerificationSeverity,
    table::{CellError, Column, TableAccess, TableSchema, ValueRef},
    value::{ColumnType, Value},
};
use alloc::{
    collections::{BTreeMap, BTreeSet},
    string::{String, ToString},
    vec,
    vec::Vec,
};
use core::convert::Infallible;

/// Full diagnostic context is distinct from the sampled public issue context.
#[derive(Clone, Debug, PartialEq)]
pub enum Json {
    Scalar(Value),
    Integer(String),
    Sequence(Vec<Json>),
    Object(BTreeMap<String, Json>),
}
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
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub records: usize,
    pub cells: usize,
    pub text: usize,
    pub json_nodes: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            records: 4096,
            cells: 65_536,
            text: 8_388_608,
            json_nodes: 1_048_576,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    InvalidRecord,
    Limit,
}

pub const VERIFICATION_COLUMNS: &[&str] = &[
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
pub const WARNING_COLUMNS: &[&str] = &[
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

struct Budget {
    text: usize,
    nodes: usize,
}
impl Budget {
    fn text(&mut self, value: &str) -> Result<(), Error> {
        self.text = self.text.checked_sub(value.len()).ok_or(Error::Limit)?;
        Ok(())
    }
    fn json(&mut self, value: &Json, depth: usize) -> Result<(), Error> {
        if depth > 64 {
            return Err(Error::Limit);
        }
        self.nodes = self.nodes.checked_sub(1).ok_or(Error::Limit)?;
        match value {
            Json::Scalar(Value::Str(text)) => self.text(text)?,
            Json::Integer(text) => {
                let digits = text.strip_prefix('-').unwrap_or(text);
                if digits.is_empty()
                    || !digits.bytes().all(|byte| byte.is_ascii_digit())
                    || (digits.len() > 1 && digits.starts_with('0'))
                    || text == "-0"
                {
                    return Err(Error::InvalidRecord);
                }
                self.text(text)?;
            }
            Json::Sequence(values) => {
                for value in values {
                    self.json(value, depth + 1)?;
                }
            }
            Json::Object(values) => {
                for (name, value) in values {
                    self.text(name)?;
                    self.json(value, depth + 1)?;
                }
            }
            _ => (),
        }
        Ok(())
    }
}
struct Writer {
    output: String,
    remaining: usize,
}
impl Writer {
    fn write(&mut self, text: &str) -> Result<(), Error> {
        self.remaining = self.remaining.checked_sub(text.len()).ok_or(Error::Limit)?;
        self.output.push_str(text);
        Ok(())
    }
    fn string(&mut self, text: &str) -> Result<(), Error> {
        self.write("\"")?;
        for character in text.chars() {
            match character {
                '"' => self.write("\\\"")?,
                '\\' => self.write("\\\\")?,
                '\u{8}' => self.write("\\b")?,
                '\u{c}' => self.write("\\f")?,
                '\n' => self.write("\\n")?,
                '\r' => self.write("\\r")?,
                '\t' => self.write("\\t")?,
                c if (' '..='~').contains(&c) => {
                    self.write(core::str::from_utf8(&[c as u8]).expect("ASCII"))?
                }
                c => {
                    for unit in c.encode_utf16(&mut [0; 2]) {
                        self.write(&alloc::format!("\\u{unit:04x}"))?;
                    }
                }
            }
        }
        self.write("\"")
    }
    fn json(&mut self, value: &Json) -> Result<(), Error> {
        match value {
            Json::Integer(value) => self.write(value),
            Json::Scalar(value) => match value {
                Value::Missing => self.write("null"),
                Value::Bool(value) => self.write(if *value { "true" } else { "false" }),
                Value::Str(value) => self.string(value),
                Value::Int(value) => self.write(&value.to_string()),
                Value::Float(value) => self.write(&float_text(*value)),
                Value::Date(value) => self.string(&value.to_string()),
                Value::DateTime(value) => self.string(&value.to_string()),
            },
            Json::Sequence(values) => self.sequence(values),
            Json::Object(values) => self.object(values),
        }
    }
    fn sequence(&mut self, values: &[Json]) -> Result<(), Error> {
        self.write("[")?;
        for (index, value) in values.iter().enumerate() {
            if index > 0 {
                self.write(",")?;
            }
            self.json(value)?;
        }
        self.write("]")
    }
    fn object(&mut self, values: &BTreeMap<String, Json>) -> Result<(), Error> {
        self.write("{")?;
        for (index, (name, value)) in values.iter().enumerate() {
            if index > 0 {
                self.write(",")?;
            }
            self.string(name)?;
            self.write(":")?;
            self.json(value)?;
        }
        self.write("}")
    }
}
fn encoded(
    budget: &mut Budget,
    write: impl FnOnce(&mut Writer) -> Result<(), Error>,
) -> Result<String, Error> {
    let mut writer = Writer {
        output: String::new(),
        remaining: budget.text,
    };
    write(&mut writer)?;
    budget.text = writer.remaining;
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
        .map_err(|_| Error::Limit)
}

/// Validate the complete record set before allocating row copies. Failed runs
/// retain verification rows only; declared empty logs retain the fixed schema.
pub fn build(
    records: &[Record],
    artifact: &str,
    verification: bool,
    warning: bool,
    success: bool,
    limits: Limits,
) -> Result<Logs, Error> {
    if records.len() > limits.records {
        return Err(Error::Limit);
    }
    let warnings = records
        .iter()
        .filter(|record| {
            record.severity == VerificationSeverity::Warning && record.failure.is_some()
        })
        .count();
    let cells = if verification {
        records.len().checked_mul(13).ok_or(Error::Limit)?
    } else {
        0
    }
    .checked_add(if warning && success {
        warnings.checked_mul(10).ok_or(Error::Limit)?
    } else {
        0
    })
    .ok_or(Error::Limit)?;
    if cells > limits.cells {
        return Err(Error::Limit);
    }
    let mut budget = Budget {
        text: limits.text,
        nodes: limits.json_nodes,
    };
    let mut paths = BTreeSet::new();
    for record in records {
        if record.path.is_empty()
            || record.check.is_empty()
            || record.requirement.is_empty()
            || !paths.insert(record.path.as_str())
        {
            return Err(Error::InvalidRecord);
        }
        count(record.evaluated)?;
        for value in [
            Some(artifact),
            Some(record.path.as_str()),
            record.id.as_deref(),
            Some(record.check.as_str()),
            record.target.as_deref(),
            Some(record.requirement.as_str()),
        ]
        .into_iter()
        .flatten()
        {
            budget.text(value)?;
        }
        if let Some(failure) = &record.failure {
            if failure.condition.is_empty()
                || failure.count == 0
                || failure.count > record.evaluated
            {
                return Err(Error::InvalidRecord);
            }
            count(failure.count)?;
            budget.text(&failure.condition)?;
            for key in &failure.keys {
                if !matches!(key, Json::Object(_)) {
                    return Err(Error::InvalidRecord);
                }
                budget.json(key, 0)?;
            }
            for (key, value) in &failure.details {
                if matches!(key.as_str(), "failure_count" | "keys" | "verification_id") {
                    return Err(Error::InvalidRecord);
                }
                budget.text(key)?;
                budget.json(value, 0)?;
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
                encoded(&mut budget, |writer| writer.object(&failure.details))?,
            ),
            None => (None, 0, "{}".into()),
        };
        if verification {
            for value in [
                Some("1.0"),
                Some(artifact),
                Some(record.path.as_str()),
                record.id.as_deref(),
                Some(record.check.as_str()),
                record.target.as_deref(),
                Some(record.requirement.as_str()),
                Some(record.severity.name()),
                Some(if condition.is_some() {
                    "violated"
                } else {
                    "held"
                }),
                condition,
                Some(details.as_str()),
            ]
            .into_iter()
            .flatten()
            {
                budget.text(value)?;
            }
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
                let keys = encoded(&mut budget, |writer| writer.sequence(&failure.keys))?;
                for value in [
                    Some("1.0"),
                    Some(artifact),
                    Some("warning"),
                    Some(failure.condition.as_str()),
                    Some(record.requirement.as_str()),
                    Some(record.path.as_str()),
                    record.id.as_deref(),
                    Some(keys.as_str()),
                    Some(details.as_str()),
                ]
                .into_iter()
                .flatten()
                {
                    budget.text(value)?;
                }
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
