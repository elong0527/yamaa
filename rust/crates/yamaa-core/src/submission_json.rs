//! Dataset-JSON byte writer over a held published table; never executes a spec.
use crate::{
    diagnostic::{ConditionCode, ContextValue},
    submission_composition::{Dataset, Finding, Study},
    table::{TableAccess, ValueRef},
    value::{ColumnType, Value},
};
use alloc::{
    collections::BTreeMap,
    format,
    string::{String, ToString},
    vec::Vec,
};

fn string(value: &str) -> String {
    let mut result = String::from("\"");
    for c in value.chars() {
        match c {
            '"' => result.push_str("\\\""),
            '\\' => result.push_str("\\\\"),
            '\n' => result.push_str("\\n"),
            '\r' => result.push_str("\\r"),
            '\t' => result.push_str("\\t"),
            '\u{8}' => result.push_str("\\b"),
            '\u{c}' => result.push_str("\\f"),
            c if c < '\u{20}' => result.push_str(&format!("\\u{:04x}", c as u32)),
            c => result.push(c),
        }
    }
    result.push('"');
    result
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
fn cell(value: ValueRef<'_>) -> Result<String, Finding> {
    Ok(match value {
        ValueRef::Missing => "null".into(),
        ValueRef::Str(s) => string(s),
        ValueRef::Int(i) => i.to_string(),
        ValueRef::Float(f) => crate::conversion::float_text(f),
        ValueRef::Date(d) => match crate::conversion::convert(&Value::Date(d), ColumnType::Str) {
            Ok(Value::Str(s)) => string(&s),
            _ => {
                return Err(Finding::cause(
                    ConditionCode::SubmissionJsonInvalid,
                    "dataset_json",
                    "date conversion",
                ))
            }
        },
        ValueRef::DateTime(d) => {
            match crate::conversion::convert(&Value::DateTime(d), ColumnType::Str) {
                Ok(Value::Str(s)) => string(&s),
                _ => {
                    return Err(Finding::cause(
                        ConditionCode::SubmissionJsonInvalid,
                        "dataset_json",
                        "datetime conversion",
                    ))
                }
            }
        }
        ValueRef::Bool(_) => {
            return Err(Finding::cause(
                ConditionCode::SubmissionJsonInvalid,
                "dataset_json",
                "schema_violation: boolean cell",
            ))
        }
    })
}
pub fn render<T: TableAccess>(
    study: &Study,
    dataset: &Dataset,
    table: &T,
    define_name: &str,
) -> Result<Vec<u8>, Finding> {
    let path = format!("datasets.{}.dataset_json", dataset.id);
    let invalid = |detail| Finding::cause(ConditionCode::SubmissionJsonInvalid, &path, detail);
    if table
        .row_count()
        .checked_mul(dataset.columns.len())
        .is_none_or(|n| n > 16_777_216)
    {
        return Err(invalid("table cell limit"));
    }
    validate_artifact(study, dataset, table)?;
    let mut fields = alloc::vec![
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
            object(alloc::vec![
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
        let mut values = alloc::vec![
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
    let mut rows = Vec::new();
    let mut bytes = columns.iter().map(String::len).sum::<usize>();
    for row in 0..table.row_count() {
        let mut cells = Vec::with_capacity(dataset.columns.len());
        for column in 0..dataset.columns.len() {
            cells.push(cell(
                table
                    .cell(row, column)
                    .map_err(|_| invalid("held table access"))?,
            )?);
        }
        let row = format!("[{}]", cells.join(", "));
        bytes = bytes
            .checked_add(row.len())
            .filter(|&n| n <= 67_108_864)
            .ok_or_else(|| invalid("render byte limit"))?;
        rows.push(row);
    }
    fields.push(("rows", list(&rows)));
    let mut lines = alloc::vec!["{".into()];
    for (index, (key, value)) in fields.iter().enumerate() {
        lines.push(format!(
            "  {}: {}{}",
            string(key),
            value,
            if index + 1 == fields.len() { "" } else { "," }
        ));
    }
    lines.push("}".into());
    let result = format!("{}\n", lines.join("\n"));
    if result.len() > 67_108_864 {
        return Err(invalid("render byte limit"));
    }
    Ok(result.into_bytes())
}
fn artifact_mismatch(dataset: &Dataset, detail: &str) -> Finding {
    let mut finding = Finding::cause(
        ConditionCode::ProducerContractMismatch,
        &format!("datasets.{}.artifact", dataset.id),
        detail,
    );
    finding.context.insert(
        "dataset".into(),
        ContextValue::Scalar(Value::Str(dataset.id.clone())),
    );
    finding.context.insert(
        "artifact".into(),
        ContextValue::Scalar(Value::Str(dataset.artifact.clone())),
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
    let mut work = 0usize;
    for column in dataset.columns.iter().flat_map(|column| {
        core::iter::once(column).chain(column.values.iter().map(|value| &value.column))
    }) {
        let meta = column
            .declaration
            .submission
            .as_ref()
            .expect("composed metadata");
        let list_work = meta
            .codelist
            .as_ref()
            .and_then(|id| {
                study
                    .codelists
                    .iter()
                    .find(|list| list.declaration.id == id.value)
            })
            .filter(|list| !list.declaration.extensible)
            .and_then(|list| list.declaration.items.as_ref())
            .map_or(1, Vec::len);
        let inventory_work = if meta
            .inventory_vocabulary
            .as_ref()
            .is_some_and(|field| field.value)
        {
            study.datasets.len()
        } else {
            0
        };
        work = list_work
            .checked_add(inventory_work)
            .and_then(|n| n.checked_add(column.values.len() + 1))
            .and_then(|n| n.checked_mul(table.row_count()))
            .and_then(|n| work.checked_add(n))
            .filter(|&n| n <= 268_435_456)
            .ok_or_else(|| {
                Finding::cause(
                    ConditionCode::ApplicationRejected,
                    &format!("datasets.{}.artifact", dataset.id),
                    "published artifact vocabulary work limit",
                )
            })?;
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
                finding.context.insert(
                    "dataset".into(),
                    ContextValue::Scalar(Value::Str(dataset.id.clone())),
                );
                finding.context.insert(
                    "column".into(),
                    ContextValue::Scalar(Value::Str(column.declaration.name.value.clone())),
                );
                finding.context.insert(
                    "artifact".into(),
                    ContextValue::Scalar(Value::Str(dataset.artifact.clone())),
                );
                finding
                    .context
                    .insert("row".into(), ContextValue::Integer(row.to_string()));
                finding
                    .context
                    .insert("value".into(), ContextValue::Scalar(owned(value)));
                if let Some(variant) = variant {
                    finding.context.insert(
                        "testcd".into(),
                        ContextValue::Scalar(Value::Str(variant.test_code.clone())),
                    );
                }
                if let Some(list) = list {
                    finding.context.insert(
                        "codelist".into(),
                        ContextValue::Scalar(Value::Str(list.id.clone())),
                    );
                }
                let mut names = Vec::new();
                let mut values = Vec::new();
                for (key, column) in dataset.columns.iter().enumerate() {
                    if column.key_sequence.is_some() {
                        names.push(ContextValue::Scalar(Value::Str(
                            column.declaration.name.value.clone(),
                        )));
                        values.push(ContextValue::Scalar(owned(
                            table
                                .cell(row, key)
                                .map_err(|_| mismatch("held artifact key access"))?,
                        )));
                    }
                }
                finding
                    .context
                    .insert("key_columns".into(), ContextValue::Sequence(names));
                finding
                    .context
                    .insert("key_values".into(), ContextValue::Sequence(values));
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
    crate::value::compare_present(&owned(actual), expected) == Ok(core::cmp::Ordering::Equal)
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
