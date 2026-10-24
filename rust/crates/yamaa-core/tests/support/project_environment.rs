use crate::support::roots::{Codelist, External, Item, Source};
use yamaa_core::{
    ast::CodelistType, project_environment::Draft, project_function::Language, value::Value,
};
/// A project run by the Python host, listing no function files, that
/// declares two invalid codelists of its own. The replay's case
/// `each_independent_finding_of_two_invalid_codelists_is_reported_in_order_at_its_path`
/// of `fixtures/diagnostics/environment.json` writes the same project as data.
pub fn draft() -> Draft {
    let items = vec![
        Item {
            value: Value::Bool(true),
            decode: Some("bad".into()),
            rank: Some(1),
            alias: None,
            extended: true,
        },
        Item {
            value: Value::Bool(true),
            decode: None,
            rank: None,
            alias: None,
            extended: false,
        },
    ];
    let list = Codelist {
        id: "SEX".into(),
        name: "Sex".into(),
        data_type: CodelistType::Text,
        extensible: false,
        alias: None,
        format_name: None,
        items: Some(items),
        external: Some(External {
            dictionary: "Other".into(),
            version: "1".into(),
            href: None,
        }),
    };
    Draft {
        language: Some(Language::Python),
        lock: None,
        functions: Some(vec![]),
        codelists: vec![Source {
            standard: None,
            codelists: vec![list.clone(), list],
        }
        .root()],
    }
}
