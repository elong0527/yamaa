//! Benchmark-wide proof that the in-memory resolver route reproduces the
//! file route: every benchmark's specification and metadata are served as
//! held bytes and every tabular input as an independently built Arrow
//! frame through a `MemoryResolver`, and the outcomes are compared with
//! the authored expected artifacts (positive) or the authored error
//! record (negative), using the file route's comparison rules (the
//! public conformance runner in `tools/conformance`). See issue #2670.
//!
//! Input frames are built here, not by the engine: authored CSV text is
//! parsed by this file's own cell parser and converted with the
//! specification's declared types (undeclared fields stay text, the
//! typed-CSV rule); authored Parquet is read with the plain Parquet
//! Arrow reader. The engine's decoded tables are never fed back.
//!
//! Benchmarks that cannot run this way are listed in `EXEMPTIONS` with
//! the concrete reason; the full-corpus test asserts the list stays
//! exact — an exempt case that starts passing fails as stale. In a
//! build without the `parquet` feature, cases involving Parquet are
//! skipped instead (the per-feature lanes exercise everything else).
#![cfg(feature = "yaml")]

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::Arc,
};

use arrow_array::{Float64Array, Int64Array, RecordBatch, StringArray};
use yamaa_adapters::{
    arrow_table::{self, ArrowTable},
    file_application::{self, Domain, RuntimePort},
    file_preparation::FileSpecification,
    host_resources::MemoryResolver,
    issue_rows::Issue,
    specification_report::Identity,
    yaml_decode::decode_yaml,
};
use yamaa_core::{
    application_observer::Scope,
    domain_entry,
    json::Value as Json,
    project_function::Language,
    schema::{Document, DocumentNode},
    table::{Column, TableSchema},
    temporal::{Date, DateTime},
    value::ColumnType,
};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../benchmarks")
}

fn yaml_json(bytes: &[u8]) -> Json {
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
    let decoded = decode_yaml(bytes).unwrap();
    value(&decoded.document, decoded.document.root())
}

/// One authored CSV cell: `None` is an unquoted empty field (missing);
/// a quoted empty field is `Some("")`, matching the conformance reader.
fn csv_cells(text: &str) -> Vec<Vec<Option<String>>> {
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

fn kind_of(name: &str) -> Option<ColumnType> {
    match name {
        "str" => Some(ColumnType::Str),
        "int" => Some(ColumnType::Int),
        "float" => Some(ColumnType::Float),
        "date" => Some(ColumnType::Date),
        "datetime" => Some(ColumnType::DateTime),
        _ => None,
    }
}

/// SAS-style non-finite spellings the typed-CSV conversion maps to
/// missing (the engine's `nonfinite_text` rule, REQ-0006 family).
fn nonfinite_text(text: &str) -> bool {
    matches!(text, ".nan" | ".NaN" | ".NAN")
        || matches!(
            text.strip_prefix(['+', '-']).unwrap_or(text),
            ".inf" | ".Inf" | ".INF"
        )
}

/// The closed ASCII number grammar of the typed-CSV conversion:
/// optional sign, digits, optional fraction (digits required after the
/// point), optional exponent (digits required). Returns whether the
/// spelling is integer-only.
fn integer_spelling(text: &str) -> Option<bool> {
    let bytes = text.as_bytes();
    let mut offset = usize::from(matches!(bytes.first(), Some(b'+' | b'-')));
    let start = offset;
    while bytes.get(offset).is_some_and(u8::is_ascii_digit) {
        offset += 1;
    }
    if offset == start {
        return None;
    }
    let mut integer = true;
    if bytes.get(offset) == Some(&b'.') {
        integer = false;
        offset += 1;
        let start = offset;
        while bytes.get(offset).is_some_and(u8::is_ascii_digit) {
            offset += 1;
        }
        if offset == start {
            return None;
        }
    }
    if matches!(bytes.get(offset), Some(b'e' | b'E')) {
        integer = false;
        offset += 1;
        if matches!(bytes.get(offset), Some(b'+' | b'-')) {
            offset += 1;
        }
        let start = offset;
        while bytes.get(offset).is_some_and(u8::is_ascii_digit) {
            offset += 1;
        }
        if offset == start {
            return None;
        }
    }
    (offset == bytes.len()).then_some(integer)
}

/// Convert one declared-int cell with the typed-CSV text rules.
fn int_cell(text: &str) -> Result<Option<i64>, String> {
    if nonfinite_text(text) {
        return Ok(None);
    }
    let integer = integer_spelling(text).ok_or_else(|| format!("invalid int cell {text:?}"))?;
    if integer {
        return text
            .parse::<i64>()
            .map(Some)
            .map_err(|error| format!("int cell {text:?}: {error}"));
    }
    let value: f64 = text
        .parse()
        .map_err(|error| format!("int cell {text:?}: {error}"))?;
    if !value.is_finite() {
        return Ok(None);
    }
    if (-9_223_372_036_854_775_808.0..9_223_372_036_854_775_808.0).contains(&value) {
        let whole = value as i64;
        if whole as f64 == value {
            return Ok(Some(whole));
        }
    }
    Err(format!("int cell {text:?}: not integral"))
}

/// Convert one declared-float cell with the typed-CSV text rules.
fn float_cell(text: &str) -> Result<Option<f64>, String> {
    if nonfinite_text(text) {
        return Ok(None);
    }
    integer_spelling(text).ok_or_else(|| format!("invalid float cell {text:?}"))?;
    let value: f64 = text
        .parse()
        .map_err(|error| format!("float cell {text:?}: {error}"))?;
    Ok(value.is_finite().then_some(value))
}

/// Build one input frame from authored CSV bytes, independently of the
/// engine's source decoder. Conversion of a declared-typed cell mirrors
/// the typed-CSV text rules; a cell that cannot convert is an error here
/// (the file route reports it during ingest instead).
fn build_frame(
    csv: &[u8],
    types: &BTreeMap<String, ColumnType>,
) -> Result<Arc<ArrowTable>, String> {
    let text = String::from_utf8(csv.to_vec()).map_err(|error| format!("not UTF-8: {error}"))?;
    let cells = csv_cells(&text);
    let header = cells.first().ok_or("CSV has no header row")?;
    let mut names: Vec<String> = Vec::new();
    for cell in header {
        let name = cell.clone().ok_or("header cell is missing")?;
        if names.contains(&name) {
            return Err(format!("duplicate header field: {name}"));
        }
        names.push(name);
    }
    let schema = TableSchema::new(
        names
            .iter()
            .map(|name| Column {
                name: name.clone(),
                kind: types.get(name).copied().unwrap_or(ColumnType::Str),
            })
            .collect(),
    )
    .map_err(|error| format!("frame schema: {error:?}"))?;
    let rows = &cells[1..];
    let mut arrays: Vec<arrow_array::ArrayRef> = Vec::new();
    for (index, column) in schema.columns().iter().enumerate() {
        let values: Vec<Option<String>> = rows
            .iter()
            .map(|row| row.get(index).cloned().unwrap_or(None))
            .collect();
        match column.kind {
            ColumnType::Str => arrays.push(Arc::new(StringArray::from(values))),
            ColumnType::Int => {
                let mut parsed = Vec::with_capacity(values.len());
                for value in &values {
                    parsed.push(match value {
                        None => None,
                        Some(text) => int_cell(text)?,
                    });
                }
                arrays.push(Arc::new(Int64Array::from(parsed)));
            }
            ColumnType::Float => {
                let mut parsed = Vec::with_capacity(values.len());
                for value in &values {
                    parsed.push(match value {
                        None => None,
                        Some(text) => float_cell(text)?,
                    });
                }
                arrays.push(Arc::new(Float64Array::from(parsed)));
            }
            ColumnType::Date => {
                let mut dates: Vec<Option<Date>> = Vec::with_capacity(values.len());
                for value in &values {
                    dates.push(
                        value
                            .as_ref()
                            .map(|text| {
                                text.parse::<Date>()
                                    .map_err(|error| format!("date cell {text:?}: {error:?}"))
                            })
                            .transpose()?,
                    );
                }
                arrays.push(Arc::new(
                    arrow_table::date_array(&dates).map_err(|error| format!("{error:?}"))?,
                ));
            }
            ColumnType::DateTime => {
                let mut datetimes: Vec<Option<DateTime>> = Vec::with_capacity(values.len());
                for value in &values {
                    datetimes.push(
                        value
                            .as_ref()
                            .map(|text| {
                                text.parse::<DateTime>()
                                    .map_err(|error| format!("datetime cell {text:?}: {error:?}"))
                            })
                            .transpose()?,
                    );
                }
                arrays.push(Arc::new(
                    arrow_table::datetime_array(&datetimes)
                        .map_err(|error| format!("{error:?}"))?,
                ));
            }
        }
    }
    let batch = RecordBatch::try_new(arrow_table::physical_schema(&schema), arrays)
        .map_err(|error| format!("record batch: {error}"))?;
    ArrowTable::try_new(schema, vec![batch])
        .map(Arc::new)
        .map_err(|error| format!("arrow table: {error:?}"))
}

/// Build one input frame from authored Parquet bytes with the plain
/// Parquet Arrow reader, independently of the engine's source decoder.
#[cfg(feature = "parquet")]
fn build_parquet_frame(bytes: &[u8]) -> Result<Arc<ArrowTable>, String> {
    use arrow_schema::DataType;
    use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;
    let builder = ParquetRecordBatchReaderBuilder::try_new(bytes::Bytes::copy_from_slice(bytes))
        .map_err(|error| format!("parquet reader: {error}"))?;
    let arrow_schema = builder.schema().clone();
    let mut columns = Vec::new();
    for field in arrow_schema.fields() {
        let kind = match field.data_type() {
            DataType::Utf8 | DataType::LargeUtf8 => ColumnType::Str,
            DataType::Int64 => ColumnType::Int,
            DataType::Float64 => ColumnType::Float,
            DataType::Date32 => ColumnType::Date,
            DataType::Timestamp(arrow_schema::TimeUnit::Microsecond, _) => ColumnType::DateTime,
            other => return Err(format!("unsupported parquet column type: {other}")),
        };
        columns.push(Column {
            name: field.name().clone(),
            kind,
        });
    }
    let schema = TableSchema::new(columns).map_err(|error| format!("frame schema: {error:?}"))?;
    let reader = builder
        .build()
        .map_err(|error| format!("parquet reader: {error}"))?;
    let physical = arrow_table::physical_schema(&schema);
    let mut batches = Vec::new();
    for batch in reader {
        let batch = batch.map_err(|error| format!("parquet batch: {error}"))?;
        // Parquet writers may store text as LargeUtf8 while the physical
        // table schema carries Utf8; widen the batch to the physical
        // schema array by array.
        if batch.schema() == physical {
            batches.push(batch);
            continue;
        }
        let mut arrays: Vec<arrow_array::ArrayRef> = Vec::new();
        for (index, field) in physical.fields().iter().enumerate() {
            let array = batch.column(index);
            if array.data_type() == field.data_type() {
                arrays.push(Arc::clone(array));
            } else if *field.data_type() == arrow_schema::DataType::Utf8 {
                let large = array
                    .as_any()
                    .downcast_ref::<arrow_array::LargeStringArray>()
                    .ok_or_else(|| format!("cannot widen {} to Utf8", array.data_type()))?;
                arrays.push(Arc::new(StringArray::from_iter(large.iter())));
            } else {
                return Err(format!(
                    "parquet column {} has type {}, physical schema wants {}",
                    field.name(),
                    array.data_type(),
                    field.data_type()
                ));
            }
        }
        batches.push(
            RecordBatch::try_new(physical.clone(), arrays)
                .map_err(|error| format!("record batch: {error}"))?,
        );
    }
    if batches.is_empty() {
        let batch = RecordBatch::new_empty(arrow_table::physical_schema(&schema));
        batches.push(batch);
    }
    ArrowTable::try_new(schema, batches)
        .map(Arc::new)
        .map_err(|error| format!("arrow table: {error:?}"))
}

#[derive(Default, Debug, Clone)]
struct SourceInfo {
    path: String,
    types: BTreeMap<String, ColumnType>,
    schema: Option<String>,
}

fn normalize(declaring: &str, written: &str) -> String {
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

fn parent_of(doc: &Json) -> Option<String> {
    doc.get("parent").and_then(Json::as_str).map(str::to_owned)
}

/// Collect the source declarations reachable from one specification,
/// walking its parent chain and schema-linked specifications (declarations only,
/// never derivations — the same walk the conformance reader performs).
/// A later declaration refines an earlier one field by field, so a child
/// may add `types:` to a parent's path-only declaration.
fn collect_sources(
    case: &Path,
    spec: &str,
    seen: &mut Vec<String>,
) -> BTreeMap<String, SourceInfo> {
    let mut out = BTreeMap::new();
    if seen.iter().any(|s| s == spec) {
        return out;
    }
    seen.push(spec.to_string());
    let Ok(bytes) = fs::read(case.join(spec)) else {
        return out;
    };
    let doc = yaml_json(&bytes);
    if let Some(parent) = parent_of(&doc) {
        let parent = normalize(spec, &parent);
        merge_sources(&mut out, collect_sources(case, &parent, seen));
    }
    if let Some(input) = doc.get("input").and_then(Json::as_object) {
        for (name, decl) in input {
            let slot = out.entry(name.clone()).or_default();
            match decl {
                Json::String(path) => slot.path = normalize(spec, path),
                Json::Object(map) => {
                    if let Some(path) = map.get("path").and_then(Json::as_str) {
                        slot.path = normalize(spec, path);
                    }
                    if let Some(Json::Object(declared)) = map.get("types") {
                        for (field, kind) in declared {
                            if let Some(kind) = kind.as_str().and_then(kind_of) {
                                slot.types.insert(field.clone(), kind);
                            }
                        }
                    }
                    if let Some(schema) = map.get("schema").and_then(Json::as_str) {
                        slot.schema = Some(schema.to_string());
                    }
                }
                _ => (),
            }
        }
    }
    out
}

fn merge_sources(into: &mut BTreeMap<String, SourceInfo>, from: BTreeMap<String, SourceInfo>) {
    for (name, info) in from {
        let slot = into.entry(name).or_default();
        if slot.path.is_empty() {
            slot.path = info.path;
        }
        for (field, kind) in info.types {
            slot.types.entry(field).or_insert(kind);
        }
        if slot.schema.is_none() {
            slot.schema = info.schema;
        }
    }
}

/// One planned benchmark entry, following `tools/benchmark.py` for the
/// Python host: explicit entries win; the environment defaults to
/// `python/environment.yaml` when an entry does not name one and that
/// file exists; a case without entries has exactly one specification.
#[derive(Debug, Clone)]
struct EntryCall {
    specification: String,
    environment: Option<String>,
}

fn plan_entries(case: &Path) -> Vec<EntryCall> {
    let default_environment = case
        .join("python/environment.yaml")
        .is_file()
        .then(|| "python/environment.yaml".to_string());
    let benchmark = case.join("benchmark.yaml");
    if let Ok(bytes) = fs::read(&benchmark) {
        let doc = yaml_json(&bytes);
        if let Some(entries) = doc
            .get("entries")
            .and_then(|entries| entries.get("python"))
            .and_then(Json::as_array)
        {
            return entries
                .iter()
                .map(|entry| {
                    let specification = entry
                        .get("specification")
                        .and_then(Json::as_str)
                        .expect("entry names a specification")
                        .to_string();
                    let environment = match entry.get("environment") {
                        Some(Json::String(path)) => Some(path.clone()),
                        Some(_) => None,
                        None => default_environment.clone(),
                    };
                    EntryCall {
                        specification,
                        environment,
                    }
                })
                .collect();
        }
    }
    let mut specs: Vec<String> = fs::read_dir(case)
        .unwrap()
        .filter_map(|entry| {
            let name = entry.ok()?.file_name().into_string().ok()?;
            (name.starts_with("spec") && name.ends_with(".yaml")).then_some(name)
        })
        .collect();
    specs.sort();
    assert_eq!(
        specs.len(),
        1,
        "case without entries needs exactly one specification: {}",
        case.display()
    );
    vec![EntryCall {
        specification: specs.pop().unwrap(),
        environment: default_environment,
    }]
}

/// Every file a case may reference, as case-relative POSIX paths
/// (the authored `expected/` truth is never served).
fn case_files(case: &Path) -> Vec<(String, PathBuf)> {
    let mut out = Vec::new();
    let mut stack = vec![(String::new(), case.to_path_buf())];
    while let Some((prefix, dir)) = stack.pop() {
        for item in fs::read_dir(&dir).unwrap().flatten() {
            let name = item.file_name().to_string_lossy().into_owned();
            let written = if prefix.is_empty() {
                name.clone()
            } else {
                format!("{prefix}/{name}")
            };
            if prefix.is_empty() && name == "expected" {
                continue;
            }
            let path = item.path();
            if path.is_dir() {
                stack.push((written, path));
            } else if path.is_file() {
                out.push((written, path));
            }
        }
    }
    out.sort();
    out
}

/// Declaring identities a capture may use: the case root and every
/// specification-like file in the case (entries, parents, producers and
/// schema specifications), each named by its canonical leading-slash form.
fn spec_identities(files: &[(String, PathBuf)]) -> Vec<String> {
    let mut identities = vec!["/".to_string()];
    for (written, _) in files {
        if written.ends_with(".yaml") {
            identities.push(format!("/{written}"));
        }
    }
    identities
}

/// The spellings under which a declaring file may name a target: the
/// case-relative path, and the path relative to the declaring file's
/// own directory (a schema specification inside `input/` names its
/// sibling inputs by bare file name).
fn spellings(identity: &str, target: &str) -> Vec<String> {
    let dir = identity.rsplit_once('/').map(|(dir, _)| dir).unwrap_or("");
    let from: Vec<&str> = dir.split('/').filter(|part| !part.is_empty()).collect();
    let to: Vec<&str> = target.split('/').filter(|part| !part.is_empty()).collect();
    let mut common = 0;
    while common < from.len() && common < to.len() && from[common] == to[common] {
        common += 1;
    }
    let mut parts: Vec<&str> = vec![".."; from.len() - common];
    parts.extend(&to[common..]);
    let relative = parts.join("/");
    if relative.is_empty() || relative == target {
        vec![target.to_string()]
    } else {
        vec![target.to_string(), relative]
    }
}

/// Datasets read through the ODM record model: a specification marks a
/// dataset as ODM-interpreted when any derivation addresses it through
/// an `odm:` item (the engine's own detection rule). ODM documents are
/// supplied as held bytes, not frames — the engine's ODM decoder owns
/// the record reshaping, exactly as on the file route.
fn odm_datasets(case: &Path, files: &[(String, PathBuf)]) -> std::collections::BTreeSet<String> {
    fn visit(value: &Json, found: &mut std::collections::BTreeSet<String>) {
        match value {
            Json::Object(map) => {
                if let Some(payload) = map.get("odm") {
                    let item = match payload {
                        Json::String(text) => Some(text.clone()),
                        Json::Object(inner) => {
                            inner.get("item").and_then(Json::as_str).map(str::to_owned)
                        }
                        _ => None,
                    };
                    if let Some(item) = item {
                        if let Some((dataset, _)) = item.split_once('.') {
                            found.insert(dataset.to_string());
                        }
                    }
                } else {
                    for (key, child) in map {
                        if key != "literal" && key != "dict" {
                            visit(child, found);
                        }
                    }
                }
            }
            Json::Array(items) => {
                for item in items {
                    visit(item, found);
                }
            }
            _ => (),
        }
    }
    let _ = case;
    let mut found = std::collections::BTreeSet::new();
    for (written, path) in files {
        if !written.ends_with(".yaml") {
            continue;
        }
        let Ok(bytes) = fs::read(path) else {
            continue;
        };
        let doc = yaml_json(&bytes);
        for collection in ["columns", "rows", "intermediates"] {
            let Some(Json::Array(items)) = doc.get(collection) else {
                continue;
            };
            for item in items {
                if collection == "columns" {
                    if let Some(derivation) = item.get("derivation") {
                        visit(derivation, &mut found);
                    }
                } else if let Some(Json::Object(derivations)) = item.get("derivations") {
                    for derivation in derivations.values() {
                        visit(derivation, &mut found);
                    }
                }
            }
        }
    }
    found
}

/// Serve one benchmark case through a memory resolver: every case file
/// as held bytes under a per-file reference, every tabular input as an
/// independent frame under the same reference (the resolver holds one
/// reply per declaring/written pair, and a repeated reference must
/// always carry identical content or the capture fails as `Changed`).
/// Frame construction failures are reported, not hidden: the affected
/// input keeps its byte reply so the run can proceed and be classified.
fn backend_for(case: &Path) -> (Arc<MemoryResolver>, BTreeMap<String, String>) {
    let backend = Arc::new(MemoryResolver::default());
    let files = case_files(case);
    let identities = spec_identities(&files);
    for (written, path) in &files {
        let bytes: Arc<[u8]> = Arc::from(fs::read(path).unwrap().as_slice());
        let reference = format!("object:{written}");
        for declaring in &identities {
            for spelling in spellings(declaring, written) {
                backend.insert(declaring, &spelling, &reference, Arc::clone(&bytes));
            }
        }
    }
    let odm = odm_datasets(case, &files);
    let mut sources: BTreeMap<String, SourceInfo> = BTreeMap::new();
    for (written, _) in &files {
        if written
            .rsplit('/')
            .next()
            .is_some_and(|name| name.starts_with("spec"))
            && written.ends_with(".yaml")
        {
            merge_sources(
                &mut sources,
                collect_sources(case, written, &mut Vec::new()),
            );
        }
    }
    let mut frame_failures = BTreeMap::new();
    for (name, info) in &sources {
        if info.schema.is_some() || info.path.is_empty() || odm.contains(name) {
            continue;
        }
        let Some((_, path)) = files.iter().find(|(written, _)| *written == info.path) else {
            continue;
        };
        let Ok(bytes) = fs::read(path) else {
            continue;
        };
        let frame = if info.path.ends_with(".csv") {
            build_frame(&bytes, &info.types)
        } else if info.path.ends_with(".parquet") {
            #[cfg(feature = "parquet")]
            {
                build_parquet_frame(&bytes)
            }
            #[cfg(not(feature = "parquet"))]
            {
                Err("parquet feature is disabled".to_string())
            }
        } else {
            continue;
        };
        match frame {
            Ok(frame) => {
                let reference = format!("object:{}", info.path);
                for declaring in &identities {
                    for spelling in spellings(declaring, &info.path) {
                        backend.insert_frame(declaring, &spelling, &reference, Arc::clone(&frame));
                    }
                }
            }
            Err(error) => {
                frame_failures.insert(info.path.clone(), error);
            }
        }
    }
    (backend, frame_failures)
}

/// A built entry. Every route this harness admits is the plain route.
struct Built(Domain);

impl Built {
    fn issues(&self) -> Vec<Issue> {
        self.0.issues()
    }

    fn output_present(&self) -> bool {
        self.0.output_table().is_some()
    }

    /// The host tables as Arrow IPC bytes, one per declared slot.
    fn table_ipc(&self, slot: &str) -> Option<&[u8]> {
        match slot {
            "output" => self.0.output(),
            "verification_log" => self.0.verification_log(),
            "warning_log" => self.0.warning_log(),
            _ => None,
        }
    }

    fn save(&mut self) -> Result<bool, file_application::FailedBuild> {
        self.0.save()
    }
}

/// The runtime port for this route: plain builds run in process; project
/// builds refuse, exactly like the adapters' native port, because project
/// functions need a host language runtime.
struct MemoryRuntime<'a> {
    identity: Identity<'a>,
    resolver: Arc<MemoryResolver>,
}

impl MemoryRuntime<'_> {
    fn identity(&self) -> Identity<'_> {
        Identity {
            runtime: self.identity.runtime,
            runtime_version: self.identity.runtime_version,
            engine_version: self.identity.engine_version,
            example: self.identity.example,
            specification: self.identity.specification,
            base_directory: self.identity.base_directory,
        }
    }
}

impl RuntimePort for MemoryRuntime<'_> {
    type Result = Built;
    type Error = file_application::Failure;

    fn resolver(&self) -> Option<Arc<dyn yamaa_adapters::host_resources::Resolver>> {
        Some(self.resolver.clone())
    }

    fn plain(
        &mut self,
        prepared: FileSpecification,
        scope: Scope<'_>,
    ) -> Result<Built, file_application::Failure> {
        file_application::plain(prepared, self.identity(), scope).map(Built)
    }

    fn project(
        &mut self,
        _: Box<yamaa_adapters::file_project::FileProject>,
        _: Scope<'_>,
    ) -> Result<Built, file_application::Failure> {
        Err(file_application::unsupported(
            "environment",
            Some("environment"),
        ))
    }
}

enum Outcome {
    Built(Box<Built>),
    Failed(Vec<Issue>),
}

/// Run one planned entry through the shared application with every
/// resource served by the memory resolver.
fn run_entry(case: &Path, call: &EntryCall, backend: &Arc<MemoryResolver>) -> Outcome {
    let _ = case;
    let identity = Identity {
        runtime: "rust",
        runtime_version: "test",
        engine_version: yamaa_core::VERSION,
        example: "domain",
        specification: &call.specification,
        base_directory: ".",
    };
    let mut application = file_application::Application {
        host: Language::Python,
        runtime: MemoryRuntime {
            identity,
            resolver: Arc::clone(backend),
        },
    };
    let request = domain_entry::Request {
        specification: &call.specification,
        environment: call.environment.as_deref(),
    };
    match domain_entry::domain(request, &mut application, Scope::default()) {
        Ok(built) => Outcome::Built(Box::new(built)),
        Err(file_application::ApplicationError::Preparation(error)) => Outcome::Failed(
            yamaa_adapters::project_application::selection_failure(&error, Language::Python),
        ),
        Err(file_application::ApplicationError::Runtime(issues)) => Outcome::Failed(issues),
    }
}

fn expected_error(case: &Path) -> Option<Json> {
    let bytes = fs::read(case.join("expected/error.yaml")).ok()?;
    Some(yaml_json(&bytes))
}

// ---------------------------------------------------------------------
// The file route's comparison, mirrored from
// tools/conformance/public_conformance.py (compare_example): typed-table
// equality against the authored expected artifacts plus byte equality of
// saved CSV, logical comparison of saved Parquet, and diagnostic
// matching for negative benchmarks. Tables here are scalar-encoded
// exactly like the runner's `scalar`: text as UTF-8 hex, integers as
// decimal, floats as binary64 bit hex, dates as epoch days, datetimes
// as epoch seconds; rows compare in order.
// ---------------------------------------------------------------------

type Sj = serde_json::Value;

fn sj(value: &Json) -> Sj {
    serde_json::from_str(&yamaa_adapters::json_wire::encode(value)).unwrap_or(Sj::Null)
}

/// The authored reason a case is excluded from the file route's own
/// conformance plan (benchmark.yaml `conformance.excluded`), if any.
fn conformance_exclusion(case: &Path) -> Option<String> {
    let bytes = fs::read(case.join("benchmark.yaml")).ok()?;
    let doc = sj(&yaml_json(&bytes));
    doc.get("conformance")?
        .get("excluded")?
        .as_str()
        .map(str::to_owned)
}

struct OutputDecl {
    path: Option<String>,
    verification_log: Option<String>,
    warning_log: Option<String>,
    decimals: Option<u32>,
}

/// Independent declared types and output declarations of one entry
/// specification, composed over its `parent` chain exactly like the conformance
/// runner's `declarations` (types and output paths, never derivations).
fn declarations_for(
    case: &Path,
    specification: &str,
) -> Result<(BTreeMap<String, String>, OutputDecl), String> {
    fn go(
        path: &Path,
        active: &mut Vec<PathBuf>,
    ) -> Result<(BTreeMap<String, String>, OutputDecl), String> {
        let canonical = path
            .canonicalize()
            .map_err(|error| format!("{}: {error}", path.display()))?;
        if active.contains(&canonical) {
            return Err(format!("cyclic declarations at {}", path.display()));
        }
        active.push(canonical);
        let bytes = fs::read(path).map_err(|error| format!("{}: {error}", path.display()))?;
        let doc = sj(&yaml_json(&bytes));
        let mut types = BTreeMap::new();
        let mut output = OutputDecl {
            path: None,
            verification_log: None,
            warning_log: None,
            decimals: None,
        };
        if let Some(parent) = doc.get("parent").and_then(Sj::as_str) {
            let parent_path = path
                .parent()
                .expect("specification has a directory")
                .join(parent);
            let (inherited_types, inherited_output) = go(&parent_path, active)?;
            types.extend(inherited_types);
            if inherited_output.path.is_some() {
                output.path = inherited_output.path;
            }
            if inherited_output.verification_log.is_some() {
                output.verification_log = inherited_output.verification_log;
            }
            if inherited_output.warning_log.is_some() {
                output.warning_log = inherited_output.warning_log;
            }
            if inherited_output.decimals.is_some() {
                output.decimals = inherited_output.decimals;
            }
        }
        if let Some(Sj::Array(columns)) = doc.get("columns") {
            for column in columns {
                if let (Some(name), Some(kind)) = (
                    column.get("name").and_then(Sj::as_str),
                    column.get("type").and_then(Sj::as_str),
                ) {
                    types.insert(name.to_string(), kind.to_string());
                }
            }
        }
        if let Some(out) = doc.get("output") {
            let text = |key: &str| out.get(key).and_then(Sj::as_str).map(str::to_owned);
            if let Some(value) = text("path") {
                output.path = Some(value);
            }
            if let Some(value) = text("verification_log") {
                output.verification_log = Some(value);
            }
            if let Some(value) = text("warning_log") {
                output.warning_log = Some(value);
            }
            if let Some(Sj::Number(number)) = out.get("decimals") {
                output.decimals = number.as_u64().map(|value| value as u32);
            }
        }
        active.pop();
        Ok((types, output))
    }
    go(&case.join(specification), &mut Vec::new())
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
    let year: i64 = parts
        .next()
        .and_then(|part| part.parse().ok())
        .ok_or_else(|| format!("bad date {text:?}"))?;
    let month: u32 = parts
        .next()
        .and_then(|part| part.parse().ok())
        .ok_or_else(|| format!("bad date {text:?}"))?;
    let day: u32 = parts
        .next()
        .and_then(|part| part.parse().ok())
        .ok_or_else(|| format!("bad date {text:?}"))?;
    Ok(days_from_civil(year, month, day))
}

/// Epoch seconds of an ISO datetime, treating the wall time as UTC
/// exactly like the runner (`replace(tzinfo=utc)` discards any offset).
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
    let hour: i64 = parts
        .next()
        .and_then(|part| part.parse().ok())
        .ok_or_else(|| format!("bad datetime {text:?}"))?;
    let minute: i64 = parts
        .next()
        .and_then(|part| part.parse().ok())
        .ok_or_else(|| format!("bad datetime {text:?}"))?;
    let second: i64 = parts
        .next()
        .map(|part| part.split('.').next().unwrap_or(part))
        .and_then(|part| part.parse().ok())
        .ok_or_else(|| format!("bad datetime {text:?}"))?;
    Ok(days * 86_400 + hour * 3_600 + minute * 60 + second)
}

/// Parse float text with Python `float()` spellings (Rust's parser plus
/// the SAS-style dotted non-finite forms).
fn parse_float_text(text: &str) -> Result<f64, String> {
    let trimmed = text.trim();
    if let Ok(value) = trimmed.parse::<f64>() {
        return Ok(value);
    }
    let lowered = trimmed.to_ascii_lowercase();
    let (sign, body) = match lowered.strip_prefix('-') {
        Some(rest) => (-1.0, rest.to_string()),
        None => (
            1.0,
            lowered.strip_prefix('+').unwrap_or(&lowered).to_string(),
        ),
    };
    match body.as_str() {
        ".inf" | ".infinity" => Ok(sign * f64::INFINITY),
        ".nan" => Ok(f64::NAN),
        _ => Err(format!("float text {text:?} does not parse")),
    }
}

/// One scalar cell in the runner's encoding (see the section comment).
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

/// A minimal big unsigned integer for the exact REQ-0747 projection.
struct BigUint(Vec<u64>);

impl BigUint {
    fn from_u64(value: u64) -> Self {
        BigUint(vec![value])
    }

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

/// REQ-0747 reported-precision projection of one binary64 value,
/// mirroring the runner's `reported_table`: the exact integer ratio is
/// rounded to `decimals` places under REQ-0418's interval (ties, and near
/// ties within 2^-26 below a scaled tie, away from zero), then read back
/// as a float.
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
    let mut scaled = BigUint::from_u64(mantissa);
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

/// A typed table in the runner's comparable shape: `(name, kind)`
/// columns and scalar-encoded cells, row order significant.
#[derive(PartialEq, Debug)]
struct TypedTable {
    columns: Vec<(String, String)>,
    rows: Vec<Vec<Option<String>>>,
}

impl TypedTable {
    fn describe_diff(&self, other: &TypedTable) -> String {
        if self.columns != other.columns {
            return format!(
                "columns differ: expected {:?}, actual {:?}",
                other.columns, self.columns
            );
        }
        if self.rows.len() != other.rows.len() {
            return format!(
                "row counts differ: expected {}, actual {}",
                other.rows.len(),
                self.rows.len()
            );
        }
        for (index, (mine, theirs)) in self.rows.iter().zip(&other.rows).enumerate() {
            if mine != theirs {
                return format!("row {index} differs: expected {theirs:?}, actual {mine:?}");
            }
        }
        "tables differ".to_string()
    }
}

/// One physical cell read from an Arrow batch, before scalar encoding.
enum CellValue {
    Missing,
    Text(String),
    Int(i64),
    Float(f64),
    Date(i32),
    DateTime(i64),
}

fn batch_cells(batch: &RecordBatch) -> Result<Vec<Vec<CellValue>>, String> {
    use arrow_array::{
        Array, Date32Array, Float64Array, Int64Array, LargeStringArray, StringArray,
        TimestampMicrosecondArray,
    };
    let mut rows: Vec<Vec<CellValue>> = (0..batch.num_rows()).map(|_| Vec::new()).collect();
    for array in batch.columns() {
        match array.data_type() {
            arrow_schema::DataType::Utf8 => {
                let values = array.as_any().downcast_ref::<StringArray>().unwrap();
                for (row, cells) in rows.iter_mut().enumerate() {
                    cells.push(if values.is_null(row) {
                        CellValue::Missing
                    } else {
                        CellValue::Text(values.value(row).to_string())
                    });
                }
            }
            arrow_schema::DataType::LargeUtf8 => {
                let values = array.as_any().downcast_ref::<LargeStringArray>().unwrap();
                for (row, cells) in rows.iter_mut().enumerate() {
                    cells.push(if values.is_null(row) {
                        CellValue::Missing
                    } else {
                        CellValue::Text(values.value(row).to_string())
                    });
                }
            }
            arrow_schema::DataType::Int64 => {
                let values = array.as_any().downcast_ref::<Int64Array>().unwrap();
                for (row, cells) in rows.iter_mut().enumerate() {
                    cells.push(if values.is_null(row) {
                        CellValue::Missing
                    } else {
                        CellValue::Int(values.value(row))
                    });
                }
            }
            arrow_schema::DataType::Float64 => {
                let values = array.as_any().downcast_ref::<Float64Array>().unwrap();
                for (row, cells) in rows.iter_mut().enumerate() {
                    cells.push(if values.is_null(row) {
                        CellValue::Missing
                    } else {
                        CellValue::Float(values.value(row))
                    });
                }
            }
            arrow_schema::DataType::Date32 => {
                let values = array.as_any().downcast_ref::<Date32Array>().unwrap();
                for (row, cells) in rows.iter_mut().enumerate() {
                    cells.push(if values.is_null(row) {
                        CellValue::Missing
                    } else {
                        CellValue::Date(values.value(row))
                    });
                }
            }
            arrow_schema::DataType::Timestamp(unit, _) => {
                // Normalize every timestamp unit to microseconds.
                let factor: i64 = match unit {
                    arrow_schema::TimeUnit::Second => 1_000_000,
                    arrow_schema::TimeUnit::Millisecond => 1_000,
                    arrow_schema::TimeUnit::Microsecond => 1,
                    arrow_schema::TimeUnit::Nanosecond => -1_000,
                };
                let raw: Vec<Option<i64>> = match unit {
                    arrow_schema::TimeUnit::Second => {
                        let values = array
                            .as_any()
                            .downcast_ref::<arrow_array::TimestampSecondArray>()
                            .unwrap();
                        (0..batch.num_rows())
                            .map(|row| (!values.is_null(row)).then(|| values.value(row)))
                            .collect()
                    }
                    arrow_schema::TimeUnit::Millisecond => {
                        let values = array
                            .as_any()
                            .downcast_ref::<arrow_array::TimestampMillisecondArray>()
                            .unwrap();
                        (0..batch.num_rows())
                            .map(|row| (!values.is_null(row)).then(|| values.value(row)))
                            .collect()
                    }
                    arrow_schema::TimeUnit::Microsecond => {
                        let values = array
                            .as_any()
                            .downcast_ref::<TimestampMicrosecondArray>()
                            .unwrap();
                        (0..batch.num_rows())
                            .map(|row| (!values.is_null(row)).then(|| values.value(row)))
                            .collect()
                    }
                    arrow_schema::TimeUnit::Nanosecond => {
                        let values = array
                            .as_any()
                            .downcast_ref::<arrow_array::TimestampNanosecondArray>()
                            .unwrap();
                        (0..batch.num_rows())
                            .map(|row| (!values.is_null(row)).then(|| values.value(row)))
                            .collect()
                    }
                };
                for (cells, value) in rows.iter_mut().zip(raw) {
                    cells.push(match value {
                        None => CellValue::Missing,
                        Some(value) => CellValue::DateTime(if factor > 0 {
                            value * factor
                        } else {
                            value / -factor
                        }),
                    });
                }
            }
            other => return Err(format!("unsupported Arrow type {other}")),
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
        CellValue::DateTime(micros) => Some(match kind {
            "datetime" => (micros / 1_000_000).to_string(),
            other => return Err(format!("cannot encode a datetime as {other}")),
        }),
    })
}

fn typed_from_batches(
    batches: &[RecordBatch],
    columns: Vec<(String, String)>,
    decimals: Option<u32>,
) -> Result<TypedTable, String> {
    let mut rows = Vec::new();
    for batch in batches {
        for row in batch_cells(batch)? {
            if row.len() != columns.len() {
                return Err("batch width differs from the column list".to_string());
            }
            let mut cells = Vec::with_capacity(row.len());
            for (cell, (_, kind)) in row.into_iter().zip(&columns) {
                let cell = match (cell, decimals) {
                    (CellValue::Float(value), Some(places)) if kind == "float" => {
                        CellValue::Float(project_reported(value, places))
                    }
                    (cell, _) => cell,
                };
                cells.push(encode_cell(cell, kind)?);
            }
            rows.push(cells);
        }
    }
    Ok(TypedTable { columns, rows })
}

/// The host table of one slot, from its Arrow IPC bytes; column kinds
/// come from the table's own schema, floats pass the REQ-0747
/// projection when the specification declares reported precision.
fn typed_from_ipc(bytes: &[u8], decimals: Option<u32>) -> Result<TypedTable, String> {
    let reader = arrow_ipc::reader::StreamReader::try_new(std::io::Cursor::new(bytes), None)
        .map_err(|error| format!("IPC reader: {error}"))?;
    let mut columns = Vec::new();
    for field in reader.schema().fields() {
        let kind = match field.data_type() {
            arrow_schema::DataType::Utf8 | arrow_schema::DataType::LargeUtf8 => "str",
            arrow_schema::DataType::Int64 => "int",
            arrow_schema::DataType::Float64 => "float",
            arrow_schema::DataType::Date32 => "date",
            arrow_schema::DataType::Timestamp(_, _) => "datetime",
            other => return Err(format!("unsupported host table type {other}")),
        };
        columns.push((field.name().clone(), kind.to_string()));
    }
    let mut batches = Vec::new();
    for batch in reader {
        batches.push(batch.map_err(|error| format!("IPC batch: {error}"))?);
    }
    typed_from_batches(&batches, columns, decimals)
}

#[cfg(feature = "parquet")]
fn parquet_batches(bytes: &[u8]) -> Result<Vec<RecordBatch>, String> {
    let builder = parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder::try_new(
        bytes::Bytes::copy_from_slice(bytes),
    )
    .map_err(|error| format!("parquet reader: {error}"))?;
    let reader = builder
        .build()
        .map_err(|error| format!("parquet reader: {error}"))?;
    let mut batches = Vec::new();
    for batch in reader {
        batches.push(batch.map_err(|error| format!("parquet batch: {error}"))?);
    }
    Ok(batches)
}

/// A Parquet artifact's typed table under the declared kinds (the
/// runner's `expected_table` for Parquet), used for both the expected
/// file and the saved artifact.
#[cfg(feature = "parquet")]
fn typed_from_parquet(
    bytes: &[u8],
    types: &BTreeMap<String, String>,
) -> Result<TypedTable, String> {
    let builder = parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder::try_new(
        bytes::Bytes::copy_from_slice(bytes),
    )
    .map_err(|error| format!("parquet reader: {error}"))?;
    let mut columns = Vec::new();
    for field in builder.schema().fields() {
        let kind = types
            .get(field.name())
            .ok_or_else(|| format!("no declared type for Parquet column {}", field.name()))?;
        columns.push((field.name().clone(), kind.clone()));
    }
    let batches = parquet_batches(bytes)?;
    typed_from_batches(&batches, columns, None)
}

/// The runner's logical Parquet comparison key: per-column name,
/// physical type, logical type and definition/repetition levels —
/// everything except writer metadata and byte layout.
/// One Parquet file's logical schema signature (see `parquet_signature`).
#[cfg(feature = "parquet")]
type ParquetSignature = Vec<(String, String, String, i16, i16)>;

#[cfg(feature = "parquet")]
fn parquet_signature(bytes: &[u8]) -> Result<ParquetSignature, String> {
    use parquet::file::reader::{FileReader, SerializedFileReader};
    let reader = SerializedFileReader::new(bytes::Bytes::copy_from_slice(bytes))
        .map_err(|error| format!("parquet metadata: {error}"))?;
    let descriptor = reader.metadata().file_metadata().schema_descr();
    Ok(descriptor
        .columns()
        .iter()
        .map(|column| {
            (
                column.name().to_string(),
                format!("{:?}", column.physical_type()),
                format!("{:?}", column.logical_type_ref()),
                column.max_def_level(),
                column.max_rep_level(),
            )
        })
        .collect())
}

/// An expected or saved CSV artifact as a typed table under the given
/// kinds (the runner's `expected_table` for CSV).
fn typed_from_csv(bytes: &[u8], types: &BTreeMap<String, String>) -> Result<TypedTable, String> {
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

#[cfg(feature = "parquet")]
fn parquet_column_names(bytes: &[u8]) -> Result<Vec<String>, String> {
    let builder = parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder::try_new(
        bytes::Bytes::copy_from_slice(bytes),
    )
    .map_err(|error| format!("parquet reader: {error}"))?;
    Ok(builder
        .schema()
        .fields()
        .iter()
        .map(|field| field.name().clone())
        .collect())
}

#[cfg(not(feature = "parquet"))]
fn parquet_column_names(_bytes: &[u8]) -> Result<Vec<String>, String> {
    Err("parquet feature is disabled".to_string())
}

/// The kinds the runner assigns a log artifact's columns: integers for
/// the two count columns, text for everything else.
fn log_types(case: &Path, truth_name: &str) -> Result<BTreeMap<String, String>, String> {
    let truth = case.join("expected").join(truth_name);
    let names: Vec<String> = if truth_name.ends_with(".parquet") {
        let bytes = fs::read(&truth).map_err(|error| format!("{truth_name}: {error}"))?;
        parquet_column_names(&bytes)?
    } else {
        let bytes = fs::read(&truth).map_err(|error| format!("{truth_name}: {error}"))?;
        let text = std::str::from_utf8(&bytes).map_err(|error| format!("not UTF-8: {error}"))?;
        csv_cells(text)
            .first()
            .ok_or_else(|| "empty log CSV".to_string())?
            .iter()
            .map(|cell| cell.clone().unwrap_or_default())
            .collect()
    };
    Ok(names
        .into_iter()
        .map(|name| {
            let kind = if name == "EVALUATED_COUNT" || name == "FAILURE_COUNT" {
                "int"
            } else {
                "str"
            };
            (name, kind.to_string())
        })
        .collect())
}

fn deep_equal(want: &Sj, got: &Sj) -> bool {
    match (want, got) {
        (Sj::Number(a), Sj::Number(b)) => a.as_f64() == b.as_f64(),
        (Sj::Array(a), Sj::Array(b)) => {
            a.len() == b.len() && a.iter().zip(b).all(|(x, y)| deep_equal(x, y))
        }
        (Sj::Object(a), Sj::Object(b)) => {
            a.len() == b.len()
                && a.iter()
                    .all(|(key, value)| b.get(key).is_some_and(|other| deep_equal(value, other)))
        }
        _ => want == got,
    }
}

fn context_subset(want: &Sj, got: &Sj) -> bool {
    match (want, got) {
        (Sj::Object(w), Sj::Object(g)) => w
            .iter()
            .all(|(key, value)| g.get(key).is_some_and(|other| context_subset(value, other))),
        _ => deep_equal(want, got),
    }
}

/// One build issue as the decoded diagnostic record the runner compares
/// (its `context` JSON text parsed back into a value).
fn issue_record(issue: &Issue) -> Sj {
    let mut record = sj(&issue.as_value());
    if let Some(Sj::String(text)) = record.get("context").cloned() {
        let parsed = serde_json::from_str(&text).unwrap_or(Sj::Null);
        record["context"] = if parsed.is_null() {
            Sj::Object(serde_json::Map::new())
        } else {
            parsed
        };
    }
    record
}

fn brief_record(record: &Sj) -> String {
    format!(
        "phase={} condition={} requirement={} spec_paths={}",
        record.get("phase").cloned().unwrap_or(Sj::Null),
        record.get("condition").cloned().unwrap_or(Sj::Null),
        record.get("requirement").cloned().unwrap_or(Sj::Null),
        record.get("spec_paths").cloned().unwrap_or(Sj::Null),
    )
}

fn brief_issue(issue: &Issue) -> String {
    format!(
        "phase={} condition={} requirement={:?} spec_paths={:?}",
        issue.phase, issue.condition, issue.requirement, issue.spec_paths
    )
}

/// The runner's `diagnostic_matches`: phase, condition and spec_paths
/// exactly; requirement when the expectation declares one; the expected
/// context a recursive subset of the reported one.
fn diagnostic_matches(want: &Sj, got: &Sj) -> bool {
    for field in ["phase", "condition", "spec_paths"] {
        let fallback = Sj::Null;
        if !deep_equal(
            want.get(field).unwrap_or(&fallback),
            got.get(field).unwrap_or(&fallback),
        ) {
            return false;
        }
    }
    if let Some(requirement) = want.get("requirement") {
        if got.get("requirement") != Some(requirement) {
            return false;
        }
    }
    let empty = Sj::Object(serde_json::Map::new());
    context_subset(
        want.get("context").unwrap_or(&empty),
        got.get("context").unwrap_or(&empty),
    )
}

fn matches_path(actual: &str, expected: &str) -> bool {
    actual == expected
        || actual.starts_with(&format!("{expected}."))
        || actual.starts_with(&format!("{expected}["))
}

enum EntryOutcome {
    Built(Box<Built>),
    Failed(Vec<Issue>),
    Panicked(String),
}

struct EntryRun {
    call: EntryCall,
    outcome: EntryOutcome,
}

fn run_entry_caught(case: &Path, call: &EntryCall, backend: &Arc<MemoryResolver>) -> EntryOutcome {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        run_entry(case, call, backend)
    })) {
        Ok(Outcome::Built(built)) => EntryOutcome::Built(built),
        Ok(Outcome::Failed(issues)) => EntryOutcome::Failed(issues),
        Err(payload) => EntryOutcome::Panicked(
            payload
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| payload.downcast_ref::<&str>().map(|text| text.to_string()))
                .unwrap_or_else(|| "unknown panic".to_string()),
        ),
    }
}

fn basename(path: &str) -> String {
    path.rsplit('/').next().unwrap_or(path).to_string()
}

fn check_negative(name: &str, want: &Sj, runs: Vec<EntryRun>, findings: &mut Vec<String>) {
    let mut issues: Vec<Sj> = Vec::new();
    for run in runs {
        match run.outcome {
            EntryOutcome::Failed(list) => {
                issues.extend(list.iter().map(issue_record));
            }
            EntryOutcome::Built(mut built) => {
                issues.extend(built.issues().iter().map(issue_record));
                if built.output_present() {
                    findings.push(format!(
                        "{name}: negative build exposed output ({})",
                        run.call.specification
                    ));
                }
                if matches!(built.save(), Ok(true)) {
                    findings.push(format!(
                        "{name}: negative build saved output ({})",
                        run.call.specification
                    ));
                }
            }
            EntryOutcome::Panicked(message) => {
                findings.push(format!(
                    "{name}: entry {} panicked: {message}",
                    run.call.specification
                ));
            }
        }
    }
    if issues.is_empty() {
        findings.push(format!("{name}: negative benchmark produced no issues"));
        return;
    }
    if !diagnostic_matches(want, &issues[0]) {
        findings.push(format!(
            "{name}: first issue differs: expected [{}], got [{}]",
            brief_record(want),
            brief_record(&issues[0])
        ));
    }
    if want.get("phase").and_then(Sj::as_str) == Some("validation") {
        let want_paths: Vec<String> = want
            .get("spec_paths")
            .and_then(Sj::as_array)
            .map(|paths| {
                paths
                    .iter()
                    .filter_map(|path| path.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default();
        let want_condition = want.get("condition").and_then(Sj::as_str).unwrap_or("");
        let mut matched: Vec<&str> = Vec::new();
        for issue in &issues {
            let paths: Vec<&str> = issue
                .get("spec_paths")
                .and_then(Sj::as_array)
                .map(|paths| paths.iter().filter_map(Sj::as_str).collect())
                .unwrap_or_default();
            let owned = issue.get("phase").and_then(Sj::as_str) == Some("validation")
                && issue.get("condition").and_then(Sj::as_str) == Some(want_condition)
                && !paths.is_empty()
                && paths
                    .iter()
                    .all(|actual| want_paths.iter().any(|x| matches_path(actual, x)));
            if !owned {
                continue;
            }
            for expected in &want_paths {
                if paths.iter().any(|actual| matches_path(actual, expected)) {
                    matched.push(expected);
                }
            }
        }
        for expected in &want_paths {
            if !matched.contains(&expected.as_str()) {
                findings.push(format!(
                    "{name}: no validation issue covers expected path {expected}"
                ));
            }
        }
    }
}

fn check_positive(
    name: &str,
    case: &Path,
    backend: &Arc<MemoryResolver>,
    runs: Vec<EntryRun>,
    findings: &mut Vec<String>,
) {
    let mut compared: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for run in runs {
        let spec = run.call.specification.clone();
        let mut built = match run.outcome {
            EntryOutcome::Panicked(message) => {
                findings.push(format!("{name}: entry {spec} panicked: {message}"));
                continue;
            }
            EntryOutcome::Failed(issues) => {
                findings.push(format!(
                    "{name}: entry {spec} failed: {}",
                    issues.first().map(brief_issue).unwrap_or_default()
                ));
                continue;
            }
            EntryOutcome::Built(built) => built,
        };
        let (types, output) = match declarations_for(case, &spec) {
            Ok(pair) => pair,
            Err(error) => {
                findings.push(format!("{name}: entry {spec}: declarations: {error}"));
                continue;
            }
        };
        let host_ipc: [Option<Vec<u8>>; 3] = [
            built.table_ipc("output").map(<[u8]>::to_vec),
            built.table_ipc("verification_log").map(<[u8]>::to_vec),
            built.table_ipc("warning_log").map(<[u8]>::to_vec),
        ];
        let issues = built.issues();
        if !issues.is_empty() {
            findings.push(format!(
                "{name}: entry {spec} reported {} issue(s), first: {}",
                issues.len(),
                brief_issue(&issues[0])
            ));
            for declared in [&output.path, &output.verification_log, &output.warning_log]
                .into_iter()
                .flatten()
            {
                compared.insert(basename(declared));
            }
            continue;
        }
        if let Err(error) = built.save() {
            findings.push(format!("{name}: entry {spec} did not save: {error:?}"));
        }
        let slots = [
            ("output", &output.path, &host_ipc[0]),
            ("verification_log", &output.verification_log, &host_ipc[1]),
            ("warning_log", &output.warning_log, &host_ipc[2]),
        ];
        for (slot, declared, host) in &slots {
            let Some(declared) = declared else {
                if host.is_some() {
                    findings.push(format!(
                        "{name}: entry {spec} produced an undeclared {slot} table"
                    ));
                }
                continue;
            };
            let artifact = basename(declared);
            compared.insert(artifact.clone());
            let truth_path = case.join("expected").join(&artifact);
            if !truth_path.is_file() {
                findings.push(format!(
                    "{name}: entry {spec} declares {slot} {declared} without an expected artifact"
                ));
                continue;
            }
            let truth_bytes = fs::read(&truth_path).unwrap();
            // An entry below the case root saves beside its own file.
            let target = normalize(&spec, declared);
            let published = backend.published(&format!("memory:/{target}"));
            match &published {
                None => findings.push(format!(
                    "{name}: entry {spec} save did not publish {declared}"
                )),
                Some(bytes) => {
                    if artifact.ends_with(".csv") && bytes.as_ref() != truth_bytes.as_slice() {
                        findings.push(format!(
                            "{name}: entry {spec} saved CSV bytes differ: {declared}"
                        ));
                    }
                }
            }
            let slot_types = if *slot == "output" {
                types.clone()
            } else {
                match log_types(case, &artifact) {
                    Ok(map) => map,
                    Err(error) => {
                        findings.push(format!("{name}: log types for {artifact}: {error}"));
                        continue;
                    }
                }
            };
            if artifact.ends_with(".parquet") {
                #[cfg(not(feature = "parquet"))]
                findings.push(format!(
                    "{name}: entry {spec} cannot compare a Parquet slot without the parquet feature"
                ));
                #[cfg(not(feature = "parquet"))]
                continue;
                #[cfg(feature = "parquet")]
                {
                    match (
                        parquet_signature(&truth_bytes),
                        published.as_deref().map(parquet_signature),
                    ) {
                        (Ok(expected_sig), Some(Ok(actual_sig))) => {
                            if actual_sig != expected_sig {
                                findings.push(format!(
                                "{name}: entry {spec} saved Parquet logical schema differs: {declared}"
                            ));
                            }
                        }
                        (Err(error), _) => {
                            findings
                                .push(format!("{name}: Parquet signature of {artifact}: {error}"));
                        }
                        (_, Some(Err(error))) => {
                            findings.push(format!(
                                "{name}: Parquet signature of saved {declared}: {error}"
                            ));
                        }
                        (_, None) => {}
                    }
                    let expected_typed = typed_from_parquet(&truth_bytes, &slot_types);
                    let actual_typed = published
                        .as_deref()
                        .map(|bytes| typed_from_parquet(bytes, &slot_types));
                    match (expected_typed, actual_typed) {
                        (Err(error), _) => {
                            findings.push(format!("{name}: expected Parquet {artifact}: {error}"));
                        }
                        (_, None) => {}
                        (Ok(expected), Some(Ok(actual))) => {
                            if actual != expected {
                                findings.push(format!(
                                    "{name}: entry {spec} typed {slot} Parquet table differs: {}",
                                    actual.describe_diff(&expected)
                                ));
                            }
                        }
                        (_, Some(Err(error))) => {
                            findings.push(format!("{name}: saved Parquet {declared}: {error}"));
                        }
                    }
                    // The host table must agree with the expected table too.
                    if let Some(ipc) = host {
                        match typed_from_ipc(ipc, None) {
                            Ok(actual) => {
                                if let Ok(expected) = typed_from_parquet(&truth_bytes, &slot_types)
                                {
                                    if actual != expected {
                                        findings.push(format!(
                                        "{name}: entry {spec} host {slot} table differs from expected Parquet: {}",
                                        actual.describe_diff(&expected)
                                    ));
                                    }
                                }
                            }
                            Err(error) => {
                                findings.push(format!(
                                    "{name}: entry {spec} host {slot} table: {error}"
                                ));
                            }
                        }
                    }
                    continue;
                }
            }
            let expected_typed = typed_from_csv(&truth_bytes, &slot_types);
            let actual_typed = host.as_ref().map(|ipc| {
                typed_from_ipc(
                    ipc,
                    if *slot == "output" {
                        output.decimals
                    } else {
                        None
                    },
                )
            });
            match (expected_typed, actual_typed) {
                (Err(error), _) => {
                    findings.push(format!("{name}: expected CSV {artifact}: {error}"));
                }
                (_, None) => {
                    findings.push(format!("{name}: entry {spec} has no host table for {slot}"));
                }
                (Ok(expected), Some(Ok(actual))) => {
                    if actual != expected {
                        findings.push(format!(
                            "{name}: entry {spec} typed {slot} table differs: {}",
                            actual.describe_diff(&expected)
                        ));
                    }
                }
                (_, Some(Err(error))) => {
                    findings.push(format!("{name}: entry {spec} host {slot} table: {error}"));
                }
            }
        }
    }
    if let Ok(entries) = fs::read_dir(case.join("expected")) {
        for entry in entries.flatten() {
            let file_name = entry.file_name().to_string_lossy().into_owned();
            if (file_name.ends_with(".csv") || file_name.ends_with(".parquet"))
                && !compared.contains(&file_name)
            {
                findings.push(format!(
                    "{name}: expected artifact {file_name} was not compared"
                ));
            }
        }
    }
}

/// The outcome of checking one case: agreement, disagreement with
/// findings, or a skip when this build lacks a feature the case needs
/// (the per-feature lanes exercise everything else).
enum CaseOutcome {
    Pass,
    Findings(Vec<String>),
    Skipped,
}

/// Whether the case involves Parquet anywhere (inputs or expected
/// artifacts); such cases need the `parquet` feature to run at all.
fn case_needs_parquet(case: &Path) -> bool {
    fn walk(dir: &Path) -> bool {
        let Ok(entries) = fs::read_dir(dir) else {
            return false;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if walk(&path) {
                    return true;
                }
            } else if path.extension().is_some_and(|ext| ext == "parquet") {
                return true;
            }
        }
        false
    }
    walk(case)
}

/// Run one benchmark case entirely through the memory resolver and
/// compare the outcome with the authored expectation, the file route's
/// way.
fn check_case(name: &str, case: &Path) -> CaseOutcome {
    if !cfg!(feature = "parquet") && case_needs_parquet(case) {
        return CaseOutcome::Skipped;
    }
    let mut findings = Vec::new();
    if let Some(reason) = conformance_exclusion(case) {
        findings.push(format!(
            "{name}: excluded from the file route's conformance plan: {reason}"
        ));
        return CaseOutcome::Findings(findings);
    }
    let (backend, frame_failures) = backend_for(case);
    for (path, error) in &frame_failures {
        findings.push(format!(
            "{name}: input {path} could not be built as an in-memory frame: {error}"
        ));
    }
    let entries = plan_entries(case);
    if entries.is_empty() {
        findings.push(format!("{name}: no entries planned"));
        return CaseOutcome::Findings(findings);
    }
    let expected = expected_error(case);
    let identities = spec_identities(&case_files(case));
    let mut runs = Vec::new();
    for call in entries {
        let mut outcome = run_entry_caught(case, &call, &backend);
        // Callers order builds: a successful entry saves its artifacts before
        // the next entry reads them, exactly as the file route's runners do.
        if expected.is_none() {
            if let EntryOutcome::Built(built) = &mut outcome {
                if built.issues().is_empty() && matches!(built.save(), Ok(true)) {
                    offer_saved(case, &call, &backend, &identities);
                }
            }
        }
        runs.push(EntryRun { call, outcome });
    }
    match expected {
        Some(want) => check_negative(name, &sj(&want), runs, &mut findings),
        None => check_positive(name, case, &backend, runs, &mut findings),
    }
    if findings.is_empty() {
        CaseOutcome::Pass
    } else {
        CaseOutcome::Findings(findings)
    }
}

/// Serve an entry's saved artifacts to later entries under the same
/// declaring identities and spellings as the authored case files.
fn offer_saved(
    case: &Path,
    call: &EntryCall,
    backend: &Arc<MemoryResolver>,
    identities: &[String],
) {
    let Ok((_, output)) = declarations_for(case, &call.specification) else {
        return;
    };
    for declared in [&output.path, &output.verification_log, &output.warning_log]
        .into_iter()
        .flatten()
    {
        let target = normalize(&call.specification, declared);
        let Some(bytes) = backend.published(&format!("memory:/{target}")) else {
            continue;
        };
        let reference = format!("object:{target}");
        for declaring in identities {
            for spelling in spellings(declaring, &target) {
                backend.insert(declaring, &spelling, &reference, Arc::clone(&bytes));
            }
        }
    }
}

fn benchmark_cases() -> Vec<(String, PathBuf)> {
    let mut cases: Vec<(String, PathBuf)> = fs::read_dir(root())
        .unwrap()
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let path = entry.path();
            if path.is_dir() && path.join("benchmark.yaml").is_file() {
                Some((entry.file_name().to_string_lossy().into_owned(), path))
            } else {
                None
            }
        })
        .collect();
    cases.sort();
    cases
}

/// Benchmarks that cannot run through the in-memory route, each with
/// the concrete reason. The full test asserts this list stays exact:
/// an exempt case that starts passing fails the test as stale, and any
/// non-exempt case that fails is reported with its findings.
const EXEMPTIONS: &[(&str, &str)] = &[
    // Project environments (5): the specification calls project
    // functions, which execute in a host language runtime. The Rust
    // adapters route embeds none, so these cannot run here at all;
    // most fail earlier still, because environment preparation looks
    // for the environment's packaging lock (python/uv.lock), a
    // host-side file the file route's hosts stage beside the
    // environment and that exists in no benchmark directory.
    (
        "adam-adsl-bmi",
        "calls project functions; the build reaches the environment and is refused (unsupported_operation: environment) because project functions need a host language runtime this route does not embed",
    ),
    (
        "adam-advs-percentiles",
        "calls project functions; environment preparation fails first (project_environment_invalid, REQ-0695) because the environment's uv.lock is a host-side file absent from the benchmark directory",
    ),
    (
        "schema-functions",
        "calls project functions; environment preparation fails first (project_environment_invalid, REQ-0695) because the environment's uv.lock is a host-side file absent from the benchmark directory",
    ),
    (
        "schema-non-finite",
        "calls project functions; environment preparation fails first (project_environment_invalid, REQ-0695) because the environment's uv.lock is a host-side file absent from the benchmark directory",
    ),
    (
        "negative-function-contract",
        "calls project functions; environment preparation fails first (project_environment_invalid, REQ-0695 on the absent uv.lock), masking the expected invalid_function_argument finding (REQ-0700)",
    ),
    // Environment terminology (1): the expected finding is a verification
    // over completed values against a codelist the host environment
    // declares. Static terminology findings still compare here, but a
    // build with an environment is refused before it reaches the data.
    (
        "negative-epoch-outside-extensible-list",
        "binds its codelist through a host environment; the build reaches the environment and is refused (unsupported_operation: environment) before the completed values the expected allowed_values_failed finding judges",
    ),
    // File-ingest diagnostics (7): the expected diagnostic has phase
    // `ingest` — it is produced by decoding the input file's text. An
    // in-memory Arrow table has no file text, so the condition cannot
    // arise on this route by construction.
    (
        "negative-ingest-unit",
        "expected diagnostic is ingest field_parse_failed: the value \"200 mg\" in a float field cannot be represented in an in-memory table at all",
    ),
    (
        "negative-source-bad-encoding",
        "expected diagnostic is ingest invalid_text: the input file is not valid UTF-8; in-memory frames carry typed values, not encoded text",
    ),
    (
        "negative-source-extra-field",
        "expected diagnostic is ingest source_record_width: record width is a property of file text; an Arrow table has no record width to violate",
    ),
    (
        "negative-source-field-duplicate",
        "expected diagnostic is ingest on a duplicated header field; an Arrow schema cannot carry duplicate fields",
    ),
    (
        "negative-source-na-age",
        "expected diagnostic is ingest field_parse_failed: \"NA\" in an integer field is rejected when text becomes typed cells, on the caller's side of this route",
    ),
    (
        "negative-source-unnamed-field",
        "expected diagnostic is ingest on a missing header cell; an Arrow schema has no unnamed field",
    ),
    (
        "negative-source-unterminated-quote",
        "expected diagnostic is ingest source_quote_unterminated: quoting is a property of file text; frames carry values, not quoted text",
    ),
    // Filesystem path semantics (4): the expected diagnostics judge
    // filesystem paths. A memory resolver holds references and content,
    // not paths, so the judged properties do not exist on this route.
    (
        "negative-path-directory",
        "expected diagnostic resource_path_not_regular_file: a memory resolver holds no directories; the held reference resolves as content instead",
    ),
    (
        "negative-path-symlink",
        "expected diagnostic judges a symlinked filesystem path; a memory resolver holds no symlinks, so the input resolves and the build succeeds",
    ),
];

/// A representative subset across every mechanism class (inheritance,
/// schema-linked stored artifacts and ordered entries, metadata documents,
/// ODM, Parquet in and out, reported precision, log artifacts, negatives of
/// several phases), for a fast signal while iterating.
const REPRESENTATIVE: &[&str] = &[
    "adam-adsl-age-group",
    "adam-adlb-reported-precision",
    "adam-adsl-randomization",
    "ars-ae-incidence-results",
    "adam-adae-exposure-adjusted-incidence",
    "sdtm-dm-race-ethnicity",
    "sdtm-dm-metadata",
    "sdtm-ae-coding",
    "schema-inheritance",
    "schema-odm-item-sample",
    "schema-parquet",
    "schema-verification-log",
    "negative-redefined-date",
    "negative-zero-division",
    "negative-odm-schema-missing",
];

#[test]
fn memory_resolver_benchmarks_representative() {
    let cases = benchmark_cases();
    let mut failures = Vec::new();
    for name in REPRESENTATIVE {
        assert!(
            !EXEMPTIONS.iter().any(|(exempt, _)| exempt == name),
            "representative case {name} is on the exemption list"
        );
        let (_, case) = cases
            .iter()
            .find(|(case_name, _)| case_name == name)
            .unwrap_or_else(|| panic!("representative case {name} not found"));
        if let CaseOutcome::Findings(findings) = check_case(name, case) {
            failures.extend(findings);
        }
    }
    assert!(
        failures.is_empty(),
        "the in-memory route disagrees with the file route:\n{}",
        failures.join("\n")
    );
}

#[test]
fn memory_resolver_benchmarks() {
    let cases = benchmark_cases();
    assert!(!cases.is_empty(), "no benchmark cases found");
    let exempt: BTreeMap<&str, &str> = EXEMPTIONS.iter().copied().collect();
    assert_eq!(
        exempt.len(),
        EXEMPTIONS.len(),
        "duplicate exemption entries"
    );
    for (name, _) in EXEMPTIONS {
        assert!(
            cases.iter().any(|(case_name, _)| case_name == name),
            "exemption names an unknown benchmark: {name}"
        );
    }
    let mut failures = Vec::new();
    for (name, case) in &cases {
        let outcome = check_case(name, case);
        if exempt.contains_key(name.as_str()) {
            if matches!(outcome, CaseOutcome::Pass) {
                failures.push(format!(
                    "{name}: exemption is stale \u{2014} the case now passes the in-memory route; remove it from EXEMPTIONS"
                ));
            }
        } else if let CaseOutcome::Findings(findings) = outcome {
            failures.extend(findings);
        }
    }
    assert!(
        failures.is_empty(),
        "the in-memory route disagrees with the file route:\n{}",
        failures.join("\n")
    );
}
