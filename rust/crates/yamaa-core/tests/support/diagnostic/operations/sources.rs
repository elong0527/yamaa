//! The source readers' refusals as core classifies them (CSV, PARQUET, SAS,
//! Dataset-JSON and the YAML source of SPEC-8, SCHEMA-3 and VALUE-11); the
//! adapters' own suites pin which bytes give which refusal.
use super::{column_type, text, File};
use crate::support::diagnostic::{json::Json, Record};
use yamaa_core::{
    csv_source::{CsvSource, Error as CsvError, Field as CsvField, ProfileCause},
    diagnostic::{DatasetJsonCause, Diagnostic, SourceFinding},
    parquet_source::{self as parquet, Field},
    typed_csv::PreparedTypes,
    value::ColumnType,
    yaml_diagnostics::{diagnostics, Failure, UnicodeIssue},
};

pub(super) const FILES: &[File] = &[fixture!("sources.json")];

pub(super) fn run(operation: &str, input: &Json) -> Option<Vec<Diagnostic>> {
    Some(match operation {
        "csv_profile" => csv_profile(input),
        "csv_types" => csv_types(input),
        "dataset_json_source" => dataset_json_source(input),
        "dataset_json_value" => dataset_json_value(input),
        "parquet_columns" => parquet_columns(input),
        "parquet_value" => parquet_value(input),
        "sas_source" => sas_source(input),
        "yaml_source" => yaml_source(input),
        _ => return None,
    })
}

/// A CSV profile refusal, `{dataset, path, cause, record, field}`: `cause`
/// names a profile cause (`byte_order_mark` with the input's `declared`), and
/// `field` is a field's 1-based index or its name.
fn csv_profile(input: &Json) -> Vec<Diagnostic> {
    let input = input.closed(&["dataset", "path", "cause", "declared", "record", "field"]);
    let cause = match text(input, "cause") {
        "invalid_text" => ProfileCause::InvalidText,
        "byte_order_mark" => ProfileCause::ByteOrderMark {
            declared: input.field("declared").boolean(),
        },
        "header_absent" => ProfileCause::HeaderAbsent,
        "field_name_empty" => ProfileCause::FieldNameEmpty,
        "field_name_duplicate" => ProfileCause::FieldNameDuplicate,
        "record_width" => ProfileCause::RecordWidth,
        "quote_unterminated" => ProfileCause::QuoteUnterminated,
        "carriage_return" => ProfileCause::CarriageReturn,
        "text_after_quote" => ProfileCause::TextAfterQuote,
        "quote_in_bare_field" => ProfileCause::QuoteInBareField,
        other => panic!("no CSV profile cause {other:?}"),
    };
    assert!(
        matches!(cause, ProfileCause::ByteOrderMark { .. }) || input.get("declared").is_none(),
        "only a byte-order mark is declared"
    );
    let field = match input.field("field") {
        Json::String(name) => CsvField::Name(name.clone()),
        index => CsvField::Index(index.integer()),
    };
    let error = CsvError::Profile {
        cause,
        record: input.field("record").integer(),
        field,
    };
    error
        .finding(text(input, "dataset"), text(input, "path"))
        .map(SourceFinding::diagnostic)
        .into_iter()
        .collect()
}

/// The typing of a CSV text table by its input's `types`, `{dataset, types,
/// names, records}`: `types` lists `[field, type]` pairs as written, and a
/// record's `null` is an empty field.
fn csv_types(input: &Json) -> Vec<Diagnostic> {
    let input = input.closed(&["dataset", "types", "names", "records"]);
    let declarations = input
        .field("types")
        .items()
        .iter()
        .map(|pair| match pair.items() {
            [name, kind] => (name.text().to_owned(), column_type(kind)),
            other => panic!("a type is a [field, type] pair: {other:?}"),
        })
        .collect::<Vec<_>>();
    let source = CsvSource {
        names: input
            .field("names")
            .items()
            .iter()
            .map(|name| name.text().to_owned())
            .collect(),
        records: input
            .field("records")
            .items()
            .iter()
            .map(|record| {
                record
                    .items()
                    .iter()
                    .map(|field| match field {
                        Json::Null => None,
                        field => Some(field.text().to_owned()),
                    })
                    .collect()
            })
            .collect(),
    };
    let types = PreparedTypes::new(&declarations).expect("admitted declarations");
    match types.convert(source) {
        Ok(_) => Vec::new(),
        Err(error) => error
            .finding(text(input, "dataset"))
            .map(SourceFinding::diagnostic)
            .into_iter()
            .collect(),
    }
}

/// The columns of a Parquet schema, `{dataset, path, fields, roots}`: each
/// field is a 64-bit integer leaf named `name` unless it writes its `leaves`
/// and `stored_type`.
fn parquet_columns(input: &Json) -> Vec<Diagnostic> {
    let input = input.closed(&["dataset", "path", "fields", "roots"]);
    let fields = input
        .field("fields")
        .items()
        .iter()
        .map(|field| {
            let field = field.closed(&["name", "leaves", "stored_type"]);
            // A field of several leaves is nested, and no column row admits
            // it; a field of one is an admitted int64.
            let leaves = field.get("leaves").map_or(1, Json::integer);
            Field {
                name: text(field, "name").into(),
                kind: (leaves == 1).then_some(ColumnType::Int),
                stored_type: field.get("stored_type").map_or("int64", Json::text).into(),
            }
        })
        .collect::<Vec<_>>();
    parquet_refusal(
        input,
        parquet::columns(fields, input.field("roots").integer()),
    )
}

/// A stored temporal value of a Parquet column, `{dataset, path, type,
/// value, field, record}`, `value` in the column's stored unit: days for a
/// date, microseconds for a datetime.
fn parquet_value(input: &Json) -> Vec<Diagnostic> {
    let input = input.closed(&["dataset", "path", "type", "value", "field", "record"]);
    parquet_refusal(
        input,
        parquet::temporal(
            column_type(input.field("type")),
            input.field("value").integer(),
            1_000_000,
            text(input, "field"),
            input.field("record").integer(),
        ),
    )
}

fn parquet_refusal<T>(input: &Json, result: Result<T, parquet::Error>) -> Vec<Diagnostic> {
    match result {
        Ok(_) => Vec::new(),
        Err(error) => vec![error
            .finding(text(input, "dataset"), text(input, "path"))
            .diagnostic()],
    }
}

/// A SAS reader's classified refusal, `{dataset, path, cause}`.
fn sas_source(input: &Json) -> Vec<Diagnostic> {
    let input = input.closed(&["dataset", "path", "cause"]);
    vec![SourceFinding::Sas {
        dataset: text(input, "dataset"),
        path: text(input, "path"),
        cause: text(input, "cause"),
    }
    .diagnostic()]
}

/// A Dataset-JSON reader's classified refusal, `{dataset, path, cause}` and
/// the facts its cause locates it by: `member` and an optional `field`
/// position, a `field` name or position, `data_type` and the declared
/// `type`, and a `record`.
fn dataset_json_source(input: &Json) -> Vec<Diagnostic> {
    let input = input.closed(&[
        "dataset",
        "path",
        "cause",
        "member",
        "field",
        "data_type",
        "type",
        "record",
    ]);
    let field = input.get("field").map(|field| match field {
        Json::String(name) => CsvField::Name(name.clone()),
        index => CsvField::Index(index.integer()),
    });
    let name = || match &field {
        Some(CsvField::Name(name)) => name.as_str(),
        other => panic!("the cause names its field: {other:?}"),
    };
    let record = || input.field("record").integer();
    let cause = match text(input, "cause") {
        "malformed" => DatasetJsonCause::Malformed,
        "version" => DatasetJsonCause::Version,
        "member" => DatasetJsonCause::Member {
            member: text(input, "member"),
            field: match &field {
                Some(CsvField::Index(index)) => Some(*index),
                None => None,
                Some(CsvField::Name(name)) => panic!("a member's field is a position: {name}"),
            },
        },
        "field_name" => DatasetJsonCause::FieldName {
            field: field.as_ref().expect("the cause names its field"),
        },
        "data_type" => DatasetJsonCause::DataType {
            field: name(),
            data_type: text(input, "data_type"),
        },
        "field_type" => DatasetJsonCause::FieldType {
            field: name(),
            data_type: text(input, "data_type"),
            declared: column_type(input.field("type")),
        },
        "records" => DatasetJsonCause::Records,
        "row_width" => DatasetJsonCause::RowWidth { record: record() },
        "cell" => DatasetJsonCause::Cell {
            record: record(),
            field: name(),
        },
        other => panic!("no Dataset-JSON cause {other:?}"),
    };
    vec![SourceFinding::DatasetJson {
        dataset: text(input, "dataset"),
        path: text(input, "path"),
        cause,
    }
    .diagnostic()]
}

/// A Dataset-JSON value that does not parse as its field's type,
/// `{dataset, field, type, value, record, declared}`.
fn dataset_json_value(input: &Json) -> Vec<Diagnostic> {
    let input = input.closed(&["dataset", "field", "type", "value", "record", "declared"]);
    vec![SourceFinding::DatasetJsonValue {
        dataset: text(input, "dataset"),
        field: text(input, "field"),
        target: column_type(input.field("type")),
        value: text(input, "value"),
        record: input.field("record").integer(),
        declared: input.field("declared").boolean(),
    }
    .diagnostic()]
}

/// The YAML source's refusal of the file `identity`, `{identity, failure}`:
/// `failure` is `{"non_ascii": {line, column}}`, `{"invalid_yaml": {line,
/// column, reason}}` or `{"invalid_text": [{path, code_point, offset}]}`,
/// each code point written in hexadecimal digits.
fn yaml_source(input: &Json) -> Vec<Diagnostic> {
    let input = input.closed(&["identity", "failure"]);
    let [(kind, facts)] = input.field("failure").members() else {
        panic!("a failure is one kind")
    };
    let issues;
    let failure = match kind.as_str() {
        "non_ascii" => {
            let facts = facts.closed(&["line", "column"]);
            Failure::NonAscii {
                line: facts.field("line").integer(),
                column: facts.field("column").integer(),
            }
        }
        "invalid_yaml" => {
            let facts = facts.closed(&["line", "column", "reason"]);
            Failure::InvalidYaml {
                line: facts.field("line").integer(),
                column: facts.field("column").integer(),
                reason: text(facts, "reason"),
            }
        }
        "invalid_text" => {
            issues = facts
                .items()
                .iter()
                .map(|issue| {
                    let issue = issue.closed(&["path", "code_point", "offset"]);
                    UnicodeIssue {
                        path: text(issue, "path").into(),
                        code_point: u32::from_str_radix(text(issue, "code_point"), 16)
                            .expect("a hexadecimal code point"),
                        offset: issue.field("offset").integer(),
                    }
                })
                .collect::<Vec<_>>();
            Failure::InvalidText(&issues)
        }
        other => panic!("no YAML source failure {other:?}"),
    };
    diagnostics(failure, text(input, "identity"))
        .iter()
        .map(Record::record)
        .collect()
}
