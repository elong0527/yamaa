//! Convert retained native check observations into governed owned log artifacts.
//! Neither host supplies these records, and no constructor accepts a publisher.
use crate::{
    public_table::PublicTable, specification_report::Error, specification_run_view::RunView,
};
use serde_json::{json, Value};
use std::{collections::BTreeMap, sync::Arc};
use yamaa_core::{
    dataset::VerificationSeverity,
    table::TableAccess,
    value::{FiniteFloat, Value as Scalar},
    verification_logs::{self as core, Failure, Json, Record},
};

#[derive(Debug)]
pub(crate) struct HeldLog {
    path: String,
    bytes: Arc<[u8]>,
    ipc: Vec<u8>,
    table: PublicTable,
    observation: Value,
}
impl HeldLog {
    pub(crate) fn path(&self) -> &str {
        &self.path
    }
    pub(crate) fn bytes(&self) -> &[u8] {
        &self.bytes
    }
    pub(crate) fn ipc(&self) -> &[u8] {
        &self.ipc
    }
    pub(crate) fn table(&self) -> &PublicTable {
        &self.table
    }
    pub(crate) fn observation(&self) -> &Value {
        &self.observation
    }
}
fn invalid<T>() -> Result<T, Error> {
    Err(Error::InvalidObservation)
}
fn text(record: &Value, field: &str) -> Result<String, Error> {
    record[field]
        .as_str()
        .map(String::from)
        .ok_or(Error::InvalidObservation)
}
fn optional(record: &Value, field: &str) -> Result<Option<String>, Error> {
    match record.get(field) {
        Some(Value::Null) => Ok(None),
        Some(Value::String(text)) => Ok(Some(text.clone())),
        _ => invalid(),
    }
}
fn count(record: &Value, field: &str) -> Result<usize, Error> {
    record[field]
        .as_u64()
        .and_then(|n| usize::try_from(n).ok())
        .ok_or(Error::InvalidObservation)
}
fn owned_json(value: &Value) -> Result<Json, Error> {
    Ok(match value {
        Value::Null => Json::Scalar(Scalar::Missing),
        Value::Bool(value) => Json::Scalar(Scalar::Bool(*value)),
        Value::String(value) => Json::Scalar(Scalar::Str(value.clone())),
        Value::Number(number) if number.is_f64() => Json::Scalar(Scalar::Float(
            FiniteFloat::new(number.as_f64().ok_or(Error::InvalidObservation)?)
                .ok_or(Error::InvalidObservation)?,
        )),
        Value::Number(number) => Json::Integer(number.to_string()),
        Value::Array(values) => {
            Json::Sequence(values.iter().map(owned_json).collect::<Result<_, _>>()?)
        }
        Value::Object(values) => Json::Object(
            values
                .iter()
                .map(|(name, value)| Ok((name.clone(), owned_json(value)?)))
                .collect::<Result<_, Error>>()?,
        ),
    })
}
fn records(report: &Value, limits: PreparationLimits) -> Result<Vec<Record>, Error> {
    let values = report["verifications"]
        .as_array()
        .ok_or(Error::InvalidObservation)?;
    if values.len() > limits.core.records {
        return Err(Error::OutputLimit);
    }
    // Admission precedes all owned metadata, full key and detail-context copies.
    let mut budget = crate::report_projection::Budget::new(limits.report_bytes);
    budget.json(&report["verifications"])?;
    values
        .iter()
        .map(|value| {
            let failure = match value.get("failure") {
                Some(Value::Null) => None,
                Some(failure) if failure.is_object() => {
                    let context = failure["log_context"]
                        .as_object()
                        .ok_or(Error::InvalidObservation)?;
                    let keys = failure["offending_keys"]
                        .as_array()
                        .ok_or(Error::InvalidObservation)?;
                    let details = context
                        .iter()
                        .filter(|(name, _)| {
                            !matches!(name.as_str(), "failure_count" | "keys" | "verification_id")
                        })
                        .map(|(name, value)| Ok((name.clone(), owned_json(value)?)))
                        .collect::<Result<BTreeMap<_, _>, Error>>()?;
                    Some(Failure {
                        condition: text(failure, "condition")?,
                        count: count(&failure["log_context"], "failure_count")?,
                        keys: keys.iter().map(owned_json).collect::<Result<_, _>>()?,
                        details,
                    })
                }
                _ => return invalid(),
            };
            Ok(Record {
                path: text(value, "spec_path")?,
                id: optional(value, "verification_id")?,
                check: text(value, "check")?,
                target: optional(value, "target")?,
                requirement: text(value, "requirement")?,
                evaluated: count(value, "evaluated_count")?,
                failure,
                severity: match value["severity"].as_str() {
                    Some("error") => VerificationSeverity::Error,
                    Some("warning") => VerificationSeverity::Warning,
                    _ => return invalid(),
                },
            })
        })
        .collect()
}
fn held(path: &str, table: core::LogTable, limits: PreparationLimits) -> Result<HeldLog, Error> {
    let (name, suffix) = path
        .rsplit('/')
        .next()
        .and_then(|name| name.rsplit_once('.'))
        .ok_or(Error::InvalidObservation)?;
    let profile = if suffix.eq_ignore_ascii_case("csv") {
        "csv"
    } else if suffix.eq_ignore_ascii_case("parquet") {
        "parquet"
    } else {
        return invalid();
    };
    let projection = (0..table.schema().columns().len()).collect::<Vec<_>>();
    let bytes = if profile == "csv" {
        crate::csv_artifact::render(&table, &projection, limits.output_bytes).map_err(|e| {
            if matches!(e, crate::csv_artifact::Error::Limit) {
                Error::OutputLimit
            } else {
                Error::InvalidObservation
            }
        })?
    } else {
        crate::parquet_artifact::render(
            &table,
            &projection,
            crate::parquet_artifact::Limits {
                output_bytes: limits.output_bytes,
                staged_bytes: limits.output_bytes,
                cells: limits.core.cells,
                columns: 13,
            },
        )
        .map_err(|e| {
            if matches!(e, crate::parquet_artifact::Error::Limit) {
                Error::OutputLimit
            } else {
                Error::InvalidObservation
            }
        })?
    };
    let mut budget = crate::report_projection::Budget::new(limits.report_bytes);
    budget.text(path)?;
    let (content, rows) = if profile == "csv" {
        let content = std::str::from_utf8(&bytes).map_err(|_| Error::InvalidObservation)?;
        budget.text(content)?;
        budget.text(content)?;
        budget.entries(content.bytes().filter(|&byte| byte == b'\n').count())?;
        (
            content,
            content
                .strip_suffix('\n')
                .ok_or(Error::InvalidObservation)?
                .split('\n')
                .map(String::from)
                .collect::<Vec<_>>(),
        )
    } else {
        budget.shape(table.row_count(), projection.len())?;
        for row in 0..table.row_count() {
            for &column in &projection {
                budget.cell(
                    &table
                        .cell(row, column)
                        .map_err(|_| Error::InvalidObservation)?,
                )?;
            }
        }
        (
            "",
            crate::parquet_artifact::records(&table, &projection, limits.report_bytes).map_err(
                |e| {
                    if matches!(e, crate::parquet_artifact::Error::Limit) {
                        Error::OutputLimit
                    } else {
                        Error::InvalidObservation
                    }
                },
            )?,
        )
    };
    let observation = json!({"name":name,"profile":profile,"columns":table.schema().columns().iter().map(|c|&c.name).collect::<Vec<_>>(),"types":table.schema().columns().iter().map(|c|crate::specification_diagnostics::type_name(c.kind)).collect::<Vec<_>>(),"row_count":table.row_count(),"records":rows,"byte_length":bytes.len(),"content":content});
    crate::ipc_table::bounded_json(&observation, limits.observation_bytes)
        .map_err(|_| Error::OutputLimit)?;
    let logical = crate::ipc_table::encode_log_table(&table).map_err(|_| Error::OutputLimit)?;
    let table = PublicTable::decode(&logical).map_err(|_| Error::InvalidObservation)?;
    let ipc = table.ipc().map_err(|_| Error::OutputLimit)?;
    Ok(HeldLog {
        path: path.into(),
        bytes: bytes.into(),
        table,
        ipc,
        observation,
    })
}
#[derive(Default)]
pub(crate) struct PreparedLogs {
    pub(crate) verification: Option<HeldLog>,
    pub(crate) warning: Option<HeldLog>,
    pub(crate) failure: Option<Error>,
}
/// Conservative graph shares admit record/context/table ownership before any
/// codec or report copy grows. Ordinary single-domain defaults are unchanged.
#[derive(Clone, Copy)]
pub(crate) struct PreparationLimits {
    pub(crate) core: core::Limits,
    pub(crate) output_bytes: usize,
    pub(crate) total_output_bytes: usize,
    pub(crate) report_bytes: usize,
    pub(crate) observation_bytes: usize,
}
impl Default for PreparationLimits {
    fn default() -> Self {
        Self {
            core: Default::default(),
            output_bytes: 8_388_608,
            total_output_bytes: 16_777_216,
            report_bytes: 16_777_216,
            observation_bytes: 8_388_608,
        }
    }
}
impl PreparationLimits {
    pub(crate) fn graph(
        nodes: usize,
        logs: usize,
        output_bytes: usize,
        report_bytes: usize,
    ) -> Self {
        let nodes = nodes.max(1);
        let slots = nodes.saturating_mul(logs.max(1));
        let defaults = Self::default();
        Self {
            core: core::Limits {
                records: defaults.core.records / nodes,
                cells: defaults.core.cells / slots,
                text: defaults.core.text / slots,
                json_nodes: defaults.core.json_nodes / slots,
            },
            output_bytes: output_bytes.min(defaults.output_bytes / slots),
            total_output_bytes: output_bytes,
            report_bytes: report_bytes / logs.max(1),
            observation_bytes: defaults.observation_bytes / slots,
        }
    }
}
pub(crate) fn prepare(
    run: &dyn RunView,
    report: &Value,
    success: bool,
) -> Result<PreparedLogs, Error> {
    let mut output = PreparedLogs::default();
    prepare_into(run, report, success, Default::default(), &mut output)?;
    Ok(output)
}
/// Fill retained caller-owned slots so a later warning failure or native unwind
/// cannot erase an already completed verification capability.
pub(crate) fn prepare_into(
    run: &dyn RunView,
    report: &Value,
    success: bool,
    limits: PreparationLimits,
    output: &mut PreparedLogs,
) -> Result<(), Error> {
    *output = PreparedLogs::default();
    let declarations = run.compiled().output_logs();
    if !declarations.declared() {
        return Ok(());
    }
    let records = records(report, limits)?;
    let logs = core::build(
        &records,
        run.compiled().output_path(),
        declarations.verification().is_some(),
        false,
        success,
        limits.core,
    )
    .map_err(|error| match error {
        core::Error::Limit => Error::OutputLimit,
        core::Error::InvalidRecord => Error::InvalidObservation,
    })?;
    output.verification = logs
        .verification
        .map(|table| {
            held(
                declarations
                    .verification()
                    .expect("declared verification log"),
                table,
                limits,
            )
        })
        .transpose()?;
    // The verification capability is retained before warning preparation enters.
    let warning_limits = PreparationLimits {
        output_bytes: limits.output_bytes.min(
            limits
                .total_output_bytes
                .checked_sub(
                    output
                        .verification
                        .as_ref()
                        .map_or(0, |log| log.bytes().len()),
                )
                .ok_or(Error::OutputLimit)?,
        ),
        ..limits
    };
    let warning = if let Some(path) = declarations.warning().filter(|_| success) {
        core::build(
            &records,
            run.compiled().output_path(),
            false,
            true,
            true,
            limits.core,
        )
        .map_err(|error| match error {
            core::Error::Limit => Error::OutputLimit,
            core::Error::InvalidRecord => Error::InvalidObservation,
        })
        .and_then(|logs| {
            held(
                path,
                logs.warning.expect("declared warning log"),
                warning_limits,
            )
        })
        .map(Some)
    } else {
        Ok(None)
    };
    match warning {
        Ok(warning) => output.warning = warning,
        Err(error) => output.failure = Some(error),
    }
    Ok(())
}

/// Compare the at-most-three declaration locations without requiring parent or
/// target existence. Operational path refusal remains at the publication gate.
#[cfg(any(unix, windows))]
pub(crate) fn location_findings(
    resources: &crate::file_resources::Resources,
    run: &dyn RunView,
) -> Vec<yamaa_core::specification::PreflightFinding> {
    use yamaa_core::specification::{LogFinding, PreflightFinding};
    if !run.compiled().output_logs().declared() {
        return Vec::new();
    }
    let paths = ["path", "warning_log", "verification_log"].map(|field| {
        run.document()
            .written_output_field_origin(field)
            .and_then(|origin| {
                resources
                    .location_from(origin.declaring_source, origin.written)
                    .ok()
            })
            .map(|location| (field, location))
    });
    let mut findings = Vec::new();
    for (left, right) in [(0, 1), (0, 2), (1, 2)] {
        if let (Some((_, a)), Some((_, b))) = (&paths[left], &paths[right]) {
            #[cfg(windows)]
            let same = crate::windows_file::equal_name(a, b);
            #[cfg(unix)]
            let same = a == b;
            if same {
                let fields = [
                    "output.path",
                    "output.warning_log",
                    "output.verification_log",
                ];
                let written = run
                    .document()
                    .written_output_field_origin(paths[right].as_ref().expect("declared").0)
                    .expect("original declaration")
                    .written;
                findings.push(PreflightFinding::Log(LogFinding::Collision {
                    left: fields[left],
                    right: fields[right],
                    path: written.into(),
                }));
            }
        }
    }
    findings
}
