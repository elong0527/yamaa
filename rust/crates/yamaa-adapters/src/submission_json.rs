//! Dataset-JSON byte writer over a held published table.
//!
//! yamaa-core owns semantic field/codelist admission (`submission_transport::admit`)
//! and the typed document composition (`submission_json::document`); this
//! writer owns only the byte projection: quoted strings, streamed escaping
//! and the output-byte boundary.
use std::io::{self, Write};
use yamaa_core::{
    application_issue::{Fault, Stage},
    diagnostic::ConditionCode,
    limits::{LimitExceeded, Limits},
    submission_composition::Finding,
    submission_json::DatasetJsonDocument,
    table::{TableAccess, ValueRef},
};

fn string(value: &str) -> String {
    serde_json::to_string(value).expect("JSON strings serialize")
}
fn object(fields: Vec<(&str, String)>) -> String {
    format!(
        "{{{}}}",
        fields
            .into_iter()
            .map(|(key, value)| format!("{}: {value}", string(key)))
            .collect::<Vec<_>>()
            .join(", ")
    )
}
fn list(values: &[String]) -> String {
    if values.is_empty() {
        return "[]".into();
    }
    format!(
        "[\n{}\n  ]",
        values
            .iter()
            .map(|s| format!("    {s}"))
            .collect::<Vec<_>>()
            .join(",\n")
    )
}

/// One saved file, checked before every physical buffer extension. JSON strings
/// stream through serde so escaping cannot allocate an oversized intermediate.
struct Writer<'a> {
    output: Vec<u8>,
    path: &'a str,
}
impl Write for Writer<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        Limits::output_extension(self.output.len(), bytes.len()).map_err(io::Error::other)?;
        self.output.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
impl Writer<'_> {
    /// A write into the held buffer fails only at the output bound; any other
    /// failure is a byte projection the writer could not complete.
    fn failure(&self, error: io::Error) -> Finding {
        match error
            .get_ref()
            .and_then(|cause| cause.downcast_ref::<LimitExceeded>())
        {
            Some(bound) => Finding::limit(self.path, Stage::Define, Fault::OutputLimit, *bound)
                .with_detail(bound.to_string()),
            None => Finding::fault(self.path, Stage::Define, Fault::OutputProjection)
                .with_detail(error.to_string()),
        }
    }
    fn text(&mut self, text: &str) -> Result<(), Finding> {
        self.write_all(text.as_bytes())
            .map_err(|error| self.failure(error))
    }
    fn string(&mut self, text: &str) -> Result<(), Finding> {
        // serde_json hands an I/O failure back as the original error.
        serde_json::to_writer(&mut *self, text).map_err(|error| self.failure(error.into()))
    }
    fn cell(&mut self, value: ValueRef<'_>) -> Result<(), Finding> {
        match value {
            ValueRef::Missing => self.text("null"),
            ValueRef::Str(value) => self.string(value),
            ValueRef::Int(value) => self.text(&value.to_string()),
            ValueRef::Float(value) => self.text(&yamaa_core::conversion::float_text(value)),
            ValueRef::Date(value) => self.string(&value.to_string()),
            ValueRef::DateTime(value) => self.string(&value.to_string()),
            ValueRef::Bool(_) => Err(Finding::cause(
                ConditionCode::SubmissionJsonInvalid,
                "dataset_json",
                "schema_violation: boolean cell",
            )),
        }
    }
}

/// Encode an admitted typed document over its held table. Callers run the
/// core admission (`yamaa_core::submission_transport::admit`) over the same table
/// first; this writer repeats no semantic validation.
pub fn render<T: TableAccess>(
    document: &DatasetJsonDocument,
    table: &T,
) -> Result<Vec<u8>, Finding> {
    let mut fields = vec![
        ("datasetJSONCreationDateTime", string(&document.created)),
        ("datasetJSONVersion", string(&document.version)),
        ("fileOID", string(&document.file_oid)),
    ];
    if let Some(originator) = &document.originator {
        fields.push(("originator", string(originator)));
    }
    fields.extend([
        (
            "sourceSystem",
            object(vec![
                ("name", string(&document.source_name)),
                ("version", string(&document.source_version)),
            ]),
        ),
        ("studyOID", string(&document.study_oid)),
        ("metaDataVersionOID", string(&document.metadata_version_oid)),
        ("metaDataRef", string(&document.metadata_ref)),
        ("itemGroupOID", string(&document.item_group_oid)),
        ("records", table.row_count().to_string()),
        ("name", string(&document.name)),
        ("label", string(&document.label)),
    ]);
    let mut columns = Vec::new();
    for column in &document.columns {
        let mut values = vec![
            ("itemOID", string(&column.item_oid)),
            ("name", string(&column.name)),
            ("label", string(&column.label)),
            ("dataType", string(&column.data_type)),
        ];
        if let Some(target) = &column.target_data_type {
            values.push(("targetDataType", string(target)));
        }
        if let Some(length) = &column.length {
            values.push(("length", length.clone()));
        }
        if let Some(display) = &column.display_format {
            values.push(("displayFormat", string(display)));
        }
        if let Some(sequence) = column.key_sequence {
            values.push(("keySequence", sequence.to_string()));
        }
        columns.push(object(values));
    }
    fields.push(("columns", list(&columns)));
    let mut writer = Writer {
        output: Vec::new(),
        path: &document.path,
    };
    writer.text("{\n")?;
    for (key, value) in fields {
        writer.text("  ")?;
        writer.string(key)?;
        writer.text(": ")?;
        writer.text(&value)?;
        writer.text(",\n")?;
    }
    writer.text("  \"rows\": [")?;
    if table.row_count() > 0 {
        writer.text("\n")?;
        for row in 0..table.row_count() {
            if row > 0 {
                writer.text(",\n")?;
            }
            writer.text("    [")?;
            for column in 0..document.columns.len() {
                if column > 0 {
                    writer.text(", ")?;
                }
                writer.cell(table.cell(row, column).map_err(|_| {
                    Finding::cause(
                        ConditionCode::SubmissionJsonInvalid,
                        &document.path,
                        "held table access",
                    )
                })?)?;
            }
            writer.text("]")?;
        }
        writer.text("\n  ")?;
    }
    writer.text("]\n}\n")?;
    Ok(writer.output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use yamaa_core::value::{FiniteFloat, Value};

    #[test]
    fn json_serialization_preserves_the_typed_output_boundary() {
        struct Refused(LimitExceeded);
        impl Write for Refused {
            fn write(&mut self, _: &[u8]) -> io::Result<usize> {
                Err(io::Error::other(self.0))
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        let writer = Writer {
            output: Vec::new(),
            path: "datasets.DM.transport",
        };
        for actual in [Some(Limits::OUTPUT_BYTES + 1), None] {
            let error = LimitExceeded {
                resource: "output_bytes",
                limit: Limits::OUTPUT_BYTES,
                actual,
            };
            let wrapped = serde_json::to_writer(Refused(error), "escaped \"text\"").unwrap_err();
            let finding = writer.failure(wrapped.into());
            assert_eq!(finding.code, ConditionCode::ApplicationRejected);
            assert_eq!(finding.definition().rule, None);
            assert_eq!(finding.path, "datasets.DM.transport");
            assert_eq!(finding.context["stage"], "define");
            assert_eq!(finding.context["code"], "output_limit");
            assert_eq!(finding.context["resource"], "output_bytes");
            assert_eq!(finding.context["limit"], Limits::OUTPUT_BYTES);
            assert_eq!(
                finding.context.get("actual"),
                actual.map(yamaa_core::json::Value::from).as_ref()
            );
        }
        let other = writer.failure(io::Error::other("opaque projection failure"));
        assert_eq!(other.context["stage"], "define");
        assert_eq!(other.context["code"], "output_projection");
        assert_eq!(other.context["detail"], "opaque projection failure");
        assert!(!other.context.contains_key("resource"));
    }

    #[test]
    fn streamed_cells_preserve_exact_json_and_full_width_scalar_spelling() {
        let values = [
            (Value::Missing, "null"),
            (
                Value::Str("\0\u{e9}\u{1f642}\"\\\n\t".into()),
                "\"\\u0000\u{e9}\u{1f642}\\\"\\\\\\n\\t\"",
            ),
            (Value::Int(i64::MIN), "-9223372036854775808"),
            (Value::Int(i64::MAX), "9223372036854775807"),
            (Value::float(-0.0), "-0"),
            (Value::Date("2024-02-01".parse().unwrap()), "\"2024-02-01\""),
            (
                Value::DateTime("2024-02-29T03:04:00".parse().unwrap()),
                "\"2024-02-29T03:04:00\"",
            ),
        ];
        for (value, expected) in values {
            let mut writer = Writer {
                output: Vec::new(),
                path: "dataset_json",
            };
            writer.cell(ValueRef::from(&value)).unwrap();
            assert_eq!(writer.output, expected.as_bytes());
        }
    }
    /// The output bound reaches the finding with its resource facts, written
    /// directly or through serde_json; any other failure is a projection fault.
    #[test]
    fn a_refused_write_names_the_output_bound_or_a_projection_fault() {
        use yamaa_core::json::Value as V;
        struct Refusing;
        impl Write for Refusing {
            fn write(&mut self, _: &[u8]) -> io::Result<usize> {
                Err(io::Error::other(LimitExceeded {
                    resource: "output_bytes",
                    limit: Limits::OUTPUT_BYTES,
                    actual: Some(Limits::OUTPUT_BYTES + 1),
                }))
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        let writer = Writer {
            output: Vec::new(),
            path: "datasets.DM.transport",
        };
        let text = |value: &str| V::String(value.into());
        let bound: yamaa_core::diagnostic::Context = [
            ("actual".into(), V::from(Limits::OUTPUT_BYTES + 1)),
            ("code".into(), text("output_limit")),
            (
                "detail".into(),
                text("output_bytes: required Some(10737418241), limit 10737418240"),
            ),
            ("limit".into(), V::from(Limits::OUTPUT_BYTES)),
            ("resource".into(), text("output_bytes")),
            ("stage".into(), text("define")),
        ]
        .into();
        let direct = writer.failure(Refusing.write(b"x").unwrap_err());
        let serialized = writer.failure(serde_json::to_writer(Refusing, "x").unwrap_err().into());
        for finding in [direct, serialized] {
            assert_eq!(finding.code, ConditionCode::ApplicationRejected);
            assert_eq!(finding.path, "datasets.DM.transport");
            assert_eq!(finding.context, bound);
        }
        let other = writer.failure(io::Error::other("not a bound"));
        assert_eq!(
            other.context,
            [
                ("code".into(), text("output_projection")),
                ("detail".into(), text("not a bound")),
                ("stage".into(), text("define")),
            ]
            .into()
        );
    }
    #[test]
    fn float_cells_use_the_portable_float_text() {
        let mut writer = Writer {
            output: Vec::new(),
            path: "dataset_json",
        };
        writer
            .cell(ValueRef::Float(FiniteFloat::new(34.0).unwrap()))
            .unwrap();
        assert_eq!(writer.output, b"34");
    }
}
