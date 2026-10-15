use yamaa_core::schema::{TypeError, TypeExpression, TypeNode, TypeResource};

#[test]
fn authored_nested_type_arena_retains_occurrences_and_original_spans() {
    let text = " dict[ str, list[dict[str, int]] ] ";
    let parsed = TypeExpression::parse(text).unwrap();
    assert_eq!(parsed.expression(), text);
    assert_eq!(parsed.names().collect::<Vec<_>>(), ["str", "str", "int"]);
    assert_eq!(
        parsed.nodes(),
        [
            TypeNode::Name(7..10),
            TypeNode::Name(22..25),
            TypeNode::Name(27..30),
            TypeNode::Dictionary { key: 1, value: 2 },
            TypeNode::List(3),
            TypeNode::Dictionary { key: 0, value: 4 },
        ]
    );
    assert_eq!(parsed.root(), 5);
}

#[test]
fn builtins_and_unresolved_names_are_syntax_not_bundle_resolution() {
    for name in [
        "str",
        "int",
        "float",
        "bool",
        "null",
        "list",
        "dict",
        "missing_type",
        "_X9",
    ] {
        let parsed = TypeExpression::parse(name).unwrap();
        assert_eq!(parsed.names().collect::<Vec<_>>(), [name]);
        assert_eq!(parsed.nodes(), [TypeNode::Name(0..name.len())]);
    }
    let parsed = TypeExpression::parse("\u{1c}\u{a0}list[\tstr\n]\u{2003}").unwrap();
    assert_eq!(parsed.names().collect::<Vec<_>>(), ["str"]);
}

#[test]
fn malformed_types_and_string_encoded_unions_are_rejected() {
    for text in [
        "",
        " ",
        "9name",
        "é",
        "str|int",
        "str int",
        "list[]",
        "list[str,int]",
        "dict[str]",
        "dict[,int]",
        "dict[str,]",
        "dict[str,int,bool]",
        "dict[str,list[int]",
        "list [str]",
        "set[str]",
        "list[str]]",
        "dict[str,int]junk",
    ] {
        assert!(
            matches!(TypeExpression::parse(text), Err(TypeError::Invalid { .. })),
            "{text:?}"
        );
    }
    assert_eq!(
        TypeExpression::parse("\u{a0}é"),
        Err(TypeError::Invalid { byte: 2 })
    );
}

#[test]
fn fixed_type_depth_refuses_the_next_layer_and_fresh_parse_succeeds() {
    let admitted = format!("{}str{}", "list[".repeat(127), "]".repeat(127));
    assert!(TypeExpression::parse(&admitted).is_ok());
    let too_deep = format!("list[{admitted}]");
    assert_eq!(
        TypeExpression::parse(&too_deep),
        Err(TypeError::Limit {
            resource: TypeResource::Depth,
            limit: 128
        })
    );
    assert!(TypeExpression::parse("dict[str,int]").is_ok());
}
