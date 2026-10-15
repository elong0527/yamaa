use yamaa_core::schema::{TypeError, TypeExpression, TypeLimits, TypeNode, TypeResource};

#[test]
fn authored_nested_type_arena_retains_occurrences_and_original_spans() {
    let text = " dict[ str, list[dict[str, int]] ] ";
    let parsed = TypeExpression::parse(text, TypeLimits::default()).unwrap();
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
        let parsed = TypeExpression::parse(name, TypeLimits::default()).unwrap();
        assert_eq!(parsed.names().collect::<Vec<_>>(), [name]);
        assert_eq!(parsed.nodes(), [TypeNode::Name(0..name.len())]);
    }
    let parsed =
        TypeExpression::parse("\u{1c}\u{a0}list[\tstr\n]\u{2003}", TypeLimits::default()).unwrap();
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
            matches!(
                TypeExpression::parse(text, TypeLimits::default()),
                Err(TypeError::Invalid { .. })
            ),
            "{text:?}"
        );
    }
    assert_eq!(
        TypeExpression::parse("\u{a0}é", TypeLimits::default()),
        Err(TypeError::Invalid { byte: 2 })
    );
}

#[test]
fn independent_byte_node_and_depth_limits_refuse_then_fresh_parse_succeeds() {
    for (limits, resource, limit, text) in [
        (
            TypeLimits {
                bytes: 2,
                ..TypeLimits::default()
            },
            TypeResource::Bytes,
            2,
            "str",
        ),
        (
            TypeLimits {
                nodes: 2,
                ..TypeLimits::default()
            },
            TypeResource::Nodes,
            2,
            "dict[str,int]",
        ),
        (
            TypeLimits {
                depth: 1,
                ..TypeLimits::default()
            },
            TypeResource::Depth,
            1,
            "list[str]",
        ),
        (
            TypeLimits {
                nodes: 0,
                ..TypeLimits::default()
            },
            TypeResource::Nodes,
            0,
            "str",
        ),
    ] {
        assert_eq!(
            TypeExpression::parse(text, limits),
            Err(TypeError::Limit { resource, limit })
        );
        assert!(TypeExpression::parse(text, TypeLimits::default()).is_ok());
    }
    let admitted = format!("{}str{}", "list[".repeat(63), "]".repeat(63));
    assert!(TypeExpression::parse(&admitted, TypeLimits::default()).is_ok());
    let too_deep = format!("list[{admitted}]");
    assert_eq!(
        TypeExpression::parse(
            &too_deep,
            TypeLimits {
                depth: usize::MAX,
                ..TypeLimits::default()
            }
        ),
        Err(TypeError::Limit {
            resource: TypeResource::Depth,
            limit: 64
        })
    );
}
