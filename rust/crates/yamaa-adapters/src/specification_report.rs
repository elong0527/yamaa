//! Format observed bounded standalone runs and publish through explicit host authority.
//! Classified resource findings retain observations; opaque host failures stay opaque.
use crate::{
    scalar_transport::ScalarValue,
    specification_diagnostics,
    specification_run::{CapturedAttempt, PortError, PreparedRun},
};
use serde_json::{json, Map, Value};
use yamaa_core::{
    table::{TableAccess, ValueRef},
    value::Value as Scalar,
};

pub struct Identity<'a> {
    pub runtime: &'a str,
    pub runtime_version: &'a str,
    pub engine_version: &'a str,
    pub example: &'a str,
    pub specification: &'a str,
    pub base_directory: &'a str,
}
#[derive(Debug)]
pub enum Error {
    UnsupportedOutcome,
    InvalidObservation,
    OutputLimit,
}
fn plain(value: &Value) -> Result<Value, Error> {
    let scalar: ScalarValue =
        serde_json::from_value(value.clone()).map_err(|_| Error::InvalidObservation)?;
    Ok(
        match scalar.into_core().map_err(|_| Error::InvalidObservation)? {
            Scalar::Missing => Value::Null,
            Scalar::Str(v) => json!(v),
            Scalar::Int(v) => json!(v),
            Scalar::Float(v) => json!(v.get()),
            Scalar::Bool(v) => json!(v),
            Scalar::Date(v) => json!(v.to_string()),
            Scalar::DateTime(v) => json!(v.to_string()),
        },
    )
}
fn condition(run: &PreparedRun, value: &Value) -> Result<Value, Error> {
    let diagnostic = value.get("diagnostic").ok_or(Error::InvalidObservation)?;
    let mut context = diagnostic
        .get("context")
        .and_then(Value::as_object)
        .ok_or(Error::InvalidObservation)?
        .iter()
        .map(|(name, v)| Ok((name.clone(), plain(v)?)))
        .collect::<Result<Map<_, _>, Error>>()?;
    if let Some(identity) = value.get("identity").filter(|v| !v.is_null()) {
        let keys = identity
            .get("keys")
            .and_then(Value::as_array)
            .ok_or(Error::InvalidObservation)?;
        let names = run.key_names().collect::<Vec<_>>();
        if names.len() != keys.len() {
            return Err(Error::InvalidObservation);
        }
        let keys = names
            .into_iter()
            .zip(keys)
            .map(|(name, v)| Ok((name.to_owned(), plain(v)?)))
            .collect::<Result<Map<_, _>, Error>>()?;
        context.insert("keys".into(), json!([keys]));
    }
    if let Some(matched) = value.get("matched_key").filter(|v| !v.is_null()) {
        let matched = matched.as_array().ok_or(Error::InvalidObservation)?;
        let mut names = Vec::new();
        let mut key = Map::new();
        for field in matched {
            let name = field["name"].as_str().ok_or(Error::InvalidObservation)?;
            names.push(name);
            key.insert(name.into(), plain(&field["value"])?);
        }
        context.insert("key".into(), json!(names));
        context.insert("intermediate_key".into(), key.into());
    }
    if let Some(partition) = value.get("partition").filter(|v| !v.is_null()) {
        let partition = partition.as_array().ok_or(Error::InvalidObservation)?;
        let mut keys = Map::new();
        for field in partition {
            let name = field["name"].as_str().ok_or(Error::InvalidObservation)?;
            keys.insert(name.into(), plain(&field["value"])?);
        }
        context.insert("keys".into(), json!([keys]));
    }
    Ok(
        json!({"phase":diagnostic["phase"],"condition":diagnostic["condition"],"requirement":diagnostic["requirement"],"spec_paths":diagnostic["spec_paths"],"context":context}),
    )
}
/// A classified capture failure terminates the request sequence. Reject malformed
/// observations rather than attaching a resource finding to an unrelated failure.
fn captured_failure<E>(
    attempt: &CapturedAttempt<E>,
) -> Result<&crate::specification_run::CapturedSource, Error> {
    if !matches!(attempt.result, Err(PortError::Capture(_))) {
        return Err(Error::UnsupportedOutcome);
    }
    let (last, prefix) = attempt
        .sources
        .split_last()
        .ok_or(Error::InvalidObservation)?;
    if last.read.captured
        || last.read.failure.is_none()
        || last.snapshot.is_some()
        || last.table.is_some()
        || prefix
            .iter()
            .any(|s| !s.read.captured || s.read.failure.is_some())
    {
        return Err(Error::UnsupportedOutcome);
    }
    Ok(last)
}

fn inspected_failures<'a, E>(
    run: &PreparedRun,
    attempt: &'a CapturedAttempt<E>,
) -> Result<&'a [crate::specification_run::InspectionFailure<E>], Error> {
    let Err(PortError::Inspect(failures)) = &attempt.result else {
        return Err(Error::UnsupportedOutcome);
    };
    if !attempt.sources.is_empty() || failures.is_empty() {
        return Err(Error::InvalidObservation);
    }
    let mut declarations = run.compiled().sources().iter();
    for failure in failures {
        if failure.failure.is_none() {
            return Err(Error::UnsupportedOutcome);
        }
        if !declarations.any(|source| source == &failure.source) {
            return Err(Error::InvalidObservation);
        }
    }
    Ok(failures)
}

/// Refuse outcomes outside this formatter instead of inventing observations.
pub fn failure<E>(
    run: &PreparedRun,
    attempt: &CapturedAttempt<E>,
    id: Identity<'_>,
) -> Result<Value, Error> {
    if attempt.sources.is_empty() && !matches!(attempt.result, Err(PortError::Inspect(_))) {
        return Err(Error::UnsupportedOutcome);
    }
    let mut verifications = Vec::new();
    let diagnostics = match &attempt.result {
        Ok(response) => {
            if response.table.is_some() {
                return Err(Error::UnsupportedOutcome);
            }
            let value: Value =
                serde_json::from_str(&response.outcome).map_err(|_| Error::InvalidObservation)?;
            match value["outcome"]["status"].as_str() {
                Some("condition") => {
                    if value["outcome"]["verifications"].is_array() {
                        verifications = check_observations(run, &value["outcome"], &id)?.0;
                    }
                    vec![condition(run, &value["outcome"])?]
                }
                Some("failure") => {
                    let (records, diagnostics) = check_observations(run, &value["outcome"], &id)?;
                    verifications = records;
                    diagnostics
                }
                _ => return Err(Error::UnsupportedOutcome),
            }
        }
        Err(PortError::Run(error)) => {
            specification_diagnostics::findings(error, Some(run.source()))
                .ok_or(Error::UnsupportedOutcome)?
        }
        Err(PortError::Capture(_)) => {
            let source = captured_failure(attempt)?;
            let written = run
                .document()
                .written_source_path(&source.read.source.name)
                .ok_or(Error::InvalidObservation)?;
            let diagnostic = source
                .read
                .failure
                .ok_or(Error::InvalidObservation)?
                .diagnostic(&source.read.source.name, written);
            vec![specification_diagnostics::portable_diagnostic(diagnostic)
                .ok_or(Error::InvalidObservation)?]
        }
        Err(PortError::Inspect(_)) => inspected_failures(run, attempt)?
            .iter()
            .map(|failure| {
                let written = run
                    .document()
                    .written_source_path(&failure.source.name)
                    .ok_or(Error::InvalidObservation)?;
                specification_diagnostics::portable_diagnostic(
                    failure
                        .failure
                        .ok_or(Error::InvalidObservation)?
                        .diagnostic(&failure.source.name, written),
                )
                .ok_or(Error::InvalidObservation)
            })
            .collect::<Result<Vec<_>, Error>>()?,
        _ => return Err(Error::UnsupportedOutcome),
    };
    let mut report = envelope(run, attempt, &id)?;
    if let Ok(response) = &attempt.result {
        let value: Value =
            serde_json::from_str(&response.outcome).map_err(|_| Error::InvalidObservation)?;
        set_handler_counts(&mut report, &value)?;
    }
    set_diagnostics(&mut report, diagnostics);
    report["verifications"] = json!(verifications);
    bounded(report)
}
fn table_observation<T: TableAccess>(
    table: &T,
    id: &Identity<'_>,
    stage: &str,
    name: &str,
) -> Result<Value, Error> {
    let columns = table.schema().columns();
    let mut rows = Vec::new();
    for row in 0..table.row_count() {
        let mut cells = Vec::new();
        for column in 0..columns.len() {
            cells.push(
                match table
                    .cell(row, column)
                    .map_err(|_| Error::InvalidObservation)?
                {
                    ValueRef::Str(value) => json!({"type":"str","value":value}),
                    ValueRef::Missing => json!({"type":"missing","value":null}),
                    ValueRef::Int(value) => json!({"type":"int","value":value.to_string()}),
                    ValueRef::Float(value) => {
                        json!({"type":"float","value":format!("{:016x}",value.get().to_bits())})
                    }
                    ValueRef::Bool(value) => json!({"type":"bool","value":value}),
                    ValueRef::Date(value) => json!({"type":"date","value":value.to_string()}),
                    ValueRef::DateTime(value) => {
                        json!({"type":"datetime","value":value.to_string()})
                    }
                },
            );
        }
        rows.push(cells);
    }
    Ok(
        json!({"specification":id.specification,"stage":stage,"name":name,"columns":columns.iter().map(|c|&c.name).collect::<Vec<_>>(),"types":columns.iter().map(|c|specification_diagnostics::type_name(c.kind)).collect::<Vec<_>>(),"rows":rows}),
    )
}
fn envelope<E>(
    run: &PreparedRun,
    attempt: &CapturedAttempt<E>,
    id: &Identity<'_>,
) -> Result<Value, Error> {
    if attempt.sources.is_empty() {
        inspected_failures(run, attempt)?;
    }
    if attempt.sources.iter().any(|source| !source.read.captured) {
        captured_failure(attempt)?;
    } else if attempt
        .sources
        .iter()
        .any(|source| source.read.failure.is_some())
    {
        return Err(Error::InvalidObservation);
    }
    let mut tables = Vec::new();
    if attempt.sources.iter().all(|source| source.table.is_some()) {
        for source in &attempt.sources {
            tables.push(table_observation(
                source.table.as_ref().expect("complete ingestion"),
                id,
                "source",
                &source.read.source.name,
            )?);
        }
    }
    let reads = attempt.sources.iter().map(|source| Ok(json!({"base_directory":id.base_directory,"path":source.read.source.path,"outcome":if source.read.captured {"captured"} else {"failure"},"condition":source.read.failure.map(|failure| failure.code().definition().condition),"snapshots_created":source.read.snapshots_created.ok_or(Error::InvalidObservation)?}))).collect::<Result<Vec<_>,Error>>()?;
    Ok(
        json!({"report_version":"0.3.0-draft","runtime":id.runtime,"backend":"rust","runtime_version":id.runtime_version,"engine_version":id.engine_version,"example":id.example,
        "outcome":"failure","artifacts":[],"diagnostics":[],"unsupported":[],"handler_counts":[],
        "nodes":[{"specification":id.specification,"outcome":"failure","diagnostics":[],"unsupported":[],"handler_counts":[]}],
        "tables":tables,"verifications":[],"callbacks":[],
        "source_reads":reads,"error":null}),
    )
}
fn bounded(report: Value) -> Result<Value, Error> {
    if report.to_string().len() > 16_777_216 {
        Err(Error::OutputLimit)
    } else {
        Ok(report)
    }
}
fn set_handler_counts(report: &mut Value, response: &Value) -> Result<(), Error> {
    let Some(raw) = response.get("handler_counts") else {
        return Ok(());
    };
    let counts = raw
        .as_array()
        .ok_or(Error::InvalidObservation)?
        .iter()
        .map(|entry| {
            let path = entry["spec_path"]
                .as_str()
                .ok_or(Error::InvalidObservation)?;
            let handler = entry["handler"].as_str().ok_or(Error::InvalidObservation)?;
            Ok(json!({"spec_path":path,"handler":handler,"count":count(entry,"count")?}))
        })
        .collect::<Result<Vec<_>, Error>>()?;
    report["handler_counts"] = json!(counts);
    report["nodes"][0]["handler_counts"] = report["handler_counts"].clone();
    Ok(())
}
fn set_diagnostics(report: &mut Value, diagnostics: Vec<Value>) {
    report["diagnostics"] = json!(diagnostics);
    report["nodes"][0]["diagnostics"] = report["diagnostics"].clone();
}

pub use yamaa_engine::specification_output::ArtifactPort;
#[derive(Debug)]
pub enum CompleteError<E> {
    Report(Error),
    Publish(E),
}

/// An owned build result; obtaining it never calls a publisher. Table bytes are
/// the admitted output projection, while private report artifacts are prospective.
#[derive(Debug)]
pub struct BuildResult {
    prepared: yamaa_engine::specification_output::PreparedOutput<Value>,
    table: Option<Vec<u8>>,
}
impl BuildResult {
    pub fn output(&self) -> Option<&[u8]> {
        self.table.as_deref()
    }
    /// Build-phase observations exclude later save requests and their artifacts.
    pub fn observations(&self) -> Value {
        let mut report = self.prepared.prepared_report().clone();
        report["artifacts"] = json!([]);
        report
    }
    /// Return complete publication observations only after the publisher succeeds.
    pub fn save<P: ArtifactPort>(
        &self,
        publisher: &mut P,
    ) -> Result<&Value, yamaa_engine::specification_output::SaveError<P::Error>> {
        self.prepared.save(publisher)
    }
}

/// Prepare a public-result candidate without recapturing data or publishing bytes.
pub fn build_result<E>(
    run: &PreparedRun,
    attempt: &CapturedAttempt<E>,
    id: Identity<'_>,
) -> Result<BuildResult, Error> {
    use yamaa_engine::specification_output::{self as output, CompleteError as E};
    let execution = attempt
        .result
        .as_ref()
        .ok()
        .and_then(|r| r.execution.as_ref());
    let prepared = output::prepare(
        run.compiled(),
        execution,
        8 * 1024 * 1024,
        &mut Report {
            run,
            attempt,
            id,
            value: None,
        },
        &mut Encoder(run.compiled().output_profile()),
    )
    .map_err(|error| match error {
        E::Report(e) | E::Encode(e) => e,
        E::Projection => Error::InvalidObservation,
        E::OutputLimit => Error::OutputLimit,
        E::Publish(never) => match never {},
    })?;
    let table = prepared
        .artifact()
        .map(|artifact| {
            crate::table_transport::encode_projected_dataset(
                &execution.ok_or(Error::InvalidObservation)?.dataset,
                artifact.projection(),
            )
            .map_err(|e| match e {
                crate::table_transport::TableTransportError::OutputLimit => Error::OutputLimit,
                _ => Error::InvalidObservation,
            })
        })
        .transpose()?;
    Ok(BuildResult { prepared, table })
}
impl<E> From<Error> for CompleteError<E> {
    fn from(error: Error) -> Self {
        Self::Report(error)
    }
}
fn count(record: &Value, name: &str) -> Result<usize, Error> {
    record[name]
        .as_str()
        .and_then(|s| s.parse::<usize>().ok())
        .ok_or(Error::InvalidObservation)
}
fn check_observations(
    run: &PreparedRun,
    outcome: &Value,
    id: &Identity<'_>,
) -> Result<(Vec<Value>, Vec<Value>), Error> {
    let records = outcome["verifications"]
        .as_array()
        .ok_or(Error::InvalidObservation)?;
    let mut observations = Vec::new();
    let mut diagnostics = Vec::new();
    let output_phase = outcome["phase"] == "output";
    for record in records {
        let condition = record["condition"]
            .as_str()
            .ok_or(Error::InvalidObservation)?;
        let path = record["spec_path"]
            .as_str()
            .ok_or(Error::InvalidObservation)?;
        let failed = count(record, "failed_count")?;
        let mut detail = Value::Null;
        if failed > 0 {
            let mut keys = Vec::new();
            for identity in record["offending_rows"]
                .as_array()
                .ok_or(Error::InvalidObservation)?
            {
                let values = identity["keys"]
                    .as_array()
                    .ok_or(Error::InvalidObservation)?;
                let names = run.key_names().collect::<Vec<_>>();
                if names.len() != values.len() {
                    return Err(Error::InvalidObservation);
                }
                keys.push(Value::Object(
                    names
                        .into_iter()
                        .zip(values)
                        .map(|(name, value)| Ok((name.into(), plain(value)?)))
                        .collect::<Result<Map<_, _>, Error>>()?,
                ));
            }
            if condition == "row_count_failed" {
                keys = vec![json!({})];
            }
            let mut context = match condition {
                "row_count_failed" => {
                    json!({"count":count(record,"output_rows")?,"failure_count":failed})
                }
                "unique_failed" => {
                    let check = run
                        .compiled()
                        .verifications()
                        .iter()
                        .find(|check| check.path == path)
                        .ok_or(Error::InvalidObservation)?;
                    let yamaa_engine::dataset::Check::Unique(columns) = &check.check else {
                        return Err(Error::InvalidObservation);
                    };
                    json!({"columns":columns.iter().map(|&c|&run.compiled().output().columns()[c].name).collect::<Vec<_>>(),"failure_count":failed})
                }
                "missing_key" => {
                    let position = path
                        .strip_prefix("keys[")
                        .and_then(|s| s.strip_suffix(']'))
                        .and_then(|s| s.parse::<usize>().ok())
                        .ok_or(Error::InvalidObservation)?;
                    json!({"column":run.key_names().nth(position).ok_or(Error::InvalidObservation)?,"missing_count":failed})
                }
                "duplicate_key" => json!({"duplicate_count":failed}),
                _ => return Err(Error::UnsupportedOutcome),
            };
            if !output_phase {
                if let Some(identity) = run.compiled().verification_identity(path) {
                    context["verification_id"] = json!(identity);
                }
            }
            let mut log_context = context.clone();
            log_context["keys"] = json!(keys);
            context["keys"] = json!(&keys[..keys.len().min(5)]);
            let diagnostic = json!({"phase":if output_phase {"output"} else {"verification"},"condition":condition,"spec_paths":[path],"requirement":record["requirement"],"context":context});
            diagnostics.push(diagnostic.clone());
            detail = diagnostic;
            detail["severity"] = json!("error");
            detail["offending_keys"] = json!(keys);
            detail["log_context"] = log_context;
        }
        if !output_phase {
            let check = condition
                .strip_suffix("_failed")
                .ok_or(Error::InvalidObservation)?;
            observations.push(json!({"specification":id.specification,"spec_path":path,"check":check,"target":null,"requirement":record["requirement"],"verification_id":run.compiled().verification_identity(path),"severity":"error","evaluated_count":count(record,"evaluated_count")?,"failure":detail}));
        }
    }
    Ok((observations, diagnostics))
}
fn output_diagnostics(
    findings: &[yamaa_core::specification::OutputFinding],
) -> Result<Vec<Value>, Error> {
    findings
        .iter()
        .map(|finding| {
            crate::specification_diagnostics::portable_diagnostic(finding.diagnostic())
                .ok_or(Error::InvalidObservation)
        })
        .collect()
}
/// Render all portable observations and enforce report/output budgets before
/// publication. A host publication failure is retained, never a success report.
pub fn complete<E, P: ArtifactPort>(
    run: &PreparedRun,
    attempt: &CapturedAttempt<E>,
    id: Identity<'_>,
    publisher: &mut P,
) -> Result<Value, CompleteError<P::Error>> {
    use yamaa_engine::specification_output::{self as output, CompleteError as E};
    let execution = attempt
        .result
        .as_ref()
        .ok()
        .and_then(|response| response.execution.as_ref());
    output::complete(
        run.compiled(),
        execution,
        8 * 1024 * 1024,
        &mut Report {
            run,
            attempt,
            id,
            value: None,
        },
        &mut Encoder(run.compiled().output_profile()),
        publisher,
    )
    .map_err(|error| match error {
        E::Report(error) | E::Encode(error) => CompleteError::Report(error),
        E::Projection => CompleteError::Report(Error::InvalidObservation),
        E::OutputLimit => CompleteError::Report(Error::OutputLimit),
        E::Publish(error) => CompleteError::Publish(error),
    })
}

struct Encoder(Option<&'static str>);
impl yamaa_engine::specification_output::ArtifactEncoder for Encoder {
    type Error = Error;
    fn encode(
        &mut self,
        dataset: &yamaa_engine::dataset::Dataset,
        projection: &[usize],
        byte_limit: usize,
    ) -> Result<Vec<u8>, Error> {
        match self.0 {
            Some("csv") => {
                crate::csv_artifact::render(dataset, projection, byte_limit).map_err(|error| {
                    match error {
                        crate::csv_artifact::Error::Limit => Error::OutputLimit,
                        _ => Error::InvalidObservation,
                    }
                })
            }
            Some("parquet") => crate::parquet_artifact::render(
                dataset,
                projection,
                crate::parquet_artifact::Limits {
                    output_bytes: byte_limit,
                    staged_bytes: 8 * 1024 * 1024,
                    cells: 1_048_576,
                    columns: 4096,
                },
            )
            .map_err(|error| match error {
                crate::parquet_artifact::Error::Limit => Error::OutputLimit,
                _ => Error::InvalidObservation,
            }),
            _ => Err(Error::InvalidObservation),
        }
    }
}

struct Report<'a, 'i, E> {
    run: &'a PreparedRun,
    attempt: &'a CapturedAttempt<E>,
    id: Identity<'i>,
    value: Option<Value>,
}
impl<E> yamaa_engine::specification_output::OutputReport for Report<'_, '_, E> {
    type Error = Error;
    type Report = Value;
    fn failure(&mut self) -> Result<Value, Error> {
        failure(self.run, self.attempt, self.id.borrow())
    }
    fn begin(&mut self, _execution: &yamaa_engine::dataset::Execution) -> Result<(), Error> {
        let response = self
            .attempt
            .result
            .as_ref()
            .map_err(|_| Error::InvalidObservation)?;
        let value: Value =
            serde_json::from_str(&response.outcome).map_err(|_| Error::InvalidObservation)?;
        let mut report = envelope(self.run, self.attempt, &self.id)?;
        let (checks, unexpected) = check_observations(self.run, &value["outcome"], &self.id)?;
        if !unexpected.is_empty() {
            return Err(Error::InvalidObservation);
        }
        set_handler_counts(&mut report, &value)?;
        report["verifications"] = json!(checks);
        self.value = Some(report);
        Ok(())
    }
    fn rejected(
        &mut self,
        findings: &[yamaa_core::specification::OutputFinding],
    ) -> Result<Value, Error> {
        let mut report = self.value.take().ok_or(Error::InvalidObservation)?;
        set_diagnostics(&mut report, output_diagnostics(findings)?);
        bounded(report)
    }
    fn success(
        &mut self,
        execution: &yamaa_engine::dataset::Execution,
        projection: &[usize],
        bytes: &[u8],
    ) -> Result<Value, Error> {
        let run = self.run;
        let table = &execution.dataset;
        let profile = run
            .compiled()
            .output_profile()
            .ok_or(Error::InvalidObservation)?;
        let (content, records) = if profile == "csv" {
            let content = std::str::from_utf8(bytes).map_err(|_| Error::InvalidObservation)?;
            let records = content
                .strip_suffix('\n')
                .ok_or(Error::InvalidObservation)?
                .split('\n')
                .map(String::from)
                .collect::<Vec<_>>();
            (content, records)
        } else {
            let records = crate::parquet_artifact::records(table, projection, 8 * 1024 * 1024)
                .map_err(|error| match error {
                    crate::parquet_artifact::Error::Limit => Error::OutputLimit,
                    _ => Error::InvalidObservation,
                })?;
            ("", records)
        };
        let mut report = self.value.take().ok_or(Error::InvalidObservation)?;
        report["artifacts"] = json!([{"name":run.compiled().output_name(),"profile":profile,"columns":run.compiled().projection(),"types":projection.iter().map(|&c|specification_diagnostics::type_name(table.schema().columns()[c].kind)).collect::<Vec<_>>(),"row_count":table.row_count(),"records":records,"byte_length":bytes.len(),"content":content}]);
        report["tables"]
            .as_array_mut()
            .ok_or(Error::InvalidObservation)?
            .push(table_observation(table, &self.id, "derived", "output")?);
        report["outcome"] = json!("success");
        report["nodes"][0]["outcome"] = json!("success");
        bounded(report)
    }
}

impl Identity<'_> {
    fn borrow(&self) -> Identity<'_> {
        Identity {
            runtime: self.runtime,
            runtime_version: self.runtime_version,
            engine_version: self.engine_version,
            example: self.example,
            specification: self.specification,
            base_directory: self.base_directory,
        }
    }
}
