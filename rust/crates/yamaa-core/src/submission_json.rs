//! Dataset-JSON semantic admission and typed document composition over a held
//! published table; never executes a spec. The byte encoding (quoted strings,
//! streamed escaping, output-byte boundary) is owned by yamaa-adapters'
//! `submission_json` writer, which encodes the typed document built here.
use crate::json::Value as JsonValue;
use crate::{
    diagnostic::ConditionCode,
    limits::Limits,
    submission_composition::{Dataset, Finding, Study},
    table::{TableAccess, ValueRef},
    value::Value,
};
use std::collections::BTreeMap;

/// One Dataset-JSON column declaration, composed semantically in core.
/// `length` retains its admitted numeric token spelling; the adapter writer
/// emits it unquoted exactly as composition produced it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DatasetJsonColumn {
    pub item_oid: String,
    pub name: String,
    pub label: String,
    pub data_type: String,
    pub length: Option<String>,
    pub display_format: Option<String>,
    pub key_sequence: Option<usize>,
}

/// The complete typed Dataset-JSON document header over one held table.
/// Field selection, ordering and values are semantic composition; the
/// adapter writer owns only their byte projection. Row cells are read by
/// the writer directly from the held table after `admit` has passed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DatasetJsonDocument {
    pub path: String,
    pub created: String,
    pub version: String,
    pub file_oid: String,
    pub originator: Option<String>,
    pub source_name: String,
    pub source_version: String,
    pub study_oid: String,
    pub metadata_version_oid: String,
    pub metadata_ref: String,
    pub item_group_oid: String,
    pub name: String,
    pub label: String,
    pub columns: Vec<DatasetJsonColumn>,
}

/// Compose the typed document for an admitted dataset. Callers must run
/// `admit` over the same held table before encoding it.
pub fn document(study: &Study, dataset: &Dataset, define_name: &str) -> DatasetJsonDocument {
    let columns = dataset
        .columns
        .iter()
        .map(|column| {
            let declaration = &column.declaration;
            let meta = declaration.submission.as_ref().expect("composed column");
            DatasetJsonColumn {
                item_oid: format!("IT.{}.{}", dataset.id, declaration.name.value),
                name: declaration.name.value.clone(),
                label: declaration
                    .label
                    .as_ref()
                    .expect("composed label")
                    .value
                    .clone(),
                data_type: match column.data_type {
                    crate::specification_submission_document::DataType::Text => "string",
                    crate::specification_submission_document::DataType::Integer => "integer",
                    crate::specification_submission_document::DataType::Float => "float",
                    crate::specification_submission_document::DataType::Date => "date",
                    crate::specification_submission_document::DataType::Datetime => "datetime",
                    crate::specification_submission_document::DataType::Time => "time",
                    crate::specification_submission_document::DataType::Uri => "URI",
                    _ => "string",
                }
                .into(),
                length: column.length.clone(),
                display_format: meta
                    .display_format
                    .as_ref()
                    .map(|display| display.value.clone()),
                key_sequence: column.key_sequence,
            }
        })
        .collect();
    DatasetJsonDocument {
        path: format!("datasets.{}.dataset_json", dataset.id),
        created: study.created.clone(),
        version: "1.1.0".into(),
        file_oid: format!("{}.{}", study.file_oid, dataset.id),
        originator: study
            .section
            .originator
            .as_ref()
            .map(|originator| originator.value.clone()),
        source_name: "yamaa".into(),
        source_version: study.source_version.clone(),
        study_oid: format!("STDY.{}", study.study.id.value),
        metadata_version_oid: format!("MDV.{}", study.metadata_id),
        metadata_ref: define_name.into(),
        item_group_oid: format!("IG.{}", dataset.id),
        name: dataset.id.clone(),
        label: dataset.metadata.label.value.clone(),
        columns,
    }
}

/// Run every semantic admission the byte writer previously performed before
/// encoding: table geometry, published-artifact field/codelist admission and
/// the boolean-cell refusal. The adapter writer repeats no validation.
pub fn admit<T: TableAccess>(study: &Study, dataset: &Dataset, table: &T) -> Result<(), Finding> {
    let path = format!("datasets.{}.dataset_json", dataset.id);
    Limits::table(table.row_count(), dataset.columns.len()).map_err(|error| {
        Finding::cause(
            ConditionCode::ApplicationRejected,
            &path,
            &error.to_string(),
        )
    })?;
    validate_artifact(study, dataset, table)?;
    for row in 0..table.row_count() {
        for column in 0..dataset.columns.len() {
            let value = table
                .cell(row, column)
                .map_err(|_| artifact_mismatch(dataset, "held artifact access"))?;
            if matches!(value, ValueRef::Bool(_)) {
                return Err(Finding::cause(
                    ConditionCode::SubmissionJsonInvalid,
                    "dataset_json",
                    "schema_violation: boolean cell",
                ));
            }
        }
    }
    Ok(())
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
                .and_then(|list| list.items.as_ref())
                .is_some_and(|items| !items.iter().any(|item| equal_value(value, &item.value)));
            // CT-11: the inventory is every dataset any submission section of
            // the study declares, not only the datasets this document represents.
            let outside_inventory =
                inventory && !matches!(value, ValueRef::Str(id) if study.inventory.contains(id));
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
