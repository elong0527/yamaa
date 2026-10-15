use yamaa_core::{
    temporal::{Date, DatePrecision, DateTime, DateTimePrecision},
    text_pad::{DeclarationError, OutputError, StringPad},
    value::{FiniteFloat, Value},
};
fn pad(value: &Value, width: &str) -> Result<Value, OutputError> {
    StringPad::new("SRC.X", "columns.Y.derivation.str_pad", width)
        .unwrap()
        .prepare_text(value)
        .unwrap()
        .render()
}
#[test]
fn unicode_characters_nul_empty_and_no_truncation() {
    for (input, width, expected) in [
        ("\u{e9}\u{1f980}", "4", "  \u{e9}\u{1f980}"),
        ("e\u{301}", "4", "  e\u{301}"),
        ("a\0b", "5", "  a\0b"),
        ("", "3", "   "),
        ("longer", "2", "longer"),
    ] {
        assert_eq!(
            pad(&Value::Str(input.into()), width).unwrap(),
            Value::Str(expected.into())
        );
    }
}
#[test]
fn canonical_primitives_preserve_exact_integer_and_zero() {
    for (input, width, expected) in [
        (Value::Int(i64::MIN), "22", "  -9223372036854775808"),
        (
            Value::Int(9_007_199_254_740_993),
            "18",
            "  9007199254740993",
        ),
        (Value::Float(FiniteFloat::new(-0.0).unwrap()), "4", "  -0"),
        (Value::float(1.25), "6", "  1.25"),
    ] {
        assert_eq!(pad(&input, width).unwrap(), Value::Str(expected.into()));
    }
}
#[test]
fn temporal_text_reuses_canonical_conversion_without_precision_suffixes() {
    let date = Date::new(2025, 7, 19, DatePrecision::Month).unwrap();
    let time = DateTime::new(date, 12, 34, 56, DateTimePrecision::Day).unwrap();
    assert_eq!(
        pad(&Value::Date(date), "12").unwrap(),
        Value::Str("  2025-07-19".into())
    );
    assert_eq!(
        pad(&Value::DateTime(time), "21").unwrap(),
        Value::Str("  2025-07-19T12:34:56".into())
    );
}
#[test]
fn missing_bypasses_positive_mathematical_width_without_output_or_handler() {
    assert_eq!(
        pad(&Value::Missing, &"9".repeat(81)).unwrap(),
        Value::Missing
    );
    assert_eq!(
        pad(&Value::Str("".into()), &"9".repeat(81)),
        Err(OutputError { required: None })
    );
}
#[test]
fn complete_authored_text_and_unicode_padding_have_no_copy_quota() {
    assert_eq!(
        pad(&Value::Str("🦀".into()), "4").unwrap(),
        Value::Str("   🦀".into())
    );
    let value = Value::Str("x".repeat(100_000));
    assert_eq!(pad(&value, "1").unwrap(), value);
}
#[test]
fn width_spelling_shape_is_independent_of_metadata_size() {
    assert_eq!(
        StringPad::new("X", "p", &"!".repeat(100_000)),
        Err(DeclarationError::Shape)
    );
    for width in ["", "+1", "01", "1.0", "\u{661}"] {
        assert_eq!(
            StringPad::new("X", "p", width),
            Err(DeclarationError::Shape)
        );
    }
}
#[test]
fn bool_preserves_core_conversion_failure_instead_of_coercion() {
    let operation = StringPad::new("X", "p", "4").unwrap();
    let error = operation.prepare_text(&Value::Bool(true)).unwrap_err();
    assert_eq!(error.condition(), "conversion_failed");
    assert_eq!(error.requirement(), "REQ-0013");
    assert_eq!(error.target, yamaa_core::value::ColumnType::Str);
}

#[test]
fn positive_width_failures_are_distinct_from_shape_and_policy_refusals() {
    for width in ["0", "-1", "-999999999999999999999999999999999"] {
        assert_eq!(
            StringPad::new("X", "p", width),
            Err(DeclarationError::Width)
        );
    }
    assert!(DeclarationError::Shape.diagnostic("p").is_none());
    assert!(DeclarationError::Width
        .diagnostic(&"p".repeat(100_000))
        .is_some());
}

#[test]
fn static_check_retains_first_bad_width_with_complete_long_source_names() {
    use yamaa_core::{
        domain,
        schema::{Document, DocumentNode as N, SpecificationDocument},
    };
    fn text(nodes: &mut Vec<N>, value: &str) -> usize {
        let id = nodes.len();
        nodes.push(N::Text(value.into()));
        id
    }
    fn map(nodes: &mut Vec<N>, fields: Vec<(&str, usize)>) -> usize {
        let fields = fields
            .into_iter()
            .map(|(name, value)| (text(nodes, name), value))
            .collect();
        let id = nodes.len();
        nodes.push(N::Mapping(fields));
        id
    }
    fn list(nodes: &mut Vec<N>, items: Vec<usize>) -> usize {
        let id = nodes.len();
        nodes.push(N::Sequence(items));
        id
    }
    let mut nodes = Vec::new();
    let mut columns = Vec::new();
    let mut names = Vec::new();
    for index in 0..32 {
        let written_name = format!("C{index}");
        let name = text(&mut nodes, &written_name);
        names.push(written_name);
        let source = text(&mut nodes, &format!("SRC.{}", "A".repeat(100_000)));
        let width = nodes.len();
        nodes.push(N::Integer(if index == 0 { "0" } else { "4" }.into()));
        let kind = text(&mut nodes, "str");
        let payload = map(&mut nodes, vec![("source", source), ("width", width)]);
        let value = map(&mut nodes, vec![("str_pad", payload)]);
        let derivation = map(&mut nodes, vec![("value", value)]);
        columns.push(map(
            &mut nodes,
            vec![("name", name), ("type", kind), ("derivation", derivation)],
        ));
    }
    let key = text(&mut nodes, "C0");
    let keys = list(&mut nodes, vec![key]);
    let columns = list(&mut nodes, columns);
    let names = names.iter().map(|name| text(&mut nodes, name)).collect();
    let output_names = list(&mut nodes, names);
    let output_path = text(&mut nodes, "output.csv");
    let output = map(
        &mut nodes,
        vec![("path", output_path), ("columns", output_names)],
    );
    let input_path = text(&mut nodes, "absent.csv");
    let input_item = map(&mut nodes, vec![("path", input_path)]);
    let input = map(&mut nodes, vec![("SRC", input_item)]);
    let version = text(&mut nodes, "1.0");
    let domain = text(&mut nodes, "TEST");
    let root = map(
        &mut nodes,
        vec![
            ("schema_version", version),
            ("domain", domain),
            ("keys", keys),
            ("columns", columns),
            ("output", output),
            ("input", input),
        ],
    );
    let raw = Document::new(nodes, root).unwrap();
    let document = SpecificationDocument::admit(raw).unwrap().unwrap();
    let checked = domain::check(&document).unwrap();
    let findings = checked.check_diagnostics();
    assert_eq!(findings.len(), 1);
    let finding = &findings[0];
    assert_eq!(finding.definition().condition, "invalid_field_type");
    assert_eq!(finding.definition().requirement, Some("REQ-1261"));
    assert_eq!(finding.spec_paths, ["columns.C0.derivation.str_pad.width"]);
}
