//! The run report: one governed table built for every run from the facts the
//! run retained, with no IO. Core owns the fact admission, the row kinds and
//! their order, the column schema, scalar semantics and the verification of the
//! assembled table. The canonical JSON text of KEYS and DETAILS cells is
//! projected by yamaa-adapters' `run_report`, which encodes the prepared cells
//! and hands plain rows back to `assemble` here, then calls `verify` before any
//! byte of the report is written.
use crate::{
    dataset::VerificationSeverity,
    diagnostic::ConditionCode,
    limits::{LimitExceeded, Limits},
    table::{CellError, Column as TableColumn, TableAccess, TableSchema, ValueRef},
    value::{ColumnType, Value},
};
use std::convert::Infallible;
use std::{
    collections::{BTreeMap, BTreeSet},
    vec,
};

use crate::json::Value as Json;

/// The report's columns, in order.
pub const COLUMNS: &[&str] = &[
    "ARTIFACT",
    "KIND",
    "SPEC_PATH",
    "NAME",
    "ID",
    "TARGET",
    "RULE",
    "SEVERITY",
    "OUTCOME",
    "CONDITION",
    "COUNT",
    "TOTAL",
    "KEYS",
    "DETAILS",
];
const COUNT: usize = 10;
const TOTAL: usize = 11;

/// Row kinds in report order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kind {
    Run,
    Input,
    Rows,
    Handler,
    Column,
    Check,
}
impl Kind {
    pub fn name(self) -> &'static str {
        match self {
            Self::Run => "run",
            Self::Input => "input",
            Self::Rows => "rows",
            Self::Handler => "handler",
            Self::Column => "column",
            Self::Check => "check",
        }
    }
}

/// The run's outcome: a failed run names its fatal condition; a successful run
/// counts its output rows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Run {
    Succeeded { rows: usize },
    Failed { condition: String },
}
/// One declared input dataset and what the run read from it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Input {
    pub name: String,
    /// The input's path as declared.
    pub path: String,
    pub profile: &'static str,
    /// The producing specification's path as declared, for a schema-linked input.
    pub schema: Option<String>,
    /// Decoded records, absent when the input was never read.
    pub records: Option<usize>,
    /// Byte length of the captured file snapshot, absent when no file bytes
    /// were read (an input supplied as a host table, or one never read).
    pub bytes: Option<usize>,
}
/// One row template, or root `base` without row templates.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rows {
    /// `rows[<i>]` or `base`.
    pub path: String,
    pub driver: String,
    /// Rows emitted and driver records read, absent unless the template completed.
    pub emitted: Option<usize>,
    pub read: Option<usize>,
}
/// One handler path and how many records it answered.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Handler {
    pub path: String,
    pub name: String,
    pub count: u64,
}
/// One output column of a successful run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Column {
    pub name: String,
    pub missing: usize,
    pub rows: usize,
    /// The longest value in Unicode scalar values, for a `str` column only.
    pub max_length: Option<usize>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Failure {
    pub condition: String,
    pub count: usize,
    pub keys: Vec<Json>,
    pub details: BTreeMap<String, Json>,
}
/// One evaluated verification.
#[derive(Clone, Debug, PartialEq)]
pub struct Check {
    pub path: String,
    pub name: String,
    pub id: Option<String>,
    pub target: Option<String>,
    pub rule: String,
    pub severity: VerificationSeverity,
    pub evaluated: usize,
    pub failure: Option<Failure>,
}
/// Every fact one report is built from, each kind in its report order.
#[derive(Clone, Debug, PartialEq)]
pub struct Facts {
    /// The specification's `output.path`.
    pub artifact: String,
    pub run: Run,
    pub inputs: Vec<Input>,
    pub rows: Vec<Rows>,
    pub handlers: Vec<Handler>,
    /// Output columns in `output.columns` order; empty for a failed run.
    pub columns: Vec<Column>,
    pub checks: Vec<Check>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    /// A fact or an assembled row breaks the report contract: an execution
    /// defect, never a finding the report could record about itself.
    #[error("run report invalid record")]
    InvalidRecord,
    #[error("run report: {0:?}")]
    Limit(#[from] LimitExceeded),
}

#[derive(Debug)]
pub struct Report {
    schema: TableSchema,
    rows: Vec<Vec<Value>>,
}
impl TableAccess for Report {
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

/// The fixed report schema: every column `str` except COUNT and TOTAL.
pub fn schema() -> TableSchema {
    TableSchema::new(
        COLUMNS
            .iter()
            .enumerate()
            .map(|(index, name)| TableColumn {
                name: (*name).into(),
                kind: if index == COUNT || index == TOTAL {
                    ColumnType::Int
                } else {
                    ColumnType::Str
                },
            })
            .collect(),
    )
    .expect("fixed unique report columns")
}

/// Assemble the report from adapter-projected rows. The projection has already
/// encoded every `PreparedCell::Details`/`PreparedCell::Keys` cell into its
/// canonical JSON text (a plain string cell here); column names, order and
/// integer kinds remain owned by this module.
pub fn assemble(rows: Vec<Vec<Value>>) -> Report {
    Report {
        schema: schema(),
        rows,
    }
}

/// One typed report cell before the canonical JSON projection. Details and
/// keys keep their owned JSON trees so the adapter projection can encode them
/// with the governed ordering, escaping and float/integer behavior.
#[derive(Clone, Debug, PartialEq)]
pub enum PreparedCell {
    Missing,
    Text(String),
    Count(i64),
    Details(BTreeMap<String, Json>),
    Keys(Vec<Json>),
}

/// The output column a handler path belongs to: `columns.<NAME>...` or
/// `rows[<i>].derivations.<NAME>...`. A handler of a named intermediate's own
/// derivations or match keys belongs to no output column.
pub fn handler_target(path: &str) -> Option<&str> {
    let rest = if let Some(rest) = path.strip_prefix("columns.") {
        rest
    } else {
        let rest = path.strip_prefix("rows[")?;
        let (index, rest) = rest.split_once(']')?;
        if index.is_empty() || !index.bytes().all(|byte| byte.is_ascii_digit()) {
            return None;
        }
        rest.strip_prefix(".derivations.")?
    };
    let name = rest.split(['.', '[']).next()?;
    (!name.is_empty()).then_some(name)
}

/// The registered condition a counted handler answers, decided by its name and
/// the operation that declares it (the path segment before the name): the
/// condition that operation raises when the handler is not declared. The
/// handler's citation is that condition's registered requirement. A handler
/// name or operation outside the counted vocabulary has none.
pub fn handler_condition(path: &str, handler: &str) -> Option<ConditionCode> {
    use crate::numeric_lifecycle::HandlerKind as H;
    use ConditionCode as C;
    let kind = [
        H::Absent,
        H::Missing,
        H::Unmapped,
        H::Invalid,
        H::Unconvertible,
        H::MultipleMatches,
        H::NoMatch,
    ]
    .into_iter()
    .find(|kind| kind.name() == handler)?;
    let operation = path.rsplit('.').nth(1).unwrap_or("");
    Some(match kind {
        H::Absent => C::SourceUnknownReference,
        H::MultipleMatches => C::DatasetMultipleMatches,
        H::NoMatch if operation == "str_extract" => C::ExtractNoMatch,
        H::NoMatch => C::DatasetUnmatchedKey,
        H::Unmapped => C::MappingUnmapped,
        H::Unconvertible => C::ConversionInput,
        H::Missing => match operation {
            "mapping" => C::MappingMissing,
            "cut" => C::CutMissing,
            "date_impute" | "date_precision" => C::DateImputeMissing,
            "datetime_impute" => C::DatetimeImputeMissing,
            "datetime_precision" => C::DatetimePrecisionMissing,
            _ => C::StrCaseMissing,
        },
        H::Invalid => match operation {
            "date_impute" | "date_precision" => C::DateImputeInvalid,
            "datetime_impute" => C::DatetimeImputeInvalid,
            "datetime_precision" => C::DatetimePrecisionInvalid,
            "duration_value" => C::DurationInvalid,
            _ => return None,
        },
    })
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
fn text(value: &str) -> PreparedCell {
    PreparedCell::Text(value.into())
}
fn optional(value: Option<&str>) -> PreparedCell {
    value.map_or(PreparedCell::Missing, text)
}
fn count(value: usize) -> Result<PreparedCell, Error> {
    i64::try_from(value)
        .map(PreparedCell::Count)
        .map_err(|_| Error::InvalidRecord)
}
fn optional_count(value: Option<usize>) -> Result<PreparedCell, Error> {
    value.map_or(Ok(PreparedCell::Missing), count)
}
fn nonempty(value: &str) -> Result<(), Error> {
    if value.is_empty() {
        Err(Error::InvalidRecord)
    } else {
        Ok(())
    }
}

/// One prepared row in column order.
#[allow(clippy::too_many_arguments)]
fn row(
    artifact: &str,
    kind: Kind,
    path: &str,
    name: Option<&str>,
    id: Option<&str>,
    target: Option<&str>,
    rule: Option<&str>,
    severity: Option<&str>,
    outcome: Option<&str>,
    condition: Option<&str>,
    count: PreparedCell,
    total: PreparedCell,
    keys: PreparedCell,
    details: BTreeMap<String, Json>,
) -> Vec<PreparedCell> {
    vec![
        text(artifact),
        text(kind.name()),
        text(path),
        optional(name),
        optional(id),
        optional(target),
        optional(rule),
        optional(severity),
        optional(outcome),
        optional(condition),
        count,
        total,
        keys,
        PreparedCell::Details(details),
    ]
}

/// The number of report rows the facts produce.
pub fn row_count(facts: &Facts) -> usize {
    1 + facts.inputs.len()
        + facts.rows.len()
        + facts.handlers.len()
        + facts.columns.len()
        + facts.checks.len()
}

/// Admit the complete fact set before allocating any row, then prepare every
/// row in report order: run, inputs, row templates, handlers, columns, checks.
pub fn prepare(facts: &Facts) -> Result<Vec<Vec<PreparedCell>>, Error> {
    Limits::table(row_count(facts), COLUMNS.len())?;
    admit(facts)?;
    let artifact = facts.artifact.as_str();
    let mut rows = Vec::new();
    rows.try_reserve_exact(row_count(facts))
        .map_err(|_| Error::InvalidRecord)?;
    let (outcome, condition, output_rows) = match &facts.run {
        Run::Succeeded { rows } => ("succeeded", None, Some(*rows)),
        Run::Failed { condition } => ("failed", Some(condition.as_str()), None),
    };
    rows.push(row(
        artifact,
        Kind::Run,
        "output",
        None,
        None,
        None,
        None,
        None,
        Some(outcome),
        condition,
        optional_count(output_rows)?,
        PreparedCell::Missing,
        PreparedCell::Missing,
        BTreeMap::new(),
    ));
    for input in &facts.inputs {
        let mut details = BTreeMap::from([
            ("bytes".into(), input.bytes.map_or(Json::Null, Json::from)),
            ("path".into(), Json::String(input.path.clone())),
            ("profile".into(), Json::String(input.profile.into())),
        ]);
        if let Some(schema) = &input.schema {
            details.insert("schema".into(), Json::String(schema.clone()));
        }
        rows.push(row(
            artifact,
            Kind::Input,
            &format!("input.{}", input.name),
            Some(&input.name),
            None,
            None,
            None,
            None,
            None,
            None,
            optional_count(input.records)?,
            PreparedCell::Missing,
            PreparedCell::Missing,
            details,
        ));
    }
    for template in &facts.rows {
        rows.push(row(
            artifact,
            Kind::Rows,
            &template.path,
            Some(&template.driver),
            None,
            None,
            None,
            None,
            None,
            None,
            optional_count(template.emitted)?,
            optional_count(template.read)?,
            PreparedCell::Missing,
            BTreeMap::new(),
        ));
    }
    for handler in &facts.handlers {
        rows.push(row(
            artifact,
            Kind::Handler,
            &handler.path,
            Some(&handler.name),
            None,
            handler_target(&handler.path),
            handler_condition(&handler.path, &handler.name)
                .and_then(|code| code.definition().requirement),
            None,
            None,
            None,
            PreparedCell::Count(i64::try_from(handler.count).map_err(|_| Error::InvalidRecord)?),
            PreparedCell::Missing,
            PreparedCell::Missing,
            BTreeMap::new(),
        ));
    }
    for column in &facts.columns {
        let details = column
            .max_length
            .map(|length| BTreeMap::from([("max_length".into(), Json::from(length))]))
            .unwrap_or_default();
        rows.push(row(
            artifact,
            Kind::Column,
            &format!("columns.{}", column.name),
            Some(&column.name),
            None,
            None,
            None,
            None,
            None,
            None,
            count(column.missing)?,
            count(column.rows)?,
            PreparedCell::Missing,
            details,
        ));
    }
    for check in &facts.checks {
        let (outcome, condition, failed, keys, details) = match &check.failure {
            Some(failure) => (
                "violated",
                Some(failure.condition.as_str()),
                failure.count,
                PreparedCell::Keys(failure.keys.clone()),
                failure.details.clone(),
            ),
            None => ("held", None, 0, PreparedCell::Missing, BTreeMap::new()),
        };
        rows.push(row(
            artifact,
            Kind::Check,
            &check.path,
            Some(&check.name),
            check.id.as_deref(),
            check.target.as_deref(),
            Some(&check.rule),
            Some(check.severity.name()),
            Some(outcome),
            condition,
            count(failed)?,
            count(check.evaluated)?,
            keys,
            details,
        ));
    }
    Ok(rows)
}

/// The paths each row's SPEC_PATH takes, in report order.
fn paths(facts: &Facts) -> impl Iterator<Item = String> + '_ {
    std::iter::once("output".to_owned())
        .chain(
            facts
                .inputs
                .iter()
                .map(|input| format!("input.{}", input.name)),
        )
        .chain(facts.rows.iter().map(|template| template.path.clone()))
        .chain(facts.handlers.iter().map(|handler| handler.path.clone()))
        .chain(
            facts
                .columns
                .iter()
                .map(|column| format!("columns.{}", column.name)),
        )
        .chain(facts.checks.iter().map(|check| check.path.clone()))
}

fn admit(facts: &Facts) -> Result<(), Error> {
    nonempty(&facts.artifact)?;
    if let Run::Failed { condition } = &facts.run {
        nonempty(condition)?;
    }
    if matches!(facts.run, Run::Failed { .. }) && !facts.columns.is_empty() {
        return Err(Error::InvalidRecord);
    }
    for input in &facts.inputs {
        nonempty(&input.name)?;
        nonempty(&input.path)?;
    }
    for template in &facts.rows {
        nonempty(&template.driver)?;
        if template.emitted.is_some() != template.read.is_some() {
            return Err(Error::InvalidRecord);
        }
    }
    for handler in &facts.handlers {
        nonempty(&handler.name)?;
    }
    for column in &facts.columns {
        nonempty(&column.name)?;
    }
    for check in &facts.checks {
        nonempty(&check.name)?;
        nonempty(&check.rule)?;
        if let Some(failure) = &check.failure {
            nonempty(&failure.condition)?;
            if failure.count == 0 || failure.count > check.evaluated {
                return Err(Error::InvalidRecord);
            }
            for key in &failure.keys {
                if !matches!(key, Json::Object(_)) {
                    return Err(Error::InvalidRecord);
                }
                validate_json(key, 0)?;
            }
            for (name, value) in &failure.details {
                if matches!(name.as_str(), "failure_count" | "keys" | "verification_id") {
                    return Err(Error::InvalidRecord);
                }
                validate_json(value, 0)?;
            }
        }
    }
    let mut seen = BTreeSet::new();
    for path in paths(facts) {
        if path.is_empty() || !seen.insert(path) {
            return Err(Error::InvalidRecord);
        }
    }
    Ok(())
}

fn str_cell(report: &Report, row: usize, column: usize) -> Result<Option<&str>, Error> {
    match report.cell(row, column).map_err(|_| Error::InvalidRecord)? {
        ValueRef::Missing => Ok(None),
        ValueRef::Str(value) => Ok(Some(value)),
        _ => Err(Error::InvalidRecord),
    }
}
fn int_cell(report: &Report, row: usize, column: usize) -> Result<Option<i64>, Error> {
    match report.cell(row, column).map_err(|_| Error::InvalidRecord)? {
        ValueRef::Missing => Ok(None),
        ValueRef::Int(value) if value >= 0 => Ok(Some(value)),
        _ => Err(Error::InvalidRecord),
    }
}
fn require(condition: bool) -> Result<(), Error> {
    if condition {
        Ok(())
    } else {
        Err(Error::InvalidRecord)
    }
}

/// Verify the assembled report before it becomes an artifact or a host view:
/// its columns, types and order; one row per fact, kinds in report order with
/// the fact's SPEC_PATH, every SPEC_PATH present and unique; the outcome,
/// severity and condition vocabularies of each kind; COUNT no greater than
/// TOTAL where both exist; and KEYS on exactly the violated checks.
pub fn verify(report: &Report, facts: &Facts) -> Result<(), Error> {
    require(report.schema() == &schema())?;
    require(report.row_count() == row_count(facts))?;
    let kinds = std::iter::once(Kind::Run)
        .chain(facts.inputs.iter().map(|_| Kind::Input))
        .chain(facts.rows.iter().map(|_| Kind::Rows))
        .chain(facts.handlers.iter().map(|_| Kind::Handler))
        .chain(facts.columns.iter().map(|_| Kind::Column))
        .chain(facts.checks.iter().map(|_| Kind::Check));
    let mut seen = BTreeSet::new();
    for (index, ((kind, path), values)) in kinds.zip(paths(facts)).zip(&report.rows).enumerate() {
        require(values.len() == COLUMNS.len())?;
        require(str_cell(report, index, 0)? == Some(facts.artifact.as_str()))?;
        require(str_cell(report, index, 1)? == Some(kind.name()))?;
        let spec_path = str_cell(report, index, 2)?.ok_or(Error::InvalidRecord)?;
        require(spec_path == path && seen.insert(spec_path))?;
        let name = str_cell(report, index, 3)?;
        let id = str_cell(report, index, 4)?;
        let target = str_cell(report, index, 5)?;
        let rule = str_cell(report, index, 6)?;
        let severity = str_cell(report, index, 7)?;
        let outcome = str_cell(report, index, 8)?;
        let condition = str_cell(report, index, 9)?;
        let count = int_cell(report, index, COUNT)?;
        let total = int_cell(report, index, TOTAL)?;
        let keys = str_cell(report, index, 12)?;
        require(str_cell(report, index, 13)?.is_some())?;
        if let (Some(count), Some(total)) = (count, total) {
            require(count <= total)?;
        }
        require(name.is_some() == (kind != Kind::Run))?;
        require((kind == Kind::Check) == severity.is_some())?;
        require(kind == Kind::Check || id.is_none())?;
        require(matches!(kind, Kind::Handler | Kind::Check) || target.is_none() && rule.is_none())?;
        require(keys.is_none() || outcome == Some("violated"))?;
        match kind {
            Kind::Run => {
                require(total.is_none())?;
                match outcome {
                    Some("succeeded") => require(condition.is_none() && count.is_some())?,
                    Some("failed") => require(condition.is_some() && count.is_none())?,
                    _ => return Err(Error::InvalidRecord),
                }
            }
            Kind::Check => {
                require(rule.is_some() && total.is_some())?;
                require(matches!(severity, Some("error" | "warning")))?;
                match outcome {
                    Some("held") => require(condition.is_none() && count == Some(0))?,
                    Some("violated") => require(
                        condition.is_some()
                            && count.is_some_and(|count| count > 0)
                            && keys.is_some(),
                    )?,
                    _ => return Err(Error::InvalidRecord),
                }
            }
            Kind::Input | Kind::Rows | Kind::Handler | Kind::Column => {
                require(outcome.is_none() && condition.is_none())?;
                require(kind != Kind::Handler || count.is_some())?;
                require(kind != Kind::Column || count.is_some() && total.is_some())?;
                require(matches!(kind, Kind::Rows | Kind::Column) || total.is_none())?;
            }
        }
    }
    Ok(())
}
