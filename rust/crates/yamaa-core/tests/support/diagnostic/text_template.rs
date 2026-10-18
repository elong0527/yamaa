use std::collections::BTreeSet;
use yamaa_core::json::Value as V;
use yamaa_core::{diagnostic::ConditionCode, text_template::StringTemplate};
pub(crate) fn reached() -> BTreeSet<ConditionCode> {
    let mut reached = BTreeSet::new();
    for (template, reason, placeholder) in [
        ("{A + B}", "invalid_placeholder", Some("A + B")),
        ("{}", "invalid_placeholder", Some("")),
        ("{A", "unmatched_brace", None),
        ("}", "unmatched_brace", None),
    ] {
        let op = StringTemplate::new(
            template.into(),
            "columns.RESULT.derivation.str_template".into(),
            None,
        )
        .unwrap();
        let diagnostic = op.declaration_diagnostic().unwrap();
        let def = diagnostic.definition();
        assert_eq!(
            (def.phase, def.condition, def.requirement),
            ("validation", "invalid_string_template", Some("REQ-0461"))
        );
        assert_eq!(
            diagnostic.spec_paths,
            ["columns.RESULT.derivation.str_template"]
        );
        let mut context =
            std::collections::BTreeMap::from([("reason".into(), V::String(reason.into()))]);
        if let Some(placeholder) = placeholder {
            context.insert("placeholder".into(), V::String(placeholder.into()));
        }
        assert_eq!(diagnostic.context, context);
        assert!(diagnostic.source_span.is_none());
        assert!(diagnostic.operand_route.is_none());
        reached.insert(diagnostic.code);
    }
    // A template, unlike a single-input verb, still fails on a missing part.
    let op = StringTemplate::new(
        "{A}:{B}".into(),
        "columns.RESULT.derivation.str_template".into(),
        None,
    )
    .unwrap();
    let diagnostic = op.missing_diagnostic(1).unwrap();
    let def = diagnostic.definition();
    assert_eq!(
        (def.phase, def.condition, def.requirement),
        ("mapping", "missing_input", Some("REQ-0334"))
    );
    assert_eq!(
        diagnostic.spec_paths,
        ["columns.RESULT.derivation.str_template"]
    );
    assert_eq!(
        diagnostic.context,
        std::collections::BTreeMap::from([("variable".into(), V::String("B".into()))])
    );
    assert!(diagnostic.source_span.is_none());
    assert!(diagnostic.operand_route.is_none());
    reached.insert(diagnostic.code);
    reached
}
