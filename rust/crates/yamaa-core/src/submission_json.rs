//! Dataset-JSON typed document composition over a held published table; never
//! executes a spec. Read-side admission shared with the XPT transport lives in
//! `submission_transport`. The byte encoding (quoted strings, streamed
//! escaping, output-byte boundary) is owned by yamaa-adapters' `submission_json`
//! writer, which encodes the typed document built here.
use crate::{
    project_environment::Submission,
    specification_submission_document::DataType,
    submission_composition::{Dataset, Study},
};

/// One Dataset-JSON column declaration, composed semantically in core.
/// `length` retains its admitted numeric token spelling; the adapter writer
/// emits it unquoted exactly as composition produced it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DatasetJsonColumn {
    pub item_oid: String,
    pub name: String,
    pub label: String,
    pub data_type: String,
    /// `integer` for a date or datetime an `adam` document submits as a SAS
    /// numeric date (JSON-10); absent otherwise.
    pub target_data_type: Option<String>,
    pub length: Option<String>,
    pub display_format: Option<String>,
    pub key_sequence: Option<usize>,
}

/// The complete typed Dataset-JSON document header over one held table.
/// Field selection, ordering and values are semantic composition; the
/// adapter writer owns only their byte projection. Row cells are read by
/// the writer directly from the held table after transport admission passed.
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

/// JSON-10: the closed mapping from a resolved submission data type.
fn data_type(data_type: DataType) -> &'static str {
    match data_type {
        DataType::Text => "string",
        DataType::Integer => "integer",
        DataType::Float => "float",
        DataType::Date => "date",
        DataType::Datetime => "datetime",
        DataType::Time => "time",
        DataType::Uri => "URI",
        _ => "string",
    }
}

/// Compose the typed document for an admitted dataset. Callers must run
/// transport admission over the same held table before encoding it.
pub fn document(study: &Study, dataset: &Dataset, define_name: &str) -> DatasetJsonDocument {
    let adam = study.section.family == Submission::Adam;
    let columns = dataset
        .columns
        .iter()
        .map(|column| {
            let declaration = &column.declaration;
            let column_type = declaration.column_type.value;
            // JSON-10: a SAS numeric date keeps its ISO value and spelling here
            // and asks the receiver to convert it to the integer it submits.
            let numeric_date = column_type.numeric_date_format(adam).is_some();
            DatasetJsonColumn {
                item_oid: format!("IT.{}.{}", dataset.id, declaration.name.value),
                name: declaration.name.value.clone(),
                label: declaration
                    .label
                    .as_ref()
                    .expect("composed label")
                    .value
                    .clone(),
                data_type: if numeric_date {
                    data_type(column_type.default_data_type())
                } else {
                    data_type(column.data_type)
                }
                .into(),
                target_data_type: numeric_date.then(|| "integer".into()),
                length: column.length.clone(),
                display_format: column.display_format.clone(),
                key_sequence: column.key_sequence,
            }
        })
        .collect();
    DatasetJsonDocument {
        path: format!("datasets.{}.transport", dataset.id),
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
