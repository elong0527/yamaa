//! Build the run report from retained native observations and render its
//! declared file. Neither host supplies these facts, no source is reopened,
//! and no constructor accepts a publisher.
use crate::json;
use crate::{
    public_table::PublicTable, specification_report::Error, specification_run::PreparedRun,
};
use std::{collections::BTreeMap, sync::Arc};
use yamaa_core::json::Value;
use yamaa_core::{
    dataset::{Check, Execution, TemplateRows, VerificationSeverity},
    run_report::{self as core, Facts},
    specification::SourceProfile,
    table::{TableAccess, ValueRef},
    value::{ColumnType, Value as Scalar},
};

/// The rendered report file a specification declares in `output.report`.
#[derive(Debug)]
pub(crate) struct HeldFile {
    path: String,
    bytes: Arc<[u8]>,
    observation: Value,
}

/// The verified report every run builds, its table and, when the
/// specification declares `output.report`, its rendered file.
#[derive(Debug)]
pub(crate) struct HeldReport {
    file: Option<HeldFile>,
    table: PublicTable,
}
impl HeldReport {
    pub(crate) fn table(&self) -> &PublicTable {
        &self.table
    }
    /// The declared file's path, rendered bytes and artifact observation.
    pub(crate) fn file(&self) -> Option<(&str, &[u8], &Value)> {
        self.file
            .as_ref()
            .map(|file| (file.path.as_str(), &*file.bytes, &file.observation))
    }
}

/// What each captured input yielded and each completed row template counted,
/// read from the attempt; reporting never captures or decodes a source again.
#[derive(Debug, Default)]
pub(crate) struct RunFacts {
    /// `(name, records, bytes, memory)` for each source the attempt entered.
    inputs: Vec<(String, Option<usize>, Option<usize>, bool)>,
    templates: Vec<TemplateRows>,
}
impl RunFacts {
    pub(crate) fn captured<T: TableAccess>(
        sources: &[yamaa_core::specification_run::CapturedSource<T>],
        templates: &[TemplateRows],
    ) -> Self {
        Self {
            inputs: sources
                .iter()
                .map(|source| {
                    (
                        source.read.source.name.clone(),
                        source.table.as_ref().map(TableAccess::row_count),
                        source.snapshot.as_ref().map(|bytes| bytes.len()),
                        source.read.origin == Some(yamaa_core::resolver::Origin::Memory),
                    )
                })
                .collect(),
            templates: templates.to_vec(),
        }
    }
}

/// Governed JSON byte projection for prepared report cells. Core owns fact
/// admission, row semantics and verification; this writer owns only the
/// authored ordering, escaping and float/integer byte behavior of KEYS and
/// DETAILS values.
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

/// Encode prepared typed rows into governed text cells and assemble the final
/// table through the core schema owner.
pub(crate) fn project(prepared: Vec<Vec<core::PreparedCell>>) -> Result<core::Report, core::Error> {
    let rows = prepared
        .into_iter()
        .map(|row| {
            row.into_iter()
                .map(|cell| {
                    Ok::<_, core::Error>(match cell {
                        core::PreparedCell::Missing => Scalar::Missing,
                        core::PreparedCell::Text(text) => Scalar::Str(text),
                        core::PreparedCell::Count(count) => Scalar::Int(count),
                        core::PreparedCell::Details(details) => {
                            Scalar::Str(governed(|writer| writer.object(&details))?)
                        }
                        core::PreparedCell::Keys(keys) => {
                            Scalar::Str(governed(|writer| writer.sequence(&keys))?)
                        }
                    })
                })
                .collect()
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(core::assemble(rows))
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
fn profile(profile: SourceProfile) -> &'static str {
    match profile {
        SourceProfile::Csv => "csv",
        SourceProfile::Parquet => "parquet",
        SourceProfile::Xport => "xpt",
        SourceProfile::Sas7bdat => "sas7bdat",
        SourceProfile::OdmXml => "odm",
        SourceProfile::DatasetJson => "dataset_json",
        SourceProfile::DatasetNdjson => "dataset_ndjson",
    }
}

/// The evaluated checks in execution order. A value-level codelist check runs
/// once per bound codelist; each run is reported at the path that declares
/// that codelist, so every check row keeps a unique specification path.
fn checks(run: &PreparedRun, report: &Value) -> Result<Vec<core::Check>, Error> {
    let values = report["verifications"]
        .as_array()
        .ok_or(Error::InvalidObservation)?;
    let mut occurrences = BTreeMap::<String, usize>::new();
    values
        .iter()
        .map(|value| {
            let path = text(value, "spec_path")?;
            let occurrence = occurrences.entry(path.clone()).or_default();
            let report_path = match run.compiled().column_check(&path) {
                Some(Check::ValueCodelists { bindings, .. }) => bindings
                    .get(*occurrence)
                    .map(|binding| binding.path.clone())
                    .ok_or(Error::InvalidObservation)?,
                _ if *occurrence > 0 => return invalid(),
                _ => path,
            };
            *occurrence += 1;
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
                    Some(core::Failure {
                        condition: text(failure, "condition")?,
                        count: count(&failure["log_context"], "failure_count")?,
                        keys: keys.clone(),
                        details,
                    })
                }
                _ => return invalid(),
            };
            Ok(core::Check {
                path: report_path,
                name: text(value, "check")?,
                id: optional(value, "verification_id")?,
                target: optional(value, "target")?,
                rule: text(value, "rule")?,
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

/// Every handler path the attempt registered, in registration order
/// (COLUMN-14), zero counts included.
fn handlers(report: &Value) -> Result<Vec<core::Handler>, Error> {
    report["handler_counts"]
        .as_array()
        .ok_or(Error::InvalidObservation)?
        .iter()
        .map(|entry| {
            Ok(core::Handler {
                path: text(entry, "spec_path")?,
                name: text(entry, "handler")?,
                count: entry["count"].as_u64().ok_or(Error::InvalidObservation)?,
            })
        })
        .collect()
}

/// Missing values, output rows and, for `str` columns, the longest value of
/// each output column in `output.columns` order, combined over the output's
/// partitions (D264): the counts add up and the longest value is the longest
/// in any partition, so the facts, and the report, are the whole output's.
fn columns(run: &PreparedRun, execution: &Execution) -> Result<Vec<core::Column>, Error> {
    let dataset = &execution.dataset;
    let partitions = execution.output_partitions();
    run.compiled()
        .projection()
        .iter()
        .map(|name| {
            let index = dataset
                .schema()
                .columns()
                .iter()
                .position(|column| &column.name == name)
                .ok_or(Error::InvalidObservation)?;
            let text = dataset.schema().columns()[index].kind == ColumnType::Str;
            let mut column = core::Column {
                name: name.clone(),
                missing: 0,
                rows: 0,
                max_length: text.then_some(0),
            };
            for partition in &partitions {
                let values = partition.clone().map(|row| {
                    dataset
                        .cell(row, index)
                        .map_err(|_| Error::InvalidObservation)
                });
                for value in values {
                    match value? {
                        ValueRef::Missing => column.missing += 1,
                        ValueRef::Str(text) => {
                            if let Some(longest) = &mut column.max_length {
                                *longest = (*longest).max(text.chars().count());
                            }
                        }
                        _ => {}
                    }
                }
                column.rows += partition.len();
            }
            Ok(column)
        })
        .collect()
}

/// Gather the report facts. `condition` is the fatal condition of a failed
/// run; a successful run supplies its execution.
fn facts(
    run: &PreparedRun,
    report: &Value,
    observed: &RunFacts,
    condition: Option<&str>,
    execution: Option<&Execution>,
) -> Result<Facts, Error> {
    let compiled = run.compiled();
    let (outcome, columns) = match (condition, execution) {
        (Some(condition), _) => (
            core::Run::Failed {
                condition: condition.into(),
            },
            Vec::new(),
        ),
        (None, Some(execution)) => (
            core::Run::Succeeded {
                rows: execution.dataset.row_count(),
            },
            columns(run, execution)?,
        ),
        (None, None) => return invalid(),
    };
    let inputs = compiled
        .sources()
        .iter()
        .map(|source| {
            let (records, bytes, memory) = observed
                .inputs
                .iter()
                .find(|(name, _, _, _)| name == &source.name)
                .map_or((None, None, false), |&(_, records, bytes, memory)| {
                    (records, bytes, memory)
                });
            Ok(core::Input {
                name: source.name.clone(),
                path: if memory {
                    format!("memory:{}", source.name)
                } else {
                    run.document()
                        .written_source_path(&source.name)
                        .ok_or(Error::InvalidObservation)?
                        .into()
                },
                profile: profile(source.profile),
                // The producing specification as written, for an input that
                // declares a producer link; a null schema declares none.
                schema: run
                    .document()
                    .written_input_origin(&source.name, "schema")
                    .map(|origin| origin.written.to_owned()),
                records,
                bytes,
            })
        })
        .collect::<Result<Vec<_>, Error>>()?;
    let constructions = compiled.row_constructions();
    if observed.templates.len() > constructions.len() {
        return invalid();
    }
    let rows = constructions
        .into_iter()
        .enumerate()
        .map(|(index, (path, driver))| {
            let counted = observed.templates.get(index);
            core::Rows {
                path,
                driver: driver.into(),
                emitted: counted.map(|template| template.emitted),
                read: counted.map(|template| template.read),
            }
        })
        .collect();
    Ok(Facts {
        artifact: compiled.output_path().into(),
        run: outcome,
        inputs,
        rows,
        handlers: handlers(report)?,
        columns,
        checks: checks(run, report)?,
    })
}

fn rendered(path: &str, report: &core::Report) -> Result<HeldFile, Error> {
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
    let projection = (0..report.schema().columns().len()).collect::<Vec<_>>();
    let bytes = if profile == "csv" {
        crate::csv_artifact::render(report, &projection, None).map_err(|e| match e {
            crate::csv_artifact::Error::Limit {
                resource,
                limit,
                actual,
                ..
            } => Error::ResourceLimit(crate::resource_diagnostics::Refusal::new(
                resource,
                limit,
                actual,
                vec!["output.report".into()],
            )),
            _ => Error::InvalidObservation,
        })?
    } else {
        crate::parquet_artifact::render(report, &projection).map_err(parquet_error)?
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
            crate::parquet_artifact::records(report, &projection, None).map_err(parquet_error)?,
        )
    };
    let observation = json!({"name":name,"profile":profile,"columns":report.schema().columns().iter().map(|c|&c.name).collect::<Vec<_>>(),"types":report.schema().columns().iter().map(|c|crate::specification_diagnostics::type_name(c.kind)).collect::<Vec<_>>(),"row_count":report.row_count(),"records":rows,"byte_length":bytes.len(),"content":content});
    Ok(HeldFile {
        path: path.into(),
        bytes: bytes.into(),
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
            vec!["output.report".into()],
        )),
        _ => Error::InvalidObservation,
    }
}

/// Build, verify and render the run report from the retained observations.
/// A failed run names its fatal condition; a successful run supplies its
/// execution. A report that cannot be built or verified is an execution
/// defect: the caller refuses the result rather than recording it.
pub(crate) fn prepare(
    run: &PreparedRun,
    report: &Value,
    observed: &RunFacts,
    condition: Option<&str>,
    execution: Option<&Execution>,
) -> Result<HeldReport, Error> {
    let facts = facts(run, report, observed, condition, execution)?;
    let table = project(core::prepare(&facts).map_err(map_core_error)?).map_err(map_core_error)?;
    core::verify(&table, &facts).map_err(map_core_error)?;
    let file = run
        .compiled()
        .output_report()
        .map(|path| rendered(path, &table))
        .transpose()?;
    let projection = (0..table.schema().columns().len()).collect::<Vec<_>>();
    let table =
        PublicTable::from_table(&table, &projection).map_err(|_| Error::InvalidObservation)?;
    Ok(HeldReport { file, table })
}

/// What a saved report file records (PROJECT-15), decoded under the profile
/// its extension selects, as [`rendered`] wrote it: `None` when the bytes are
/// not a run report.
pub(crate) fn recorded(bytes: &[u8], path: &str) -> Option<core::Recorded> {
    let (_, suffix) = path.rsplit('/').next()?.rsplit_once('.')?;
    if suffix.eq_ignore_ascii_case("csv") {
        // The report's own column types, so that COUNT and TOTAL read as
        // integers and an empty field as missing.
        let types = core::schema()
            .columns()
            .iter()
            .map(|column| (column.name.clone(), column.kind))
            .collect::<Vec<_>>();
        let table = yamaa_core::typed_csv::PreparedTypes::new(&types)
            .ok()?
            .convert(crate::csv_source::parse(bytes).ok()?)
            .ok()?;
        core::recorded(&table, details_bytes)
    } else if suffix.eq_ignore_ascii_case("parquet") {
        core::recorded(&crate::parquet_source::parse(bytes).ok()?, details_bytes)
    } else {
        None
    }
}

/// The `bytes` member of an input row's `DETAILS` (CHECK-24): `Some(None)`
/// for `null`, `None` when the text holds no byte length.
fn details_bytes(details: &str) -> Option<Option<usize>> {
    match crate::json_wire::decode(details).ok()?.get("bytes")? {
        Value::Null => Some(None),
        value => value
            .as_u64()
            .and_then(|bytes| usize::try_from(bytes).ok())
            .map(Some),
    }
}

/// The run report's columns without rows, the stacked report of runs of
/// which none built a report.
pub(crate) fn empty_table() -> Result<PublicTable, Error> {
    let table = core::assemble(Vec::new());
    let projection = (0..table.schema().columns().len()).collect::<Vec<_>>();
    PublicTable::from_table(&table, &projection).map_err(|_| Error::InvalidObservation)
}

/// The output locations a run admits before any study read: `output.path`
/// and `output.report` name two files, and neither names an `output.path` or
/// `output.report` location of a producer an input links, whose files the run
/// reads. The comparison needs neither file nor parent to exist; operational
/// path refusal remains at the publication gate.
pub(crate) fn location_findings(
    resources: &crate::file_resources::Resources,
    run: &PreparedRun,
    links: &[crate::specification_source::LinkedSpecification],
) -> Vec<yamaa_core::specification::PreflightFinding> {
    use crate::file_resources::same_location;
    use yamaa_core::specification::{PreflightFinding, ReportFinding};
    let fields = [("path", "output.path"), ("report", "output.report")];
    fn location<'a>(
        resources: &crate::file_resources::Resources,
        document: &'a crate::specification_source::PreparedDocument,
        field: &str,
    ) -> Option<(&'a str, String)> {
        let origin = document.written_output_field_origin(field)?;
        let location = resources
            .location_from(origin.base_source, origin.written)
            .ok()?;
        Some((origin.written, location))
    }
    let declares_report = run.compiled().output_report().is_some();
    let own = fields
        .into_iter()
        .filter(|(field, _)| *field == "path" || declares_report)
        .filter_map(|(field, right)| {
            location(resources, run.document(), field)
                .map(|(written, location)| (right, written, location))
        })
        .collect::<Vec<_>>();
    let mut findings = Vec::new();
    if let [(_, _, primary), (_, written, report)] = own.as_slice() {
        if same_location(primary, report) {
            findings.push(PreflightFinding::Report(ReportFinding::Collision {
                path: (*written).into(),
            }));
        }
    }
    // A producer that several inputs link is compared once.
    let mut producers: Vec<&str> = Vec::new();
    for link in links {
        let Some(producer) = &link.document else {
            continue;
        };
        if producers.contains(&link.identity.as_str()) {
            continue;
        }
        producers.push(&link.identity);
        for (field, left) in fields {
            let Some((_, theirs)) = location(resources, producer, field) else {
                continue;
            };
            for (right, written, ours) in &own {
                if same_location(&theirs, ours) {
                    findings.push(PreflightFinding::Report(ReportFinding::GraphCollision {
                        left,
                        right,
                        path: (*written).into(),
                    }));
                }
            }
        }
    }
    findings
}

#[cfg(test)]
mod tests {
    use super::*;
    use yamaa_core::table::ValueRef;

    fn range_failed() -> yamaa_core::diagnostic::Definition {
        yamaa_core::diagnostic::ConditionCode::VerificationRangeFailed.definition()
    }
    fn facts(checks: Vec<core::Check>) -> Facts {
        Facts {
            artifact: "out.csv".into(),
            run: core::Run::Succeeded { rows: 8 },
            inputs: vec![core::Input {
                name: "SRC".into(),
                path: "input/é.csv".into(),
                profile: "csv",
                schema: None,
                records: Some(8),
                bytes: Some(120),
            }],
            rows: vec![core::Rows {
                path: "base".into(),
                driver: "SRC".into(),
                emitted: Some(8),
                read: Some(8),
            }],
            handlers: Vec::new(),
            columns: vec![core::Column {
                name: "AGE".into(),
                missing: 1,
                rows: 8,
                max_length: None,
            }],
            checks,
        }
    }
    fn held() -> core::Check {
        core::Check {
            path: "columns.AGE.verifications[0].not_missing".into(),
            name: "not_missing".into(),
            id: None,
            target: Some("AGE".into()),
            rule: yamaa_core::diagnostic::ConditionCode::VerificationNotMissingFailed
                .definition()
                .rule
                .expect("registered rule")
                .into(),
            severity: VerificationSeverity::Error,
            evaluated: 8,
            failure: None,
        }
    }
    fn warning() -> core::Check {
        core::Check {
            path: "columns.AGE.verifications[1].range".into(),
            name: "range".into(),
            id: Some("é🙂".into()),
            target: Some("AGE".into()),
            rule: range_failed().rule.expect("registered rule").into(),
            severity: VerificationSeverity::Warning,
            evaluated: 8,
            failure: Some(core::Failure {
                condition: range_failed().condition.into(),
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
    fn report(facts: &Facts) -> core::Report {
        let report = project(core::prepare(facts).unwrap()).unwrap();
        core::verify(&report, facts).unwrap();
        report
    }

    #[test]
    fn governed_projection_preserves_ascii_details_and_all_keys() {
        let facts = facts(vec![held(), warning()]);
        let report = report(&facts);
        assert_eq!(report.row_count(), 6);
        assert_eq!(report.cell(0, 1).unwrap(), ValueRef::Str("run"));
        assert_eq!(report.cell(0, 8).unwrap(), ValueRef::Str("succeeded"));
        assert_eq!(report.cell(0, 10).unwrap(), ValueRef::Int(8));
        assert_eq!(
            report.cell(1, 13).unwrap(),
            ValueRef::Str("{\"bytes\":120,\"path\":\"input/\\u00e9.csv\",\"profile\":\"csv\"}")
        );
        assert_eq!(report.cell(3, 13).unwrap(), ValueRef::Str("{}"));
        assert_eq!(report.cell(4, 4).unwrap(), ValueRef::Missing);
        assert_eq!(report.cell(4, 8).unwrap(), ValueRef::Str("held"));
        assert_eq!(report.cell(4, 9).unwrap(), ValueRef::Missing);
        assert_eq!(report.cell(4, 10).unwrap(), ValueRef::Int(0));
        assert_eq!(report.cell(4, 12).unwrap(), ValueRef::Missing);
        assert_eq!(report.cell(4, 13).unwrap(), ValueRef::Str("{}"));
        let expected =
            "{\"minimum\":-9223372036854775808,\"z\":\"\\u00e9\\ud83d\\ude42\\n\\t\\u0000\\\"\\\\\"}";
        assert_eq!(report.cell(5, 13).unwrap(), ValueRef::Str(expected));
        assert_eq!(report.cell(5, 10).unwrap(), ValueRef::Int(7));
        assert_eq!(report.cell(5, 11).unwrap(), ValueRef::Int(8));
        assert_eq!(
            report.cell(5, 12).unwrap(),
            ValueRef::Str(
                "[{\"ID\":0},{\"ID\":1},{\"ID\":2},{\"ID\":3},{\"ID\":4},{\"ID\":5},{\"ID\":6}]"
            )
        );
    }

    #[test]
    fn governed_projection_preserves_canonical_numbers_and_temporal_values() {
        let mut check = warning();
        check.failure.as_mut().unwrap().details = BTreeMap::from([
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
        let report = report(&facts(vec![check]));
        let expected = r#"{"date":"2025-01-02","datetime":"2025-01-02T03:04:00","integer_float":1,"large":100000000000000000000,"negzero":-0,"tiny":0.0000001,"wide":12345678901234567890123456789012345678901234567890}"#;
        assert_eq!(report.cell(4, 13).unwrap(), ValueRef::Str(expected));
    }
}
