//! Private fixture observation of a typed table that the engine wrote as an
//! IPC stream; no installed probe. The independent Arrow reader decodes it.
use arrow_schema::DataType;
use serde::{
    ser::{SerializeSeq, SerializeStruct},
    Serialize, Serializer,
};
use yamaa_adapters::{arrow_table::ArrowTable, ipc_table};
use yamaa_core::{
    table::{Column, TableAccess, TableSchema, ValueRef},
    value::ColumnType,
};
/// The typed table of an IPC stream the engine wrote: each field's physical
/// type names its column type, as `arrow_table::physical_schema` writes it.
pub fn decode(input: &[u8]) -> ArrowTable {
    let reader = arrow_ipc::reader::StreamReader::try_new(input, None).unwrap();
    let columns = reader
        .schema()
        .fields()
        .iter()
        .map(|field| Column {
            name: field.name().clone(),
            kind: match field.data_type() {
                DataType::Utf8 => ColumnType::Str,
                DataType::Int64 => ColumnType::Int,
                DataType::Float64 => ColumnType::Float,
                DataType::Struct(children) if children[0].data_type() == &DataType::Date32 => {
                    ColumnType::Date
                }
                DataType::Struct(_) => ColumnType::DateTime,
                other => panic!("no column type is written as {other}"),
            },
        })
        .collect();
    let batches = reader.collect::<Result<Vec<_>, _>>().unwrap();
    ArrowTable::try_new(TableSchema::new(columns).unwrap(), batches).unwrap()
}
pub fn table_snapshot(input: &[u8]) -> String {
    ipc_table::serialize_json(&Snapshot(&decode(input))).unwrap()
}
fn scalar(value: yamaa_core::value::Value) -> yamaa_core::json::Value {
    use yamaa_adapters::json;
    use yamaa_core::value::Value;
    match value {
        Value::Missing => json!({"missing":null}),
        Value::Int(n) => json!({"int":n.to_string()}),
        Value::Float(n) => json!({"float":format!("{:016x}",n.get().to_bits())}),
        Value::Str(s) => json!({"str":s}),
        Value::Bool(b) => json!({"bool":b}),
        Value::Date(d) => {
            json!({"date":{"text":d.to_string(),"precision":d.collected_precision().word()}})
        }
        Value::DateTime(d) => {
            json!({"datetime":{"text":d.to_string(),"precision":d.collected_precision().word()}})
        }
    }
}
struct Snapshot<'a>(&'a ArrowTable);
impl Serialize for Snapshot<'_> {
    /// Stream rows without building an expanded JSON value tree.
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
    /// Reuse the established scalar codec one typed cell at a time.
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
            let scalar = scalar(value);
            row.serialize_element(&yamaa_adapters::json_wire::WireValue(&scalar))?;
        }
        row.end()
    }
}
