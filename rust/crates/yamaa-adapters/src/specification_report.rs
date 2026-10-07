//! Format observed failed standalone runs without reading resources or executing
//! semantics. Successful artifact publication and capture-error mapping are pending.
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
    Ok(
        json!({"phase":diagnostic["phase"],"condition":diagnostic["condition"],"requirement":diagnostic["requirement"],"spec_paths":diagnostic["spec_paths"],"context":context}),
    )
}
/// The admitted subset contains no callbacks, handlers, verifications or parent
/// nodes. Refuse outcomes outside this formatter instead of inventing observations.
pub fn failure<E>(
    run: &PreparedRun,
    attempt: &CapturedAttempt<E>,
    id: Identity<'_>,
) -> Result<Value, Error> {
    if !attempt.read.captured {
        return Err(Error::UnsupportedOutcome);
    }
    let diagnostics = match &attempt.result {
        Ok(response) => {
            if response.table.is_some() {
                return Err(Error::UnsupportedOutcome);
            }
            let value: Value =
                serde_json::from_str(&response.outcome).map_err(|_| Error::InvalidObservation)?;
            if value["outcome"]["status"] != "condition" {
                return Err(Error::UnsupportedOutcome);
            }
            vec![condition(run, &value["outcome"])?]
        }
        Err(PortError::Run(error)) => {
            specification_diagnostics::findings(error, Some(run.source()))
                .ok_or(Error::UnsupportedOutcome)?
        }
        _ => return Err(Error::UnsupportedOutcome),
    };
    let mut tables = Vec::new();
    if let Some(table) = &attempt.table {
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
                        _ => return Err(Error::InvalidObservation),
                    },
                );
            }
            rows.push(cells);
        }
        tables.push(json!({"specification":id.specification,"stage":"source","name":run.source().name,
            "columns":columns.iter().map(|c| &c.name).collect::<Vec<_>>(),"types":vec!["str";columns.len()],"rows":rows}));
    }
    let report = json!({"report_version":"0.3.0-draft","runtime":id.runtime,"backend":"rust","runtime_version":id.runtime_version,"engine_version":id.engine_version,"example":id.example,
        "outcome":"failure","artifacts":[],"diagnostics":diagnostics,"unsupported":[],"handler_counts":[],
        "nodes":[{"specification":id.specification,"outcome":"failure","diagnostics":diagnostics,"unsupported":[],"handler_counts":[]}],
        "tables":tables,"verifications":[],"callbacks":[],
        "source_reads":[{"base_directory":id.base_directory,"path":attempt.read.source.path,"outcome":"captured","condition":null,"snapshots_created":attempt.read.snapshots_created.ok_or(Error::InvalidObservation)?}],"error":null});
    // Source rows/text already have fixed admission budgets; additionally refuse
    // reports exceeding the host response budget rather than returning truncation.
    if report.to_string().len() > 16_777_216 {
        return Err(Error::OutputLimit);
    }
    Ok(report)
}
