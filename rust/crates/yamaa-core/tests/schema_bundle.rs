use yamaa_core::schema::{
    BundleError, BundleIssueKind as I, BundleLimits, BundleResource, Document, DocumentLimits,
    DocumentNode as N, SchemaAliasKind, SchemaModule, SchemaShape, SchemaStructure,
};

enum V {
    Text(&'static str),
    Bool(bool),
    Int(&'static str),
    Float(f64),
    List(Vec<V>),
    Map(Vec<(&'static str, V)>),
    Pairs(Vec<(V, V)>),
}

/// Build explicitly authored decoded schema documents without an interpreter oracle.
fn module(
    name: &str,
    declarations: Vec<(&'static str, V)>,
    includes: &[&'static str],
) -> SchemaModule {
    fn push(value: V, nodes: &mut Vec<N>) -> usize {
        let node = match value {
            V::Text(v) => N::Text(v.into()),
            V::Bool(v) => N::Boolean(v),
            V::Int(v) => N::Integer(v.into()),
            V::Float(v) => N::Float(v),
            V::List(items) => N::Sequence(items.into_iter().map(|v| push(v, nodes)).collect()),
            V::Map(items) => N::Mapping(
                items
                    .into_iter()
                    .map(|(key, value)| (push(V::Text(key), nodes), push(value, nodes)))
                    .collect(),
            ),
            V::Pairs(items) => N::Mapping(
                items
                    .into_iter()
                    .map(|(key, value)| (push(key, nodes), push(value, nodes)))
                    .collect(),
            ),
        };
        let id = nodes.len();
        nodes.push(node);
        id
    }
    let mut entries = vec![
        ("version", V::Text("1.0")),
        (
            "includes",
            V::List(includes.iter().map(|name| V::Text(name)).collect()),
        ),
    ];
    entries.extend(declarations);
    let mut nodes = Vec::new();
    let root = push(V::Map(entries), &mut nodes);
    SchemaModule {
        name: name.into(),
        document: Document::new(nodes, root, DocumentLimits::default()).unwrap(),
    }
}

fn descriptor(kind: &'static str) -> V {
    V::Map(vec![("type", V::Text(kind))])
}
fn reuse(name: &'static str) -> V {
    V::Map(vec![("fields_from", V::Text(name))])
}
fn field(name: &'static str, kind: &'static str) -> V {
    V::Map(vec![(name, descriptor(kind))])
}
fn kinds(error: BundleError) -> Vec<I> {
    match error {
        BundleError::Invalid(issues) => issues.into_iter().map(|i| i.kind).collect(),
        other => panic!("unexpected {other:?}"),
    }
}

#[test]
fn complete_bundle_resolves_forward_names_and_preserves_reused_default_origin() {
    let shared = module(
        "schema_shared.yaml",
        vec![
            (
                "shared",
                V::List(vec![
                    V::Map(vec![(
                        "id",
                        V::Map(vec![("type", V::Text("key")), ("required", V::Bool(true))]),
                    )]),
                    V::Map(vec![(
                        "label",
                        V::Map(vec![
                            ("type", V::Text("str")),
                            ("default", V::Text("untitled")),
                        ]),
                    )]),
                ]),
            ),
            ("key", descriptor("str")),
            (
                "recursive",
                V::Map(vec![(
                    "type",
                    V::List(vec![V::Text("recursive"), V::Text("str")]),
                )]),
            ),
        ],
        &[],
    );
    let root = module(
        "schema.yaml",
        vec![("root_class", V::List(vec![reuse("shared")]))],
        &["schema_shared.yaml"],
    );
    let bundle =
        SchemaStructure::admit(vec![shared, root], 1, "root_class", BundleLimits::default())
            .unwrap();
    assert_eq!(bundle.version(), "1.0");
    assert_eq!(
        bundle
            .root_class()
            .fields
            .iter()
            .map(|f| f.name.as_str())
            .collect::<Vec<_>>(),
        ["id", "label"]
    );
    let fields = &bundle.root_class().fields;
    assert!(bundle.descriptors()[fields[0].descriptor]
        .descriptor
        .required());
    let origin = &bundle.descriptors()[fields[1].descriptor];
    assert_eq!(origin.module, 0);
    assert_eq!(origin.path, "root_class.label");
    assert_eq!(
        bundle.modules()[0].document.nodes()[origin.descriptor.default_node().unwrap()],
        N::Text("untitled".into())
    );
    assert_eq!(
        bundle
            .aliases()
            .iter()
            .map(|a| a.name.as_str())
            .collect::<Vec<_>>(),
        ["key", "recursive"]
    );
}

#[test]
fn registry_contributions_merge_after_complete_include_loading() {
    let root = module(
        "schema.yaml",
        vec![
            ("root_class", V::List(vec![field("derive", "expression")])),
            (
                "expression",
                V::Map(vec![("registry", V::Text("expressions"))]),
            ),
            ("expressions", V::Map(vec![("source", descriptor("str"))])),
        ],
        &["schema_more.yaml"],
    );
    let more = module(
        "schema_more.yaml",
        vec![(
            "expressions",
            V::Map(vec![
                ("literal", descriptor("int")),
                ("wrapped", V::List(vec![field("value", "str")])),
            ]),
        )],
        &[],
    );
    let bundle =
        SchemaStructure::admit(vec![root, more], 0, "root_class", BundleLimits::default()).unwrap();
    assert_eq!(bundle.aliases()[0].kind, SchemaAliasKind::Registry(0));
    assert_eq!(
        bundle.registries()[0]
            .entries
            .iter()
            .map(|e| e.name.as_str())
            .collect::<Vec<_>>(),
        ["source", "literal", "wrapped"]
    );
    assert!(
        matches!(&bundle.registries()[0].entries[2].shape,SchemaShape::Class(fields) if fields[0].name=="value")
    );
}

#[test]
fn include_and_field_reuse_cycles_do_not_become_recursive_aliases() {
    let a = module(
        "schema_a.yaml",
        vec![("root_class", V::List(vec![]))],
        &["schema_b.yaml"],
    );
    let b = module("schema_b.yaml", vec![], &["schema_a.yaml"]);
    assert_eq!(
        kinds(
            SchemaStructure::admit(vec![a, b], 0, "root_class", BundleLimits::default())
                .unwrap_err()
        ),
        [I::IncludeCycle(vec![
            "schema_a.yaml".into(),
            "schema_b.yaml".into(),
            "schema_a.yaml".into()
        ])]
    );
    let root = module(
        "schema.yaml",
        vec![
            ("root_class", V::List(vec![reuse("a")])),
            ("a", V::List(vec![reuse("b")])),
            ("b", V::List(vec![reuse("a")])),
        ],
        &[],
    );
    assert_eq!(
        kinds(
            SchemaStructure::admit(vec![root], 0, "root_class", BundleLimits::default())
                .unwrap_err()
        ),
        [I::FieldsFromCycle(vec![
            "root_class".into(),
            "a".into(),
            "b".into(),
            "a".into()
        ])]
    );
}

#[test]
fn duplicate_reused_fields_and_unknown_types_preserve_stage_order() {
    let root = module(
        "schema.yaml",
        vec![
            (
                "root_class",
                V::List(vec![reuse("common"), field("id", "missing")]),
            ),
            ("common", V::List(vec![field("id", "str")])),
            ("orphan", V::Map(vec![])),
        ],
        &[],
    );
    assert_eq!(
        kinds(
            SchemaStructure::admit(vec![root], 0, "root_class", BundleLimits::default())
                .unwrap_err()
        ),
        [
            I::DuplicateField("id".into()),
            I::EmptyRegistry,
            I::UnreferencedRegistry,
            I::UnknownType("missing".into())
        ]
    );
}

#[test]
fn duplicate_registry_entries_and_closed_include_names_are_rejected() {
    let root = module(
        "schema.yaml",
        vec![
            ("root_class", V::List(vec![])),
            ("ops", V::Map(vec![("one", descriptor("str"))])),
        ],
        &["schema_more.yaml"],
    );
    let more = module(
        "schema_more.yaml",
        vec![("ops", V::Map(vec![("one", descriptor("int"))]))],
        &[],
    );
    assert_eq!(
        kinds(
            SchemaStructure::admit(vec![root, more], 0, "root_class", BundleLimits::default())
                .unwrap_err()
        ),
        [I::DuplicateRegistryEntry("one".into())]
    );
    for include in [
        "../schema_x.yaml",
        "schema_.yaml",
        "schema_X.yaml",
        "https://host/schema_x.yaml",
        "/schema_x.yaml",
    ] {
        let root = module(
            "schema.yaml",
            vec![("root_class", V::List(vec![]))],
            &[include],
        );
        assert!(matches!(
            kinds(
                SchemaStructure::admit(vec![root], 0, "root_class", BundleLimits::default())
                    .unwrap_err()
            )
            .as_slice(),
            [I::UnsafeInclude { .. }]
        ));
    }
    let root = module(
        "schema.yaml",
        vec![("root_class", V::List(vec![]))],
        &["schema_missing.yaml"],
    );
    assert_eq!(
        kinds(
            SchemaStructure::admit(vec![root], 0, "root_class", BundleLimits::default())
                .unwrap_err()
        ),
        [I::MissingInclude("schema_missing.yaml".into())]
    );
}

#[test]
fn reuse_and_input_budgets_bound_expansion_and_allow_independent_retry() {
    let root = module(
        "schema.yaml",
        vec![
            ("root_class", V::List(vec![reuse("common")])),
            ("common", V::List(vec![field("id", "str")])),
        ],
        &[],
    );
    for (limits, resource, limit) in [
        (
            BundleLimits {
                modules: 0,
                ..Default::default()
            },
            BundleResource::Modules,
            0,
        ),
        (
            BundleLimits {
                input_nodes: 0,
                ..Default::default()
            },
            BundleResource::InputNodes,
            0,
        ),
        (
            BundleLimits {
                input_text_bytes: 0,
                ..Default::default()
            },
            BundleResource::InputTextBytes,
            0,
        ),
        (
            BundleLimits {
                work: 0,
                ..Default::default()
            },
            BundleResource::Work,
            0,
        ),
        (
            BundleLimits {
                expanded_fields: 1,
                ..Default::default()
            },
            BundleResource::ExpandedFields,
            1,
        ),
    ] {
        assert!(
            matches!(SchemaStructure::admit(vec![root.clone()],0,"root_class",limits),Err(BundleError::Limit {resource:r,limit:l}) if r==resource && l==limit)
        );
        assert!(SchemaStructure::admit(
            vec![root.clone()],
            0,
            "root_class",
            BundleLimits::default()
        )
        .is_ok());
    }
}

#[test]
fn cross_module_registry_duplicates_use_exact_decoded_scalar_key_equality() {
    for (left, right) in [
        (V::Bool(true), V::Int("1")),
        (V::Int("1"), V::Float(1.0)),
        (V::Float(-0.0), V::Int("0")),
        (V::Int("9007199254740992"), V::Float(9007199254740992.0)),
    ] {
        let root = module(
            "schema.yaml",
            vec![
                ("root_class", V::List(vec![])),
                ("ops", V::Pairs(vec![(left, descriptor("str"))])),
            ],
            &["schema_more.yaml"],
        );
        let more = module(
            "schema_more.yaml",
            vec![("ops", V::Pairs(vec![(right, descriptor("str"))]))],
            &[],
        );
        assert_eq!(
            kinds(
                SchemaStructure::admit(vec![root, more], 0, "root_class", BundleLimits::default())
                    .unwrap_err()
            ),
            [I::DuplicateRegistryEntry("<non-text>".into())]
        );
    }
    let root = module(
        "schema.yaml",
        vec![
            ("root_class", V::List(vec![])),
            (
                "ops",
                V::Pairs(vec![(V::Int("9007199254740993"), descriptor("str"))]),
            ),
        ],
        &["schema_more.yaml"],
    );
    let more = module(
        "schema_more.yaml",
        vec![(
            "ops",
            V::Pairs(vec![(V::Float(9007199254740992.0), descriptor("str"))]),
        )],
        &[],
    );
    let result = kinds(
        SchemaStructure::admit(vec![root, more], 0, "root_class", BundleLimits::default())
            .unwrap_err(),
    );
    assert!(!result
        .iter()
        .any(|kind| matches!(kind, I::DuplicateRegistryEntry(_))));
    assert!(result.contains(&I::RegistryEntryName));
}
