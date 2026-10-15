use std::collections::BTreeSet;
use yamaa_core::json::Value as JsonValue;
use yamaa_core::{
    diagnostic::ConditionCode,
    text_pad::{DeclarationError, StringPad},
};
pub(crate) fn reached() -> BTreeSet<ConditionCode> {
    let cause = StringPad::new("SRC.V", "columns.V.derivation.str_pad", "0").unwrap_err();
    assert_eq!(cause, DeclarationError::Width);
    let d = cause.diagnostic("columns.V.derivation.str_pad").unwrap();
    assert_eq!(
        (
            d.definition().phase,
            d.definition().condition,
            d.definition().requirement
        ),
        ("validation", "invalid_field_type", Some("REQ-1261"))
    );
    assert_eq!(d.spec_paths, ["columns.V.derivation.str_pad.width"]);
    assert_eq!(
        d.context,
        [(
            "expected".into(),
            JsonValue::String("a positive integer width".into())
        )]
        .into_iter()
        .collect()
    );
    assert_eq!(d.source_span, None);
    assert_eq!(d.operand_route, None);
    BTreeSet::from([d.code])
}
