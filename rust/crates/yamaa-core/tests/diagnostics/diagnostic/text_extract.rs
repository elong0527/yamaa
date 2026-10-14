use std::collections::BTreeSet;
use yamaa_core::{
    diagnostic::{ConditionCode, ContextValue as V},
    predicate,
    table::ValueRef,
    text_extract::{Error, Step, StringExtract},
    value::Value,
};
pub(super) fn reached() -> BTreeSet<ConditionCode> {
    let path = "columns.TEXT.derivation.str_extract";
    let mut reached = BTreeSet::new();
    let make = |pattern: &str, group: &str| {
        StringExtract::new(
            "SRC.TEXT".into(),
            path.into(),
            pattern.into(),
            group.into(),
            None,
            None,
        )
        .unwrap()
    };
    let op = make("(a)", "1");
    let mut budget = predicate::Budget::new(predicate::Limits::default().into());
    let Error::Diagnostic(input) = op
        .apply(ValueRef::Int(7), Default::default(), &mut budget)
        .unwrap_err()
    else {
        panic!("typed input")
    };
    let cases = [
        (
            input,
            "validation",
            "incompatible_input_type",
            "REQ-0308",
            format!("{path}.source"),
            vec![
                ("source", V::Scalar(Value::Str("SRC.TEXT".into()))),
                ("expected", V::Scalar(Value::Str("str".into()))),
                ("actual", V::Scalar(Value::Str("int".into()))),
            ],
        ),
        (
            op.failure_diagnostic(Step::Missing, 4096).unwrap(),
            "mapping",
            "missing_input",
            "REQ-0334",
            path.into(),
            vec![("variable", V::Scalar(Value::Str("SRC.TEXT".into())))],
        ),
        (
            op.failure_diagnostic(Step::NoMatch("abc"), 4096).unwrap(),
            "mapping",
            "unmatched_pattern",
            "REQ-0334",
            path.into(),
            vec![
                ("value", V::Scalar(Value::Str("abc".into()))),
                ("pattern", V::Scalar(Value::Str("(a)".into()))),
            ],
        ),
        (
            op.unknown_diagnostic(),
            "validation",
            "unknown_field",
            "REQ-0308",
            format!("{path}.source"),
            vec![("identifier", V::Scalar(Value::Str("SRC.TEXT".into())))],
        ),
        (
            make("[", "0").declaration_diagnostic().unwrap().clone(),
            "validation",
            "invalid_regex",
            "REQ-0827",
            format!("{path}.pattern"),
            vec![
                ("pattern", V::Scalar(Value::Str("[".into()))),
                (
                    "reason",
                    V::Scalar(Value::Str("a character class is never closed".into())),
                ),
            ],
        ),
        (
            make("(a)", "2").declaration_diagnostic().unwrap().clone(),
            "validation",
            "regex_group_out_of_range",
            "REQ-0816",
            format!("{path}.group"),
            vec![
                ("group", V::Scalar(Value::Int(2))),
                ("group_count", V::Scalar(Value::Int(1))),
                ("pattern", V::Scalar(Value::Str("(a)".into()))),
            ],
        ),
    ];
    for (finding, phase, condition, requirement, path, context) in cases {
        let def = finding.definition();
        assert_eq!(
            (def.phase, def.condition, def.requirement),
            (phase, condition, Some(requirement))
        );
        assert_eq!(finding.spec_paths, [path]);
        assert_eq!(
            finding.context,
            context.into_iter().map(|(k, v)| (k.into(), v)).collect()
        );
        assert!(finding.source_span.is_none());
        assert!(finding.operand_route.is_none());
        reached.insert(finding.code);
    }
    let wide = "100000000000000000000000000000000000000000000000000000000000000000000000000000000";
    let invalid = make("(a)", wide);
    assert_eq!(
        invalid.declaration_diagnostic().unwrap().context["group"],
        V::Integer(wide.into())
    );
    reached
}
