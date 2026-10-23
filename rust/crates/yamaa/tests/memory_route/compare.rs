//! The file route's comparison, mirrored from
//! `tools/conformance/public_conformance.py` (`compare_example`): typed-table
//! equality against the authored expected artifacts, byte equality of saved
//! CSV, a typed comparison of saved Parquet, and diagnostic matching for
//! negative benchmarks. Tables are scalar-encoded exactly like the runner's
//! `scalar`: text as UTF-8 hex, integers as decimal, floats as binary64 bit
//! hex, dates as epoch days, datetimes as epoch seconds; rows compare in
//! order.
//!
//! Authored CSV is read by this file's own cell parser, never by the engine.
//! Parquet, which only two benchmarks write, is read by the engine's own
//! Parquet source reader, which shares no code with the writer that saved
//! the artifact; the conformance runners compare those files with an
//! independent reader in both hosts.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use yamaa::{Array, DataType, RecordBatch, Table, TimeUnit};
use yamaa_core::json::Value as Json;
use yamaa_core::schema::{Document, DocumentNode};

/// A YAML document as a JSON value, read through the engine's YAML reader
/// as an authored input, never as truth.
pub fn yaml_json(bytes: &[u8]) -> Json {
    fn value(document: &Document, id: usize) -> Json {
        match &document.nodes()[id] {
            DocumentNode::Null => Json::Null,
            DocumentNode::Boolean(value) => Json::from(*value),
            DocumentNode::Integer(value) => yamaa_adapters::json_wire::decode(value).unwrap(),
            DocumentNode::Float(value) => {
                Json::Number(yamaa_core::json::Number::from_f64(*value).expect("finite float"))
            }
            DocumentNode::Text(value) => Json::from(value.as_str()),
            DocumentNode::Sequence(items) => {
                Json::Array(items.iter().map(|&id| value(document, id)).collect())
            }
            DocumentNode::Mapping(items) => Json::Object(
                items
                    .iter()
                    .map(|&(key, child)| {
                        let DocumentNode::Text(key) = &document.nodes()[key] else {
                            panic!("mapping keys must be text")
                        };
                        (key.clone(), value(document, child))
                    })
                    .collect(),
            ),
        }
    }
    let decoded = yamaa_adapters::yaml_decode::decode_yaml(bytes).unwrap();
    value(&decoded.document, decoded.document.root())
}

/// One authored CSV cell: `None` is an unquoted empty field (missing); a
/// quoted empty field is `Some("")`, matching the conformance reader.
pub fn csv_cells(text: &str) -> Vec<Vec<Option<String>>> {
    let mut rows = Vec::new();
    let mut row: Vec<Option<String>> = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut in_quotes = false;
    let mut start = true;
    let chars: Vec<char> = text.chars().collect();
    let mut index = 0;
    while index < chars.len() {
        let char = chars[index];
        if in_quotes {
            if char == '"' && index + 1 < chars.len() && chars[index + 1] == '"' {
                field.push('"');
                index += 1;
            } else if char == '"' {
                in_quotes = false;
            } else {
                field.push(char);
            }
        } else if char == '"' && start {
            quoted = true;
            in_quotes = true;
            start = false;
        } else if char == ',' || char == '\r' || char == '\n' {
            row.push(if !field.is_empty() || quoted {
                Some(std::mem::take(&mut field))
            } else {
                None
            });
            field.clear();
            quoted = false;
            start = true;
            if char != ',' {
                rows.push(std::mem::take(&mut row));
                if char == '\r' && index + 1 < chars.len() && chars[index + 1] == '\n' {
                    index += 1;
                }
            }
        } else {
            field.push(char);
            start = false;
        }
        index += 1;
    }
    if !field.is_empty() || quoted || !row.is_empty() {
        row.push(if !field.is_empty() || quoted {
            Some(field)
        } else {
            None
        });
        rows.push(row);
    }
    rows
}

/// A path as a declaring file names it, resolved from that file's directory
/// to a case-relative POSIX path.
pub fn normalize(declaring: &str, written: &str) -> String {
    let mut parts: Vec<String> = declaring
        .rsplit_once('/')
        .map(|(base, _)| {
            base.split('/')
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default();
    for part in written.split('/') {
        match part {
            "" | "." => (),
            ".." => {
                parts.pop();
            }
            name => parts.push(name.to_string()),
        }
    }
    parts.join("/")
}

/// The declared output files of one specification.
pub struct OutputDecl {
    pub path: Option<String>,
    pub report: Option<String>,
    pub decimals: Option<u32>,
}

/// Independent declared types and output declarations of one entry
/// specification, composed over its `parent` chain and the fragments its
/// layers include exactly like the conformance runner's `declarations` (types
/// and output paths, never
/// derivations). A copy entry declares each column it lists with the type its
/// producer declares: the specification the input's final `schema` link names,
/// from the base its final `relative_to` selects (SPEC-9).
pub fn declarations_for(
    case: &Path,
    specification: &str,
) -> Result<(BTreeMap<String, String>, OutputDecl), String> {
    declarations(&case.join(specification), &mut Vec::new())
}

/// The layers of the composition whose entry is `path`, in contribution
/// order: at each layer its parent chain, then the fragments it includes, in
/// order, then the layer itself (LAYER-3). Each layer of the chain is pushed
/// onto `active`.
fn chain(path: &Path, active: &mut Vec<PathBuf>) -> Result<Vec<(PathBuf, Json)>, String> {
    let canonical = path
        .canonicalize()
        .map_err(|error| format!("{}: {error}", path.display()))?;
    if active.contains(&canonical) {
        return Err(format!("cyclic declarations at {}", path.display()));
    }
    active.push(canonical);
    let bytes = fs::read(path).map_err(|error| format!("{}: {error}", path.display()))?;
    let doc = yaml_json(&bytes);
    let mut layers = match doc.get("parent").and_then(Json::as_str) {
        Some(parent) => chain(
            &path
                .parent()
                .expect("specification has a directory")
                .join(parent),
            active,
        )?,
        None => Vec::new(),
    };
    if let Some(Json::Array(include)) = doc.get("include") {
        for written in include.iter().filter_map(Json::as_str) {
            let fragment = path
                .parent()
                .expect("specification has a directory")
                .join(written);
            let bytes =
                fs::read(&fragment).map_err(|error| format!("{}: {error}", fragment.display()))?;
            layers.push((fragment, yaml_json(&bytes)));
        }
    }
    layers.push((path.to_path_buf(), doc));
    Ok(layers)
}

fn declarations(
    path: &Path,
    active: &mut Vec<PathBuf>,
) -> Result<(BTreeMap<String, String>, OutputDecl), String> {
    let depth = active.len();
    let layers = chain(path, active)?;
    let mut output = OutputDecl {
        path: None,
        report: None,
        decimals: None,
    };
    // Each input's `schema` link, as its declaring directory and spelling,
    // and whether it resolves from the entry, composed over every layer
    // before any copy is typed.
    let mut links: BTreeMap<String, (Option<(PathBuf, String)>, bool)> = BTreeMap::new();
    for (layer, doc) in &layers {
        if let Some(Json::Array(clears)) = doc.get("clear") {
            for clear in clears.iter().filter_map(Json::as_str) {
                if let ["input", name, field] = clear.split('.').collect::<Vec<_>>()[..] {
                    if let Some(link) = links.get_mut(name) {
                        match field {
                            "schema" => link.0 = None,
                            "relative_to" => link.1 = false,
                            _ => {}
                        }
                    }
                }
            }
        }
        if let Some(Json::Object(inputs)) = doc.get("input") {
            for (name, declaration) in inputs {
                if !declaration.is_object() {
                    continue;
                }
                let link = links.entry(name.clone()).or_default();
                if let Some(schema) = declaration.get("schema").and_then(Json::as_str) {
                    let base = layer.parent().expect("specification has a directory");
                    link.0 = Some((base.to_path_buf(), schema.to_owned()));
                }
                if let Some(base) = declaration.get("relative_to").and_then(Json::as_str) {
                    link.1 = base == "entry";
                }
            }
        }
        if let Some(out) = doc.get("output") {
            let text = |key: &str| out.get(key).and_then(Json::as_str).map(str::to_owned);
            if let Some(value) = text("path") {
                output.path = Some(value);
            }
            if let Some(value) = text("report") {
                output.report = Some(value);
            }
            if let Some(number) = out.get("decimals").and_then(Json::as_u64) {
                output.decimals = u32::try_from(number).ok();
            }
        }
    }
    let mut types = BTreeMap::new();
    for (_, doc) in &layers {
        let Some(Json::Array(columns)) = doc.get("columns") else {
            continue;
        };
        for column in columns {
            if let (Some(name), Some(kind)) = (
                column.get("name").and_then(Json::as_str),
                column.get("type").and_then(Json::as_str),
            ) {
                types.insert(name.to_string(), kind.to_string());
            }
            let Some(copy) = column.get("copy") else {
                continue;
            };
            let link = copy
                .get("dataset")
                .and_then(Json::as_str)
                .and_then(|dataset| links.get(dataset));
            let Some((Some((base, written)), from_entry)) = link else {
                continue;
            };
            let base = if *from_entry {
                path.parent().expect("specification has a directory")
            } else {
                base.as_path()
            };
            let (copied, _) = declarations(&base.join(written), active)?;
            if let Some(Json::Array(names)) = copy.get("columns") {
                for name in names.iter().filter_map(Json::as_str) {
                    if let Some(kind) = copied.get(name) {
                        types.insert(name.to_string(), kind.clone());
                    }
                }
            }
        }
    }
    active.truncate(depth);
    Ok((types, output))
}

/// Days from civil date (Howard Hinnant's algorithm), the proleptic
/// Gregorian calendar Python's `datetime.date` arithmetic uses.
fn days_from_civil(year: i64, month: u32, day: u32) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let yoe = year - era * 400;
    let mp = i64::from((month + 9) % 12);
    let doy = (153 * mp + 2) / 5 + i64::from(day) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

fn days_from_iso_date(text: &str) -> Result<i64, String> {
    let mut parts = text.trim().split('-');
    let mut part = || {
        parts
            .next()
            .and_then(|part| part.parse::<i64>().ok())
            .ok_or_else(|| format!("bad date {text:?}"))
    };
    let (year, month, day) = (part()?, part()?, part()?);
    let field = |value: i64| u32::try_from(value).map_err(|_| format!("bad date {text:?}"));
    Ok(days_from_civil(year, field(month)?, field(day)?))
}

/// Epoch seconds of an ISO datetime, treating the wall time as UTC exactly
/// like the runner (`replace(tzinfo=utc)` discards any offset).
fn datetime_epoch_seconds(text: &str) -> Result<i64, String> {
    let text = text.trim();
    let (date, rest) = text
        .split_once(['T', ' '])
        .ok_or_else(|| format!("bad datetime {text:?}"))?;
    let days = days_from_iso_date(date)?;
    // The wall time runs to the first offset or Z marker; a negative
    // offset's '-' can only appear after the HH:MM:SS prefix.
    let end = rest.find(['+', 'Z', 'z']).unwrap_or(rest.len());
    let mut time = &rest[..end];
    if time.len() > 8 {
        if let Some(offset) = time[8..].find('-') {
            time = &time[..8 + offset];
        }
    }
    let mut parts = time.split(':');
    let mut part = || {
        parts
            .next()
            .map(|part| part.split('.').next().unwrap_or(part))
            .and_then(|part| part.parse::<i64>().ok())
            .ok_or_else(|| format!("bad datetime {text:?}"))
    };
    let (hour, minute, second) = (part()?, part()?, part()?);
    Ok(days * 86_400 + hour * 3_600 + minute * 60 + second)
}

/// Parse float text with Python `float()` spellings (Rust's parser plus the
/// SAS-style dotted non-finite forms).
fn parse_float_text(text: &str) -> Result<f64, String> {
    let trimmed = text.trim();
    if let Ok(value) = trimmed.parse::<f64>() {
        return Ok(value);
    }
    let lowered = trimmed.to_ascii_lowercase();
    let (sign, body) = match lowered.strip_prefix('-') {
        Some(rest) => (-1.0, rest),
        None => (1.0, lowered.strip_prefix('+').unwrap_or(&lowered)),
    };
    match body {
        ".inf" | ".infinity" => Ok(sign * f64::INFINITY),
        ".nan" => Ok(f64::NAN),
        _ => Err(format!("float text {text:?} does not parse")),
    }
}

/// One scalar cell in the runner's encoding (see the module comment).
fn scalar_text(text: &str, kind: &str) -> Result<String, String> {
    Ok(match kind {
        "str" => text.bytes().map(|byte| format!("{byte:02x}")).collect(),
        "int" => text
            .trim()
            .parse::<i64>()
            .map(|value| value.to_string())
            .map_err(|error| format!("int text {text:?}: {error}"))?,
        "float" => format!("{:016x}", parse_float_text(text)?.to_bits()),
        "date" => days_from_iso_date(text)?.to_string(),
        "datetime" => datetime_epoch_seconds(text)?.to_string(),
        other => return Err(format!("unknown declared type {other}")),
    })
}

/// A minimal big unsigned integer for the exact CSV-9 projection.
struct BigUint(Vec<u64>);

impl BigUint {
    fn mul_small(&mut self, factor: u64) {
        let mut carry: u128 = 0;
        for limb in &mut self.0 {
            let total = u128::from(*limb) * u128::from(factor) + carry;
            *limb = total as u64;
            carry = total >> 64;
        }
        if carry > 0 {
            self.0.push(carry as u64);
        }
    }

    fn shl(&mut self, bits: u32) {
        let limbs = (bits / 64) as usize;
        let rest = bits % 64;
        if rest > 0 {
            let mut carry = 0u64;
            for limb in &mut self.0 {
                let moved = (*limb << rest) | carry;
                carry = *limb >> (64 - rest);
                *limb = moved;
            }
            if carry > 0 {
                self.0.push(carry);
            }
        }
        if limbs > 0 {
            let mut shifted = vec![0u64; limbs];
            shifted.extend_from_slice(&self.0);
            self.0 = shifted;
        }
    }

    fn shr(&mut self, bits: u32) {
        let limbs = (bits / 64) as usize;
        if limbs >= self.0.len() {
            self.0 = vec![0];
            return;
        }
        self.0.drain(..limbs);
        let rest = bits % 64;
        if rest > 0 {
            for index in 0..self.0.len() {
                let high = if index + 1 < self.0.len() {
                    self.0[index + 1] << (64 - rest)
                } else {
                    0
                };
                self.0[index] = (self.0[index] >> rest) | high;
            }
        }
        while self.0.len() > 1 && self.0.last() == Some(&0) {
            self.0.pop();
        }
    }

    /// Add 2^index, extending the limbs as needed.
    fn add_power_of_two(&mut self, index: u32) {
        let start = (index / 64) as usize;
        if self.0.len() <= start {
            self.0.resize(start + 1, 0);
        }
        let mut carry = u128::from(1u64 << (index % 64));
        for limb in &mut self.0[start..] {
            let total = u128::from(*limb) + carry;
            *limb = total as u64;
            carry = total >> 64;
            if carry == 0 {
                break;
            }
        }
        if carry > 0 {
            self.0.push(carry as u64);
        }
    }

    fn is_zero(&self) -> bool {
        self.0.iter().all(|limb| *limb == 0)
    }

    fn to_decimal(&self) -> String {
        if self.is_zero() {
            return "0".to_string();
        }
        let mut limbs = self.0.clone();
        let mut chunks = Vec::new();
        while limbs.iter().any(|limb| *limb != 0) {
            let mut remainder: u128 = 0;
            for index in (0..limbs.len()).rev() {
                let current = (remainder << 64) | u128::from(limbs[index]);
                limbs[index] = (current / 1_000_000_000) as u64;
                remainder = current % 1_000_000_000;
            }
            chunks.push(remainder as u64);
            while limbs.len() > 1 && limbs.last() == Some(&0) {
                limbs.pop();
            }
        }
        let mut text = chunks.last().copied().unwrap_or(0).to_string();
        for chunk in chunks.iter().rev().skip(1) {
            text.push_str(&format!("{chunk:09}"));
        }
        text
    }
}

/// CSV-9 reported-precision projection of one binary64 value, mirroring the
/// runner's `reported_table`: the exact integer ratio is rounded to
/// `decimals` places under FORMULA-16's interval (ties, and near ties within
/// 2^-26 below a scaled tie, away from zero), then read back as a float.
fn project_reported(value: f64, decimals: u32) -> f64 {
    if !value.is_finite() {
        return value;
    }
    let bits = value.to_bits();
    let negative = bits >> 63 == 1;
    let exponent = ((bits >> 52) & 0x7ff) as i64;
    let fraction = bits & ((1u64 << 52) - 1);
    let (mantissa, shift) = if exponent == 0 {
        (fraction, -1074)
    } else {
        (fraction | (1u64 << 52), exponent - 1075)
    };
    if mantissa == 0 {
        return 0.0;
    }
    // scaled = |value| * 10^decimals = mantissa * 2^shift * 10^decimals
    let mut scaled = BigUint(vec![mantissa]);
    if shift > 0 {
        scaled.shl(shift as u32);
    }
    for _ in 0..decimals {
        scaled.mul_small(10);
    }
    let rounded = if shift >= 0 {
        scaled
    } else {
        let k = (-shift) as u32;
        // With denominator 2^k, remainder / 2^k >= 1/2 - 2^-26 holds exactly
        // when remainder + 2^(k-1) + 2^(k-26) reaches 2^k. Below k = 26 the
        // integer remainder makes the 2^(k-26) term vanish.
        let mut quotient = scaled;
        quotient.add_power_of_two(k - 1);
        if k >= 26 {
            quotient.add_power_of_two(k - 26);
        }
        quotient.shr(k);
        quotient
    };
    let digits = rounded.to_decimal();
    let text = if decimals == 0 {
        digits
    } else {
        let width = decimals as usize + 1;
        let padded = format!("{digits:0>width$}");
        let split = padded.len() - decimals as usize;
        format!("{}.{}", &padded[..split], &padded[split..])
    };
    let magnitude: f64 = text.parse().expect("projected decimal parses");
    if negative && !rounded.is_zero() {
        -magnitude
    } else {
        magnitude
    }
}

/// A typed table in the runner's comparable shape: `(name, kind)` columns
/// and scalar-encoded cells, row order significant.
#[derive(PartialEq, Debug)]
pub struct TypedTable {
    pub columns: Vec<(String, String)>,
    pub rows: Vec<Vec<Option<String>>>,
}

impl TypedTable {
    pub fn describe_diff(&self, expected: &TypedTable) -> String {
        if self.columns != expected.columns {
            return format!(
                "columns differ: expected {:?}, actual {:?}",
                expected.columns, self.columns
            );
        }
        if self.rows.len() != expected.rows.len() {
            return format!(
                "row counts differ: expected {}, actual {}",
                expected.rows.len(),
                self.rows.len()
            );
        }
        for (index, (mine, theirs)) in self.rows.iter().zip(&expected.rows).enumerate() {
            if mine != theirs {
                return format!("row {index} differs: expected {theirs:?}, actual {mine:?}");
            }
        }
        "tables differ".to_string()
    }
}

/// One physical cell, before scalar encoding.
enum CellValue {
    Missing,
    Text(String),
    Int(i64),
    Float(f64),
    Date(i64),
    DateTime(i64),
}

/// The cells of one public table batch: `Utf8` text, `Int64`, `Float64`,
/// `Date32` days and `TimestampSecond` seconds, as the facade commits them.
fn batch_cells(batch: &RecordBatch) -> Result<Vec<Vec<CellValue>>, String> {
    use yamaa::{Date32Array, Float64Array, Int64Array, StringArray, TimestampSecondArray};
    let mut rows: Vec<Vec<CellValue>> = (0..batch.num_rows()).map(|_| Vec::new()).collect();
    for array in batch.columns() {
        let cell = |row: usize| -> Result<CellValue, String> {
            if array.is_null(row) {
                return Ok(CellValue::Missing);
            }
            let any = array.as_any();
            Ok(match array.data_type() {
                DataType::Utf8 => CellValue::Text(
                    any.downcast_ref::<StringArray>()
                        .unwrap()
                        .value(row)
                        .to_owned(),
                ),
                DataType::Int64 => {
                    CellValue::Int(any.downcast_ref::<Int64Array>().unwrap().value(row))
                }
                DataType::Float64 => {
                    CellValue::Float(any.downcast_ref::<Float64Array>().unwrap().value(row))
                }
                DataType::Date32 => CellValue::Date(i64::from(
                    any.downcast_ref::<Date32Array>().unwrap().value(row),
                )),
                DataType::Timestamp(TimeUnit::Second, None) => CellValue::DateTime(
                    any.downcast_ref::<TimestampSecondArray>()
                        .unwrap()
                        .value(row),
                ),
                other => return Err(format!("unsupported public table type {other}")),
            })
        };
        for (row, cells) in rows.iter_mut().enumerate() {
            cells.push(cell(row)?);
        }
    }
    Ok(rows)
}

/// Encode one physical cell under a declared kind, the coercions the
/// runner's `scalar` applies to a host or Parquet value.
fn encode_cell(cell: CellValue, kind: &str) -> Result<Option<String>, String> {
    Ok(match cell {
        CellValue::Missing => None,
        CellValue::Text(text) => Some(scalar_text(&text, kind)?),
        CellValue::Int(value) => Some(match kind {
            "int" => value.to_string(),
            "float" => format!("{:016x}", (value as f64).to_bits()),
            "str" => scalar_text(&value.to_string(), "str")?,
            other => return Err(format!("cannot encode an integer as {other}")),
        }),
        CellValue::Float(value) => Some(match kind {
            "float" => format!("{:016x}", value.to_bits()),
            "int" => (value as i64).to_string(),
            other => return Err(format!("cannot encode a float as {other}")),
        }),
        CellValue::Date(days) => Some(match kind {
            "date" => days.to_string(),
            other => return Err(format!("cannot encode a date as {other}")),
        }),
        CellValue::DateTime(seconds) => Some(match kind {
            "datetime" => seconds.to_string(),
            other => return Err(format!("cannot encode a datetime as {other}")),
        }),
    })
}

fn typed_from_cells(
    cells: impl IntoIterator<Item = Vec<CellValue>>,
    columns: Vec<(String, String)>,
    decimals: Option<u32>,
) -> Result<TypedTable, String> {
    let mut rows = Vec::new();
    for row in cells {
        if row.len() != columns.len() {
            return Err("row width differs from the column list".to_string());
        }
        let mut encoded = Vec::with_capacity(row.len());
        for (cell, (_, kind)) in row.into_iter().zip(&columns) {
            let cell = match (cell, decimals) {
                (CellValue::Float(value), Some(places)) if kind == "float" => {
                    CellValue::Float(project_reported(value, places))
                }
                (cell, _) => cell,
            };
            encoded.push(encode_cell(cell, kind)?);
        }
        rows.push(encoded);
    }
    Ok(TypedTable { columns, rows })
}

/// A host table as the runner reads it: column kinds from the table's own
/// schema, floats through the CSV-9 projection when the specification
/// declares reported precision.
pub fn typed_from_table(table: &Table, decimals: Option<u32>) -> Result<TypedTable, String> {
    let mut columns = Vec::new();
    for field in table.columns() {
        let kind = match field.data_type() {
            DataType::Utf8 => "str",
            DataType::Int64 => "int",
            DataType::Float64 => "float",
            DataType::Date32 => "date",
            DataType::Timestamp(_, _) => "datetime",
            other => return Err(format!("unsupported host table type {other}")),
        };
        columns.push((field.name().clone(), kind.to_string()));
    }
    let mut cells = Vec::new();
    for batch in table.batches() {
        cells.extend(batch_cells(batch)?);
    }
    typed_from_cells(cells, columns, decimals)
}

/// A table's columns, each with its kind.
type Columns = Vec<(String, String)>;

/// A table's cells, row by row.
type Cells = Vec<Vec<CellValue>>;

/// A Parquet file's columns, with their kinds, and its cells, read by the
/// engine's Parquet source reader.
fn parquet_cells(bytes: &[u8]) -> Result<(Columns, Cells), String> {
    use yamaa_core::table::{TableAccess, ValueRef};
    use yamaa_core::value::ColumnType;
    let table = yamaa_adapters::parquet_source::parse(bytes)
        .map_err(|error| format!("parquet reader: {error:?}"))?;
    let columns = table
        .schema()
        .columns()
        .iter()
        .map(|column| {
            let kind = match column.kind {
                ColumnType::Str => "str",
                ColumnType::Int => "int",
                ColumnType::Float => "float",
                ColumnType::Date => "date",
                ColumnType::DateTime => "datetime",
            };
            (column.name.clone(), kind.to_owned())
        })
        .collect::<Vec<_>>();
    let mut rows = Vec::with_capacity(table.row_count());
    for row in 0..table.row_count() {
        let mut cells = Vec::with_capacity(columns.len());
        for column in 0..columns.len() {
            let cell = table
                .cell(row, column)
                .map_err(|error| format!("parquet cell: {error:?}"))?;
            cells.push(match cell {
                ValueRef::Missing => CellValue::Missing,
                ValueRef::Str(text) => CellValue::Text(text.to_owned()),
                ValueRef::Int(value) => CellValue::Int(value),
                ValueRef::Float(value) => CellValue::Float(value.get()),
                ValueRef::Date(date) => {
                    CellValue::Date(i64::from(yamaa_adapters::arrow_temporal::date_days(date)))
                }
                ValueRef::DateTime(datetime) => {
                    CellValue::DateTime(yamaa_adapters::arrow_temporal::datetime_seconds(datetime))
                }
                ValueRef::Bool(_) => return Err("a Parquet column holds booleans".to_owned()),
            });
        }
        rows.push(cells);
    }
    Ok((columns, rows))
}

/// A Parquet artifact's column names and the kinds its file declares.
pub fn parquet_schema(bytes: &[u8]) -> Result<Vec<(String, String)>, String> {
    parquet_cells(bytes).map(|(columns, _)| columns)
}

/// A Parquet artifact's typed table under the declared kinds (the runner's
/// `expected_table` for Parquet), used for both the expected file and the
/// saved artifact.
pub fn typed_from_parquet(
    bytes: &[u8],
    types: &BTreeMap<String, String>,
) -> Result<TypedTable, String> {
    let (stored, cells) = parquet_cells(bytes)?;
    let mut columns = Vec::with_capacity(stored.len());
    for (name, _) in stored {
        let kind = types
            .get(&name)
            .ok_or_else(|| format!("no declared type for Parquet column {name}"))?;
        columns.push((name, kind.clone()));
    }
    typed_from_cells(cells, columns, None)
}

/// An expected or saved CSV artifact as a typed table under the given kinds
/// (the runner's `expected_table` for CSV).
pub fn typed_from_csv(
    bytes: &[u8],
    types: &BTreeMap<String, String>,
) -> Result<TypedTable, String> {
    let text = std::str::from_utf8(bytes).map_err(|error| format!("not UTF-8: {error}"))?;
    let cells = csv_cells(text);
    let (header, body) = cells.split_first().ok_or_else(|| "empty CSV".to_string())?;
    let mut columns = Vec::new();
    for name in header {
        let name = name
            .clone()
            .ok_or_else(|| "missing header cell".to_string())?;
        let kind = types
            .get(&name)
            .ok_or_else(|| format!("no declared type for column {name}"))?;
        columns.push((name, kind.clone()));
    }
    let mut rows = Vec::new();
    for row in body {
        if row.len() != columns.len() {
            return Err("ragged CSV row".to_string());
        }
        let mut encoded = Vec::with_capacity(row.len());
        for (value, (_, kind)) in row.iter().zip(&columns) {
            encoded.push(match value {
                None => None,
                Some(text) => Some(scalar_text(text, kind)?),
            });
        }
        rows.push(encoded);
    }
    Ok(TypedTable { columns, rows })
}

/// The kinds the runner assigns a run report artifact's columns: integers
/// for the two count columns, text for everything else.
pub fn report_types(case: &Path, truth_name: &str) -> Result<BTreeMap<String, String>, String> {
    let truth = case.join("expected").join(truth_name);
    let bytes = fs::read(&truth).map_err(|error| format!("{truth_name}: {error}"))?;
    let names: Vec<String> = if truth_name.ends_with(".parquet") {
        parquet_schema(&bytes)?
            .into_iter()
            .map(|(name, _)| name)
            .collect()
    } else {
        let text = std::str::from_utf8(&bytes).map_err(|error| format!("not UTF-8: {error}"))?;
        csv_cells(text)
            .first()
            .ok_or_else(|| "empty report CSV".to_string())?
            .iter()
            .map(|cell| cell.clone().unwrap_or_default())
            .collect()
    };
    Ok(names
        .into_iter()
        .map(|name| {
            let kind = if name == "COUNT" || name == "TOTAL" {
                "int"
            } else {
                "str"
            };
            (name, kind.to_string())
        })
        .collect())
}

fn deep_equal(want: &Json, got: &Json) -> bool {
    match (want, got) {
        (Json::Number(a), Json::Number(b)) => a.as_f64() == b.as_f64(),
        (Json::Array(a), Json::Array(b)) => {
            a.len() == b.len() && a.iter().zip(b).all(|(x, y)| deep_equal(x, y))
        }
        (Json::Object(a), Json::Object(b)) => {
            a.len() == b.len()
                && a.iter()
                    .all(|(key, value)| b.get(key).is_some_and(|other| deep_equal(value, other)))
        }
        _ => want == got,
    }
}

fn context_subset(want: &Json, got: &Json) -> bool {
    match (want, got) {
        (Json::Object(w), Json::Object(g)) => w
            .iter()
            .all(|(key, value)| g.get(key).is_some_and(|other| context_subset(value, other))),
        _ => deep_equal(want, got),
    }
}

/// The runner's `diagnostic_matches`: phase, condition and spec_paths
/// exactly; rule when the expectation declares one; the expected context a
/// recursive subset of the reported one.
pub fn diagnostic_matches(want: &Json, got: &Json) -> bool {
    for field in ["phase", "condition", "spec_paths"] {
        let fallback = Json::Null;
        if !deep_equal(
            want.get(field).unwrap_or(&fallback),
            got.get(field).unwrap_or(&fallback),
        ) {
            return false;
        }
    }
    if let Some(rule) = want.get("rule") {
        if got.get("rule") != Some(rule) {
            return false;
        }
    }
    let empty = Json::Object(yamaa_core::json::Map::new());
    context_subset(
        want.get("context").unwrap_or(&empty),
        got.get("context").unwrap_or(&empty),
    )
}

/// One issue record as the runner compares it, in brief.
pub fn brief_record(record: &Json) -> String {
    let field = |name: &str| {
        record
            .get(name)
            .map(yamaa_adapters::json_wire::encode)
            .unwrap_or_else(|| "null".to_owned())
    };
    format!(
        "phase={} condition={} rule={} spec_paths={}",
        field("phase"),
        field("condition"),
        field("rule"),
        field("spec_paths")
    )
}

/// Every row of an issues table as the record the runner compares: its
/// phase, condition, rule and spec paths, and its `context` JSON text parsed
/// back into a value. Columns are read by name.
pub fn issue_records(issues: &Table) -> Vec<Json> {
    use yamaa::{ListArray, StringArray};
    let mut records = Vec::new();
    for batch in issues.batches() {
        let column = |name: &str| {
            batch
                .column_by_name(name)
                .unwrap_or_else(|| panic!("the issues table has a {name} column"))
        };
        let text = |name: &str, row: usize| {
            let values = column(name)
                .as_any()
                .downcast_ref::<StringArray>()
                .unwrap_or_else(|| panic!("{name} is text"));
            if values.is_null(row) {
                Json::Null
            } else {
                Json::from(values.value(row))
            }
        };
        let paths = column("spec_paths")
            .as_any()
            .downcast_ref::<ListArray>()
            .expect("spec_paths is a list");
        for row in 0..batch.num_rows() {
            let listed = paths.value(row);
            let listed = listed
                .as_any()
                .downcast_ref::<StringArray>()
                .expect("spec paths are text");
            let context = match text("context", row) {
                Json::String(context) => {
                    yamaa_adapters::json_wire::decode(&context).expect("context is JSON text")
                }
                _ => Json::Object(yamaa_core::json::Map::new()),
            };
            records.push(Json::Object(
                [
                    ("phase".to_owned(), text("phase", row)),
                    ("condition".to_owned(), text("condition", row)),
                    ("rule".to_owned(), text("rule", row)),
                    (
                        "spec_paths".to_owned(),
                        Json::Array(
                            (0..listed.len())
                                .map(|index| Json::from(listed.value(index)))
                                .collect(),
                        ),
                    ),
                    ("context".to_owned(), context),
                ]
                .into_iter()
                .collect(),
            ));
        }
    }
    records
}
