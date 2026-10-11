use yamaa_core::{
    temporal::{Date, DatePrecision, DateTime, DateTimePrecision},
    text_pad::{DeclarationError, OutputLimit, StringPad, MAX_DECLARATION_BYTES},
    value::{FiniteFloat, Value},
};
fn pad(value: &Value, width: &str, maximum: usize) -> Result<Value, OutputLimit> {
    StringPad::new("SRC.X", "columns.Y.derivation.str_pad", width)
        .unwrap()
        .prepare_text(value)
        .unwrap()
        .render(maximum)
}
#[test]
fn unicode_characters_nul_empty_and_no_truncation() {
    for (input, width, expected) in [
        ("é🦀", "4", "  é🦀"),
        ("e\u{301}", "4", "  e\u{301}"),
        ("a\0b", "5", "  a\0b"),
        ("", "3", "   "),
        ("longer", "2", "longer"),
    ] {
        assert_eq!(
            pad(&Value::Str(input.into()), width, expected.len()).unwrap(),
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
        assert_eq!(
            pad(&input, width, 100).unwrap(),
            Value::Str(expected.into())
        );
    }
}
#[test]
fn temporal_text_reuses_canonical_conversion_without_precision_suffixes() {
    let date = Date::new(2025, 7, 19, DatePrecision::Month).unwrap();
    let time = DateTime::new(date, 12, 34, 56, DateTimePrecision::Day).unwrap();
    assert_eq!(
        pad(&Value::Date(date), "12", 100).unwrap(),
        Value::Str("  2025-07-19".into())
    );
    assert_eq!(
        pad(&Value::DateTime(time), "21", 100).unwrap(),
        Value::Str("  2025-07-19T12:34:56".into())
    );
}
#[test]
fn missing_bypasses_positive_mathematical_width_without_output_or_handler() {
    assert_eq!(
        pad(&Value::Missing, &"9".repeat(81), 0).unwrap(),
        Value::Missing
    );
    assert_eq!(
        pad(&Value::Str("".into()), &"9".repeat(81), usize::MAX),
        Err(OutputLimit { required: None })
    );
}
#[test]
fn byte_quota_precedes_authored_copy_and_padding_allocation() {
    let input = Value::Str("🦀".into());
    assert_eq!(pad(&input, "4", 6), Err(OutputLimit { required: Some(7) }));
    assert_eq!(pad(&input, "4", 7).unwrap(), Value::Str("   🦀".into()));
    assert_eq!(
        pad(&Value::Str("x".repeat(100_000)), "1", 1),
        Err(OutputLimit {
            required: Some(100_000)
        })
    );
}
#[test]
fn metadata_admission_precedes_width_scan_and_shape_failure() {
    assert_eq!(
        StringPad::new("X", "p", &"!".repeat(MAX_DECLARATION_BYTES)),
        Err(DeclarationError::MetadataLimit)
    );
    for width in ["", "+1", "01", "1.0", "١"] {
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
    assert!(DeclarationError::MetadataLimit.diagnostic("p").is_none());
    assert!(DeclarationError::Width
        .diagnostic(&"p".repeat(MAX_DECLARATION_BYTES))
        .is_none());
}

#[test]
fn compiler_admits_every_occurrence_before_owning_first_bad_width_or_any_input() {
    use yamaa_core::{
        schema::{
            Document, DocumentLimits, DocumentNode as N, SpecificationDocument, ValidationBudget,
        },
        specification::{CompilationLimits, PrepareError, PreparedSpecification},
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
    let raw = Document::new(nodes, root, DocumentLimits::default()).unwrap();
    let document =
        SpecificationDocument::admit(raw, &mut ValidationBudget::new(Default::default()))
            .unwrap()
            .unwrap();
    let error = PreparedSpecification::prepare_with_limits(
        &document,
        CompilationLimits {
            model_text_bytes: 32_000_000,
            ..Default::default()
        },
    )
    .unwrap_err();
    assert!(
        matches!(error, PrepareError::Limit("str_pad_metadata")),
        "{error:?}"
    );
}
