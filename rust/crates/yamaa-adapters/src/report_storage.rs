//! Report metadata owns immutable encoded table rows. Sharing a report never
//! copies its cell objects or re-enters a source/host port.
use serde::{
    ser::{SerializeMap, SerializeSeq},
    Serialize, Serializer,
};
use serde_json::value::RawValue;
use std::{
    fmt,
    ops::{Deref, DerefMut},
    sync::Arc,
};
use yamaa_core::diagnostic::Diagnostic;
use yamaa_core::json::{Map, Value};
use yamaa_core::table::{TableAccess, ValueRef};

/// Why a report could not be built from what an attempt observed.
#[derive(Debug)]
pub enum Error {
    UnsupportedOutcome,
    CapabilityUnavailable { feature: &'static str },
    InvalidObservation,
    ResourceLimit(crate::resource_diagnostics::Refusal),
}
impl From<crate::source_diagnostics::SourceProjectionError> for Error {
    fn from(error: crate::source_diagnostics::SourceProjectionError) -> Self {
        match error {
            crate::source_diagnostics::SourceProjectionError::InvalidContext => {
                Self::InvalidObservation
            }
        }
    }
}

struct WireCell<'a>(ValueRef<'a>);
impl Serialize for WireCell<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        struct Text<T: fmt::Display>(T);
        impl<T: fmt::Display> Serialize for Text<T> {
            fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                serializer.collect_str(&self.0)
            }
        }
        struct Bits(u64);
        impl fmt::Display for Bits {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{:016x}", self.0)
            }
        }
        let mut map = serializer.serialize_map(Some(2))?;
        match &self.0 {
            ValueRef::Missing => {
                map.serialize_entry("type", "missing")?;
                map.serialize_entry("value", &Option::<bool>::None)?;
            }
            ValueRef::Str(value) => {
                map.serialize_entry("type", "str")?;
                map.serialize_entry("value", value)?;
            }
            ValueRef::Int(value) => {
                map.serialize_entry("type", "int")?;
                map.serialize_entry("value", &Text(value))?;
            }
            ValueRef::Float(value) => {
                map.serialize_entry("type", "float")?;
                map.serialize_entry("value", &Text(Bits(value.get().to_bits())))?;
            }
            ValueRef::Bool(value) => {
                map.serialize_entry("type", "bool")?;
                map.serialize_entry("value", value)?;
            }
            ValueRef::Date(value) => {
                map.serialize_entry("type", "date")?;
                map.serialize_entry("value", &Text(value))?;
            }
            ValueRef::DateTime(value) => {
                map.serialize_entry("type", "datetime")?;
                map.serialize_entry("value", &Text(value))?;
            }
        }
        map.end()
    }
}
/// Every record of `table`, in order, as report rows. Its records divide into
/// `partitions` of these sizes, each admitted within the fixed limits on its
/// own: the whole table for a table held whole, and the partitions of an
/// input read by partition (#3792).
pub fn encode_partitions<T: TableAccess>(
    table: &T,
    partitions: &[usize],
) -> Result<Arc<RawValue>, Error> {
    for &rows in partitions {
        admit(rows, table.schema().columns().len())?;
    }
    let mut writer = RowWriter::default();
    writer.table(table)?;
    writer.finish()
}

/// Admit each of `partitions`, records of `columns` fields, within the
/// fixed limits on its own.
pub fn admit_partitions(columns: usize, partitions: &[usize]) -> Result<(), Error> {
    partitions.iter().try_for_each(|&rows| admit(rows, columns))
}

/// Admit `rows` records of `columns` fields within the fixed limits.
fn admit(rows: usize, columns: usize) -> Result<(), Error> {
    yamaa_core::limits::Limits::table(rows, columns).map_err(|error| {
        Error::ResourceLimit(crate::resource_diagnostics::Refusal::new(
            error.resource,
            error.limit,
            error.actual,
            Vec::new(),
        ))
    })
}

/// Report rows written a table after another, in order: the rows of an
/// input read by partition are written a partition at a time (#3830).
#[derive(Default)]
pub struct RowWriter {
    bytes: Vec<u8>,
    rows: usize,
}

impl RowWriter {
    /// Write every record of `table`, after the records written before.
    pub fn table<T: TableAccess>(&mut self, table: &T) -> Result<(), Error> {
        use std::io::Write;
        let bytes = &mut self.bytes;
        for row in 0..table.row_count() {
            bytes
                .write_all(if self.rows == 0 { b"[[" } else { b",[" })
                .map_err(|_| Error::InvalidObservation)?;
            self.rows += 1;
            for column in 0..table.schema().columns().len() {
                if column != 0 {
                    bytes
                        .write_all(b",")
                        .map_err(|_| Error::InvalidObservation)?;
                }
                let value = table
                    .cell(row, column)
                    .map_err(|_| Error::InvalidObservation)?;
                serde_json::to_writer(&mut *bytes, &WireCell(value))
                    .map_err(|_| Error::InvalidObservation)?;
            }
            bytes
                .write_all(b"]")
                .map_err(|_| Error::InvalidObservation)?;
        }
        Ok(())
    }

    /// The rows written, as one array.
    pub fn finish(mut self) -> Result<Arc<RawValue>, Error> {
        self.bytes
            .extend_from_slice(if self.rows == 0 { b"[]" } else { b"]" });
        let string = String::from_utf8(self.bytes).map_err(|_| Error::InvalidObservation)?;
        let rows = RawValue::from_string(string).map_err(|_| Error::InvalidObservation)?;
        Ok(Arc::from(rows))
    }
}

/// The rows of one table of a report: encoded once, or, for an input read
/// by partition, encoded from its held bytes whenever the report is
/// serialized, so the run keeps no copy of its records (#3830).
#[derive(Clone)]
pub enum Rows {
    Encoded(Arc<RawValue>),
    Deferred(Arc<dyn Fn() -> Result<Arc<RawValue>, Error> + Send + Sync>),
}

impl Rows {
    /// The rows, encoded.
    fn encoded(&self) -> Result<Arc<RawValue>, Error> {
        match self {
            Self::Encoded(rows) => Ok(Arc::clone(rows)),
            Self::Deferred(encode) => encode(),
        }
    }
}

impl fmt::Debug for Rows {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Encoded(rows) => formatter.debug_tuple("Encoded").field(rows).finish(),
            Self::Deferred(_) => formatter.write_str("Deferred(..)"),
        }
    }
}

pub struct StoredTable {
    pub metadata: Value,
    pub rows: Rows,
}

/// The `records` and the `content` of the artifact named `name`, encoded
/// from the run's output whenever the report is serialized, so the report
/// holds no copy of them (#3870). The artifact entry holds `null` for each.
#[derive(Clone)]
pub struct ArtifactContent {
    pub name: String,
    pub encode: Arc<EncodeContent>,
}

/// Encode an artifact's `records`, then its `content`.
pub type EncodeContent = dyn Fn() -> Result<(Arc<RawValue>, Arc<RawValue>), Error> + Send + Sync;

impl fmt::Debug for ArtifactContent {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ArtifactContent")
            .field("name", &self.name)
            .finish_non_exhaustive()
    }
}

/// The report's form of one diagnostic: phase, condition, rule,
/// paths and context, with `source_span` and `operand_route` only when
/// recorded, so an absent span stays distinct from a zero-width one.
pub fn diagnostic_value(diagnostic: &Diagnostic) -> Value {
    let definition = diagnostic.definition();
    let mut object = Map::new();
    object.insert("phase".into(), Value::String(definition.phase.into()));
    object.insert(
        "condition".into(),
        Value::String(definition.condition.into()),
    );
    object.insert(
        "rule".into(),
        definition
            .rule
            .map_or(Value::Null, |rule| Value::String(rule.into())),
    );
    object.insert(
        "spec_paths".into(),
        Value::Array(
            diagnostic
                .spec_paths
                .iter()
                .map(|path| Value::String(path.clone()))
                .collect(),
        ),
    );
    object.insert("context".into(), Value::Object(diagnostic.context.clone()));
    if let Some(span) = diagnostic.source_span {
        object.insert(
            "source_span".into(),
            Value::Object(
                [
                    ("start".into(), Value::from(span.start)),
                    ("end".into(), Value::from(span.end)),
                ]
                .into(),
            ),
        );
    }
    if let Some(route) = &diagnostic.operand_route {
        object.insert(
            "operand_route".into(),
            Value::Array(
                route
                    .iter()
                    .map(|step| Value::String(step.clone()))
                    .collect(),
            ),
        );
    }
    Value::Object(object)
}

/// The row blocks are private, validated JSON arrays. Metadata retains the
/// existing reserved rows field; serialization supplies the immutable block.
/// The report holds its diagnostics as typed records and renders them into
/// the metadata it serializes.
#[derive(Clone, Debug)]
pub struct StoredReport {
    metadata: Value,
    rows: Vec<Rows>,
    diagnostics: Vec<Diagnostic>,
    contents: Vec<ArtifactContent>,
}
impl PartialEq for StoredReport {
    fn eq(&self, other: &Self) -> bool {
        self.metadata == other.metadata
            && self.diagnostics == other.diagnostics
            && self.contents.len() == other.contents.len()
            && self
                .contents
                .iter()
                .zip(&other.contents)
                .all(|(left, right)| {
                    left.name == right.name
                        && match ((left.encode)(), (right.encode)()) {
                            (Ok(left), Ok(right)) => {
                                left.0.get() == right.0.get() && left.1.get() == right.1.get()
                            }
                            _ => false,
                        }
                })
            && self.rows.len() == other.rows.len()
            && self.rows.iter().zip(&other.rows).all(|(left, right)| {
                match (left.encoded(), right.encoded()) {
                    (Ok(left), Ok(right)) => left.get() == right.get(),
                    _ => false,
                }
            })
    }
}
impl StoredReport {
    /// Attach tables to metadata whose first node can record diagnostics.
    /// Reject metadata without an object containing a nonempty `nodes` array
    /// whose first entry is an object.
    pub fn with_tables(mut metadata: Value, tables: Vec<StoredTable>) -> Result<Self, Error> {
        metadata
            .get("nodes")
            .and_then(Value::as_array)
            .and_then(|nodes| nodes.first())
            .and_then(Value::as_object)
            .ok_or(Error::InvalidObservation)?;
        let mut rows = Vec::with_capacity(tables.len());
        metadata["tables"] = Value::Array(
            tables
                .into_iter()
                .map(|table| {
                    rows.push(table.rows);
                    table.metadata
                })
                .collect(),
        );
        Ok(Self {
            metadata,
            rows,
            diagnostics: Vec::new(),
            contents: Vec::new(),
        })
    }
    /// Encode the `records` and the `content` of the artifact `content` names
    /// only when the report is serialized (#3870).
    pub fn defer_content(&mut self, content: ArtifactContent) {
        self.contents.retain(|held| held.name != content.name);
        self.contents.push(content);
    }
    /// The diagnostics the report records, in authored order.
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }
    /// Record `diagnostics` and render them into the report's diagnostics
    /// and its node's.
    pub fn set_diagnostics(&mut self, diagnostics: Vec<Diagnostic>) {
        let rendered = Value::Array(diagnostics.iter().map(diagnostic_value).collect());
        self.metadata["diagnostics"] = rendered;
        self.metadata["nodes"][0]["diagnostics"] = self.metadata["diagnostics"].clone();
        self.diagnostics = diagnostics;
    }
    pub fn push_table(&mut self, table: StoredTable) -> Result<(), Error> {
        self.metadata["tables"]
            .as_array_mut()
            .ok_or(Error::InvalidObservation)?
            .push(table.metadata);
        self.rows.push(table.rows);
        Ok(())
    }
    pub fn without_artifacts_cloned(&self) -> Result<Self, Error> {
        let mut metadata = Value::Object(
            self.metadata
                .as_object()
                .ok_or(Error::InvalidObservation)?
                .iter()
                .filter(|(name, _)| name.as_str() != "artifacts")
                .map(|(name, value)| (name.clone(), value.clone()))
                .collect(),
        );
        metadata["artifacts"] = Value::Array(Vec::new());
        Ok(Self {
            metadata,
            rows: self.rows.clone(),
            diagnostics: self.diagnostics.clone(),
            contents: self.contents.clone(),
        })
    }
    pub fn observations_json(&self, build: bool) -> String {
        serde_json::to_string(&ReportView {
            report: self,
            build,
        })
        .expect("admitted immutable report serialization")
    }
    pub fn observations_value(&self, build: bool) -> Value {
        crate::json_wire::decode(&self.observations_json(build))
            .expect("admitted immutable report JSON")
    }
}
impl Deref for StoredReport {
    type Target = Value;
    fn deref(&self) -> &Value {
        &self.metadata
    }
}
impl DerefMut for StoredReport {
    fn deref_mut(&mut self) -> &mut Value {
        &mut self.metadata
    }
}
impl fmt::Display for StoredReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.observations_json(false))
    }
}
impl Serialize for StoredReport {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        ReportView {
            report: self,
            build: false,
        }
        .serialize(serializer)
    }
}
struct ReportView<'a> {
    report: &'a StoredReport,
    build: bool,
}
impl Serialize for ReportView<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let values = self
            .report
            .metadata
            .as_object()
            .ok_or_else(|| serde::ser::Error::custom("report metadata object"))?;
        let tables = values
            .get("tables")
            .and_then(Value::as_array)
            .ok_or_else(|| serde::ser::Error::custom("report tables array"))?;
        if tables.len() != self.report.rows.len() {
            return Err(serde::ser::Error::custom("report row block geometry"));
        }
        let mut map = serializer.serialize_map(Some(values.len()))?;
        for (name, value) in values {
            if name == "tables" {
                map.serialize_entry(
                    name,
                    &TablesView {
                        tables,
                        rows: &self.report.rows,
                    },
                )?;
            } else if self.build && name == "artifacts" {
                map.serialize_entry(name, &[] as &[crate::json_wire::WireValue<'_>])?;
            } else if name == "artifacts" && !self.report.contents.is_empty() {
                map.serialize_entry(
                    name,
                    &ArtifactsView {
                        artifacts: value,
                        contents: &self.report.contents,
                    },
                )?;
            } else {
                map.serialize_entry(name, &crate::json_wire::WireValue(value))?;
            }
        }
        map.end()
    }
}
/// The artifacts, each deferred artifact's `records` and `content` encoded
/// as it is serialized (#3870).
struct ArtifactsView<'a> {
    artifacts: &'a Value,
    contents: &'a [ArtifactContent],
}
impl Serialize for ArtifactsView<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let artifacts = self
            .artifacts
            .as_array()
            .ok_or_else(|| serde::ser::Error::custom("report artifacts array"))?;
        let mut sequence = serializer.serialize_seq(Some(artifacts.len()))?;
        for artifact in artifacts {
            let deferred = artifact
                .get("records")
                .is_some_and(Value::is_null)
                .then(|| {
                    self.contents.iter().find(|content| {
                        artifact.get("name").and_then(Value::as_str) == Some(&content.name)
                    })
                })
                .flatten();
            let Some(content) = deferred else {
                sequence.serialize_element(&crate::json_wire::WireValue(artifact))?;
                continue;
            };
            let (records, text) = (content.encode)()
                .map_err(|_| serde::ser::Error::custom("report artifact content"))?;
            let values = artifact
                .as_object()
                .ok_or_else(|| serde::ser::Error::custom("report artifact object"))?;
            sequence.serialize_element(&ArtifactView {
                values,
                records: &records,
                content: &text,
            })?;
        }
        sequence.end()
    }
}
struct ArtifactView<'a> {
    values: &'a Map<String, Value>,
    records: &'a RawValue,
    content: &'a RawValue,
}
impl Serialize for ArtifactView<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(Some(self.values.len()))?;
        for (name, value) in self.values {
            match name.as_str() {
                "records" => map.serialize_entry(name, self.records)?,
                "content" => map.serialize_entry(name, self.content)?,
                _ => map.serialize_entry(name, &crate::json_wire::WireValue(value))?,
            }
        }
        map.end()
    }
}
struct TablesView<'a> {
    tables: &'a [Value],
    rows: &'a [Rows],
}
impl Serialize for TablesView<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut sequence = serializer.serialize_seq(Some(self.tables.len()))?;
        for (metadata, rows) in self.tables.iter().zip(self.rows) {
            let rows = rows
                .encoded()
                .map_err(|_| serde::ser::Error::custom("report rows"))?;
            sequence.serialize_element(&TableView {
                metadata,
                rows: &rows,
            })?;
        }
        sequence.end()
    }
}
struct TableView<'a> {
    metadata: &'a Value,
    rows: &'a RawValue,
}
impl Serialize for TableView<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let values = self
            .metadata
            .as_object()
            .ok_or_else(|| serde::ser::Error::custom("table metadata object"))?;
        if !values.get("rows").is_some_and(Value::is_null) {
            return Err(serde::ser::Error::custom("reserved immutable rows field"));
        }
        let mut map = serializer.serialize_map(Some(values.len()))?;
        for (name, value) in values {
            if name == "rows" {
                map.serialize_entry(name, self.rows)?;
            } else {
                map.serialize_entry(name, &crate::json_wire::WireValue(value))?;
            }
        }
        map.end()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use yamaa_core::diagnostic::ConditionCode;
    use yamaa_core::expression::SourceSpan;

    fn record() -> Diagnostic {
        Diagnostic {
            code: ConditionCode::SourceUnknownReference,
            spec_paths: vec!["input.SRC".into()],
            context: Map::new(),
            source_span: None,
            operand_route: None,
        }
    }

    /// Frozen pre-migration wire expectations, independent of registry construction.
    fn truth(name: &str) -> Value {
        let truth = crate::json_wire::decode(include_str!(
            "../tests/fixtures/diagnostic_literal_truth.json"
        ))
        .unwrap();
        truth.get(name).expect("known diagnostic fixture").clone()
    }

    #[test]
    fn a_rendered_record_keeps_the_frozen_portable_shape() {
        assert_eq!(diagnostic_value(&record()), truth("unknown-input-field"));
    }

    #[test]
    fn rendered_geometry_is_present_only_when_recorded() {
        let absent = diagnostic_value(&record());
        assert!(absent.get("source_span").is_none());
        assert!(absent.get("operand_route").is_none());
        let zero = diagnostic_value(&Diagnostic {
            source_span: Some(SourceSpan { start: 0, end: 0 }),
            operand_route: Some(Vec::new()),
            ..record()
        });
        assert_eq!(zero["source_span"], crate::json!({"start": 0, "end": 0}));
        assert_eq!(zero["operand_route"], crate::json!([]));
        let located = diagnostic_value(&Diagnostic {
            source_span: Some(SourceSpan { start: 3, end: 9 }),
            operand_route: Some(vec!["operand".into(), "root".into()]),
            ..record()
        });
        assert_eq!(located["source_span"], crate::json!({"start": 3, "end": 9}));
        assert_eq!(located["operand_route"], crate::json!(["operand", "root"]));
    }

    #[test]
    fn a_rendered_context_keeps_wide_numbers_unicode_and_nesting() {
        let text = r#"{"maximum":9223372036854775807,"minimum":-9223372036854775808,"nested":{"empty":{},"list":[null,true,"é🙂",{"deep":[1.0]}]},"wide":[18446744073709551616,-18446744073709551617,92233720368547758081234567890]}"#;
        let Value::Object(context) = crate::json_wire::decode(text).unwrap() else {
            panic!("object fixture")
        };
        let rendered = diagnostic_value(&Diagnostic {
            context,
            ..record()
        });
        assert_eq!(crate::json_wire::encode(&rendered["context"]), text);
    }

    #[test]
    fn encoded_cells_preserve_all_scalar_tags_exact_bits_and_civil_values() {
        use yamaa_core::value::FiniteFloat;
        let cases = [
            (ValueRef::Missing, r#"{"type":"missing","value":null}"#),
            (ValueRef::Str("é\n\""), r#"{"type":"str","value":"é\n\""}"#),
            (
                ValueRef::Int(i64::MIN),
                r#"{"type":"int","value":"-9223372036854775808"}"#,
            ),
            (
                ValueRef::Int(i64::MAX),
                r#"{"type":"int","value":"9223372036854775807"}"#,
            ),
            (
                ValueRef::Float(FiniteFloat::new(-0.0).unwrap()),
                r#"{"type":"float","value":"8000000000000000"}"#,
            ),
            (ValueRef::Bool(false), r#"{"type":"bool","value":false}"#),
            (
                ValueRef::Date("0001-01-01".parse().unwrap()),
                r#"{"type":"date","value":"0001-01-01"}"#,
            ),
            (
                ValueRef::DateTime("9999-12-31T23:59:59".parse().unwrap()),
                r#"{"type":"datetime","value":"9999-12-31T23:59:59"}"#,
            ),
        ];
        for (value, expected) in cases {
            assert_eq!(serde_json::to_string(&WireCell(value)).unwrap(), expected);
        }
    }
    #[test]
    fn row_encoding_stops_at_the_original_cell_failure_prefix() {
        use std::cell::RefCell;
        use yamaa_core::{
            table::{CellError, Column, TableSchema},
            value::ColumnType,
        };
        struct Cells {
            schema: TableSchema,
            visited: RefCell<Vec<(usize, usize)>>,
        }
        impl TableAccess for Cells {
            type Error = &'static str;
            fn schema(&self) -> &TableSchema {
                &self.schema
            }
            fn row_count(&self) -> usize {
                2
            }
            fn cell(
                &self,
                row: usize,
                column: usize,
            ) -> Result<ValueRef<'_>, CellError<Self::Error>> {
                self.visited.borrow_mut().push((row, column));
                if (row, column) == (1, 0) {
                    Err(CellError::Access("original"))
                } else {
                    Ok(ValueRef::Int(17))
                }
            }
        }
        let table = Cells {
            schema: TableSchema::new(
                ["LEFT", "RIGHT"]
                    .into_iter()
                    .map(|name| Column {
                        name: name.into(),
                        kind: ColumnType::Int,
                    })
                    .collect(),
            )
            .unwrap(),
            visited: Default::default(),
        };
        assert!(matches!(
            encode_partitions(&table, &[table.row_count()]),
            Err(Error::InvalidObservation)
        ));
        assert_eq!(*table.visited.borrow(), vec![(0, 0), (0, 1), (1, 0)]);
    }
    #[test]
    fn shared_rows_keep_exact_wire_bits_and_outlive_the_original_report() {
        let raw =
            r#"[[{"type":"float","value":"8000000000000000"},{"type":"missing","value":null}]]"#;
        let rows: Arc<RawValue> = Arc::from(RawValue::from_string(raw.to_owned()).unwrap());
        let first = StoredReport::with_tables(
            crate::json!({"nodes":[{}],"tables":[],"artifacts":[{"path":"out.csv"}]}),
            vec![StoredTable {
                metadata: crate::json!({"name":"ROOT","rows":null}),
                rows: Rows::Encoded(Arc::clone(&rows)),
            }],
        )
        .unwrap();
        let clone = first.clone();
        match (&first.rows[0], &clone.rows[0]) {
            (Rows::Encoded(first_rows), Rows::Encoded(clone_rows)) => {
                assert!(Arc::ptr_eq(first_rows, clone_rows))
            }
            _ => panic!("encoded rows"),
        }
        drop(first);
        let value = clone.observations_value(true);
        assert_eq!(
            value["tables"][0]["rows"],
            crate::json_wire::decode(raw).unwrap()
        );
        assert_eq!(value["artifacts"], crate::json!([]));
        assert_eq!(
            clone.observations_value(false)["artifacts"][0]["path"],
            "out.csv"
        );
    }
}
