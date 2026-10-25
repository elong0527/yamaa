//! The run report: one governed table built for every run from the facts the
//! run retained, with no IO. Core owns the fact admission, the row kinds and
//! their order, the column schema, scalar semantics and the verification of the
//! assembled table. The canonical JSON text of KEYS and DETAILS cells is
//! projected by yamaa-application's `run_report`, which encodes the prepared cells
//! and hands plain rows back to `assemble` here, then calls `verify` before any
//! byte of the report is written.
use crate::expression_table::Verb;
use crate::{
    dataset::{VerificationSeverity, SEVERITIES},
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

/// Values of the KIND column, in report order.
pub const KINDS: [&str; 6] = ["run", "input", "rows", "handler", "column", "check"];

/// Values of the OUTCOME column: a run row's two, then a check row's two.
pub const OUTCOMES: [&str; 4] = [SUCCEEDED, FAILED, HELD, VIOLATED];
const SUCCEEDED: &str = "succeeded";
const FAILED: &str = "failed";
const HELD: &str = "held";
const VIOLATED: &str = "violated";

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
            Self::Run => KINDS[0],
            Self::Input => KINDS[1],
            Self::Rows => KINDS[2],
            Self::Handler => KINDS[3],
            Self::Column => KINDS[4],
            Self::Check => KINDS[5],
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
    /// The row template's path, `rows.<id>` (P-CLR-2), or `base`.
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

/// The output column a handler path belongs to: `columns.<NAME>...` or a row
/// template's `<row>.derivations.<NAME>...`, the row written `rows.<id>` or
/// `rows['<id>']` (P-CLR-2). A handler of a named intermediate's own
/// derivations or match keys belongs to no output column.
pub fn handler_target(path: &str) -> Option<&str> {
    let rest = if let Some(rest) = path.strip_prefix("columns.") {
        rest
    } else {
        let row = path.strip_prefix("rows")?;
        crate::schema::read_step(row)?
            .1
            .strip_prefix(".derivations.")?
    };
    let name = rest.split(['.', '[']).next()?;
    (!name.is_empty()).then_some(name)
}

/// The registered condition a counted handler answers, decided by its name and
/// the operation that declares it (the path segment before the name): the
/// condition that operation raises when the handler is not declared. An
/// `absent` answers `unmatched_key`, since it answers only a lookup without a
/// candidate, as LOOKUP-24 states (D67, D204). The handler's citation is
/// that condition's registered rule. A handler name or operation outside the
/// counted vocabulary has none.
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
        H::Partial,
        H::CrossedBounds,
    ]
    .into_iter()
    .find(|kind| kind.name() == handler)?;
    let operation = Verb::from_name(path.rsplit('.').nth(1).unwrap_or(""));
    Some(match kind {
        H::Absent => C::LookupUnmatchedKey,
        H::MultipleMatches => C::DatasetMultipleMatches,
        H::NoMatch if operation == Some(Verb::StrExtract) => C::ExtractNoMatch,
        H::NoMatch => C::LookupUnmatchedKey,
        H::Unmapped => C::MappingUnmapped,
        H::Unconvertible => C::ConversionInput,
        // A missing input gives missing by default and raises nothing; only a
        // string template fails on a missing part.
        H::Missing => match operation {
            Some(Verb::StrTemplate) => C::TemplateMissing,
            _ => return None,
        },
        H::Invalid => match operation {
            Some(Verb::Impute | Verb::Precision | Verb::ToDate) => C::ImputeInvalid,
            Some(Verb::DurationValue) => C::DurationInvalid,
            Some(Verb::Dtc) => C::DateTextInvalid,
            Some(Verb::DateText) => C::DateFormsInvalid,
            _ => return None,
        },
        H::Partial | H::CrossedBounds => return None,
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

/// One report row before it is prepared: its kind and path, and each other
/// column it fills; a column it leaves out is missing ([`Row::new`]).
struct Row<'a> {
    kind: Kind,
    path: &'a str,
    name: Option<&'a str>,
    id: Option<&'a str>,
    target: Option<&'a str>,
    rule: Option<&'a str>,
    severity: Option<&'a str>,
    outcome: Option<&'a str>,
    condition: Option<&'a str>,
    count: PreparedCell,
    total: PreparedCell,
    keys: PreparedCell,
    details: BTreeMap<String, Json>,
}

impl<'a> Row<'a> {
    /// A row of `kind` at `path` whose every other column is missing.
    fn new(kind: Kind, path: &'a str) -> Self {
        Self {
            kind,
            path,
            name: None,
            id: None,
            target: None,
            rule: None,
            severity: None,
            outcome: None,
            condition: None,
            count: PreparedCell::Missing,
            total: PreparedCell::Missing,
            keys: PreparedCell::Missing,
            details: BTreeMap::new(),
        }
    }
}

/// One prepared row in column order.
fn row(artifact: &str, fields: Row<'_>) -> Vec<PreparedCell> {
    let Row {
        kind,
        path,
        name,
        id,
        target,
        rule,
        severity,
        outcome,
        condition,
        count,
        total,
        keys,
        details,
    } = fields;
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
        Run::Succeeded { rows } => (SUCCEEDED, None, Some(*rows)),
        Run::Failed { condition } => (FAILED, Some(condition.as_str()), None),
    };
    rows.push(row(
        artifact,
        Row {
            outcome: Some(outcome),
            condition,
            count: optional_count(output_rows)?,
            ..Row::new(Kind::Run, "output")
        },
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
            Row {
                name: Some(&input.name),
                count: optional_count(input.records)?,
                details,
                ..Row::new(Kind::Input, &format!("input.{}", input.name))
            },
        ));
    }
    for template in &facts.rows {
        rows.push(row(
            artifact,
            Row {
                name: Some(&template.driver),
                count: optional_count(template.emitted)?,
                total: optional_count(template.read)?,
                ..Row::new(Kind::Rows, &template.path)
            },
        ));
    }
    for handler in &facts.handlers {
        rows.push(row(
            artifact,
            Row {
                name: Some(&handler.name),
                target: handler_target(&handler.path),
                rule: handler_condition(&handler.path, &handler.name)
                    .and_then(|code| code.definition().rule),
                count: PreparedCell::Count(
                    i64::try_from(handler.count).map_err(|_| Error::InvalidRecord)?,
                ),
                ..Row::new(Kind::Handler, &handler.path)
            },
        ));
    }
    for column in &facts.columns {
        let details = column
            .max_length
            .map(|length| BTreeMap::from([("max_length".into(), Json::from(length))]))
            .unwrap_or_default();
        rows.push(row(
            artifact,
            Row {
                name: Some(&column.name),
                count: count(column.missing)?,
                total: count(column.rows)?,
                details,
                ..Row::new(Kind::Column, &format!("columns.{}", column.name))
            },
        ));
    }
    for check in &facts.checks {
        let (outcome, condition, failed, keys, details) = match &check.failure {
            Some(failure) => (
                VIOLATED,
                Some(failure.condition.as_str()),
                failure.count,
                PreparedCell::Keys(failure.keys.clone()),
                failure.details.clone(),
            ),
            None => (HELD, None, 0, PreparedCell::Missing, BTreeMap::new()),
        };
        rows.push(row(
            artifact,
            Row {
                name: Some(&check.name),
                id: check.id.as_deref(),
                target: check.target.as_deref(),
                rule: Some(&check.rule),
                severity: Some(check.severity.name()),
                outcome: Some(outcome),
                condition,
                count: count(failed)?,
                total: count(check.evaluated)?,
                keys,
                details,
                ..Row::new(Kind::Check, &check.path)
            },
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

/// What a saved run report records of its run and its inputs, read back from
/// the file a specification declares in `output.report` (PROJECT-15).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Recorded {
    /// The `ARTIFACT` of every row: the run's `output.path`, as declared.
    pub artifact: String,
    /// The `run` row: the rows the run built, or the condition that failed it.
    pub run: Run,
    /// The `input` rows, in report order.
    pub inputs: Vec<RecordedInput>,
}
/// One `input` row of a saved report (CHECK-24).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecordedInput {
    /// `NAME`: the input dataset.
    pub name: String,
    /// `COUNT`: the records the run decoded, absent for an input never read.
    pub records: Option<usize>,
    /// `DETAILS.bytes`: the byte length the run read, absent when `null`.
    pub bytes: Option<usize>,
}

/// Read what a decoded report table records: its first row, the run, and its
/// input rows. `bytes` reads an input row's `DETAILS` text: the value of its
/// `bytes` member, `Some(None)` for `null`, or `None` when the text holds no
/// such member. `None` when the table is not a run report: other columns, a
/// first row that is not a run row, a row naming another artifact, or a cell
/// of a type or value [`verify`] would refuse.
pub fn recorded<T: TableAccess>(
    table: &T,
    mut bytes: impl FnMut(&str) -> Option<Option<usize>>,
) -> Option<Recorded> {
    let names = table.schema().columns().iter().map(|c| c.name.as_str());
    if !names.eq(COLUMNS.iter().copied()) {
        return None;
    }
    let artifact = recorded_text(table, 0, 0)??;
    if recorded_text(table, 0, 1)? != Some(Kind::Run.name()) {
        return None;
    }
    let run = match (
        recorded_text(table, 0, 8)?,
        recorded_text(table, 0, 9)?,
        recorded_count(table, 0, COUNT)?,
    ) {
        (Some(SUCCEEDED), None, Some(rows)) => Run::Succeeded { rows },
        (Some(FAILED), Some(condition), None) => Run::Failed {
            condition: condition.into(),
        },
        _ => return None,
    };
    let mut inputs = Vec::new();
    for row in 1..table.row_count() {
        if recorded_text(table, row, 0)? != Some(artifact) {
            return None;
        }
        if recorded_text(table, row, 1)? != Some(Kind::Input.name()) {
            continue;
        }
        inputs.push(RecordedInput {
            name: recorded_text(table, row, 3)??.into(),
            records: recorded_count(table, row, COUNT)?,
            bytes: bytes(recorded_text(table, row, 13)??)?,
        });
    }
    Some(Recorded {
        artifact: artifact.into(),
        run,
        inputs,
    })
}
/// A `str` cell of a saved report: `Some(None)` when missing, `None` when it
/// holds another type.
fn recorded_text<T: TableAccess>(table: &T, row: usize, column: usize) -> Option<Option<&str>> {
    match table.cell(row, column).ok()? {
        ValueRef::Missing => Some(None),
        ValueRef::Str(value) => Some(Some(value)),
        _ => None,
    }
}
/// An `int` cell of a saved report: `Some(None)` when missing, `None` when it
/// holds another type or a negative count.
fn recorded_count<T: TableAccess>(table: &T, row: usize, column: usize) -> Option<Option<usize>> {
    match table.cell(row, column).ok()? {
        ValueRef::Missing => Some(None),
        ValueRef::Int(value) => usize::try_from(value).ok().map(Some),
        _ => None,
    }
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
        require(keys.is_none() || outcome == Some(VIOLATED))?;
        match kind {
            Kind::Run => {
                require(total.is_none())?;
                match outcome {
                    Some(SUCCEEDED) => require(condition.is_none() && count.is_some())?,
                    Some(FAILED) => require(condition.is_some() && count.is_none())?,
                    _ => return Err(Error::InvalidRecord),
                }
            }
            Kind::Check => {
                require(rule.is_some() && total.is_some())?;
                require(severity.is_some_and(|severity| SEVERITIES.contains(&severity)))?;
                match outcome {
                    Some(HELD) => require(condition.is_none() && count == Some(0))?,
                    Some(VIOLATED) => require(
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
