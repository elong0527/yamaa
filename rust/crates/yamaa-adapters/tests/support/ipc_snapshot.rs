//! Private fixture observation of a typed decoded IPC table; no installed probe.
use serde::{
    ser::{SerializeSeq, SerializeStruct},
    Serialize, Serializer,
};
use yamaa_adapters::{
    arrow_table::ArrowTable,
    ipc_table::{self, TableError},
};
use yamaa_core::{
    table::{TableAccess, ValueRef},
    value::ColumnType,
};
pub fn table_snapshot(input: &[u8]) -> Result<String, TableError> {
    let table = ipc_table::decode_snapshot(input)?;
    ipc_table::bounded_json(&Snapshot(&table), ipc_table::MAX_OUTPUT_BYTES)
}
fn scalar(value: yamaa_core::value::Value) -> serde_json::Value {
    use serde_json::json;
    use yamaa_core::{
        temporal::{DatePrecision, DateTimePrecision},
        value::Value,
    };
    match value {
        Value::Missing => json!({"missing":null}),
        Value::Int(n) => json!({"int":n.to_string()}),
        Value::Float(n) => json!({"float":format!("{:016x}",n.get().to_bits())}),
        Value::Str(s) => json!({"str":s}),
        Value::Bool(b) => json!({"bool":b}),
        Value::Date(d) => {
            json!({"date":{"text":d.to_string(),"precision":match d.collected_precision(){DatePrecision::Year=>"year",DatePrecision::Month=>"month",DatePrecision::Day=>"day"}}})
        }
        Value::DateTime(d) => {
            json!({"datetime":{"text":d.to_string(),"precision":match d.collected_precision(){DateTimePrecision::Day=>"day",DateTimePrecision::Second=>"second"}}})
        }
    }
}
struct Snapshot<'a>(&'a ArrowTable);
impl Serialize for Snapshot<'_> {
    /// Stream rows through the cap instead of building an expanded JSON value tree.
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let table = self.0;
        let mut output = serializer.serialize_struct("Snapshot", 5)?;
        output.serialize_field("protocol", "table/1")?;
        let columns: Vec<_> = table
            .schema()
            .columns()
            .iter()
            .map(|c| {
                (
                    &c.name,
                    match c.kind {
                        ColumnType::Str => "str",
                        ColumnType::Int => "int",
                        ColumnType::Float => "float",
                        ColumnType::Date => "date",
                        ColumnType::DateTime => "datetime",
                    },
                )
            })
            .collect();
        output.serialize_field("columns", &columns)?;
        output.serialize_field("row_count", &table.row_count().to_string())?;
        output.serialize_field(
            "chunks",
            &table
                .batches()
                .iter()
                .map(|b| b.num_rows().to_string())
                .collect::<Vec<_>>(),
        )?;
        output.serialize_field("rows", &Rows(table))?;
        output.end()
    }
}
struct Rows<'a>(&'a ArrowTable);
impl Serialize for Rows<'_> {
    /// Serialize each row independently without retaining previous cell strings.
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut rows = serializer.serialize_seq(Some(self.0.row_count()))?;
        for row in 0..self.0.row_count() {
            rows.serialize_element(&Row(self.0, row))?;
        }
        rows.end()
    }
}
struct Row<'a>(&'a ArrowTable, usize);
impl Serialize for Row<'_> {
    /// Reuse the established scalar codec one bounded cell at a time.
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use yamaa_core::value::Value;
        let mut row = serializer.serialize_seq(Some(self.0.schema().columns().len()))?;
        for column in 0..self.0.schema().columns().len() {
            let value = match self.0.cell(self.1, column).expect("validated coordinate") {
                ValueRef::Missing => Value::Missing,
                ValueRef::Str(v) => Value::Str(v.into()),
                ValueRef::Int(v) => Value::Int(v),
                ValueRef::Float(v) => Value::Float(v),
                ValueRef::Date(v) => Value::Date(v),
                ValueRef::DateTime(v) => Value::DateTime(v),
                ValueRef::Bool(v) => Value::Bool(v),
            };
            row.serialize_element(&scalar(value))?;
        }
        row.end()
    }
}
