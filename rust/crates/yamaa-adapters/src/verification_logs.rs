//! Convert retained native check observations into governed owned log artifacts.
//! Neither host supplies these records, and no constructor accepts a publisher.
use crate::json;
use crate::{
    public_table::PublicTable, specification_report::Error, specification_run_view::RunView,
};
use std::{collections::BTreeMap, sync::Arc};
use yamaa_core::json::Value;
use yamaa_core::{
    dataset::VerificationSeverity,
    table::TableAccess,
    verification_logs::{self as core, Failure, Record},
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

/// Governed JSON byte projection for prepared log cells. Core owns record
/// admission, count/severity rules and scalar semantics; this writer owns
/// only the authored ordering, escaping and float/integer byte behavior of
/// DETAILS/OFFENDING_KEYS values.
#[derive(Default)]
struct GovernedWriter {
    output: String,
}
impl GovernedWriter {
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
    fn json(&mut self, value: &Value) -> Result<(), core::Error> {
        match value {
            Value::Null => self.write("null"),
            Value::Bool(value) => self.write(if *value { "true" } else { "false" }),
            Value::String(value) => self.string(value),
            Value::Number(number) if !number.is_integer() => {
                let value = number
                    .as_f64()
                    .and_then(yamaa_core::value::FiniteFloat::new)
                    .ok_or(core::Error::InvalidRecord)?;
                self.write(&yamaa_core::conversion::float_text(value))
            }
            Value::Number(number) => self.write(number.as_str()),
            Value::Array(values) => return self.sequence(values),
            Value::Object(values) => return self.object(values),
        }
        Ok(())
    }
    fn sequence(&mut self, values: &[Value]) -> Result<(), core::Error> {
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
        values: impl IntoIterator<Item = (&'a String, &'a Value)>,
    ) -> Result<(), core::Error> {
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
fn governed(
    write: impl FnOnce(&mut GovernedWriter) -> Result<(), core::Error>,
) -> Result<String, core::Error> {
    let mut writer = GovernedWriter::default();
    write(&mut writer)?;
    Ok(writer.output)
}

/// Encode prepared typed rows into governed text cells and assemble the
/// final tables through the core schema owner.
pub(crate) fn project(prepared: core::Prepared) -> Result<core::Logs, core::Error> {
    fn rows(
        prepared: Vec<Vec<core::PreparedCell>>,
    ) -> Result<Vec<Vec<yamaa_core::value::Value>>, core::Error> {
        prepared
            .into_iter()
            .map(|row| {
                row.into_iter()
                    .map(|cell| {
                        Ok(match cell {
                            core::PreparedCell::Missing => yamaa_core::value::Value::Missing,
                            core::PreparedCell::Text(text) => yamaa_core::value::Value::Str(text),
                            core::PreparedCell::Count(count) => {
                                yamaa_core::value::Value::Int(count)
                            }
                            core::PreparedCell::Details(details) => {
                                yamaa_core::value::Value::Str(governed(|writer| {
                                    writer.object(&details)
                                })?)
                            }
                            core::PreparedCell::Keys(keys) => {
                                yamaa_core::value::Value::Str(governed(|writer| {
                                    writer.sequence(&keys)
                                })?)
                            }
                        })
                    })
                    .collect()
            })
            .collect()
    }
    Ok(core::assemble(
        prepared.verification.map(rows).transpose()?,
        prepared.warning.map(rows).transpose()?,
    ))
}

fn core_logs(
    records: &[Record],
    artifact: &str,
    verification: bool,
    warning: bool,
    success: bool,
) -> Result<core::Logs, Error> {
    let prepared =
        core::prepare(records, artifact, verification, warning, success).map_err(map_core_error)?;
    project(prepared).map_err(map_core_error)
}

fn map_core_error(error: core::Error) -> Error {
    match error {
        core::Error::Limit(error) => {
            Error::ResourceLimit(crate::resource_diagnostics::Refusal::new(
                error.resource,
                error.limit,
                error.actual,
                Vec::new(),
            ))
        }
        core::Error::InvalidRecord => Error::InvalidObservation,
    }
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
fn records(report: &Value) -> Result<Vec<Record>, Error> {
    let values = report["verifications"]
        .as_array()
        .ok_or(Error::InvalidObservation)?;
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
                        .map(|(name, value)| (name.clone(), value.clone()))
                        .collect::<BTreeMap<_, _>>();
                    Some(Failure {
                        condition: text(failure, "condition")?,
                        count: count(&failure["log_context"], "failure_count")?,
                        keys: keys.clone(),
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
fn held(path: &str, table: core::LogTable) -> Result<HeldLog, Error> {
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
        crate::csv_artifact::render(&table, &projection, None).map_err(|e| match e {
            crate::csv_artifact::Error::Limit {
                resource,
                limit,
                actual,
                ..
            } => Error::ResourceLimit(crate::resource_diagnostics::Refusal::new(
                resource,
                limit,
                actual,
                Vec::new(),
            )),
            _ => Error::InvalidObservation,
        })?
    } else {
        crate::parquet_artifact::render(&table, &projection).map_err(parquet_error)?
    };
    let (content, rows) = if profile == "csv" {
        let content = std::str::from_utf8(&bytes).map_err(|_| Error::InvalidObservation)?;
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
        (
            "",
            crate::parquet_artifact::records(&table, &projection).map_err(parquet_error)?,
        )
    };
    let observation = json!({"name":name,"profile":profile,"columns":table.schema().columns().iter().map(|c|&c.name).collect::<Vec<_>>(),"types":table.schema().columns().iter().map(|c|crate::specification_diagnostics::type_name(c.kind)).collect::<Vec<_>>(),"row_count":table.row_count(),"records":rows,"byte_length":bytes.len(),"content":content});
    let logical =
        crate::ipc_table::encode_log_table(&table).map_err(|_| Error::InvalidObservation)?;
    let table = PublicTable::decode(&logical).map_err(|_| Error::InvalidObservation)?;
    let ipc = table.ipc().map_err(|_| Error::InvalidObservation)?;
    Ok(HeldLog {
        path: path.into(),
        bytes: bytes.into(),
        table,
        ipc,
        observation,
    })
}
fn parquet_error(error: crate::parquet_artifact::Error<std::convert::Infallible>) -> Error {
    match error {
        crate::parquet_artifact::Error::Unavailable => {
            Error::CapabilityUnavailable { feature: "parquet" }
        }
        crate::parquet_artifact::Error::Limit {
            resource,
            limit,
            actual,
        } => Error::ResourceLimit(crate::resource_diagnostics::Refusal::new(
            resource,
            limit,
            actual,
            Vec::new(),
        )),
        _ => Error::InvalidObservation,
    }
}
#[derive(Default)]
pub(crate) struct PreparedLogs {
    pub(crate) verification: Option<HeldLog>,
    pub(crate) warning: Option<HeldLog>,
    pub(crate) failure: Option<Error>,
}
pub(crate) fn prepare(
    run: &dyn RunView,
    report: &Value,
    success: bool,
) -> Result<PreparedLogs, Error> {
    let mut output = PreparedLogs::default();
    prepare_into(run, report, success, &mut output)?;
    Ok(output)
}
/// Fill retained caller-owned slots so a later warning failure or native unwind
/// cannot erase an already completed verification capability.
pub(crate) fn prepare_into(
    run: &dyn RunView,
    report: &Value,
    success: bool,
    output: &mut PreparedLogs,
) -> Result<(), Error> {
    *output = PreparedLogs::default();
    let declarations = run.compiled().output_logs();
    if !declarations.declared() {
        return Ok(());
    }
    let records = records(report)?;
    let logs = core_logs(
        &records,
        run.compiled().output_path(),
        declarations.verification().is_some(),
        false,
        success,
    )?;
    output.verification = logs
        .verification
        .map(|table| {
            held(
                declarations
                    .verification()
                    .expect("declared verification log"),
                table,
            )
        })
        .transpose()?;
    // The verification capability is retained before warning preparation enters.
    let warning = if let Some(path) = declarations.warning().filter(|_| success) {
        core_logs(&records, run.compiled().output_path(), false, true, true)
            .and_then(|logs| held(path, logs.warning.expect("declared warning log")))
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
                    .location_from(origin.base_source, origin.written)
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

#[cfg(test)]
mod tests {
    use super::*;
    use yamaa_core::table::ValueRef;

    fn held() -> Record {
        Record {
            path: "columns.AGE.verifications[0].not_missing".into(),
            id: None,
            check: "not_missing".into(),
            target: Some("AGE".into()),
            requirement: "REQ-0376".into(),
            severity: VerificationSeverity::Error,
            evaluated: 8,
            failure: None,
        }
    }
    fn warning() -> Record {
        Record {
            path: "columns.AGE.verifications[1].range".into(),
            id: Some("é🙂".into()),
            check: "range".into(),
            target: Some("AGE".into()),
            requirement: "REQ-0377".into(),
            severity: VerificationSeverity::Warning,
            evaluated: 8,
            failure: Some(Failure {
                condition: "range_failed".into(),
                count: 7,
                keys: (0..7)
                    .map(|n| Value::Object([("ID".into(), Value::from(n))].into_iter().collect()))
                    .collect(),
                details: BTreeMap::from([
                    ("z".into(), Value::String("é🙂\n\t\0\"\\".into())),
                    ("minimum".into(), Value::from(i64::MIN)),
                ]),
            }),
        }
    }
    fn logs(records: &[Record]) -> core::Logs {
        project(core::prepare(records, "out.csv", true, true, true).unwrap()).unwrap()
    }

    #[test]
    fn governed_projection_preserves_ascii_details_and_all_keys() {
        let logs = logs(&[held(), warning()]);
        let verification = logs.verification.unwrap();
        assert_eq!(verification.row_count(), 2);
        assert_eq!(verification.cell(0, 3).unwrap(), ValueRef::Missing);
        assert_eq!(verification.cell(0, 8).unwrap(), ValueRef::Str("held"));
        assert_eq!(verification.cell(0, 9).unwrap(), ValueRef::Missing);
        assert_eq!(verification.cell(0, 11).unwrap(), ValueRef::Int(0));
        assert_eq!(verification.cell(0, 12).unwrap(), ValueRef::Str("{}"));
        let expected =
            "{\"minimum\":-9223372036854775808,\"z\":\"\\u00e9\\ud83d\\ude42\\n\\t\\u0000\\\"\\\\\"}";
        assert_eq!(verification.cell(1, 12).unwrap(), ValueRef::Str(expected));
        let warning = logs.warning.unwrap();
        assert_eq!(warning.row_count(), 1);
        assert_eq!(warning.schema().columns().len(), 10);
        assert_eq!(warning.cell(0, 7).unwrap(), ValueRef::Int(7));
        assert_eq!(
            warning.cell(0, 8).unwrap(),
            ValueRef::Str(
                "[{\"ID\":0},{\"ID\":1},{\"ID\":2},{\"ID\":3},{\"ID\":4},{\"ID\":5},{\"ID\":6}]"
            )
        );
        assert_eq!(warning.cell(0, 9).unwrap(), ValueRef::Str(expected));
    }

    #[test]
    fn governed_projection_preserves_canonical_numbers_and_temporal_values() {
        let mut record = warning();
        record.failure.as_mut().unwrap().details = BTreeMap::from([
            (
                "date".into(),
                Value::from(yamaa_core::value::Value::Date(
                    "2025-01-02".parse().unwrap(),
                )),
            ),
            (
                "datetime".into(),
                Value::from(yamaa_core::value::Value::DateTime(
                    "2025-01-02T03:04".parse().unwrap(),
                )),
            ),
            (
                "integer_float".into(),
                Value::from(yamaa_core::value::Value::float(1.0)),
            ),
            (
                "large".into(),
                Value::from(yamaa_core::value::Value::float(1.0e20)),
            ),
            (
                "negzero".into(),
                Value::from(yamaa_core::value::Value::float(-0.0)),
            ),
            (
                "tiny".into(),
                Value::from(yamaa_core::value::Value::float(1.0e-7)),
            ),
            (
                "wide".into(),
                Value::Number(
                    "12345678901234567890123456789012345678901234567890"
                        .parse()
                        .unwrap(),
                ),
            ),
        ]);
        let logs = logs(&[record]);
        let expected = r#"{"date":"2025-01-02","datetime":"2025-01-02T03:04:00","integer_float":1,"large":100000000000000000000,"negzero":-0,"tiny":0.0000001,"wide":12345678901234567890123456789012345678901234567890}"#;
        assert_eq!(
            logs.verification.as_ref().unwrap().cell(0, 12).unwrap(),
            ValueRef::Str(expected)
        );
        assert_eq!(
            logs.warning.as_ref().unwrap().cell(0, 9).unwrap(),
            ValueRef::Str(expected)
        );
    }
}
