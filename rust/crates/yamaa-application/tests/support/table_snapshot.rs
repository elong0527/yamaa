//! Private fixture observation of a typed table the engine kept; no
//! installed probe.
use yamaa_adapters::json;
use yamaa_core::{
    json::Value,
    table::{TableAccess, ValueRef},
};

/// Each row of `table`, each cell a one-key object that names its type: an
/// integer's decimal text, a float's bits, and a date or datetime with its
/// collected precision.
pub fn rows<T: TableAccess>(table: &T) -> Value
where
    T::Error: std::fmt::Debug,
{
    let cell = |row, column| match table.cell(row, column).expect("validated coordinate") {
        ValueRef::Missing => json!({"missing":null}),
        ValueRef::Int(n) => json!({"int":n.to_string()}),
        ValueRef::Float(n) => json!({"float":format!("{:016x}",n.get().to_bits())}),
        ValueRef::Str(s) => json!({"str":s}),
        ValueRef::Bool(b) => json!({"bool":b}),
        ValueRef::Date(d) => {
            json!({"date":{"text":d.to_string(),"precision":d.collected_precision().word()}})
        }
        ValueRef::DateTime(d) => {
            json!({"datetime":{"text":d.to_string(),"precision":d.collected_precision().word()}})
        }
    };
    let width = table.schema().columns().len();
    Value::Array(
        (0..table.row_count())
            .map(|row| Value::Array((0..width).map(|column| cell(row, column)).collect()))
            .collect(),
    )
}
