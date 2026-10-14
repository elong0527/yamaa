use std::collections::BTreeSet;
use yamaa_core::{
    diagnostic::{ConditionCode, ContextValue as V, Diagnostic},
    mapping_cut::{AdmissionError, Cut, Mapping},
    table::ValueRef,
    value::Value,
};
fn text(s: &str) -> V {
    V::Scalar(Value::Str(s.into()))
}
pub fn reached() -> BTreeSet<ConditionCode> {
    let mut reached = BTreeSet::new();
    let mapping = Mapping::new(
        "SRC.V".into(),
        "columns.V.derivation.mapping".into(),
        vec![],
        true,
        None,
        None,
    )
    .unwrap();
    let cut = Cut::new(
        "AGE".into(),
        "columns.C.derivation.cut".into(),
        vec![18.0],
        vec!["child".into(), "adult".into()],
        false,
        None,
    )
    .unwrap();
    let AdmissionError::Diagnostic(collision) = Mapping::new(
        "SRC.V".into(),
        "columns.V.derivation.mapping".into(),
        vec![
            ("Y".into(), Value::Int(1)),
            ("y".into(), Value::Int(2)),
            ("a".into(), Value::Int(3)),
            ("A".into(), Value::Int(4)),
        ],
        false,
        None,
        None,
    )
    .unwrap_err() else {
        panic!()
    };
    let AdmissionError::Diagnostic(declaration) = Cut::new(
        "AGE".into(),
        "columns.C.derivation.cut".into(),
        vec![2.0, 1.0],
        vec!["a".into(), "b".into(), "c".into()],
        false,
        None,
    )
    .unwrap_err() else {
        panic!()
    };
    for (finding, phase, condition, requirement, path, context) in [
        (
            mapping.select(ValueRef::Int(1)).unwrap_err(),
            "validation",
            "incompatible_input_type",
            "REQ-0304",
            "columns.V.derivation.mapping.source",
            vec![
                ("source", text("SRC.V")),
                ("expected", text("str")),
                ("actual", text("int")),
            ],
        ),
        (
            collision,
            "validation",
            "ambiguous_dictionary",
            "REQ-0714",
            "columns.V.derivation.mapping.dict",
            vec![
                ("folded_key", text("A")),
                ("entries", V::Sequence(vec![text("a"), text("A")])),
            ],
        ),
        (
            mapping.select(ValueRef::Missing).unwrap_err(),
            "mapping",
            "missing_input",
            "REQ-0334",
            "columns.V.derivation.mapping",
            vec![("variable", text("SRC.V"))],
        ),
        (
            mapping.select(ValueRef::Str("unlisted")).unwrap_err(),
            "mapping",
            "unmapped_value",
            "REQ-0334",
            "columns.V.derivation.mapping",
            vec![("source", text("SRC.V")), ("value", text("unlisted"))],
        ),
        (
            cut.select(ValueRef::Str("1")).unwrap_err(),
            "validation",
            "incompatible_input_type",
            "REQ-0306",
            "columns.C.derivation.cut.source",
            vec![
                ("source", text("AGE")),
                ("expected", text("numeric")),
                ("actual", text("str")),
            ],
        ),
        (
            cut.select(ValueRef::Missing).unwrap_err(),
            "cut",
            "missing_input",
            "REQ-0334",
            "columns.C.derivation.cut",
            vec![("variable", text("AGE"))],
        ),
        (
            declaration,
            "validation",
            "invalid_field_type",
            "REQ-1118",
            "columns.C.derivation.cut",
            vec![
                ("operation", text("cut")),
                ("expected", text("ascending breaks")),
            ],
        ),
    ] {
        check(&finding, phase, condition, requirement, path, context);
        reached.insert(finding.code);
    }
    reached
}
fn check(
    finding: &Diagnostic,
    phase: &str,
    condition: &str,
    requirement: &str,
    path: &str,
    context: Vec<(&str, V)>,
) {
    let definition = finding.definition();
    assert_eq!(
        (
            definition.phase,
            definition.condition,
            definition.requirement
        ),
        (phase, condition, Some(requirement))
    );
    assert_eq!(finding.spec_paths, [path]);
    assert_eq!(
        finding.context,
        context.into_iter().map(|(k, v)| (k.into(), v)).collect()
    );
    assert_eq!(finding.source_span, None);
    assert_eq!(finding.operand_route, None);
}
#[test]
fn failures_have_complete_independently_authored_geometry() {
    assert_eq!(reached().len(), 7);
}
