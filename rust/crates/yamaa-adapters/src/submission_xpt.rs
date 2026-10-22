//! SAS Transport version 5 documents, their file-level refusals and their
//! byte writer over a held published table (XPT-1 through XPT-10): the public
//! TS-140 record layout, blank padding and IBM System/360 doubles.
//!
//! yamaa-core owns the composed metadata model (`submission_composition`),
//! value admission and the measured lengths (`submission_transport`) and the
//! findings XPT-10 raises (`define_diagnostics`); this module builds the typed
//! document from that model, refuses a file the transport cannot carry and
//! owns the byte projection.
use yamaa_core::{
    application_issue::{Fault, Step},
    define_diagnostics::{xpt_file_too_large, xpt_last_row_blank},
    limits::{LimitExceeded, Limits},
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

/// The typed document for an admitted dataset of `study` whose transport
/// lengths were measured. Callers run `submission_transport::admit` and
/// `measure`, and [`admit`] here, over the same held table before encoding it.
pub fn document(study: &Study, dataset: &Dataset) -> XptDocument {
    let adam = study.section().family == Submission::Adam;
    let variables = dataset
        .columns()
        .iter()
        .map(|column| {
            let declaration = column.declaration();
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
                XptKind::Date | XptKind::DateTime => column.display_format().and_then(sas_format),
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
                    XptKind::Character => column.transport_length().expect("measured transport"),
                    _ => 8,
                },
                format,
            }
        })
        .collect();
    XptDocument {
        path: format!("datasets.{}.transport", dataset.id()),
        name: dataset.id().into(),
        label: dataset.metadata().label.value.clone(),
        created: header_time(study.created()),
        variables,
    }
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
        return Err(xpt_file_too_large(dataset, size));
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
        return Err(xpt_last_row_blank(dataset, rows - 1));
    }
    Ok(())
}

/// Days from 1960-01-01, the SAS date origin, to 1970-01-01.
const SAS_EPOCH_DAYS: i64 = 3653;
/// The SAS release and operating system the first header records name.
const VERSION: &str = "9.4";
const SYSTEM: &str = "yamaa";

/// Text left aligned and blank padded to `width` bytes. Admission already
/// bounded every name, label and value to printable ASCII within its field.
fn padded(text: &str, width: usize) -> Vec<u8> {
    let mut bytes = text.as_bytes()[..text.len().min(width)].to_vec();
    bytes.resize(width, b' ');
    bytes
}
/// A header record: the fixed prefix, the record kind and its counts.
fn header(kind: &str, counts: &str) -> Vec<u8> {
    let mut record =
        format!("HEADER RECORD*******{kind:<8}HEADER RECORD!!!!!!!{counts}").into_bytes();
    record.resize(RECORD, b' ');
    record
}

/// Convert a finite binary64 exactly to an IBM System/360 double: a sign bit,
/// a seven-bit excess-64 hexadecimal exponent and a 56-bit fraction in
/// [1/16, 1). Zero, including negative zero, is eight zero bytes. A magnitude
/// at or above 16^63 or below 16^-65 has no IBM double and returns None.
pub fn ibm(value: f64) -> Option<[u8; 8]> {
    if value == 0.0 {
        return Some([0; 8]);
    }
    if !value.is_finite() {
        return None;
    }
    let bits = value.to_bits();
    let sign = (bits >> 63) as u8;
    let biased = ((bits >> 52) & 0x7ff) as i64;
    let mut significand = bits & ((1 << 52) - 1);
    // value = significand * 2^low, with significand's top bit at 2^(low + 52)
    // for a normal number; a subnormal is below 16^-65 and has no IBM double.
    if biased == 0 {
        return None;
    }
    significand |= 1 << 52;
    let low = biased - 1075;
    let top = low + 52;
    let exponent = top.div_euclid(4) + 1;
    if !(-64..=63).contains(&exponent) {
        return None;
    }
    // fraction = value / 16^exponent * 2^56, an integer in [2^52, 2^56): the
    // shift is top mod 4, so the conversion is exact.
    let fraction = significand << top.rem_euclid(4);
    let mut bytes = [0; 8];
    bytes[0] = (sign << 7) | (exponent + 64) as u8;
    bytes[1..].copy_from_slice(&fraction.to_be_bytes()[1..]);
    Some(bytes)
}

/// One output buffer, checked before every extension against the shared
/// output boundary.
struct Writer<'a> {
    output: Vec<u8>,
    path: &'a str,
}
impl Writer<'_> {
    /// A held cell the writer cannot read or the record layout cannot carry.
    /// Admission already refused every authored one, so this is a byte
    /// projection the writer could not complete.
    fn failure(&self, detail: &str) -> Finding {
        Finding::fault(self.path, Step::Define, Fault::OutputProjection).with_detail(detail)
    }
    fn limit(&self, bound: LimitExceeded) -> Finding {
        let detail = if bound.actual.is_none() {
            "output byte limit".into()
        } else {
            bound.to_string()
        };
        Finding::limit(self.path, Step::Define, Fault::OutputLimit, bound).with_detail(detail)
    }
    /// Admit `required` output bytes against the shared output bound; a size
    /// with no representation exceeds it.
    fn admit(&self, required: Option<usize>) -> Result<usize, Finding> {
        required
            .ok_or(LimitExceeded {
                resource: "output_bytes",
                limit: Limits::OUTPUT_BYTES,
                actual: None,
            })
            .and_then(|required| Limits::output_bytes(required).map(|()| required))
            .map_err(|bound| self.limit(bound))
    }
    fn bytes(&mut self, bytes: &[u8]) -> Result<(), Finding> {
        Limits::output_extension(self.output.len(), bytes.len())
            .map_err(|bound| self.limit(bound))?;
        self.output.extend_from_slice(bytes);
        Ok(())
    }
    /// Blank padding to the next whole record.
    fn pad(&mut self) -> Result<(), Finding> {
        let remainder = self.output.len() % RECORD;
        if remainder > 0 {
            self.bytes(&vec![b' '; RECORD - remainder])?;
        }
        Ok(())
    }
    fn number(&mut self, value: f64) -> Result<(), Finding> {
        let bytes = ibm(value).ok_or_else(|| self.failure("number outside IBM double range"))?;
        self.bytes(&bytes)
    }
}

/// Encode an admitted typed document over its held table. Callers run the
/// core admissions over the same table first.
pub fn render<T: TableAccess>(document: &XptDocument, table: &T) -> Result<Vec<u8>, Finding> {
    let size = document.file_bytes(table.row_count());
    let mut writer = Writer {
        output: Vec::new(),
        path: &document.path,
    };
    let capacity = writer.admit(usize::try_from(size).ok())?;
    writer.output.reserve_exact(capacity);
    let created = &document.created;
    // XPT-2, XPT-3: library header, first and second real headers.
    writer.bytes(&header("LIBRARY", &"0".repeat(30)))?;
    let first = format!(
        "{:<8}{:<8}{:<8}{:<8}{:<8}{:24}{created}",
        "SAS", "SAS", "SASLIB", VERSION, SYSTEM, ""
    );
    writer.bytes(&padded(&first, RECORD))?;
    writer.bytes(&padded(created, RECORD))?;
    // Member and descriptor headers, then the two member records.
    writer.bytes(&header("MEMBER", "000000000000000001600000000140"))?;
    writer.bytes(&header("DSCRPTR", &"0".repeat(30)))?;
    let member = format!(
        "{:<8}{:<8}{:<8}{:<8}{:<8}{:24}{created}",
        "SAS", document.name, "SASDATA", VERSION, SYSTEM, ""
    );
    writer.bytes(&padded(&member, RECORD))?;
    let mut second = padded(created, 32);
    second.extend(padded(&document.label, 40));
    second.extend(padded("", 8));
    writer.bytes(&second)?;
    // XPT-4: the NAMESTR header and one 140-byte NAMESTR per variable.
    writer.bytes(&header(
        "NAMESTR",
        &format!("000000{:04}00000000000000000000", document.variables.len()),
    ))?;
    let mut position = 0u32;
    for (index, variable) in document.variables.iter().enumerate() {
        let short = |value: usize| -> Result<[u8; 2], Finding> {
            u16::try_from(value)
                .ok()
                .filter(|value| *value <= 0x7fff)
                .map(u16::to_be_bytes)
                .ok_or_else(|| writer.failure("NAMESTR field out of range"))
        };
        let mut namestr = Vec::with_capacity(140);
        namestr.extend(short(if variable.kind == XptKind::Character {
            2
        } else {
            1
        })?);
        namestr.extend([0, 0]);
        namestr.extend(short(variable.width)?);
        namestr.extend(short(index + 1)?);
        namestr.extend(padded(&variable.name, 8));
        namestr.extend(padded(&variable.label, 40));
        match &variable.format {
            Some(format) => {
                namestr.extend(padded(&format.name, 8));
                namestr.extend(format.width.to_be_bytes());
                namestr.extend(format.decimals.to_be_bytes());
            }
            None => {
                namestr.extend(padded("", 8));
                namestr.extend([0, 0, 0, 0]);
            }
        }
        // Left justification, two fill bytes, then no informat (XPT-9).
        namestr.extend([0, 0, 0, 0]);
        namestr.extend(padded("", 8));
        namestr.extend([0, 0, 0, 0]);
        namestr.extend(position.to_be_bytes());
        namestr.resize(140, 0);
        writer.bytes(&namestr)?;
        position +=
            u32::try_from(variable.width).map_err(|_| writer.failure("observation width"))?;
    }
    writer.pad()?;
    // XPT-2: the observation header, then each row's cells in column order.
    writer.bytes(&header("OBS", &"0".repeat(30)))?;
    for row in 0..table.row_count() {
        for (column, variable) in document.variables.iter().enumerate() {
            let value = table
                .cell(row, column)
                .map_err(|_| writer.failure("held table access"))?;
            match (variable.kind, value) {
                (XptKind::Character, ValueRef::Missing) => {
                    writer.bytes(&padded("", variable.width))?
                }
                (XptKind::Character, ValueRef::Str(text)) if text.len() <= variable.width => {
                    writer.bytes(&padded(text, variable.width))?
                }
                (XptKind::Character, ValueRef::Date(date)) => {
                    writer.bytes(&padded(&date.to_string(), variable.width))?
                }
                (XptKind::Character, ValueRef::DateTime(moment)) => {
                    writer.bytes(&padded(&moment.to_string(), variable.width))?
                }
                (_, ValueRef::Missing) => writer.bytes(&[b'.', 0, 0, 0, 0, 0, 0, 0])?,
                (XptKind::Number, ValueRef::Int(value)) => writer.number(value as f64)?,
                (XptKind::Number, ValueRef::Float(value)) => writer.number(value.get())?,
                (XptKind::Date, ValueRef::Date(date)) => writer.number(
                    (i64::from(crate::arrow_temporal::date_days(date)) + SAS_EPOCH_DAYS) as f64,
                )?,
                (XptKind::DateTime, ValueRef::DateTime(moment)) => writer.number(
                    (crate::arrow_temporal::datetime_seconds(moment) + SAS_EPOCH_DAYS * 86_400)
                        as f64,
                )?,
                _ => return Err(writer.failure("held cell does not match its variable")),
            }
        }
    }
    writer.pad()?;
    Ok(writer.output)
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

    #[test]
    fn oversized_transport_refuses_before_reserving_or_reading_cells() {
        use yamaa_core::{
            diagnostic::ConditionCode,
            table::{CellError, TableSchema},
        };
        struct Oversized;
        impl TableAccess for Oversized {
            type Error = ();
            fn schema(&self) -> &TableSchema {
                panic!("size admission precedes schema access")
            }
            fn row_count(&self) -> usize {
                Limits::OUTPUT_BYTES / 8
            }
            fn cell(&self, _: usize, _: usize) -> Result<ValueRef<'_>, CellError<()>> {
                panic!("size admission precedes cell access")
            }
        }
        let document = XptDocument {
            path: "datasets.DM.transport".into(),
            name: "DM".into(),
            label: "Demographics".into(),
            created: "01JAN26:00:00:00".into(),
            variables: vec![XptVariable {
                name: "AGE".into(),
                label: "Age".into(),
                kind: XptKind::Number,
                width: 8,
                format: None,
            }],
        };
        let finding = render(&document, &Oversized).unwrap_err();
        assert_eq!(finding.code, ConditionCode::ApplicationRejected);
        assert_eq!(finding.path, "datasets.DM.transport");
        assert_eq!(finding.context["step"], "define");
        assert_eq!(finding.context["code"], "output_limit");
        assert_eq!(finding.context["resource"], "output_bytes");
        assert_eq!(finding.context["limit"], Limits::OUTPUT_BYTES);
        // 640 header bytes + 160 padded NAMESTR bytes + 80 observation-header
        // bytes + the 10 GiB observations (already a multiple of 80).
        assert_eq!(finding.context["actual"], Limits::OUTPUT_BYTES + 880);

        let mut writer = Writer {
            output: Vec::new(),
            path: "datasets.DM.transport",
        };
        let projection = writer.number(f64::INFINITY).unwrap_err();
        assert_eq!(projection.context["step"], "define");
        assert_eq!(projection.context["code"], "output_projection");
        assert_eq!(
            projection.context["detail"],
            "number outside IBM double range"
        );
        assert!(writer.output.is_empty());
        let overflow = writer.limit(LimitExceeded {
            resource: "output_bytes",
            limit: Limits::OUTPUT_BYTES,
            actual: None,
        });
        assert_eq!(overflow.context["detail"], "output byte limit");
        assert_eq!(overflow.context["code"], "output_limit");
        assert_eq!(overflow.context["limit"], Limits::OUTPUT_BYTES);
        assert!(!overflow.context.contains_key("actual"));
    }

    fn hex(bytes: [u8; 8]) -> String {
        bytes.iter().map(|byte| format!("{byte:02X}")).collect()
    }

    /// Independently derived IBM doubles: sign, excess-64 hexadecimal exponent
    /// and the binary64 significand shifted into a normalized 56-bit fraction.
    #[test]
    fn ibm_doubles_are_exact_and_normalized() {
        for (value, expected) in [
            (1.0, "4110000000000000"),
            (-1.0, "C110000000000000"),
            (0.5, "4080000000000000"),
            (100.0, "4264000000000000"),
            (0.1, "401999999999999A"),
            (-118.625, "C276A00000000000"),
            (3653.0, "43E4500000000000"),
            (f64::powi(2.0, 53), "4E20000000000000"),
            (f64::powi(2.0, -260), "0010000000000000"),
            (f64::MIN_POSITIVE * f64::powi(2.0, 762), "0010000000000000"),
        ] {
            assert_eq!(hex(ibm(value).unwrap()), expected, "{value}");
        }
        assert_eq!(ibm(0.0), Some([0; 8]));
        assert_eq!(ibm(-0.0), Some([0; 8]));
        assert_eq!(ibm(f64::powi(2.0, 252)), None);
        assert_eq!(ibm(f64::powi(2.0, -261)), None);
        assert_eq!(ibm(f64::MIN_POSITIVE / 2.0), None);
        let largest = f64::powi(2.0, 252) * (1.0 - f64::EPSILON / 2.0);
        assert_eq!(hex(ibm(largest).unwrap()), "7FFFFFFFFFFFFFF8");
    }

    /// A size past the output bound, or with no representation, is the bound;
    /// a held number with no IBM double is a projection fault.
    #[test]
    fn refusals_name_the_output_bound_or_a_projection_fault() {
        use yamaa_core::{diagnostic::ConditionCode, json::Value as V};
        let mut writer = Writer {
            output: Vec::new(),
            path: "datasets.DM.transport",
        };
        let text = |value: &str| V::String(value.into());
        assert_eq!(
            writer.admit(Some(Limits::OUTPUT_BYTES)),
            Ok(Limits::OUTPUT_BYTES)
        );
        for (required, actual) in [
            (
                Some(Limits::OUTPUT_BYTES + 1),
                Some(Limits::OUTPUT_BYTES + 1),
            ),
            (None, None),
        ] {
            let finding = writer.admit(required).unwrap_err();
            assert_eq!(finding.code, ConditionCode::ApplicationRejected);
            assert_eq!(finding.path, "datasets.DM.transport");
            let mut expected: yamaa_core::diagnostic::Context = [
                ("code".into(), text("output_limit")),
                ("limit".into(), V::from(Limits::OUTPUT_BYTES)),
                ("resource".into(), text("output_bytes")),
                ("step".into(), text("define")),
            ]
            .into();
            if let Some(actual) = actual {
                expected.insert("actual".into(), V::from(actual));
                expected.insert(
                    "detail".into(),
                    text("output_bytes: required Some(10737418241), limit 10737418240"),
                );
            } else {
                expected.insert("detail".into(), text("output byte limit"));
            }
            assert_eq!(finding.context, expected);
        }
        let finding = writer.number(f64::MAX).unwrap_err();
        assert_eq!(
            finding.context,
            [
                ("code".into(), text("output_projection")),
                ("detail".into(), text("number outside IBM double range")),
                ("step".into(), text("define")),
            ]
            .into()
        );
        assert!(writer.output.is_empty());
    }
}
