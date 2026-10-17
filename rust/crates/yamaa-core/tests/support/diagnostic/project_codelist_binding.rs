use std::collections::BTreeSet;
use yamaa_core::{
    diagnostic::ConditionCode,
    json::Value as JsonValue,
    project_codelist_binding::{bind, Binding},
    project_terminology::{Catalogue, Codelist, DataType, Item, Source},
    value::{ColumnType, Value},
};
pub fn reached() -> BTreeSet<ConditionCode> {
    let cat = Catalogue::admit(vec![Source {
        standard: None,
        codelists: vec![Codelist {
            id: "SEX".into(),
            name: "Sex".into(),
            data_type: DataType::Text,
            extensible: false,
            alias: None,
            format_name: None,
            items: Some(vec![Item {
                value: Value::Str("F".into()),
                decode: None,
                rank: None,
                alias: None,
                extended: false,
            }]),
            external: None,
        }],
    }])
    .unwrap();
    let allowed = ["columns.X.verifications[0].allowed_values"];
    let base = Binding {
        column: 0,
        name: "X",
        kind: ColumnType::Str,
        path: yamaa_core::project_codelist_binding::Path::Authored("columns.X.submission.codelist"),
        codelist: "UNKNOWN",
        allowed_values: &[],
    };
    let Err(findings) = bind(
        &cat,
        &[
            base,
            Binding {
                codelist: "SEX",
                kind: ColumnType::Int,
                allowed_values: &allowed,
                ..base
            },
        ],
    ) else {
        panic!("real catalogue faults")
    };
    let mut reached = BTreeSet::new();
    for (finding, condition, requirement) in findings
        .iter()
        .zip([
            ("unknown_codelist", "REQ-0953"),
            ("codelist_type_mismatch", "REQ-0954"),
            ("redundant_declaration", "REQ-0955"),
        ])
        .map(|(f, (c, r))| (f, c, r))
    {
        let diagnostic = finding.diagnostic();
        assert_eq!(
            (
                diagnostic.definition().phase,
                diagnostic.definition().condition,
                diagnostic.definition().requirement
            ),
            ("validation", condition, Some(requirement))
        );
        reached.insert(diagnostic.code);
    }
    // The redundant declaration is the verification, and the binding that
    // already states its value set is named beside it.
    let redundant = findings[2].diagnostic();
    assert_eq!(redundant.spec_paths, allowed);
    assert_eq!(
        redundant.context["stated_by"],
        JsonValue::String("columns.X.submission.codelist".into())
    );
    assert_eq!(findings.len(), 3);
    reached
}
