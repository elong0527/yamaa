use std::collections::BTreeSet;
use yamaa_core::json::Value as V;
use yamaa_core::{
    diagnostic::ConditionCode,
    table::ValueRef,
    text_extract::{Error, Step, StringExtract},
    value::Value,
};
pub(crate) fn reached() -> BTreeSet<ConditionCode> {
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
    // A missing source is answered with missing, never failed (COLUMN-10).
    assert_eq!(op.apply(ValueRef::Missing), Ok(Step::Recovered("missing")));
    assert_eq!(op.substitute("missing"), Value::Missing);
    assert_eq!(op.failure_diagnostic(Step::Recovered("missing")), None);
    let Error::Diagnostic(input) = op.apply(ValueRef::Int(7)).unwrap_err();
    let cases = [
        (
            input,
            "validation",
            "incompatible_input_type",
            "TEXT-1",
            format!("{path}.source"),
            vec![
                ("source", V::String("SRC.TEXT".into())),
                ("expected", V::String("str".into())),
                ("actual", V::String("int".into())),
            ],
        ),
        (
            op.failure_diagnostic(Step::NoMatch("abc")).unwrap(),
            "mapping",
            "unmatched_pattern",
            "COLUMN-10",
            path.into(),
            vec![
                ("value", V::String("abc".into())),
                ("pattern", V::String("(a)".into())),
            ],
        ),
        (
            op.unknown_diagnostic(),
            "validation",
            "unknown_field",
            "NAME-3",
            format!("{path}.source"),
            vec![("identifier", V::String("SRC.TEXT".into()))],
        ),
        (
            make("[", "0").declaration_diagnostic().unwrap().clone(),
            "validation",
            "invalid_regex",
            "TEXT-6",
            format!("{path}.pattern"),
            vec![
                ("pattern", V::String("[".into())),
                (
                    "reason",
                    V::String("a character class is never closed".into()),
                ),
            ],
        ),
        (
            make("(a)", "2").declaration_diagnostic().unwrap().clone(),
            "validation",
            "regex_group_out_of_range",
            "TEXT-10",
            format!("{path}.group"),
            vec![
                ("group", V::from(2)),
                ("group_count", V::from(1)),
                ("pattern", V::String("(a)".into())),
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
        V::Number((wide).parse().expect("canonical integer"))
    );
    reached
}
