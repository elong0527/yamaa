use std::collections::BTreeSet;
use yamaa_core::{
    diagnostic::{ConditionCode, ContextValue as V},
    table::ValueRef,
    text_case::{Case, Error, StringCase},
    value::Value,
};
pub(super) fn reached() -> BTreeSet<ConditionCode> {
    let operation = StringCase::new(
        "SRC.V".into(),
        "columns.V.derivation.str_case".into(),
        Case::Title,
        None,
    )
    .unwrap();
    let mut reached = BTreeSet::new();
    for (value, phase, condition, requirement, path, fields) in [
        (
            Value::Int(1),
            "validation",
            "incompatible_input_type",
            "REQ-0308",
            "columns.V.derivation.str_case.source",
            vec![("source", "SRC.V"), ("expected", "str"), ("actual", "int")],
        ),
        (
            Value::Missing,
            "mapping",
            "missing_input",
            "REQ-0334",
            "columns.V.derivation.str_case",
            vec![("variable", "SRC.V")],
        ),
    ] {
        let Error::Diagnostic(diagnostic) =
            operation.apply(ValueRef::from(&value), 100).unwrap_err()
        else {
            panic!("semantic cause");
        };
        let definition = diagnostic.definition();
        assert_eq!(
            (
                definition.phase,
                definition.condition,
                definition.requirement
            ),
            (phase, condition, Some(requirement))
        );
        assert_eq!(diagnostic.spec_paths, [path]);
        assert_eq!(
            diagnostic.context,
            fields
                .into_iter()
                .map(|(key, text)| (key.into(), V::Scalar(Value::Str(text.into()))))
                .collect()
        );
        assert_eq!(diagnostic.source_span, None);
        assert_eq!(diagnostic.operand_route, None);
        reached.insert(diagnostic.code);
    }
    reached
}
