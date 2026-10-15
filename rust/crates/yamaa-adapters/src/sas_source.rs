//! Read-only SAS source decoding over the already captured immutable snapshot.
//! ReadStat supplies physical values; authored types and strict text decoding are
//! shared by both hosts. Display formats are never semantic type authorities.
use crate::arrow_table::ArrowTable;
use std::{
    collections::BTreeSet,
    ffi::{c_char, c_int, c_void, CStr},
    panic::{catch_unwind, AssertUnwindSafe},
};
use yamaa_core::{
    diagnostic::SourceFinding,
    limits::Limits,
    specification::{SourceDeclaration, SourceProfile},
    table::{CellError, Column, TableAccess, TableSchema, ValueRef},
    temporal::{DatePrecision, DateTimePrecision},
    value::{ColumnType, Value},
};

#[derive(Debug)]
pub enum Error {
    Invalid(&'static str),
    Typing(yamaa_core::typed_csv::Error),
    Limit {
        resource: &'static str,
        limit: usize,
        actual: Option<usize>,
    },
    Internal,
}
impl Error {
    pub fn finding<'a>(&'a self, dataset: &'a str, path: &'a str) -> Option<SourceFinding<'a>> {
        match self {
            Self::Invalid(cause) => Some(SourceFinding::Sas {
                dataset,
                path,
                cause,
            }),
            Self::Typing(error) => error.finding(dataset),
            Self::Limit { .. } | Self::Internal => None,
        }
    }
}

impl From<yamaa_core::limits::LimitExceeded> for Error {
    fn from(error: yamaa_core::limits::LimitExceeded) -> Self {
        Self::Limit {
            resource: error.resource,
            limit: error.limit,
            actual: error.actual,
        }
    }
}
unsafe extern "C" {
    fn yamaa_readstat_parse(
        bytes: *const u8,
        length: usize,
        xport: c_int,
        rows: usize,
        columns: usize,
        cells: usize,
        metadata: unsafe extern "C" fn(i64, c_int, c_int, *const c_char, *mut c_void) -> c_int,
        variable: unsafe extern "C" fn(c_int, *const c_char, usize, c_int, *mut c_void) -> c_int,
        value: unsafe extern "C" fn(
            c_int,
            c_int,
            *const c_char,
            f64,
            c_int,
            c_int,
            *mut c_void,
        ) -> c_int,
        user: *mut c_void,
        resource: *mut c_int,
        limit: *mut usize,
        actual: *mut usize,
        known: *mut c_int,
    ) -> c_int;
}

struct Rows {
    schema: TableSchema,
    rows: Vec<Vec<Value>>,
}
impl TableAccess for Rows {
    type Error = std::convert::Infallible;
    fn schema(&self) -> &TableSchema {
        &self.schema
    }
    fn row_count(&self) -> usize {
        self.rows.len()
    }
    fn cell(&self, row: usize, column: usize) -> Result<ValueRef<'_>, CellError<Self::Error>> {
        self.rows
            .get(row)
            .and_then(|r| r.get(column))
            .map(ValueRef::from)
            .ok_or(CellError::OutOfBounds { row, column })
    }
}
struct Context<'a> {
    source: &'a SourceDeclaration,
    columns: Vec<Column>,
    rows: Vec<Vec<Value>>,
    declared_rows: Option<usize>,
    declared_columns: usize,
    error: Option<Error>,
}
impl Context<'_> {
    fn declarations(&self) -> Result<(), Error> {
        if self.columns.len() != self.declared_columns {
            return Err(Error::Invalid("header"));
        }
        for (name, _) in &self.source.types {
            if !self.columns.iter().any(|column| &column.name == name) {
                return Err(Error::Typing(yamaa_core::typed_csv::Error::UnknownField {
                    field: name.clone(),
                }));
            }
        }
        Ok(())
    }
    fn text(&self, bytes: &[u8]) -> Result<String, Error> {
        decode_text(
            bytes,
            self.source
                .encoding
                .as_deref()
                .ok_or(Error::Invalid("encoding"))?,
        )
    }
}

/// Every Rust callback contains its own panic fence. Nothing unwinds through C.
unsafe fn callback(
    opaque: *mut c_void,
    action: impl FnOnce(&mut Context<'_>) -> Result<(), Error>,
) -> c_int {
    let context = &mut *opaque.cast::<Context<'_>>();
    let result = catch_unwind(AssertUnwindSafe(|| action(context)));
    match result {
        Ok(Ok(())) => 0,
        Ok(Err(error)) => {
            context.error = Some(error);
            1
        }
        Err(_) => {
            context.error = Some(Error::Internal);
            1
        }
    }
}
unsafe extern "C" fn metadata(
    rows: i64,
    columns: c_int,
    version: c_int,
    _encoding: *const c_char,
    opaque: *mut c_void,
) -> c_int {
    callback(opaque, |context| {
        if context.source.profile == SourceProfile::Xport && !matches!(version, 5 | 8) {
            return Err(Error::Invalid("version"));
        }
        let columns = usize::try_from(columns).map_err(|_| Error::Invalid("header"))?;
        if columns == 0 {
            return Err(Error::Invalid("header"));
        }
        Limits::columns(columns)?;
        context.declared_columns = columns;
        if rows >= 0 {
            let rows = usize::try_from(rows).map_err(|_| Error::Invalid("header"))?;
            Limits::table(rows, columns)?;
            context.declared_rows = Some(rows);
        }
        Ok(())
    })
}
unsafe extern "C" fn variable(
    column: c_int,
    name: *const c_char,
    _width: usize,
    text: c_int,
    opaque: *mut c_void,
) -> c_int {
    callback(opaque, |context| {
        if usize::try_from(column).ok() != Some(context.columns.len())
            || context.columns.len() >= context.declared_columns
            || name.is_null()
        {
            return Err(Error::Invalid("header"));
        }
        let name = context.text(CStr::from_ptr(name).to_bytes())?;
        if name.is_empty() || context.columns.iter().any(|column| column.name == name) {
            return Err(Error::Invalid("field_name"));
        }
        let kind = context
            .source
            .types
            .iter()
            .find(|(field, _)| field == &name)
            .map_or(
                if text != 0 {
                    ColumnType::Str
                } else {
                    ColumnType::Float
                },
                |(_, kind)| *kind,
            );
        // SAS types narrow numeric storage. Character values carry no numeric
        // epoch or integer meaning and numeric storage cannot become text.
        if (text != 0) != (kind == ColumnType::Str) {
            return Err(Error::Invalid("field_type"));
        }
        context.columns.push(Column { name, kind });
        Ok(())
    })
}
unsafe extern "C" fn value(
    row: c_int,
    column: c_int,
    text: *const c_char,
    number: f64,
    missing: c_int,
    exact: c_int,
    opaque: *mut c_void,
) -> c_int {
    callback(opaque, |context| {
        if row == 0 && column == 0 {
            context.declarations()?;
        }
        let row = usize::try_from(row).map_err(|_| Error::Invalid("record_order"))?;
        let column = usize::try_from(column).map_err(|_| Error::Invalid("record_order"))?;
        Limits::table(
            row.checked_add(1).ok_or(Error::Invalid("record_order"))?,
            context.columns.len(),
        )?;
        if column == 0 {
            if row != context.rows.len() {
                return Err(Error::Invalid("record_order"));
            }
            context.rows.push(Vec::with_capacity(context.columns.len()));
        }
        if row + 1 != context.rows.len() || context.rows[row].len() != column {
            return Err(Error::Invalid("record_order"));
        }
        let kind = context.columns[column].kind;
        let decoded = if missing != 0 || (text.is_null() && !number.is_finite()) {
            Value::Missing
        } else if !text.is_null() {
            let text = context.text(CStr::from_ptr(text).to_bytes())?;
            if context.source.text_is_missing(&text) {
                Value::Missing
            } else {
                Value::Str(text)
            }
        } else {
            (if exact == 0 && kind != ColumnType::Float {
                None
            } else {
                numeric(number, kind)
            })
            .ok_or_else(|| {
                Error::Typing(yamaa_core::typed_csv::Error::FieldParse {
                    field: context.columns[column].name.clone(),
                    target: kind,
                    value: number.to_string(),
                })
            })?
        };
        context.rows[row].push(decoded);
        Ok(())
    })
}

fn exact_i64(value: f64) -> Option<i64> {
    // The exclusive upper bound prevents saturating f64 -> i64 casts admitting
    // 2^63. This checks the stored number; lost source digits are not recovered.
    (value.is_finite()
        && value.fract() == 0.0
        && (-9_223_372_036_854_775_808.0..9_223_372_036_854_775_808.0).contains(&value))
    .then_some(value as i64)
}
fn numeric(value: f64, kind: ColumnType) -> Option<Value> {
    if kind == ColumnType::Float {
        return Some(Value::float(value));
    }
    let integer = exact_i64(value)?;
    match kind {
        ColumnType::Int => Some(Value::Int(integer)),
        ColumnType::Date => {
            crate::arrow_temporal::date_from_days(integer.checked_sub(3653)?, DatePrecision::Day)
                .map(Value::Date)
        }
        ColumnType::DateTime => crate::arrow_temporal::datetime_from_seconds(
            integer.checked_sub(315_619_200)?,
            DateTimePrecision::Second,
        )
        .map(Value::DateTime),
        ColumnType::Str | ColumnType::Float => None,
    }
}

fn decode_text(bytes: &[u8], encoding: &str) -> Result<String, Error> {
    let mut text = String::new();
    if encoding == "utf-8" {
        text = std::str::from_utf8(bytes)
            .map_err(|_| Error::Invalid("encoding"))?
            .into();
    } else {
        // Undefined CP1252 octets are rejected. Latin-1 and ASCII are explicit
        // mappings, never replacement decoding or host locale behavior.
        const CP1252: [u16; 32] = [
            0x20ac, 0, 0x201a, 0x0192, 0x201e, 0x2026, 0x2020, 0x2021, 0x02c6, 0x2030, 0x0160,
            0x2039, 0x0152, 0, 0x017d, 0, 0, 0x2018, 0x2019, 0x201c, 0x201d, 0x2022, 0x2013,
            0x2014, 0x02dc, 0x2122, 0x0161, 0x203a, 0x0153, 0, 0x017e, 0x0178,
        ];
        for &byte in bytes {
            let scalar = match encoding {
                "ascii" if byte <= 0x7f => u32::from(byte),
                "iso-8859-1" => u32::from(byte),
                "windows-1252" if (0x80..=0x9f).contains(&byte) => {
                    u32::from(CP1252[usize::from(byte - 0x80)])
                }
                "windows-1252" => u32::from(byte),
                _ => return Err(Error::Invalid("encoding")),
            };
            if scalar == 0 {
                return Err(Error::Invalid("encoding"));
            }
            text.push(char::from_u32(scalar).ok_or(Error::Invalid("encoding"))?);
        }
    }
    if text.contains('\0') {
        return Err(Error::Invalid("encoding"));
    }
    Ok(text)
}

fn admit_xport(bytes: &[u8]) -> Result<(), Error> {
    if bytes.len() < 640 || !bytes.len().is_multiple_of(80) {
        return Err(Error::Invalid("framing"));
    }
    let header = |index: usize, kind: &[u8]| {
        bytes
            .get(index * 80..index * 80 + 80)
            .is_some_and(|record| {
                record.starts_with(b"HEADER RECORD*******")
                    && record.get(20..28) == Some(kind)
                    && record.get(28..48) == Some(b"HEADER RECORD!!!!!!!")
            })
    };
    let mut members = 0;
    for index in 0..bytes.len() / 80 {
        let version = if header(index, b"MEMBER  ") {
            5
        } else if header(index, b"MEMBV8  ") {
            8
        } else {
            continue;
        };
        // A header-looking character value is ordinary observation data.
        // Count the complete member descriptor chain, including its NAMESTR
        // framing, rather than treating one magic string as a member.
        if header(
            index + 1,
            if version == 5 {
                b"DSCRPTR "
            } else {
                b"DSCPTV8 "
            },
        ) && bytes.get((index + 2) * 80..(index + 2) * 80 + 8) == Some(b"SAS     ")
            && header(
                index + 4,
                if version == 5 {
                    b"NAMESTR "
                } else {
                    b"NAMSTV8 "
                },
            )
        {
            members += 1;
        }
    }
    if members != 1 {
        return Err(Error::Invalid("members"));
    }
    Ok(())
}

pub fn parse(bytes: &[u8], source: &SourceDeclaration) -> Result<ArrowTable, Error> {
    if !matches!(
        source.encoding.as_deref(),
        Some("utf-8" | "windows-1252" | "iso-8859-1" | "ascii")
    ) {
        return Err(Error::Invalid("encoding"));
    }
    Limits::source_bytes(bytes.len())?;
    if !matches!(
        source.profile,
        SourceProfile::Xport | SourceProfile::Sas7bdat
    ) {
        return Err(Error::Internal);
    }
    if source.profile == SourceProfile::Xport {
        admit_xport(bytes)?;
    }
    // Declaration admission precedes C or source value allocation.
    let mut declared = BTreeSet::new();
    for (name, _) in &source.types {
        if name.is_empty() || !declared.insert(name) {
            return Err(Error::Internal);
        }
    }
    let mut context = Context {
        source,
        columns: Vec::new(),
        rows: Vec::new(),
        declared_rows: None,
        declared_columns: 0,
        error: None,
    };
    let (mut refusal_resource, mut refusal_limit, mut refusal_actual, mut refusal_known) =
        (0, 0, 0, 0);
    let status = unsafe {
        yamaa_readstat_parse(
            bytes.as_ptr(),
            bytes.len(),
            c_int::from(source.profile == SourceProfile::Xport),
            Limits::TABLE_ROWS,
            Limits::COLUMNS,
            Limits::TABLE_CELLS,
            metadata,
            variable,
            value,
            (&mut context as *mut Context<'_>).cast(),
            &mut refusal_resource,
            &mut refusal_limit,
            &mut refusal_actual,
            &mut refusal_known,
        )
    };

    if let Some(error) = context.error {
        return Err(error);
    }
    if status == -2 {
        let resource = match refusal_resource {
            1 => "table_rows",
            2 => "columns",
            3 => "table_cells",
            _ => return Err(Error::Internal),
        };
        return Err(Error::Limit {
            resource,
            limit: refusal_limit,
            actual: (refusal_known != 0).then_some(refusal_actual),
        });
    }
    if status == -6 {
        return Err(Error::Internal);
    }
    if status == -3 {
        return Err(Error::Invalid("compression"));
    }
    if status == -4 {
        return Err(Error::Invalid("encoding"));
    }
    if status == -5 {
        return Err(Error::Invalid("version"));
    }
    if status != 0 {
        return Err(Error::Invalid("malformed"));
    }
    context.declarations()?;
    if context
        .rows
        .last()
        .is_some_and(|row| row.len() != context.columns.len())
        || context
            .declared_rows
            .is_some_and(|rows| rows != context.rows.len())
    {
        return Err(Error::Invalid("truncated"));
    }
    let rows = Rows {
        schema: TableSchema::new(context.columns).map_err(|_| Error::Invalid("header"))?,
        rows: context.rows,
    };
    let batch = crate::ipc_table::sanitized_batch(&rows, 0, rows.row_count())
        .map_err(|_| Error::Internal)?;
    ArrowTable::try_new(rows.schema, vec![batch]).map_err(|_| Error::Internal)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source() -> SourceDeclaration {
        SourceDeclaration {
            name: "SRC".into(),
            path: "fractional.xpt".into(),
            types: vec![],
            profile: SourceProfile::Xport,
            encoding: Some("windows-1252".into()),
            empty_string_present: false,
            ordinal: None,
        }
    }

    #[test]
    fn fixed_header_geometry_is_checked_before_retaining_any_columns_or_rows() {
        let source = source();
        for (rows, columns, expected) in [
            (9_999_999, 1, None),
            (10_000_001, 1, Some(("table_rows", 10_000_001))),
            (1, 1023, None),
            (1, 1025, Some(("columns", 1025))),
            (3_575_418, 179, None),
            (3_575_419, 179, Some(("table_cells", 640_000_001))),
        ] {
            let mut context = Context {
                source: &source,
                columns: Vec::new(),
                rows: Vec::new(),
                declared_rows: None,
                declared_columns: 0,
                error: None,
            };
            let status = unsafe {
                metadata(
                    rows,
                    columns,
                    5,
                    std::ptr::null(),
                    (&mut context as *mut Context<'_>).cast(),
                )
            };
            match (context.error, expected) {
                (None, None) => assert_eq!(status, 0),
                (
                    Some(Error::Limit {
                        resource,
                        actual: Some(actual),
                        ..
                    }),
                    Some(expected),
                ) => {
                    assert_eq!(status, 1);
                    assert_eq!((resource, actual), expected);
                }
                (other, _) => panic!("unexpected source header admission: {other:?}"),
            }
            assert!(context.columns.is_empty());
            assert!(context.rows.is_empty());
        }
    }

    #[test]
    fn native_xport_decodes_more_than_a_million_small_records() {
        // Independently authored v5 layout: 880 header/descriptor bytes then
        // repeated exact IBM cells for 1.5. Counting callbacks exercise the
        // real memory transport without allocating a million Rust table rows.
        const RECORDS: usize = 1_100_000;
        let fixture = include_bytes!("../tests/fixtures/sas/fractional.xpt");
        let mut bytes = fixture[..880].to_vec();
        bytes.extend_from_slice(&fixture[880..888].repeat(RECORDS));
        struct Seen {
            rows: usize,
            invalid: bool,
        }
        unsafe extern "C" fn meta(
            _rows: i64,
            columns: c_int,
            version: c_int,
            _encoding: *const c_char,
            _opaque: *mut c_void,
        ) -> c_int {
            c_int::from(columns != 1 || version != 5)
        }
        unsafe extern "C" fn field(
            column: c_int,
            _name: *const c_char,
            width: usize,
            text: c_int,
            _opaque: *mut c_void,
        ) -> c_int {
            c_int::from(column != 0 || width != 8 || text != 0)
        }
        unsafe extern "C" fn cell(
            row: c_int,
            column: c_int,
            text: *const c_char,
            number: f64,
            missing: c_int,
            exact: c_int,
            opaque: *mut c_void,
        ) -> c_int {
            let seen = &mut *opaque.cast::<Seen>();
            seen.invalid |= usize::try_from(row).ok() != Some(seen.rows)
                || column != 0
                || !text.is_null()
                || number != 1.5
                || missing != 0
                || exact != 1;
            seen.rows = seen.rows.saturating_add(1);
            c_int::from(seen.invalid)
        }
        let mut seen = Seen {
            rows: 0,
            invalid: false,
        };
        let (mut resource, mut limit, mut actual, mut known) = (0, 0, 0, 0);
        let status = unsafe {
            yamaa_readstat_parse(
                bytes.as_ptr(),
                bytes.len(),
                1,
                Limits::TABLE_ROWS,
                Limits::COLUMNS,
                Limits::TABLE_CELLS,
                meta,
                field,
                cell,
                (&mut seen as *mut Seen).cast(),
                &mut resource,
                &mut limit,
                &mut actual,
                &mut known,
            )
        };
        assert_eq!(status, 0);
        assert_eq!(seen.rows, RECORDS);
        assert!(!seen.invalid);
    }

    #[test]
    fn wide_xport_observation_preserves_all_fields_and_exact_text() {
        // The existing independent MEMBER-text fixture supplies v5 framing and
        // one character NAMESTR. Author 513 unique width32767 fields directly:
        // their one observation is 32767 bytes larger than 512 such fields and
        // exceeds the former 16 MiB allocator ceiling while below source policy.
        const COLUMNS: usize = 513;
        const WIDTH: usize = 32767;
        let fixture = include_bytes!("../tests/fixtures/sas/member-text.xpt");
        let mut bytes = fixture[..640].to_vec();
        bytes[613..618].copy_from_slice(b"00513");
        for column in 0..COLUMNS {
            let mut descriptor = fixture[640..780].to_vec();
            descriptor[4..6].copy_from_slice(&(WIDTH as u16).to_be_bytes());
            descriptor[6..8].copy_from_slice(&((column + 1) as u16).to_be_bytes());
            descriptor[8..16].fill(b' ');
            descriptor[8..13].copy_from_slice(format!("C{column:04}").as_bytes());
            descriptor[84..88].copy_from_slice(&((column * WIDTH) as u32).to_be_bytes());
            bytes.extend_from_slice(&descriptor);
        }
        bytes.resize(bytes.len().div_ceil(80) * 80, b' ');
        bytes.extend_from_slice(&fixture[960..1040]);
        let observation = bytes.len();
        bytes.resize(observation + COLUMNS * WIDTH, b' ');
        bytes[observation + (COLUMNS - 1) * WIDTH] = b'!';
        bytes.resize(bytes.len().div_ceil(80) * 80, b' ');
        let source = source();
        let table = parse(&bytes, &source).unwrap();
        assert_eq!(table.row_count(), 1);
        assert_eq!(table.schema().columns().len(), COLUMNS);
        assert_eq!(table.cell(0, 0).unwrap(), ValueRef::Missing);
        assert_eq!(table.cell(0, COLUMNS - 1).unwrap(), ValueRef::Str("!"));
    }

    #[test]
    fn malformed_sas_dimensions_precede_large_row_or_page_allocation() {
        // This independent haven fixture is a 32-bit little-endian SAS file:
        // header/page sizes at 196/200, row-size subheader at 4640, and column
        // attributes at 4084. Its original first numeric width is 8 bytes.
        let fixture = include_bytes!("../tests/fixtures/sas/datetime.sas7bdat");
        assert_eq!(&fixture[196..204], &[0, 4, 0, 0, 0, 16, 0, 0]);
        assert_eq!(&fixture[4660..4664], &40u32.to_le_bytes());
        assert_eq!(&fixture[4100..4104], &8u32.to_le_bytes());
        let mut source = source();
        source.profile = SourceProfile::Sas7bdat;
        source.path = "datetime.sas7bdat".into();
        let mut bad_row = fixture.to_vec();
        bad_row[4660..4664].copy_from_slice(&(64u32 * 1024 * 1024).to_le_bytes());
        bad_row[4100..4104].copy_from_slice(&32768u32.to_le_bytes());
        bad_row[4106] = 2; // Character storage exceeds the SAS signed 16-bit width.
        let mut bad_page = fixture.to_vec();
        bad_page[200..204].copy_from_slice(&(8u32 * 1024 * 1024).to_le_bytes());
        for bytes in [bad_row, bad_page] {
            assert!(matches!(
                parse(&bytes, &source),
                Err(Error::Invalid("malformed"))
            ));
        }
    }

    #[test]
    fn resized_independent_sas_page_above_sixteen_mib_keeps_exact_physical_truth() {
        // Independent little-endian fixture framing: one MIX page, four
        // observations of five 8-byte numeric fields at page offset 184. The
        // stored row cells and every subheader pointer remain at their original
        // offsets; only page size and unused zero padding change.
        let fixture = include_bytes!("../tests/fixtures/sas/datetime.sas7bdat");
        let page_size = 32usize * 1024 * 1024;
        let mut bytes = fixture.to_vec();
        bytes[200..204].copy_from_slice(&(page_size as u32).to_le_bytes());
        bytes.resize(1024 + page_size, 0);
        let mut source = source();
        source.profile = SourceProfile::Sas7bdat;
        source.path = "datetime.sas7bdat".into();
        let table = parse(&bytes, &source).unwrap();
        // Literal physical binary64 truth, read independently from the fixture
        // slots (not obtained from the candidate parser or its display formats).
        let expected = [
            [1_738_507_332.0, 20_121.0, 20_121.0, 20_121.0, 52_932.0],
            [1_704_190_463.0, 19_724.0, 19_724.0, 19_724.0, 36_863.0],
            [1_736_921_722.0, 20_254.0, 20_254.0, 20_254.0, 22_522.0],
            [-356_840_880.0, -4_124.0, -4_124.0, -4_124.0, 77_520.0],
        ];
        assert_eq!(table.row_count(), expected.len());
        assert_eq!(table.schema().columns().len(), 5);
        for (row, values) in expected.into_iter().enumerate() {
            for (column, value) in values.into_iter().enumerate() {
                assert_eq!(
                    table.cell(row, column).unwrap(),
                    ValueRef::Float(yamaa_core::value::FiniteFloat::new(value).unwrap()),
                );
            }
        }
    }

    #[test]
    fn valid_sas_metadata_above_four_thousand_blobs_keeps_exact_cells() {
        // Repack the independent haven MIX page using its documented u32
        // offsets/lengths and twelve-byte subheader pointers. Keep the original
        // COLUMN_TEXT blob first so every existing uint16 reference is stable.
        // Duplicate it 4096 times as valid unused text metadata. pandas 2.3.3's
        // independent SAS reader accepts this 525312-byte file and all four rows.
        const EXTRA: usize = 4096;
        const ORIGINAL: usize = 13;
        const PAGE: usize = 524288;
        let fixture = include_bytes!("../tests/fixtures/sas/datetime.sas7bdat");
        let original = &fixture[1024..];
        let mut page = vec![0; PAGE];
        page[..24].copy_from_slice(&original[..24]);
        page[18..20].copy_from_slice(&((17 + EXTRA) as u16).to_le_bytes());
        page[20..22].copy_from_slice(&((ORIGINAL + EXTRA) as u16).to_le_bytes());
        let row_start = (24 + (ORIGINAL + EXTRA) * 12).div_ceil(8) * 8;
        page[row_start..row_start + 160].copy_from_slice(&original[184..344]);
        let mut position = row_start + 160;
        for index in 0..ORIGINAL + EXTRA {
            position = position.div_ceil(8) * 8;
            let (blob, flag) = if index < ORIGINAL {
                let pointer = 24 + index * 12;
                let offset =
                    u32::from_le_bytes(original[pointer..pointer + 4].try_into().unwrap()) as usize;
                let length =
                    u32::from_le_bytes(original[pointer + 4..pointer + 8].try_into().unwrap())
                        as usize;
                (
                    &original[offset..offset + length],
                    &original[pointer + 8..pointer + 12],
                )
            } else {
                (&original[3200..3300], &original[68..72])
            };
            let pointer = 24 + index * 12;
            page[pointer..pointer + 4].copy_from_slice(&(position as u32).to_le_bytes());
            page[pointer + 4..pointer + 8].copy_from_slice(&(blob.len() as u32).to_le_bytes());
            page[pointer + 8..pointer + 12].copy_from_slice(flag);
            page[position..position + blob.len()].copy_from_slice(blob);
            position += blob.len();
        }
        let mut bytes = fixture[..1024].to_vec();
        bytes[200..204].copy_from_slice(&(PAGE as u32).to_le_bytes());
        bytes.extend_from_slice(&page);
        assert_eq!(bytes.len(), 525312);
        let mut source = source();
        source.profile = SourceProfile::Sas7bdat;
        source.path = "datetime.sas7bdat".into();
        let table = parse(&bytes, &source).unwrap();
        let expected = [
            [1_738_507_332.0, 20_121.0, 20_121.0, 20_121.0, 52_932.0],
            [1_704_190_463.0, 19_724.0, 19_724.0, 19_724.0, 36_863.0],
            [1_736_921_722.0, 20_254.0, 20_254.0, 20_254.0, 22_522.0],
            [-356_840_880.0, -4_124.0, -4_124.0, -4_124.0, 77_520.0],
        ];
        assert_eq!(table.row_count(), expected.len());
        assert_eq!(table.schema().columns().len(), 5);
        for (row, values) in expected.into_iter().enumerate() {
            for (column, value) in values.into_iter().enumerate() {
                assert_eq!(
                    table.cell(row, column).unwrap(),
                    ValueRef::Float(yamaa_core::value::FiniteFloat::new(value).unwrap()),
                );
            }
        }
    }

    #[test]
    fn semantic_and_encoding_refusals_keep_their_exact_causes() {
        let bytes = include_bytes!("../tests/fixtures/sas/fractional.xpt");
        let mut source = source();
        source.types = vec![("NUM".into(), ColumnType::Int)];
        assert!(matches!(
            parse(bytes, &source),
            Err(Error::Typing(
                yamaa_core::typed_csv::Error::FieldParse { .. }
            ))
        ));
        source.encoding = None;
        assert!(matches!(
            parse(bytes, &source),
            Err(Error::Invalid("encoding"))
        ));
        source.encoding = Some("windows-1252".into());
        source.profile = SourceProfile::Csv;
        assert!(matches!(parse(bytes, &source), Err(Error::Internal)));
    }

    #[test]
    fn numeric_narrowing_has_exact_bounds_and_epoch() {
        assert_eq!(
            numeric(0.0, ColumnType::Date),
            Some(Value::Date("1960-01-01".parse().unwrap()))
        );
        assert_eq!(
            numeric(-1.0, ColumnType::DateTime),
            Some(Value::DateTime("1959-12-31T23:59:59".parse().unwrap()))
        );
        for kind in [ColumnType::Int, ColumnType::Date, ColumnType::DateTime] {
            assert_eq!(numeric(1.5, kind), None);
            assert_eq!(numeric(f64::INFINITY, kind), None);
        }
        assert_eq!(exact_i64(9_223_372_036_854_775_808.0), None);
        assert_eq!(exact_i64(-9_223_372_036_854_775_808.0), Some(i64::MIN));
    }
    #[test]
    fn declared_decoders_reject_incomplete_or_undefined_text() {
        assert_eq!(decode_text(b"caf\xe9", "windows-1252").unwrap(), "café");
        assert!(decode_text(b"\xc3", "utf-8").is_err());
        assert!(decode_text(b"\x81", "windows-1252").is_err());
        assert!(decode_text(b"\xff", "ascii").is_err());
        assert!(decode_text(b"a\0b", "utf-8").is_err());
    }
}
