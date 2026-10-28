//! Read-side admission of a published artifact shared by both transports, and
//! the transport lengths and empty columns measured from it (JSON-3, JSON-5,
//! SUB-24, SUB-28, XPT-7, XPT-8). Nothing here executes a specification or
//! encodes a byte.
use crate::json::Value as JsonValue;
use crate::{
    application_issue::{Fault, Step},
    ast::Transport,
    cdisc_vocabulary::{DataType, DefineContext},
    diagnostic::ConditionCode,
    limits::Limits,
    project_environment::Submission,
    specification_submission_document as spec,
    submission_composition::{printable_ascii, Dataset, Finding, Study},
    table::{TableAccess, ValueRef},
    value::Value,
};
use std::collections::BTreeMap;

/// The longest character value a transport carries, in bytes (XPT-7).
pub const TEXT_BYTES: usize = 200;
/// `2^53`: the largest integer magnitude that converts exactly (XPT-8).
pub const EXACT_INTEGER: i64 = 1 << 53;

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

/// The key columns and the row's key values, reported with a row finding.
fn keys<T: TableAccess>(
    dataset: &Dataset,
    table: &T,
    row: usize,
    finding: &mut Finding,
) -> Result<(), Finding> {
    let mut names = Vec::new();
    let mut values = Vec::new();
    for (key, column) in dataset.columns.iter().enumerate() {
        if column.key_sequence.is_some() {
            names.push(JsonValue::String(column.declaration.name.value.clone()));
            values.push(JsonValue::from(owned(table.cell(row, key).map_err(
                |_| artifact_mismatch(dataset, "held artifact key access"),
            )?)));
        }
    }
    finding
        .context
        .insert("key_columns".into(), JsonValue::Array(names));
    finding
        .context
        .insert("key_values".into(), JsonValue::Array(values));
    Ok(())
}

/// A value the transport cannot carry: the dataset, column, row and value.
fn value_finding<T: TableAccess>(
    code: ConditionCode,
    dataset: &Dataset,
    table: &T,
    (row, column): (usize, usize),
    value: ValueRef<'_>,
    cause: &str,
) -> Result<Finding, Finding> {
    let name = &dataset.columns[column].declaration.name.value;
    let detail = match cause {
        "empty_string" => "a present empty string reads back as missing",
        "ascii" => "text is not printable ASCII",
        "length" => "text is longer than 200 bytes",
        "trailing_blank" => "a trailing blank reads back as padding",
        "integer" => "an int beyond 2^53 has no exact IBM double",
        "magnitude" => "the magnitude is outside the IBM double range",
        _ => "a SAS date counts from 1582-01-01",
    };
    let mut finding = Finding::cause(
        code,
        &format!("datasets.{}.columns.{name}", dataset.id),
        detail,
    );
    for (key, text) in [
        ("dataset", dataset.id.as_str()),
        ("column", name.as_str()),
        ("artifact", dataset.artifact.as_str()),
        ("cause", cause),
    ] {
        finding
            .context
            .insert(key.into(), JsonValue::String(text.into()));
    }
    finding.context.insert("row".into(), JsonValue::from(row));
    finding
        .context
        .insert("value".into(), JsonValue::from(owned(value)));
    keys(dataset, table, row, &mut finding)?;
    Ok(finding)
}

/// Why a text value cannot travel (XPT-7), or None. Under `xpt` a present
/// empty string and a trailing blank are indistinguishable from padding.
pub fn text_cause(value: &str, xpt: bool) -> Option<&'static str> {
    if xpt && value.is_empty() {
        Some("empty_string")
    } else if !printable_ascii(value) {
        Some("ascii")
    } else if value.len() > TEXT_BYTES {
        Some("length")
    } else if xpt && value.ends_with(' ') {
        Some("trailing_blank")
    } else {
        None
    }
}
/// Why a number cannot become an IBM double exactly (XPT-8), or None.
pub fn number_cause(value: ValueRef<'_>) -> Option<&'static str> {
    match value {
        ValueRef::Int(value) if !(-EXACT_INTEGER..=EXACT_INTEGER).contains(&value) => {
            Some("integer")
        }
        ValueRef::Float(value) => {
            let magnitude = value.get().abs();
            // 16^63 = 2^252 is the first magnitude past the largest IBM
            // double; 16^-65 = 2^-260 is the smallest normalized one.
            (magnitude != 0.0 && !(f64::powi(2.0, -260)..f64::powi(2.0, 252)).contains(&magnitude))
                .then_some("magnitude")
        }
        _ => None,
    }
}
/// The first day a SAS date counts: 1582-01-01 (XPT-8).
fn before_sas_dates(value: ValueRef<'_>) -> bool {
    let first = (1582, 1, 1);
    match value {
        ValueRef::Date(date) => date.fields() < first,
        ValueRef::DateTime(moment) => {
            let (year, month, day, ..) = moment.fields();
            (year, month, day) < first
        }
        _ => false,
    }
}

/// Every read-side admission before any transport length is measured: table
/// geometry, the producer contract, bound codelists, the zero-row rule and the
/// values the requested transport cannot carry.
pub fn admit<T: TableAccess>(study: &Study, dataset: &Dataset, table: &T) -> Result<(), Finding> {
    let path = format!("datasets.{}.transport", dataset.id);
    let transport = study.transport();
    let xpt = transport == Some(Transport::Xpt);
    Limits::table(table.row_count(), dataset.columns.len()).map_err(|bound| {
        Finding::limit(&path, Step::Define, Fault::ResourceLimit, bound)
            .with_detail(bound.to_string())
    })?;
    validate_artifact(study, dataset, table)?;
    for row in 0..table.row_count() {
        for column in 0..dataset.columns.len() {
            let value = table
                .cell(row, column)
                .map_err(|_| artifact_mismatch(dataset, "held artifact access"))?;
            if let ValueRef::Bool(cell) = value {
                return Err(if xpt {
                    artifact_mismatch(dataset, "held artifact boolean cell")
                } else {
                    Finding::cause(
                        ConditionCode::SubmissionJsonInvalid,
                        "dataset_json",
                        "schema_violation: boolean cell",
                    )
                    .with("value", cell)
                });
            }
        }
    }
    if transport.is_none() {
        return Ok(());
    }
    // JSON-3: a dataset without rows declares it has no data.
    if table.row_count() == 0 {
        let mut finding = Finding::cause(
            ConditionCode::SubmissionTransportEmpty,
            &format!("datasets.{}.artifact", dataset.id),
            "a dataset without rows must declare has_no_data",
        );
        for (key, text) in [("dataset", &dataset.id), ("artifact", &dataset.artifact)] {
            finding
                .context
                .insert(key.into(), JsonValue::String(text.clone()));
        }
        return Err(finding);
    }
    let text_limits = xpt || study.section.context == DefineContext::Submission;
    let adam = study.section.family == Submission::Adam;
    for (index, column) in dataset.columns.iter().enumerate() {
        let column_type = column.declaration.column_type.value;
        let numeric_date = column_type.numeric_date_format(adam).is_some();
        for row in 0..table.row_count() {
            let value = table
                .cell(row, index)
                .map_err(|_| artifact_mismatch(dataset, "held artifact access"))?;
            let refusal = match value {
                ValueRef::Str(text) if text_limits => {
                    text_cause(text, xpt).map(|cause| (ConditionCode::TransportTextInvalid, cause))
                }
                ValueRef::Int(_) | ValueRef::Float(_) if xpt => {
                    number_cause(value).map(|cause| (ConditionCode::XptValueOutOfRange, cause))
                }
                ValueRef::Date(_) | ValueRef::DateTime(_)
                    if xpt && numeric_date && before_sas_dates(value) =>
                {
                    Some((ConditionCode::XptValueOutOfRange, "date"))
                }
                _ => None,
            };
            if let Some((code, cause)) = refusal {
                return Err(value_finding(
                    code,
                    dataset,
                    table,
                    (row, index),
                    value,
                    cause,
                )?);
            }
        }
    }
    Ok(())
}

/// Whether a column is a character variable in a transport file: every `str`
/// column, and a date or datetime that keeps its canonical text (XPT-6).
pub fn character(study: &Study, column: &crate::submission_composition::Column) -> bool {
    let column_type = column.declaration.column_type.value;
    column_type == spec::ColumnType::String
        || column_type.temporal()
            && column_type
                .numeric_date_format(study.section.family == Submission::Adam)
                .is_none()
}
/// The bytes one value occupies as transport text: canonical temporal text
/// has a fixed width, and missing occupies none.
fn width(value: ValueRef<'_>) -> usize {
    match value {
        ValueRef::Str(text) => text.len(),
        ValueRef::Date(_) => 10,
        ValueRef::DateTime(_) => 19,
        _ => 0,
    }
}

/// Whether a value-level entry selects one held row: every where column's
/// value is one of the values its clause lists, or none of them under `NE`
/// and `NOTIN`.
fn selects<T: TableAccess>(
    dataset: &Dataset,
    table: &T,
    row: usize,
    value: &crate::submission_composition::ValueDefinition,
) -> Result<bool, Finding> {
    value.selects(|column, listed| {
        let cell = table
            .cell(row, column)
            .map_err(|_| artifact_mismatch(dataset, "held artifact where column access"))?;
        Ok(matches!(cell, ValueRef::Str(text) if listed.iter().any(|value| value == text)))
    })
}

/// What the held rows show of one column or value-level entry: its longest
/// value in transport bytes, and whether any row holds a value.
#[derive(Clone, Copy, Default)]
struct Measured {
    longest: usize,
    held: bool,
}
impl Measured {
    fn add(&mut self, value: ValueRef<'_>) {
        self.longest = self.longest.max(width(value));
        self.held |= value != ValueRef::Missing;
    }
}

/// SUB-24 and SUB-28: measure every column of the selected section from the
/// held tables. A character column's length is the longest value, in bytes
/// and at least one, across the section's datasets carrying a column of that
/// name; a SUPP-- dataset is measured on its own. A value-level definition is
/// measured over the rows its where clause selects. A column, or an entry,
/// of a dataset with rows has no data when none of the rows it describes
/// holds a value. `tables` holds one entry per dataset, absent for a dataset
/// declared to have no data: its columns add no length and carry no flag,
/// since the dataset's own flag says so.
pub fn measure<T: TableAccess>(study: &mut Study, tables: &[Option<&T>]) -> Result<(), Finding> {
    let mut groups = BTreeMap::<(Option<usize>, String), usize>::new();
    let mut measured = Vec::new();
    for (index, dataset) in study.datasets.iter().enumerate() {
        let scope = dataset.supplemental().then_some(index);
        let table = tables.get(index).copied().flatten();
        let mut dataset_measures = Vec::new();
        for (position, column) in dataset.columns.iter().enumerate() {
            let mut whole = Measured::default();
            let mut by_entry = vec![Measured::default(); column.values.len()];
            if let Some(table) = table {
                for row in 0..table.row_count() {
                    let value = table
                        .cell(row, position)
                        .map_err(|_| artifact_mismatch(dataset, "held artifact access"))?;
                    whole.add(value);
                    for (entry, measure) in column.values.iter().zip(&mut by_entry) {
                        if selects(dataset, table, row, entry)? {
                            measure.add(value);
                        }
                    }
                }
            }
            if character(study, column) {
                let group = groups
                    .entry((scope, column.declaration.name.value.clone()))
                    .or_insert(0);
                *group = (*group).max(whole.longest);
            }
            dataset_measures.push((whole, by_entry));
        }
        measured.push((table.is_some(), dataset_measures));
    }
    let adam = study.section.family == Submission::Adam;
    for (index, dataset) in study.datasets.iter_mut().enumerate() {
        let scope = dataset.supplemental().then_some(index);
        let (has_rows, dataset_measures) = &measured[index];
        for (column, (whole, by_entry)) in dataset.columns.iter_mut().zip(dataset_measures) {
            column.has_no_data = *has_rows && !whole.held;
            for (value, measure) in column.values.iter_mut().zip(by_entry) {
                value.column.has_no_data = *has_rows && !measure.held;
            }
            let column_type = column.declaration.column_type.value;
            let is_character = column_type == spec::ColumnType::String
                || column_type.temporal() && column_type.numeric_date_format(adam).is_none();
            if !is_character {
                continue;
            }
            let length = groups[&(scope, column.declaration.name.value.clone())].max(1);
            column.transport_length = Some(length);
            if column.data_type == DataType::Text {
                column.length = Some(length.to_string());
            }
            for (value, measure) in column.values.iter_mut().zip(by_entry) {
                if value.column.data_type == DataType::Text {
                    value.column.length = Some(measure.longest.max(1).to_string());
                }
            }
        }
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
        .map(|list| (list.declaration.id.name.as_str(), &list.declaration))
        .collect();
    for (index, base) in dataset.columns.iter().enumerate() {
        for row in 0..table.row_count() {
            // The build admitted at most one entry per row; the first entry
            // whose where clause selects the row decides its metadata.
            let mut variant = None;
            for value in &base.values {
                if selects(dataset, table, row, value)? {
                    variant = Some(value);
                    break;
                }
            }
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
                    finding
                        .context
                        .insert("entry".into(), JsonValue::String(variant.id.clone()));
                }
                if let Some(list) = list {
                    finding
                        .context
                        .insert("codelist".into(), JsonValue::String(list.id.name.clone()));
                }
                keys(dataset, table, row, &mut finding)?;
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

    #[test]
    fn text_causes_follow_one_order_and_only_xpt_refuses_padding_lookalikes() {
        assert_eq!(text_cause("", true), Some("empty_string"));
        assert_eq!(text_cause("", false), None);
        assert_eq!(text_cause("caf\u{e9}", false), Some("ascii"));
        assert_eq!(text_cause("A\tB", true), Some("ascii"));
        assert_eq!(text_cause(&"X".repeat(200), true), None);
        assert_eq!(text_cause(&"X".repeat(201), false), Some("length"));
        assert_eq!(text_cause("SITE ", true), Some("trailing_blank"));
        assert_eq!(text_cause("SITE ", false), None);
        assert_eq!(text_cause(" SITE", true), None);
    }

    #[test]
    fn number_causes_bound_exact_integers_and_ibm_magnitudes() {
        let float = |value: f64| ValueRef::Float(FiniteFloat::new(value).unwrap());
        assert_eq!(number_cause(ValueRef::Int(EXACT_INTEGER)), None);
        assert_eq!(number_cause(ValueRef::Int(-EXACT_INTEGER)), None);
        assert_eq!(
            number_cause(ValueRef::Int(EXACT_INTEGER + 1)),
            Some("integer")
        );
        assert_eq!(
            number_cause(ValueRef::Int(-EXACT_INTEGER - 1)),
            Some("integer")
        );
        assert_eq!(number_cause(float(0.0)), None);
        assert_eq!(number_cause(float(-0.0)), None);
        assert_eq!(number_cause(float(f64::powi(2.0, -260))), None);
        assert_eq!(number_cause(float(f64::powi(2.0, -261))), Some("magnitude"));
        assert_eq!(
            number_cause(float(f64::powi(2.0, 252) * (1.0 - f64::EPSILON / 2.0))),
            None
        );
        assert_eq!(number_cause(float(-f64::powi(2.0, 252))), Some("magnitude"));
    }

    #[test]
    fn sas_dates_begin_on_the_first_day_of_1582() {
        use crate::temporal::{Date, DateTime, Precision};
        let date = |year, month, day| Date::new(year, month, day, Precision::Day).unwrap();
        assert!(before_sas_dates(ValueRef::Date(date(1581, 12, 31))));
        assert!(!before_sas_dates(ValueRef::Date(date(1582, 1, 1))));
        let moment =
            |day| DateTime::new(date(1581, 12, day), 23, 59, 59, Precision::Second).unwrap();
        assert!(before_sas_dates(ValueRef::DateTime(moment(31))));
        assert!(!before_sas_dates(ValueRef::Missing));
    }
}
