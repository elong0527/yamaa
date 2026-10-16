//! Fixed governed verification tables from completed check records, with no IO.
//! Core owns record admission, count/severity rules, scalar semantics and the
//! governed column schemas, and prepares typed rows. The governed JSON byte
//! projection of DETAILS/OFFENDING_KEYS cells is owned by yamaa-adapters'
//! `verification_logs` projection, which encodes the prepared cells and hands
//! plain rows back to `assemble` here so table construction stays with the
//! schema owner.
use crate::{
    dataset::VerificationSeverity,
    limits::{LimitExceeded, Limits},
    table::{CellError, Column, TableAccess, TableSchema, ValueRef},
    value::{ColumnType, Value},
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

/// Assemble governed tables from adapter-projected rows. The projection has
/// already encoded every `PreparedCell::Details`/`PreparedCell::Keys` cell
/// into its governed JSON text (a plain string cell here); column names,
/// order and integer kinds remain owned by this module.
pub fn assemble(verification: Option<Vec<Vec<Value>>>, warning: Option<Vec<Vec<Value>>>) -> Logs {
    Logs {
        verification: verification.map(|rows| table(VERIFICATION_COLUMNS, &[10, 11], rows)),
        warning: warning.map(|rows| table(WARNING_COLUMNS, &[7], rows)),
    }
}

/// One typed log cell before governed byte projection. Details and keys keep
/// their owned JSON trees so the adapter projection can encode them with the
/// governed ordering, escaping and float/integer behavior.
#[derive(Clone, Debug, PartialEq)]
pub enum PreparedCell {
    Missing,
    Text(String),
    Count(i64),
    Details(BTreeMap<String, Json>),
    Keys(Vec<Json>),
}

/// Typed governed rows: the complete result of core admission and row
/// semantics, awaiting only the adapter byte projection of JSON cells.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Prepared {
    pub verification: Option<Vec<Vec<PreparedCell>>>,
    pub warning: Option<Vec<Vec<PreparedCell>>>,
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
fn text(value: &str) -> PreparedCell {
    PreparedCell::Text(value.into())
}
fn optional(value: Option<&str>) -> PreparedCell {
    value.map_or(PreparedCell::Missing, text)
}
fn count(value: usize) -> Result<i64, Error> {
    i64::try_from(value).map_err(|_| Error::InvalidRecord)
}

/// Validate the complete record set before allocating row copies. Failed runs
/// retain verification rows only; declared empty logs retain the fixed schema.
pub fn prepare(
    records: &[Record],
    artifact: &str,
    verification: bool,
    warning: bool,
    success: bool,
) -> Result<Prepared, Error> {
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
                PreparedCell::Details(failure.details.clone()),
            ),
            None => (None, 0, PreparedCell::Details(BTreeMap::new())),
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
                PreparedCell::Count(count(record.evaluated)?),
                PreparedCell::Count(count(failed)?),
                details.clone(),
            ]);
        }
        if emits_warning {
            if let Some(failure) = &record.failure {
                warning_rows.push(vec![
                    text("1.0"),
                    text(artifact),
                    text("warning"),
                    text(&failure.condition),
                    text(&record.requirement),
                    text(&record.path),
                    optional(record.id.as_deref()),
                    PreparedCell::Count(count(failed)?),
                    PreparedCell::Keys(failure.keys.clone()),
                    details,
                ]);
            }
        }
    }
    Ok(Prepared {
        verification: verification.then_some(verification_rows),
        warning: (warning && success).then_some(warning_rows),
    })
}
