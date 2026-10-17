//! SAS Transport version 5 typed document composition and its file-level
//! refusals over a held published table (XPT-1 through XPT-10). The byte
//! encoding is owned by yamaa-adapters' `submission_xpt` writer, which encodes
//! the typed document built here; value admission is `submission_transport`'s.
use crate::json::Value as JsonValue;
use crate::{
    diagnostic::ConditionCode,
    project_environment::Submission,
    specification_submission_document::ColumnType,
    submission_composition::{sas_format, Dataset, Finding, SasFormat, Study},
    table::{TableAccess, ValueRef},
};

/// One record of the transport layout (XPT-2).
pub const RECORD: usize = 80;
/// One NAMESTR, the variable descriptor (XPT-4).
pub const NAMESTR: usize = 140;
/// The library, member and NAMESTR headers and the four header data records
/// before the NAMESTRs: eight records (XPT-2).
pub const HEADER_BYTES: u64 = 8 * RECORD as u64;
/// The largest file a transport writes, in bytes (XPT-10).
pub const FILE_BYTES: u64 = 5_000_000_000;

/// How a variable's cells are encoded (XPT-6).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum XptKind {
    /// Printable ASCII, blank padded; `date` and `datetime` canonical text
    /// outside `adam`.
    Character,
    /// An `int` or `float` as an IBM double.
    Number,
    /// Days since 1960-01-01 as an IBM double, under `adam`.
    Date,
    /// Seconds since 1960-01-01T00:00:00 as an IBM double, under `adam`.
    DateTime,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct XptVariable {
    pub name: String,
    pub label: String,
    pub kind: XptKind,
    /// Bytes in an observation: the measured width of a character variable
    /// (SUB-24), and 8 for every numeric one.
    pub width: usize,
    pub format: Option<SasFormat>,
}
/// The complete typed transport document over one held table.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct XptDocument {
    pub path: String,
    /// The member name: the dataset name.
    pub name: String,
    /// The member label: the dataset's `submission.label`.
    pub label: String,
    /// The selected creation time as `ddMMMyy:hh:mm:ss`, upper-case month.
    pub created: String,
    pub variables: Vec<XptVariable>,
}
impl XptDocument {
    /// Bytes in one observation.
    pub fn observation(&self) -> usize {
        self.variables.iter().map(|variable| variable.width).sum()
    }
    /// The complete file size, computed before any byte is written (XPT-10):
    /// headers, NAMESTRs padded to a record, the observation header and the
    /// observations padded to a record.
    pub fn file_bytes(&self, rows: usize) -> u64 {
        let records = |bytes: u64| bytes.div_ceil(RECORD as u64) * RECORD as u64;
        HEADER_BYTES
            + records((NAMESTR * self.variables.len()) as u64)
            + RECORD as u64
            + records(rows as u64 * self.observation() as u64)
    }
}

const MONTHS: [&str; 12] = [
    "JAN", "FEB", "MAR", "APR", "MAY", "JUN", "JUL", "AUG", "SEP", "OCT", "NOV", "DEC",
];
/// `YYYY-MM-DDThh:mm:ss` as the transport header's `ddMMMyy:hh:mm:ss`.
pub fn header_time(created: &str) -> String {
    let field = |range: std::ops::Range<usize>| &created[range];
    let month: usize = field(5..7).parse().expect("admitted creation time");
    format!(
        "{}{}{}:{}",
        field(8..10),
        MONTHS[month - 1],
        field(2..4),
        field(11..19)
    )
}

/// Compose the typed document for an admitted dataset whose transport lengths
/// were measured. Callers run `submission_transport::admit`, `measure` and
/// `admit` here over the same held table before encoding it.
pub fn document(study: &Study, dataset: &Dataset) -> XptDocument {
    let adam = study.section.family == Submission::Adam;
    let variables = dataset
        .columns
        .iter()
        .map(|column| {
            let declaration = &column.declaration;
            let column_type = declaration.column_type.value;
            let kind = match column_type {
                ColumnType::Integer | ColumnType::Float => XptKind::Number,
                ColumnType::Date if adam => XptKind::Date,
                ColumnType::Datetime if adam => XptKind::DateTime,
                _ => XptKind::Character,
            };
            let format = match kind {
                XptKind::Character => None,
                XptKind::Number => declaration
                    .submission
                    .as_ref()
                    .and_then(|meta| meta.display_format.as_ref())
                    .and_then(|declared| sas_format(&declared.value)),
                XptKind::Date | XptKind::DateTime => {
                    column.display_format.as_deref().and_then(sas_format)
                }
            };
            XptVariable {
                name: declaration.name.value.clone(),
                label: declaration
                    .label
                    .as_ref()
                    .expect("composed label")
                    .value
                    .clone(),
                kind,
                width: match kind {
                    XptKind::Character => column.transport_length.expect("measured transport"),
                    _ => 8,
                },
                format,
            }
        })
        .collect();
    XptDocument {
        path: format!("datasets.{}.transport", dataset.id),
        name: dataset.id.clone(),
        label: dataset.metadata.label.value.clone(),
        created: header_time(&study.created),
        variables,
    }
}

fn file_finding(code: ConditionCode, dataset: &Dataset, detail: &str) -> Finding {
    let mut finding = Finding::cause(code, &format!("datasets.{}.artifact", dataset.id), detail);
    for (key, text) in [("dataset", &dataset.id), ("artifact", &dataset.artifact)] {
        finding
            .context
            .insert(key.into(), JsonValue::String(text.clone()));
    }
    finding
}

/// XPT-10: refuse a file larger than the transport carries, computed before
/// writing, and an artifact whose last row would encode to blanks only, which
/// a reader cannot tell from the final record's padding.
pub fn admit<T: TableAccess>(
    document: &XptDocument,
    dataset: &Dataset,
    table: &T,
) -> Result<(), Finding> {
    let rows = table.row_count();
    let size = document.file_bytes(rows);
    if size > FILE_BYTES {
        let mut finding = file_finding(
            ConditionCode::XptFileTooLarge,
            dataset,
            "transport file larger than 5000000000 bytes",
        );
        finding
            .context
            .insert("bytes".into(), JsonValue::from(size));
        return Err(finding);
    }
    let blank = rows > 0
        && document
            .variables
            .iter()
            .enumerate()
            .all(|(column, variable)| {
                variable.kind == XptKind::Character
                    && table
                        .cell(rows - 1, column)
                        .is_ok_and(|value| value == ValueRef::Missing)
            });
    if blank {
        let mut finding = file_finding(
            ConditionCode::XptLastRowBlank,
            dataset,
            "the last row encodes to blanks only",
        );
        finding
            .context
            .insert("row".into(), JsonValue::from(rows - 1));
        return Err(finding);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_time_uses_two_digit_years_and_upper_case_months() {
        assert_eq!(header_time("2026-09-11T00:00:00"), "11SEP26:00:00:00");
        assert_eq!(header_time("1999-12-31T23:59:58"), "31DEC99:23:59:58");
        assert_eq!(header_time("2000-01-05T07:08:09"), "05JAN00:07:08:09");
    }

    #[test]
    fn file_size_pads_namestrs_and_observations_to_whole_records() {
        let variable = |width| XptVariable {
            name: "A".into(),
            label: "A".into(),
            kind: XptKind::Character,
            width,
            format: None,
        };
        let document = XptDocument {
            path: "datasets.DM.transport".into(),
            name: "DM".into(),
            label: "Demographics".into(),
            created: "11SEP26:00:00:00".into(),
            variables: vec![variable(3), variable(8)],
        };
        // 640 header bytes, 280 NAMESTR bytes padded to 320, one header record
        // and 3 rows of 11 bytes padded to one record.
        assert_eq!(document.file_bytes(3), 640 + 320 + 80 + 80);
        assert_eq!(document.file_bytes(0), 640 + 320 + 80);
        assert_eq!(document.file_bytes(8), 640 + 320 + 80 + 160);
    }
}
