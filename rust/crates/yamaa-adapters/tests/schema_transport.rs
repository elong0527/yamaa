use serde_json::{json, Value};
use yamaa_adapters::schema_transport::{
    compile_schema, interpret_schema, CompiledSchema, TransportError,
};

enum V {
    Text(&'static str),
    Int(&'static str),
    Bool(bool),
    Float(f64),
    Null,
    List(Vec<V>),
    Map(Vec<(&'static str, V)>),
}
use V::*;

#[test]
fn installed_hosts_share_independent_layer_composition_truth() {
    let fixture = include_str!("fixtures/schema_composition.tsv");
    assert_eq!(fixture.lines().count(), 7);
    for line in fixture.lines().skip(1) {
        let fields: Vec<_> = line.split('\t').collect();
        assert_eq!(fields.len(), 3);
        assert_eq!(
            interpret_schema(fields[1]).unwrap(),
            fields[2],
            "{}",
            fields[0]
        );
    }
}

#[test]
fn installed_hosts_share_independently_authored_complete_wire_truth() {
    for line in include_str!("fixtures/schema_transport.tsv")
        .lines()
        .skip(1)
    {
        let fields: Vec<_> = line.split('\t').collect();
        assert_eq!(fields.len(), 3);
        assert_eq!(
            interpret_schema(fields[1]).unwrap(),
            fields[2],
            "{}",
            fields[0]
        );
    }
}

#[test]
fn installed_hosts_share_independent_named_window_truth() {
    assert_eq!(
        include_str!("fixtures/schema_windows.tsv").lines().count(),
        7
    );
    for line in include_str!("fixtures/schema_windows.tsv").lines().skip(1) {
        let fields: Vec<_> = line.split('\t').collect();
        assert_eq!(fields.len(), 3);
        assert_eq!(
            interpret_schema(fields[1]).unwrap(),
            fields[2],
            "{}",
            fields[0]
        );
    }
}

fn tree(value: V) -> Value {
    fn push(value: V, nodes: &mut Vec<Value>) -> usize {
        let node = match value {
            Text(value) => json!({"kind":"text","value":value}),
            Int(value) => json!({"kind":"integer","value":value}),
            Bool(value) => json!({"kind":"boolean","value":value}),
            Float(value) => json!({"kind":"float","bits":format!("{:016x}",value.to_bits())}),
            Null => json!({"kind":"null"}),
            List(items) => {
                json!({"kind":"sequence","items":items.into_iter().map(|v|push(v,nodes)).collect::<Vec<_>>()})
            }
            Map(items) => {
                json!({"kind":"mapping","entries":items.into_iter().map(|(k,v)|(push(Text(k),nodes),push(v,nodes))).collect::<Vec<_>>()})
            }
        };
        let id = nodes.len();
        nodes.push(node);
        id
    }
    let mut nodes = Vec::new();
    let root = push(value, &mut nodes);
    json!({"nodes":nodes,"root":root})
}
fn descriptor(kind: &'static str) -> V {
    Map(vec![("type", Text(kind))])
}
fn field(name: &'static str, kind: &'static str) -> V {
    Map(vec![(name, descriptor(kind))])
}
fn schema(fields: Vec<V>, declarations: Vec<(&'static str, V)>) -> Value {
    let mut classes = vec![field("schema_version", "str")];
    classes.extend(fields);
    let mut entries = vec![("version", Text("1.0")), ("root_class", List(classes))];
    entries.extend(declarations);
    json!({"modules":[{"name":"schema.yaml","document":tree(Map(entries))}],"entry":0,"root_class":"root_class"})
}
fn compile(schema: Value) -> (CompiledSchema, Value) {
    let request = json!({"protocol":"schema/1","schema":schema}).to_string();
    let (compiled, response) = compile_schema(&request).unwrap();
    let outcome: Value = serde_json::from_str(&response).unwrap();
    assert_eq!(outcome["outcome"]["status"], "compiled");
    drop(request);
    (compiled.unwrap(), outcome["outcome"].clone())
}
fn query(compiled: &CompiledSchema, queries: Vec<Value>) -> Value {
    let response = compiled
        .analyze(&json!({"protocol":"schema/1","queries":queries}).to_string())
        .unwrap();
    serde_json::from_str::<Value>(&response).unwrap()["outcome"].clone()
}
fn item_id(metadata: &Value, name: &str) -> usize {
    metadata["classes"][0]["fields"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["name"] == name)
        .unwrap()["descriptor"]
        .as_u64()
        .unwrap() as usize
}

#[test]
fn prepared_and_stateless_services_return_the_same_independent_normalization_truth() {
    let schema = schema(
        vec![Map(vec![(
            "datasets",
            Map(vec![("type", List(vec![Text("str"), Text("list[str]")]))]),
        )])],
        vec![],
    );
    let (compiled, metadata) = compile(schema.clone());
    assert_eq!(metadata["root_class"], "root_class");
    assert_eq!(metadata["modules"], json!(["schema.yaml"]));
    assert_eq!(metadata["diagnostic_unicode_version"], "18.0.0");
    let id = item_id(&metadata, "datasets");
    let queries = vec![
        json!({"operation":"normalize_descriptor","descriptor":id,"fragment":false,"document":tree(Text("DM"))}),
        json!({"operation":"matching_member","descriptor":id,"fragment":false,"document":tree(Text("DM"))}),
        json!({"operation":"validate_document","document":tree(Map(vec![("schema_version",Text("old")),("other",Null)]))}),
    ];
    let result = query(&compiled, queries.clone());
    assert_eq!(
        result["results"][0]["document"],
        tree(List(vec![Text("DM")]))
    );
    assert_eq!(
        result["results"][0]["origins"],
        json!([
            {"source":{"kind":"input"},"node":0,"generated":false},
            {"source":{"kind":"input"},"node":0,"generated":true},
        ])
    );
    assert_eq!(
        result["results"][1],
        json!({"status":"matched","member":"str"})
    );
    assert_eq!(
        result["results"][2]["diagnostics"],
        json!([{
            "path":"schema_version","condition":"schema_version_mismatch","requirement":null,
            "context":[{"name":"expected","value":{"kind":"text","value":"1.0"}},{"name":"actual","value":{"kind":"input_value","node":1}}]
        }])
    );
    let stateless = interpret_schema(
        &json!({"protocol":"schema/1","schema":schema,"queries":queries}).to_string(),
    )
    .unwrap();
    let mut stateless = serde_json::from_str::<Value>(&stateless).unwrap()["outcome"].clone();
    assert_eq!(
        stateless.as_object_mut().unwrap().remove("schema").unwrap(),
        metadata
    );
    assert_eq!(stateless, result);
}

#[test]
fn transport_preserves_wide_integer_and_negative_zero_without_host_number_coercion() {
    let (compiled, metadata) = compile(schema(vec![field("value", "float")], vec![]));
    let id = item_id(&metadata, "value");
    for value in [
        Int("12345678901234567890123456789012345678901234567890"),
        Float(-0.0),
    ] {
        let input = tree(value);
        let result = query(
            &compiled,
            vec![
                json!({"operation":"normalize_descriptor","descriptor":id,"fragment":false,"document":input}),
            ],
        );
        assert_eq!(result["results"][0]["document"], input);
        if input["nodes"][0]["kind"] == "float" {
            assert!(result.to_string().contains("8000000000000000"));
        }
    }
    let result = query(
        &compiled,
        vec![
            json!({"operation":"validate_descriptor","descriptor":id,"fragment":false,"path":"root.value","document":tree(Bool(true))}),
        ],
    );
    assert_eq!(result["results"][0]["status"], "invalid");
    assert_eq!(
        result["results"][0]["diagnostics"][0]["context"],
        json!([
            {"name":"expected","value":{"kind":"text","value":"float"}},
            {"name":"actual","value":{"kind":"text","value":"bool"}},
        ])
    );
}

#[test]
fn schema_float_transport_preserves_sampled_bits_and_rejects_coercible_spellings() {
    let (compiled, metadata) = compile(schema(vec![field("value", "float")], vec![]));
    let id = item_id(&metadata, "value");
    let mut bits = 0x123456789abcdef0_u64;
    let mut checked = 0;
    for _ in 0..16384 {
        bits ^= bits << 13;
        bits ^= bits >> 7;
        bits ^= bits << 17;
        if !f64::from_bits(bits).is_finite() {
            continue;
        }
        let input = json!({"root":0,"nodes":[{"kind":"float","bits":format!("{bits:016x}")}]});
        let result = query(
            &compiled,
            vec![
                json!({"operation":"normalize_descriptor","descriptor":id,"fragment":false,"document":input}),
            ],
        );
        assert_eq!(result["results"][0]["document"], input);
        checked += 1;
    }
    assert!(checked > 16000);
    for bits in [
        "0",
        "3FF0000000000000",
        "g000000000000000",
        "7ff0000000000000",
        "7ff8000000000000",
        "fff0000000000000",
    ] {
        let request = json!({"protocol":"schema/1","queries":[{"operation":"normalize_descriptor","descriptor":id,"fragment":false,"document":{"root":0,"nodes":[{"kind":"float","bits":bits}]}}]}).to_string();
        assert_eq!(
            compiled.analyze(&request),
            Err(TransportError::InvalidDocument)
        );
    }
    let request = json!({"protocol":"schema/1","queries":[{"operation":"normalize_descriptor","descriptor":id,"fragment":false,"document":{"root":0,"nodes":[{"kind":"float","value":1.0}]}}]}).to_string();
    assert_eq!(
        compiled.analyze(&request),
        Err(TransportError::InvalidRequest)
    );
}

#[test]
fn malformed_envelopes_and_arenas_are_transport_errors_without_poisoning_a_snapshot() {
    let (compiled, _) = compile(schema(vec![], vec![]));
    for request in [
        "{}",
        r#"{"protocol":"schema/1","protocol":"schema/1","queries":[]}"#,
        r#"{"protocol":"schema/1","queries":[],"limits":{}}"#,
        r#"{"protocol":"schema/1","queries":[{"operation":"validate_document","document":{"root":0,"nodes":[{"kind":"null","value":1}]}}]}"#,
        r#"{"protocol":"schema/1","queries":[{"operation":"validate_document","document":{"root":0,"nodes":[{"kind":"text","value":"\ud800"}]}}]}"#,
    ] {
        assert_eq!(
            compiled.analyze(request),
            Err(TransportError::InvalidRequest),
            "{request}"
        );
    }
    for document in [
        json!({"nodes":[],"root":0}),
        json!({"nodes":[{"kind":"sequence","items":[0]}],"root":0}),
        json!({"nodes":[{"kind":"null"},{"kind":"sequence","items":[0,0]}],"root":1}),
        json!({"nodes":[{"kind":"integer","value":"01"}],"root":0}),
        json!({"nodes":[{"kind":"null"},{"kind":"null"}],"root":0}),
    ] {
        assert_eq!(compiled.analyze(&json!({"protocol":"schema/1","queries":[{"operation":"validate_document","document":document}]}).to_string()), Err(TransportError::InvalidDocument));
    }
    assert_eq!(
        compiled.analyze(r#"{"protocol":"different","queries":[]}"#),
        Err(TransportError::UnsupportedProtocol)
    );
    assert_eq!(
        query(&compiled, vec![]),
        json!({"status":"analyzed","results":[]})
    );
}

#[test]
fn schema_findings_and_invalid_defaults_never_produce_a_prepared_handle() {
    let invalid = schema(
        vec![Map(vec![(
            "value",
            Map(vec![("type", Text("str")), ("pattern", Text("a)|(?:b"))]),
        )])],
        vec![],
    );
    let (prepared, result) =
        compile_schema(&json!({"protocol":"schema/1","schema":invalid}).to_string()).unwrap();
    assert!(prepared.is_none());
    let result: Value = serde_json::from_str(&result).unwrap();
    assert_eq!(result["outcome"]["status"], "invalid_schema");
    assert_eq!(
        result["outcome"]["issues"][0]["issue"]["issue"]["code"],
        "invalid_pattern"
    );
    let invalid = schema(
        vec![Map(vec![(
            "value",
            Map(vec![("type", Text("str")), ("default", Int("2"))]),
        )])],
        vec![],
    );
    let (prepared, result) =
        compile_schema(&json!({"protocol":"schema/1","schema":invalid}).to_string()).unwrap();
    assert!(prepared.is_none());
    let result: Value = serde_json::from_str(&result).unwrap();
    assert_eq!(result["outcome"]["status"], "invalid_defaults");
    assert_eq!(
        result["outcome"]["defaults"][0]["diagnostics"][0]["path"],
        "root_class.value.default"
    );
}

#[test]
fn union_attempt_diagnostics_accumulate_across_queries_and_fresh_requests_reset() {
    let mut types: Vec<_> = (0..1000).map(|_| Text("int")).collect();
    types.push(Text("str"));
    let (compiled, metadata) = compile(schema(
        vec![Map(vec![("value", Map(vec![("type", List(types))]))])],
        vec![],
    ));
    let id = item_id(&metadata, "value");
    let one = json!({"operation":"validate_descriptor","descriptor":id,"fragment":false,"path":"value","document":tree(Text("accepted"))});
    let result = query(&compiled, vec![one.clone(); 80]);
    assert_eq!(result["results"][0]["status"], "valid");
    assert!(result["results"]
        .as_array()
        .unwrap()
        .iter()
        .any(|o| o["status"] == "resource_limit" && o["resource"] == "diagnostics"));
    assert_eq!(
        query(&compiled, vec![one.clone()])["results"][0]["status"],
        "valid"
    );
    assert_eq!(
        query(&compiled, vec![one; 257]),
        json!({"status":"resource_limit","phase":"query","resource":"queries","limit":256})
    );
}

#[test]
fn fragment_defaults_and_unsupported_normalized_keys_have_explicit_outcomes() {
    let (compiled, metadata) = compile(schema(
        vec![
            field("record", "record"),
            field("keys", "dict[key_type, int]"),
        ],
        vec![
            (
                "record",
                List(vec![
                    Map(vec![(
                        "name",
                        Map(vec![("type", Text("str")), ("required", Bool(true))]),
                    )]),
                    Map(vec![(
                        "enabled",
                        Map(vec![("type", Text("bool")), ("default", Bool(true))]),
                    )]),
                ]),
            ),
            (
                "key_type",
                Map(vec![("type", List(vec![Text("str"), Text("record")]))]),
            ),
        ],
    ));
    let result = query(
        &compiled,
        vec![
            json!({"operation":"normalize_descriptor","descriptor":item_id(&metadata,"record"),"fragment":true,"document":tree(Map(vec![]))}),
            json!({"operation":"normalize_descriptor","descriptor":item_id(&metadata,"keys"),"fragment":false,"document":tree(Map(vec![("DM",Int("1"))]))}),
        ],
    );
    assert_eq!(result["results"][0]["document"], tree(Map(vec![])));
    assert_eq!(
        result["results"][1],
        json!({"status":"unsupported","feature":"normalized_mapping_key"})
    );
}

#[test]
fn projected_types_bind_aliases_and_preserve_fragments_and_ordered_union_selection() {
    let (compiled, _) = compile(schema(
        vec![],
        vec![
            (
                "identifier",
                Map(vec![("type", Text("str")), ("pattern", Text("[A-Z]+"))]),
            ),
            (
                "record",
                List(vec![
                    Map(vec![(
                        "name",
                        Map(vec![("type", Text("identifier")), ("required", Bool(true))]),
                    )]),
                    Map(vec![(
                        "enabled",
                        Map(vec![("type", Text("bool")), ("default", Bool(true))]),
                    )]),
                ]),
            ),
        ],
    ));
    let results = query(&compiled, vec![
        json!({"operation":"matching_types","types":[],"fragment":true,"document":tree(Text("DM"))}),
        json!({"operation":"matching_types","types":["identifier","str"],"fragment":false,"document":tree(Text("dm"))}),
        json!({"operation":"validate_types","types":["identifier"],"path":"parents[0]","fragment":false,"document":tree(Text("dm"))}),
        json!({"operation":"normalize_types","types":["str","list[str]"],"fragment":false,"document":tree(Text("DM"))}),
        json!({"operation":"normalize_types","types":["record"],"fragment":true,"document":tree(Map(vec![]))}),
        json!({"operation":"normalize_types","types":["identifier","record"],"fragment":false,"document":tree(Text("DM"))}),
        json!({"operation":"matching_types","types":["dict[str, list[identifier]]"],"fragment":false,"document":tree(Map(vec![("domains",List(vec![Text("DM")]))]))}),
        json!({"operation":"validate_types","types":["record"],"path":"columns","fragment":false,"document":tree(Map(vec![]))}),
    ])["results"].clone();
    assert_eq!(results[0], json!({"status":"matched","member":null}));
    assert_eq!(results[1], json!({"status":"matched","member":"str"}));
    assert_eq!(results[2]["status"], "invalid");
    assert_eq!(results[2]["diagnostics"][0]["path"], "parents[0]");
    assert_eq!(results[3]["document"], tree(List(vec![Text("DM")])));
    assert_eq!(results[4]["document"], tree(Map(vec![])));
    // Arena node order is topological, not necessarily key-before-value. Read
    // the authored expected fields through their edges, including a default.
    let normalized = &results[5]["document"];
    let nodes = &normalized["nodes"];
    let entries = &nodes[normalized["root"].as_u64().unwrap() as usize]["entries"];
    for (entry, (key, value)) in entries
        .as_array()
        .unwrap()
        .iter()
        .zip([("name", json!("DM")), ("enabled", json!(true))])
    {
        assert_eq!(nodes[entry[0].as_u64().unwrap() as usize]["value"], key);
        assert_eq!(nodes[entry[1].as_u64().unwrap() as usize]["value"], value);
    }
    assert_eq!(entries.as_array().unwrap().len(), 2);
    assert_eq!(
        results[6],
        json!({"status":"matched","member":"dict[str, list[identifier]]"})
    );
    assert_eq!(results[7]["status"], "invalid");
    assert_eq!(results[7]["diagnostics"][0]["path"], "columns.name");
}

#[test]
fn projected_type_binding_rejects_unknown_names_and_retains_resource_failure_categories() {
    let (compiled, _) = compile(schema(vec![], vec![]));
    for types in [
        json!(["str", "unknown"]),
        json!(["list[unknown]"]),
        json!(["dict[str]"]),
    ] {
        let request = json!({"protocol":"schema/1","queries":[{"operation":"matching_types","types":types,"fragment":false,"document":tree(Text("DM"))}]}).to_string();
        assert_eq!(
            compiled.analyze(&request),
            Err(TransportError::InvalidQuery)
        );
    }
    let result = query(
        &compiled,
        vec![
            json!({"operation":"matching_types","types":["x".repeat(65537)],"fragment":false,"document":tree(Null)}),
        ],
    );
    assert_eq!(
        result["results"][0],
        json!({"status":"resource_limit","phase":"type","resource":"bytes","limit":65536})
    );
    let mut types = vec!["int"; 1000];
    types.push("str");
    let one = json!({"operation":"matching_types","types":types,"fragment":false,"document":tree(Text("DM"))});
    let result = query(&compiled, vec![one.clone(); 80]);
    assert_eq!(result["results"][0]["member"], "str");
    assert!(result["results"]
        .as_array()
        .unwrap()
        .iter()
        .any(|r| r["status"] == "resource_limit" && r["resource"] == "diagnostics"));
    assert_eq!(query(&compiled, vec![one])["results"][0]["member"], "str");
}

#[test]
fn reused_fields_report_original_descriptor_occurrences() {
    let source = schema(
        vec![Map(vec![("fields_from", Text("base"))])],
        vec![(
            "base",
            List(vec![Map(vec![(
                "item",
                Map(vec![
                    ("type", Text("str")),
                    ("description", Text("kept from source")),
                ]),
            )])]),
        )],
    );
    let (_, metadata) = compile(source.clone());
    let descriptors = metadata["descriptors"].as_array().unwrap();
    let reused = descriptors
        .iter()
        .find(|d| d["path"] == "root_class.item")
        .unwrap();
    let original = descriptors
        .iter()
        .find(|d| d["path"] == "base.item")
        .unwrap();
    assert_eq!(reused["source_node"], original["source_node"]);
    let nodes = &source["modules"][0]["document"]["nodes"];
    let descriptor = &nodes[reused["source_node"].as_u64().unwrap() as usize];
    let entry = &descriptor["entries"][1];
    assert_eq!(
        nodes[entry[0].as_u64().unwrap() as usize]["value"],
        "description"
    );
    assert_eq!(
        nodes[entry[1].as_u64().unwrap() as usize]["value"],
        "kept from source"
    );
}

#[test]
fn prepared_normalization_does_not_spend_query_budgets_readmitting_defaults() {
    let mut types = (0..1000).map(|_| Text("int")).collect::<Vec<_>>();
    types.push(Text("str"));
    let (compiled, metadata) = compile(schema(
        vec![Map(vec![(
            "value",
            Map(vec![("type", List(types)), ("default", Text("accepted"))]),
        )])],
        vec![],
    ));
    let mut queries = Vec::new();
    for _ in 0..80 {
        // The written integer overrides the expensive default, so no failed
        // default-union alternatives belong to this document's query budget.
        queries.push(json!({"operation":"normalize_document","document":tree(Map(vec![("schema_version",Text("1.0")),("value",Int("1"))]))}));
        queries.push(json!({"operation":"normalize_descriptor","descriptor":item_id(&metadata,"schema_version"),"fragment":false,"document":tree(Text("1.0"))}));
        queries.push(json!({"operation":"normalize_types","types":["str"],"fragment":false,"document":tree(Text("1.0"))}));
    }
    let output = query(&compiled, queries);
    let results = output["results"].as_array().unwrap();
    assert_eq!(results.len(), 240);
    for (index, result) in results.iter().enumerate() {
        assert_eq!(result["status"], "normalized", "query {index}: {result}");
        let expected = if index % 3 == 0 {
            tree(Map(vec![
                ("schema_version", Text("1.0")),
                ("value", Int("1")),
            ]))
        } else {
            tree(Text("1.0"))
        };
        assert_eq!(result["document"], expected);
    }
}

#[test]
fn window_queries_require_explicit_boolean_mode_and_share_batch_expansion_budgets() {
    let (compiled, _) = compile(schema(
        vec![
            field("windows", "dict[str, dict]"),
            field("refs", "list[window_selection]"),
        ],
        vec![("window_selection", descriptor("str"))],
    ));
    for extra in [
        json!({}),
        json!({"strict":"true"}),
        json!({"strict":true,"fallback":true}),
    ] {
        let mut item = json!({"operation":"expand_windows","document":tree(Map(vec![]))});
        item.as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        assert_eq!(
            compiled.analyze(&json!({"protocol":"schema/1","queries":[item]}).to_string()),
            Err(TransportError::InvalidRequest)
        );
    }
    let source = tree(Map(vec![
        (
            "windows",
            Map(vec![(
                "W",
                Map(vec![(
                    "order_by",
                    List((0..41).map(|_| Text("SEQ")).collect()),
                )]),
            )]),
        ),
        ("refs", List((0..1000).map(|_| Text("W")).collect())),
    ]));
    let expand = json!({"operation":"expand_windows","document":source,"strict":true});
    let result = query(
        &compiled,
        vec![expand.clone(), expand.clone(), expand.clone()],
    );
    assert_eq!(result["results"][0]["status"], "expanded");
    assert_eq!(result["results"][1]["status"], "expanded");
    assert_eq!(result["results"][2]["status"], "resource_limit");
    assert_eq!(
        result["results"][2],
        json!({"status":"resource_limit","phase":"normalization","resource":"nodes","limit":131072})
    );
    assert_eq!(
        query(&compiled, vec![expand])["results"][0]["status"],
        "expanded"
    );
}

#[test]
fn composition_batches_share_copy_limits_and_reject_unknown_fields() {
    let schema = schema(vec![field("windows", "dict[str, dict]")], vec![]);
    let (compiled, _) = compile(schema);
    let mut nodes = Vec::new();
    let mut emit = |node: Value| {
        let id = nodes.len();
        nodes.push(node);
        id
    };
    let mut definitions = Vec::new();
    for index in 0..1000 {
        let key = emit(json!({"kind":"text","value":format!("W{index}")}));
        let order = emit(json!({"kind":"text","value":"order_by"}));
        let items = (0..41)
            .map(|_| emit(json!({"kind":"text","value":"SEQ"})))
            .collect::<Vec<_>>();
        let sequence = emit(json!({"kind":"sequence","items":items}));
        let definition = emit(json!({"kind":"mapping","entries":[[order,sequence]]}));
        definitions.push((key, definition));
    }
    let key = emit(json!({"kind":"text","value":"windows"}));
    let definitions = emit(json!({"kind":"mapping","entries":definitions}));
    let root = emit(json!({"kind":"mapping","entries":[[key,definitions]]}));
    let layer = json!({"nodes":nodes,"root":root});
    let compose = json!({"operation":"compose_layers","layers":[layer]});
    let batch = query(&compiled, vec![compose.clone(), compose.clone()]);
    assert_eq!(batch["results"][0]["status"], "composed");
    assert_eq!(
        batch["results"][1],
        json!({"status":"resource_limit","phase":"normalization","resource":"nodes","limit":131072})
    );
    assert_eq!(
        query(&compiled, vec![compose.clone()])["results"][0]["status"],
        "composed"
    );
    let mut unknown = compose;
    unknown["extra"] = json!(true);
    assert_eq!(
        compiled.analyze(&json!({"protocol":"schema/1","queries":[unknown]}).to_string()),
        Err(TransportError::InvalidRequest)
    );
}
