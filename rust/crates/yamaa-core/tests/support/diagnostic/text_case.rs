use std::collections::BTreeSet;
use yamaa_core::json::Value as V;
use yamaa_core::{
    diagnostic::ConditionCode,
    table::ValueRef,
    text_case::{Case, Error, Step, StringCase},
    value::Value,
};
pub(crate) fn reached() -> BTreeSet<ConditionCode> {
    let operation = StringCase::new(
        "SRC.V".into(),
        "columns.V.derivation.str_case".into(),
        Case::Title,
        None,
    )
    .unwrap();
    // A missing source is answered with missing, never failed (COLUMN-10).
    assert_eq!(operation.apply(ValueRef::Missing), Ok(Step::Recovered));
    assert_eq!(operation.missing_substitute(), Value::Missing);
    // Only a source of another type fails.
    let Error::Diagnostic(diagnostic) = operation.apply(ValueRef::Int(1)).unwrap_err();
    let definition = diagnostic.definition();
    assert_eq!(
        (definition.phase, definition.condition, definition.rule),
        ("validation", "incompatible_input_type", Some("TEXT-1"))
    );
    assert_eq!(
        diagnostic.spec_paths,
        ["columns.V.derivation.str_case.source"]
    );
    assert_eq!(
        diagnostic.context,
        [("source", "SRC.V"), ("expected", "str"), ("actual", "int")]
            .into_iter()
            .map(|(key, text)| (key.into(), V::String(text.into())))
            .collect()
    );
    assert_eq!(diagnostic.source_span, None);
    assert_eq!(diagnostic.operand_route, None);
    BTreeSet::from([diagnostic.code])
}
