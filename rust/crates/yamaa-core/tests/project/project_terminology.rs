use yamaa_core::{
    project_terminology::{
        validate, BindingFinding, Catalogue, Codelist, DataType, External, Fault, Finding, Item,
        Source, Standard,
    },
    value::{ColumnType, Value},
};
fn item(value: Value) -> Item {
    Item {
        value,
        decode: None,
        rank: None,
        alias: None,
        extended: false,
    }
}
fn list() -> Codelist {
    Codelist {
        id: "SEX".into(),
        name: "Sex".into(),
        data_type: DataType::Text,
        extensible: false,
        alias: None,
        format_name: None,
        items: Some(vec![
            item(Value::Str("F".into())),
            item(Value::Str("M".into())),
        ]),
        external: None,
    }
}
fn source(list: Codelist) -> Source {
    Source {
        standard: None,
        codelists: vec![list],
    }
}
fn published(mut source: Source) -> Source {
    source.standard = Some(Standard {
        name: "CDISC/NCI".into(),
        publishing_set: "SDTM".into(),
        version: "2023-12-15".into(),
    });
    source
}

#[test]
fn global_duplicates_across_sources_have_no_override() {
    let first = source(list());
    let second = published(first.clone());
    assert_eq!(
        validate(&[first, second]),
        vec![
            Finding {
                source: 1,
                codelist: 0,
                fault: Fault::DuplicateId
            },
            Finding {
                source: 1,
                codelist: 0,
                fault: Fault::DuplicateName
            }
        ]
    );
}
#[test]
fn complete_independent_item_findings_survive_invalid_shape() {
    let mut list = list();
    list.external = Some(External {
        dictionary: "dictionary".into(),
        version: "1".into(),
        href: None,
    });
    list.items = Some(vec![
        Item {
            decode: Some("a".into()),
            rank: Some(1),
            extended: true,
            ..item(Value::Int(1))
        },
        item(Value::Int(1)),
    ]);
    assert_eq!(
        validate(&[source(list)])
            .iter()
            .map(|f| &f.fault)
            .collect::<Vec<_>>(),
        vec![
            &Fault::InvalidShape,
            &Fault::InvalidValue { item: 0 },
            &Fault::ExtensionNotAdmitted { item: 0 },
            &Fault::InvalidValue { item: 1 },
            &Fault::DuplicateValue { item: 1 },
            &Fault::PartialDecode,
            &Fault::PartialRank
        ]
    );
}
#[test]
fn extensions_require_both_a_published_source_and_an_extensible_list() {
    for extensible in [false, true] {
        for standard in [false, true] {
            let mut list = list();
            list.extensible = extensible;
            list.items.as_mut().unwrap()[0].extended = true;
            let mut source = source(list);
            if standard {
                source = published(source);
            }
            let f = validate(&[source]);
            assert_eq!(f.is_empty(), extensible && standard);
            if !f.is_empty() {
                assert_eq!(f[0].fault, Fault::ExtensionNotAdmitted { item: 0 });
            }
        }
    }
}
#[test]
fn numeric_equality_is_exact_at_i64_and_binary64_boundaries() {
    let mut list = list();
    list.data_type = DataType::Float;
    list.items = Some(vec![
        item(Value::Int(9_007_199_254_740_993)),
        item(Value::float(9_007_199_254_740_992.0)),
        item(Value::Int(9_007_199_254_740_992)),
        item(Value::float(-0.0)),
        item(Value::Int(0)),
        item(Value::Int(i64::MAX)),
        item(Value::float(9_223_372_036_854_775_808.0)),
    ]);
    let f = validate(&[source(list)]);
    assert_eq!(
        f.iter().map(|f| &f.fault).collect::<Vec<_>>(),
        vec![
            &Fault::DuplicateValue { item: 2 },
            &Fault::DuplicateValue { item: 4 }
        ]
    );
}
#[test]
fn text_keeps_nul_unicode_normalization_and_authored_order() {
    let mut list = list();
    list.items = Some(vec![
        item(Value::Str("e\u{301}".into())),
        item(Value::Str("\u{e9}".into())),
        item(Value::Str("\0".into())),
        item(Value::Str("".into())),
    ]);
    let expected = source(list);
    let catalogue = Catalogue::admit(vec![expected.clone()]).unwrap();
    assert_eq!(catalogue.sources(), &[expected]);
    assert_eq!(catalogue.enforced_items("SEX").unwrap().len(), 4);
}
#[test]
fn binding_checks_exact_column_type_and_item_sets() {
    let catalogue = Catalogue::admit(vec![source(list())]).unwrap();
    assert_eq!(
        catalogue.bind("unknown", ColumnType::Str, None),
        vec![BindingFinding::UnknownCodelist]
    );
    assert_eq!(
        catalogue.bind("SEX", ColumnType::Int, Some(&[Value::Str("F".into())])),
        vec![
            BindingFinding::TypeMismatch {
                expected: ColumnType::Str,
                actual: ColumnType::Int
            },
            BindingFinding::ValuesConflict
        ]
    );
    assert!(catalogue
        .bind(
            "SEX",
            ColumnType::Str,
            Some(&[
                Value::Str("M".into()),
                Value::Str("F".into()),
                Value::Str("F".into())
            ])
        )
        .is_empty());
    for external in [false, true] {
        let mut list = list();
        if external {
            list.items = None;
            list.external = Some(External {
                dictionary: "d".into(),
                version: "1".into(),
                href: Some("relative/path".into()),
            });
        } else {
            list.extensible = true;
        }
        let catalogue = Catalogue::admit(vec![source(list)]).unwrap();
        assert!(catalogue
            .bind(
                "SEX",
                ColumnType::Str,
                Some(&[Value::Str("anything".into())])
            )
            .is_empty());
        assert_eq!(catalogue.enforced_items("SEX"), None);
    }
}
#[test]
fn invalid_shapes_and_missing_boolean_fractional_item_types_are_rejected() {
    let mut shape = list();
    shape.items = None;
    assert_eq!(validate(&[source(shape)])[0].fault, Fault::InvalidShape);
    for (kind, value) in [
        (DataType::Text, Value::Missing),
        (DataType::Integer, Value::Bool(true)),
        (DataType::Integer, Value::float(1.5)),
        (DataType::Float, Value::Str("1".into())),
    ] {
        let mut list = list();
        list.data_type = kind;
        list.items = Some(vec![item(value)]);
        assert_eq!(
            validate(&[source(list)])[0].fault,
            Fault::InvalidValue { item: 0 }
        );
    }
}
