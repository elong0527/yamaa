//! Report metadata owns immutable encoded table rows. Sharing a report never
//! copies its cell objects or re-enters a source/host port.
use crate::specification_report::Error;
use serde::{
    ser::{SerializeMap, SerializeSeq},
    Serialize, Serializer,
};
use serde_json::{value::RawValue, Value};
use std::{
    fmt,
    ops::{Deref, DerefMut},
    sync::Arc,
};
use yamaa_core::table::{TableAccess, ValueRef};

struct RowWriter<'a> {
    bytes: Vec<u8>,
    budget: Option<&'a mut crate::report_projection::Budget>,
    limit: usize,
    refused: bool,
}
impl std::io::Write for RowWriter<'_> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        let admitted = (|| {
            let needed = self
                .bytes
                .len()
                .checked_add(bytes.len())
                .ok_or(Error::OutputLimit)?;
            if needed > self.limit {
                return Err(Error::OutputLimit);
            }
            if let Some(budget) = self.budget.as_deref_mut() {
                budget.work(bytes.len())?;
            }
            if needed > self.bytes.capacity() {
                let capacity = needed
                    .max(self.bytes.capacity().saturating_mul(2).max(256))
                    .min(self.limit);
                // Bound requested old/new buffer coexistence, shrinking to a
                // boxed str and copying into Arc<RawValue>. Admission precedes
                // every reserve, and the charge follows actual requested capacity.
                if let Some(budget) = self.budget.as_deref_mut() {
                    budget.retained_bytes(
                        capacity
                            .checked_sub(self.bytes.capacity())
                            .and_then(|n| n.checked_mul(3))
                            .ok_or(Error::OutputLimit)?,
                    )?;
                }
                self.bytes
                    .try_reserve_exact(capacity - self.bytes.len())
                    .map_err(|_| Error::OutputLimit)?;
            }
            self.bytes.extend_from_slice(bytes);
            Ok::<_, Error>(())
        })();
        match admitted {
            Ok(()) => Ok(bytes.len()),
            Err(_) => {
                self.refused = true;
                Err(std::io::Error::other("encoded row capacity"))
            }
        }
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
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
pub(crate) fn encode_rows<T: TableAccess>(
    table: &T,
    budget: Option<&mut crate::report_projection::Budget>,
) -> Result<Arc<RawValue>, Error> {
    use std::io::Write;
    let mut writer = RowWriter {
        bytes: Vec::new(),
        budget,
        limit: yamaa_engine::capacity::REPORT_ENCODED_BYTES,
        refused: false,
    };
    writer.write_all(b"[").map_err(|_| Error::OutputLimit)?;
    for row in 0..table.row_count() {
        if row != 0 {
            writer.write_all(b",").map_err(|_| Error::OutputLimit)?;
        }
        writer.write_all(b"[").map_err(|_| Error::OutputLimit)?;
        for column in 0..table.schema().columns().len() {
            if column != 0 {
                writer.write_all(b",").map_err(|_| Error::OutputLimit)?;
            }
            // Preserve the exact row/column read prefix and source failure.
            let value = table
                .cell(row, column)
                .map_err(|_| Error::InvalidObservation)?;
            if serde_json::to_writer(&mut writer, &WireCell(value)).is_err() {
                return Err(if writer.refused {
                    Error::OutputLimit
                } else {
                    Error::InvalidObservation
                });
            }
        }
        writer.write_all(b"]").map_err(|_| Error::OutputLimit)?;
    }
    writer.write_all(b"]").map_err(|_| Error::OutputLimit)?;
    if let Some(budget) = writer.budget.as_deref_mut() {
        budget.work(writer.bytes.len())?;
    }
    let string = String::from_utf8(writer.bytes).map_err(|_| Error::InvalidObservation)?;
    let rows = RawValue::from_string(string).map_err(|_| Error::InvalidObservation)?;
    Ok(Arc::from(rows))
}

pub(crate) struct StoredTable {
    pub(crate) metadata: Value,
    pub(crate) rows: Arc<RawValue>,
}

/// The row blocks are private, validated JSON arrays. Metadata retains the
/// existing reserved rows field; serialization supplies the immutable block.
#[derive(Clone, Debug)]
pub struct StoredReport {
    metadata: Value,
    rows: Vec<Arc<RawValue>>,
}
impl PartialEq for StoredReport {
    fn eq(&self, other: &Self) -> bool {
        self.metadata == other.metadata
            && self.rows.len() == other.rows.len()
            && self
                .rows
                .iter()
                .zip(&other.rows)
                .all(|(left, right)| left.get() == right.get())
    }
}
impl StoredReport {
    pub(crate) fn empty(metadata: Value) -> Self {
        Self {
            metadata,
            rows: Vec::new(),
        }
    }
    pub(crate) fn with_tables(mut metadata: Value, tables: Vec<StoredTable>) -> Self {
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
        Self { metadata, rows }
    }
    pub(crate) fn push_table(&mut self, table: StoredTable) -> Result<(), Error> {
        self.metadata["tables"]
            .as_array_mut()
            .ok_or(Error::InvalidObservation)?
            .push(table.metadata);
        self.rows.push(table.rows);
        Ok(())
    }
    pub(crate) fn append_tables(&mut self, other: &mut Self) -> Result<(), Error> {
        self.metadata["tables"]
            .as_array_mut()
            .ok_or(Error::InvalidObservation)?
            .append(
                other.metadata["tables"]
                    .as_array_mut()
                    .ok_or(Error::InvalidObservation)?,
            );
        self.rows.append(&mut other.rows);
        Ok(())
    }
    pub(crate) fn row_blocks(&self) -> usize {
        self.rows.len()
    }
    pub(crate) fn without_artifacts_cloned(&self) -> Result<Self, Error> {
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
        serde_json::from_str(&self.observations_json(build))
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
                map.serialize_entry(name, &[] as &[Value])?;
            } else {
                map.serialize_entry(name, value)?;
            }
        }
        map.end()
    }
}
struct TablesView<'a> {
    tables: &'a [Value],
    rows: &'a [Arc<RawValue>],
}
impl Serialize for TablesView<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut sequence = serializer.serialize_seq(Some(self.tables.len()))?;
        for (metadata, rows) in self.tables.iter().zip(self.rows) {
            sequence.serialize_element(&TableView { metadata, rows })?;
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
                map.serialize_entry(name, value)?;
            }
        }
        map.end()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
    fn row_buffer_equality_and_one_over_admit_before_any_owned_bytes() {
        use std::io::Write;
        for (allowance, accepted) in [(12, true), (11, false)] {
            let mut budget = crate::report_projection::Budget::new(allowance);
            let mut writer = RowWriter {
                bytes: Vec::new(),
                budget: Some(&mut budget),
                limit: 16,
                refused: false,
            };
            assert_eq!(writer.write_all(b"[").is_ok(), accepted);
            assert_eq!(writer.bytes.is_empty(), !accepted);
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
            encode_rows(&table, None),
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
            serde_json::json!({"tables":[],"artifacts":[{"path":"out.csv"}]}),
            vec![StoredTable {
                metadata: serde_json::json!({"name":"ROOT","rows":null}),
                rows: Arc::clone(&rows),
            }],
        );
        let clone = first.clone();
        assert!(Arc::ptr_eq(&first.rows[0], &clone.rows[0]));
        drop(first);
        let value = clone.observations_value(true);
        assert_eq!(
            value["tables"][0]["rows"],
            serde_json::from_str::<Value>(raw).unwrap()
        );
        assert_eq!(value["artifacts"], serde_json::json!([]));
        assert_eq!(
            clone.observations_value(false)["artifacts"][0]["path"],
            "out.csv"
        );
    }
}
