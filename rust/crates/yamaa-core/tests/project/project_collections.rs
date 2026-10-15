use yamaa_core::{
    project_environment::{Draft, Environment},
    project_function::{Case, Definition, Function, Language},
    project_terminology::{Catalogue, Codelist, DataType, Item, Source},
    value::{ColumnType, Value},
};
fn definition() -> Definition {
    Definition {
        name: "constant".into(),
        function: "project.constant".into(),
        description: "μ\0".into(),
        params: vec![],
        returns: ColumnType::Int,
        may_return_missing: false,
        comparison_decimals: 4,
        tests: vec![Case {
            id: "ordinary".into(),
            covers: vec!["normal".into(), "boundary".into()],
            args: vec![],
            result: Value::Int(1),
        }],
    }
}
#[test]
fn function_metadata_keeps_exact_utf8_and_nul_bytes() {
    let function = Function::admit(Language::Python, definition()).unwrap();
    assert_eq!(
        function.definition().description.as_bytes(),
        "μ\0".as_bytes()
    );
}

#[test]
fn large_ordered_codelists_preserve_every_admitted_value() {
    let source = Source {
        standard: None,
        codelists: vec![Codelist {
            id: "LARGE".into(),
            name: "Large".into(),
            data_type: DataType::Integer,
            extensible: false,
            alias: None,
            format_name: None,
            external: None,
            items: Some(
                (0..10_000)
                    .map(|i| Item {
                        value: Value::Int(i),
                        decode: None,
                        rank: None,
                        alias: None,
                        extended: false,
                    })
                    .collect(),
            ),
        }],
    };
    let catalogue = Catalogue::admit(vec![source.clone()]).unwrap();
    let values: Vec<_> = (0..10_000).rev().map(Value::Int).collect();
    assert!(catalogue
        .bind("LARGE", ColumnType::Int, Some(&values))
        .is_empty());
}

#[test]
fn many_small_functions_retain_independent_admission() {
    use yamaa_core::project_environment::{LockKind, LockReference};
    let functions: Vec<_> = (0..200)
        .map(|i| {
            let mut def = definition();
            def.name = format!("function_{i}");
            def
        })
        .collect();
    for function in &functions {
        assert!(Function::admit(Language::Python, function.clone()).is_ok());
    }
    let draft = Draft {
        language: Some(Language::Python),
        lock: Some(LockReference {
            written: "uv.lock".into(),
            kind: LockKind::Uv,
        }),
        functions: Some(functions),
        codelists: vec![],
        has_study: false,
        submissions: vec![],
    };
    assert!(Environment::admit(Language::Python, draft).is_ok());
}
