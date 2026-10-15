//! Dataset-JSON byte writer over a held published table; never executes a spec.
use crate::{
    diagnostic::ConditionCode,
    limits::{LimitExceeded, Limits},
    submission_composition::{Dataset, Finding, Study},
    table::{TableAccess, ValueRef},
    value::Value,
};
use serde_json::Value as JsonValue;
use std::{
    collections::BTreeMap,
    io::{self, Write},
};

fn string(value: &str) -> String {
    serde_json::to_string(value).expect("JSON strings serialize")
}
fn object(fields: Vec<(&str, String)>) -> String {
    format!(
        "{{{}}}",
        fields
            .into_iter()
            .map(|(key, value)| format!("{}: {value}", string(key)))
            .collect::<Vec<_>>()
            .join(", ")
    )
}
/// One saved file, checked before every physical buffer extension. JSON strings
/// stream through serde so escaping cannot allocate an oversized intermediate.
struct Writer<'a> {
    output: Vec<u8>,
    path: &'a str,
}
impl Write for Writer<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let required = self.output.len().checked_add(bytes.len());
        let actual = required.ok_or_else(|| {
            io::Error::other(LimitExceeded {
                resource: "output_bytes",
                limit: Limits::OUTPUT_BYTES,
                actual: None,
            })
        })?;
        Limits::output_bytes(actual).map_err(io::Error::other)?;
        self.output.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
impl Writer<'_> {
    fn failure(&self, cause: impl std::fmt::Display) -> Finding {
        Finding::cause(
            ConditionCode::ApplicationRejected,
            self.path,
            &cause.to_string(),
        )
    }
    fn text(&mut self, text: &str) -> Result<(), Finding> {
        self.write_all(text.as_bytes())
            .map_err(|error| self.failure(error))
    }
    fn string(&mut self, text: &str) -> Result<(), Finding> {
        serde_json::to_writer(&mut *self, text).map_err(|error| self.failure(error))
    }
    fn cell(&mut self, value: ValueRef<'_>) -> Result<(), Finding> {
        match value {
            ValueRef::Missing => self.text("null"),
            ValueRef::Str(value) => self.string(value),
            ValueRef::Int(value) => self.text(&value.to_string()),
            ValueRef::Float(value) => self.text(&crate::conversion::float_text(value)),
            ValueRef::Date(value) => self.string(&value.to_string()),
            ValueRef::DateTime(value) => self.string(&value.to_string()),
            ValueRef::Bool(_) => Err(Finding::cause(
                ConditionCode::SubmissionJsonInvalid,
                "dataset_json",
                "schema_violation: boolean cell",
            )),
        }
    }
}
pub fn render<T: TableAccess>(
    study: &Study,
    dataset: &Dataset,
    table: &T,
    define_name: &str,
) -> Result<Vec<u8>, Finding> {
    let path = format!("datasets.{}.dataset_json", dataset.id);
    let invalid = |detail| Finding::cause(ConditionCode::SubmissionJsonInvalid, &path, detail);
    Limits::table(table.row_count(), dataset.columns.len()).map_err(|error| {
        Finding::cause(
            ConditionCode::ApplicationRejected,
            &path,
            &error.to_string(),
        )
    })?;
    validate_artifact(study, dataset, table)?;
    let mut fields = std::vec![
        ("datasetJSONCreationDateTime", string(&study.created)),
        ("datasetJSONVersion", string("1.1.0")),
        (
            "fileOID",
            string(&format!("{}.{}", study.file_oid, dataset.id))
        )
    ];
    if let Some(originator) = &study.section.originator {
        fields.push(("originator", string(&originator.value)));
    }
    fields.extend([
        (
            "sourceSystem",
            object(std::vec![
                ("name", string("yamaa")),
                ("version", string(&study.source_version))
            ]),
        ),
        (
            "studyOID",
            string(&format!("STDY.{}", study.study.id.value)),
        ),
        (
            "metaDataVersionOID",
            string(&format!("MDV.{}", study.metadata_id)),
        ),
        ("metaDataRef", string(define_name)),
        ("itemGroupOID", string(&format!("IG.{}", dataset.id))),
        ("records", table.row_count().to_string()),
        ("name", string(&dataset.id)),
        ("label", string(&dataset.metadata.label.value)),
    ]);
    let mut columns = Vec::new();
    for column in &dataset.columns {
        let declaration = &column.declaration;
        let meta = declaration.submission.as_ref().expect("composed column");
        let mut values = std::vec![
            (
                "itemOID",
                string(&format!("IT.{}.{}", dataset.id, declaration.name.value))
            ),
            ("name", string(&declaration.name.value)),
            (
                "label",
                string(&declaration.label.as_ref().expect("composed label").value)
            ),
            (
                "dataType",
                string(match column.data_type {
                    crate::specification_submission_document::DataType::Text => "string",
                    crate::specification_submission_document::DataType::Integer => "integer",
                    crate::specification_submission_document::DataType::Float => "float",
                    crate::specification_submission_document::DataType::Date => "date",
                    crate::specification_submission_document::DataType::Datetime => "datetime",
                    crate::specification_submission_document::DataType::Time => "time",
                    crate::specification_submission_document::DataType::Uri => "URI",
                    _ => "string",
                })
            ),
        ];
        if let Some(length) = &column.length {
            values.push(("length", length.clone()));
        }
        if let Some(display) = &meta.display_format {
            values.push(("displayFormat", string(&display.value)));
        }
        if let Some(sequence) = column.key_sequence {
            values.push(("keySequence", sequence.to_string()));
        }
        columns.push(object(values));
    }
    fields.push(("columns", list(&columns)));
    let mut writer = Writer {
        output: Vec::new(),
        path: &path,
    };
    writer.text("{\n")?;
    for (key, value) in fields {
        writer.text("  ")?;
        writer.string(key)?;
        writer.text(": ")?;
        writer.text(&value)?;
        writer.text(",\n")?;
    }
    writer.text("  \"rows\": [")?;
    if table.row_count() > 0 {
        writer.text("\n")?;
        for row in 0..table.row_count() {
            if row > 0 {
                writer.text(",\n")?;
            }
            writer.text("    [")?;
            for column in 0..dataset.columns.len() {
                if column > 0 {
                    writer.text(", ")?;
                }
                writer.cell(
                    table
                        .cell(row, column)
                        .map_err(|_| invalid("held table access"))?,
                )?;
            }
            writer.text("]")?;
        }
        writer.text("\n  ")?;
    }
    writer.text("]\n}\n")?;
    Ok(writer.output)
}

fn artifact_mismatch(dataset: &Dataset, detail: &str) -> Finding {
    let mut finding = Finding::cause(
        ConditionCode::ProducerContractMismatch,
        &format!("datasets.{}.artifact", dataset.id),
        detail,
    );
    finding
        .context
        .insert("dataset".into(), JsonValue::String(dataset.id.clone()));
    finding.context.insert(
        "artifact".into(),
        JsonValue::String(dataset.artifact.clone()),
    );
    finding
}
/// Admit the original stored names before any declared-type conversion. Missing
/// or reordered fields are producer-contract failures, not scalar parse errors.
pub fn admit_fields<'a>(
    dataset: &Dataset,
    actual: impl ExactSizeIterator<Item = &'a str>,
) -> Result<(), Finding> {
    if actual.len() != dataset.columns.len()
        || actual
            .zip(&dataset.columns)
            .any(|(name, column)| name != column.declaration.name.value)
    {
        return Err(artifact_mismatch(
            dataset,
            "published artifact fields or order differ from the output contract",
        ));
    }
    Ok(())
}
fn validate_artifact<T: TableAccess>(
    study: &Study,
    dataset: &Dataset,
    table: &T,
) -> Result<(), Finding> {
    let mismatch = |detail| artifact_mismatch(dataset, detail);
    admit_fields(
        dataset,
        table
            .schema()
            .columns()
            .iter()
            .map(|field| field.name.as_str()),
    )?;
    if table
        .schema()
        .columns()
        .iter()
        .zip(&dataset.columns)
        .any(|(actual, expected)| actual.kind != expected.runtime_type())
    {
        return Err(mismatch(
            "published artifact fields, order or types differ from the output contract",
        ));
    }
    let lists: BTreeMap<_, _> = study
        .codelists
        .iter()
        .map(|list| (list.declaration.id.as_str(), &list.declaration))
        .collect();
    let variants: BTreeMap<_, _> = dataset
        .columns
        .iter()
        .enumerate()
        .flat_map(|(index, column)| {
            column
                .values
                .iter()
                .map(move |value| ((index, value.test_code.as_str()), value))
        })
        .collect();
    for (index, base) in dataset.columns.iter().enumerate() {
        for row in 0..table.row_count() {
            let variant = if base.values.is_empty() {
                None
            } else {
                let discriminator = table
                    .cell(
                        row,
                        dataset.discriminator.expect("admitted value discriminator"),
                    )
                    .map_err(|_| mismatch("held artifact discriminator access"))?;
                match discriminator {
                    ValueRef::Str(code) => variants.get(&(index, code)).copied(),
                    _ => None,
                }
            };
            let column = variant.map_or(base, |value| &value.column);
            let meta = column
                .declaration
                .submission
                .as_ref()
                .expect("composed metadata");
            let list = meta
                .codelist
                .as_ref()
                .and_then(|id| lists.get(id.value.as_str()).copied());
            let inventory = meta
                .inventory_vocabulary
                .as_ref()
                .is_some_and(|field| field.value);
            let value = table
                .cell(row, index)
                .map_err(|_| mismatch("held artifact access"))?;
            if value == ValueRef::Missing {
                continue;
            }
            let outside_list = list
                .filter(|list| !list.extensible)
                .and_then(|list| list.items.as_ref())
                .is_some_and(|items| !items.iter().any(|item| equal_value(value, &item.value)));
            let outside_inventory = inventory
                && !matches!(value, ValueRef::Str(id) if study.datasets.iter().any(|dataset| dataset.id == id));
            if outside_list || outside_inventory {
                let mut finding = Finding::cause(
                    ConditionCode::VerificationCodelistFailed,
                    &format!(
                        "{}.{}",
                        variant.map_or_else(
                            || format!(
                                "datasets.{}.columns.{}.submission",
                                dataset.id, column.declaration.name.value
                            ),
                            |value| value.path.clone()
                        ),
                        if outside_list {
                            "codelist"
                        } else {
                            "inventory_vocabulary"
                        }
                    ),
                    "published artifact vocabulary value",
                );
                if outside_inventory && !outside_list {
                    finding.code = ConditionCode::SubmissionInventoryFailed;
                }
                finding
                    .context
                    .insert("dataset".into(), JsonValue::String(dataset.id.clone()));
                finding.context.insert(
                    "column".into(),
                    JsonValue::String(column.declaration.name.value.clone()),
                );
                finding.context.insert(
                    "artifact".into(),
                    JsonValue::String(dataset.artifact.clone()),
                );
                finding.context.insert("row".into(), JsonValue::from(row));
                finding
                    .context
                    .insert("value".into(), JsonValue::from(owned(value)));
                if let Some(variant) = variant {
                    finding.context.insert(
                        "testcd".into(),
                        JsonValue::String(variant.test_code.clone()),
                    );
                }
                if let Some(list) = list {
                    finding
                        .context
                        .insert("codelist".into(), JsonValue::String(list.id.clone()));
                }
                let mut names = Vec::new();
                let mut values = Vec::new();
                for (key, column) in dataset.columns.iter().enumerate() {
                    if column.key_sequence.is_some() {
                        names.push(JsonValue::String(column.declaration.name.value.clone()));
                        values.push(JsonValue::from(owned(
                            table
                                .cell(row, key)
                                .map_err(|_| mismatch("held artifact key access"))?,
                        )));
                    }
                }
                finding
                    .context
                    .insert("key_columns".into(), JsonValue::Array(names));
                finding
                    .context
                    .insert("key_values".into(), JsonValue::Array(values));
                return Err(finding);
            }
        }
    }
    Ok(())
}
fn equal_value(actual: ValueRef<'_>, expected: &Value) -> bool {
    if let (ValueRef::Str(left), Value::Str(right)) = (actual, expected) {
        return left == right;
    }
    if matches!(actual, ValueRef::Str(_)) {
        return false;
    }
    crate::value::compare_present(&owned(actual), expected) == Ok(std::cmp::Ordering::Equal)
}
fn owned(value: ValueRef<'_>) -> Value {
    match value {
        ValueRef::Missing => Value::Missing,
        ValueRef::Str(value) => Value::Str(value.into()),
        ValueRef::Int(value) => Value::Int(value),
        ValueRef::Float(value) => Value::Float(value),
        ValueRef::Bool(value) => Value::Bool(value),
        ValueRef::Date(value) => Value::Date(value),
        ValueRef::DateTime(value) => Value::DateTime(value),
    }
}
fn list(values: &[String]) -> String {
    if values.is_empty() {
        return "[]".into();
    }
    format!(
        "[\n{}\n  ]",
        values
            .iter()
            .map(|s| format!("    {s}"))
            .collect::<Vec<_>>()
            .join(",\n")
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::value::FiniteFloat;

    #[test]
    fn streamed_cells_preserve_exact_json_and_full_width_scalar_spelling() {
        let values = [
            (Value::Missing, "null"),
            (
                Value::Str("\0\u{e9}\u{1f642}\"\\\n\t".into()),
                "\"\\u0000\u{e9}\u{1f642}\\\"\\\\\\n\\t\"",
            ),
            (Value::Int(i64::MIN), "-9223372036854775808"),
            (Value::Int(i64::MAX), "9223372036854775807"),
            (Value::float(-0.0), "-0"),
            (Value::Date("2024-02".parse().unwrap()), "\"2024-02-01\""),
            (
                Value::DateTime("2024-02-29T03:04".parse().unwrap()),
                "\"2024-02-29T03:04:00\"",
            ),
        ];
        for (value, expected) in values {
            let mut writer = Writer {
                output: Vec::new(),
                path: "dataset_json",
            };
            writer.cell(ValueRef::from(&value)).unwrap();
            assert_eq!(writer.output, expected.as_bytes());
        }
    }
    #[test]
    fn codelist_numeric_equality_preserves_integer_precision() {
        assert!(equal_value(
            ValueRef::Float(FiniteFloat::new(34.0).unwrap()),
            &Value::Int(34)
        ));
        assert!(!equal_value(
            ValueRef::Float(FiniteFloat::new(9_007_199_254_740_992.0).unwrap()),
            &Value::Int(9_007_199_254_740_993)
        ));
        assert!(equal_value(
            ValueRef::Int(9_007_199_254_740_992),
            &Value::Float(FiniteFloat::new(9_007_199_254_740_992.0).unwrap())
        ));
        assert!(!equal_value(ValueRef::Str("34"), &Value::Int(34)));
    }
}
