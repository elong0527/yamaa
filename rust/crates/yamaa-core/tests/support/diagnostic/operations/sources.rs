//! The source readers' refusals as core classifies them (CSV, PARQUET, SAS
//! and the YAML source of SPEC-8, SCHEMA-3 and VALUE-11); the adapters' own
//! suites pin which bytes give which refusal.
use super::{column_type, text};
use crate::support::diagnostic::{json::Json, Record};
use yamaa_core::{
    csv_source::{CsvSource, Error as CsvError, Field as CsvField, ProfileCause},
    diagnostic::{Diagnostic, SourceFinding},
    parquet_source::{self as parquet, Converted, Field, Logical, Physical, Representation},
    typed_csv::PreparedTypes,
    yaml_diagnostics::{diagnostics, Failure, UnicodeIssue},
};

/// A CSV profile refusal, `{dataset, path, cause, record, field}`: `cause`
/// names a profile cause (`byte_order_mark` with the input's `declared`), and
/// `field` is a field's 1-based index or its name.
pub(super) fn csv_profile(input: &Json) -> Vec<Diagnostic> {
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
pub(super) fn csv_types(input: &Json) -> Vec<Diagnostic> {
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
pub(super) fn parquet_columns(input: &Json) -> Vec<Diagnostic> {
    let fields = input
        .field("fields")
        .items()
        .iter()
        .map(|field| Field {
            name: text(field, "name").into(),
            leaves: field.get("leaves").map_or(1, Json::integer),
            path: text(field, "name").into(),
            repetition: 0,
            physical: Physical::Int64,
            logical: Logical::None,
            converted: Converted::None,
            representation: Representation::Int64,
            stored_type: field.get("stored_type").map_or("int64", Json::text).into(),
        })
        .collect::<Vec<_>>();
    parquet_refusal(
        input,
        parquet::columns(fields, input.field("roots").integer()),
    )
}

/// A stored temporal value of a Parquet column, `{dataset, path, type,
/// value, field, record}`, `value` in the column's stored unit.
pub(super) fn parquet_value(input: &Json) -> Vec<Diagnostic> {
    parquet_refusal(
        input,
        parquet::temporal(
            column_type(input.field("type")),
            input.field("value").integer(),
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
pub(super) fn sas_source(input: &Json) -> Vec<Diagnostic> {
    vec![SourceFinding::Sas {
        dataset: text(input, "dataset"),
        path: text(input, "path"),
        cause: text(input, "cause"),
    }
    .diagnostic()]
}

/// The YAML source's refusal of the file `identity`, `{identity, failure}`:
/// `failure` is `{"non_ascii": {line, column}}`, `{"invalid_yaml": {line,
/// column, reason}}` or `{"invalid_text": [{path, code_point, offset}]}`,
/// each code point written in hexadecimal digits.
pub(super) fn yaml_source(input: &Json) -> Vec<Diagnostic> {
    let [(kind, facts)] = input.field("failure").members() else {
        panic!("a failure is one kind")
    };
    let issues;
    let failure = match kind.as_str() {
        "non_ascii" => Failure::NonAscii {
            line: facts.field("line").integer(),
            column: facts.field("column").integer(),
        },
        "invalid_yaml" => Failure::InvalidYaml {
            line: facts.field("line").integer(),
            column: facts.field("column").integer(),
            reason: text(facts, "reason"),
        },
        "invalid_text" => {
            issues = facts
                .items()
                .iter()
                .map(|issue| UnicodeIssue {
                    path: text(issue, "path").into(),
                    code_point: u32::from_str_radix(text(issue, "code_point"), 16)
                        .expect("a hexadecimal code point"),
                    offset: issue.field("offset").integer(),
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
