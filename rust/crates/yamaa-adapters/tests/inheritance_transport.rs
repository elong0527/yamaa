use serde_json::{json, Value};
use yamaa_adapters::{
    inheritance_transport::{traverse, Failure, TransportError, MAX_REPLY_BYTES},
    schema_transport::{compile_schema, CompiledSchema},
};

fn text(value: &str) -> Value {
    json!({"kind":"text","value":value})
}
fn schema() -> CompiledSchema {
    let nodes = vec![
        text("version"),
        text("1.0"),
        text("root_class"),
        text("schema_version"),
        text("type"),
        text("str"),
        json!({"kind":"mapping","entries":[[4,5]]}),
        json!({"kind":"mapping","entries":[[3,6]]}),
        text("parents"),
        text("type"),
        text("str"),
        text("list[str]"),
        json!({"kind":"sequence","items":[10,11]}),
        json!({"kind":"mapping","entries":[[9,12]]}),
        json!({"kind":"mapping","entries":[[8,13]]}),
        json!({"kind":"sequence","items":[7,14]}),
        json!({"kind":"mapping","entries":[[0,1],[2,15]]}),
    ];
    let request = json!({"protocol":"schema/1","schema":{"entry":0,"root_class":"root_class","modules":[{"name":"schema.yaml","document":{"root":16,"nodes":nodes}}]}});
    compile_schema(&request.to_string()).unwrap().0.unwrap()
}
fn layer(version: &str, parents: &[&str]) -> Value {
    let mut nodes = vec![text("schema_version"), text(version), text("parents")];
    let mut items = vec![];
    for parent in parents {
        items.push(nodes.len());
        nodes.push(text(parent));
    }
    let list = nodes.len();
    nodes.push(json!({"kind":"sequence","items":items}));
    let root = nodes.len();
    nodes.push(json!({"kind":"mapping","entries":[[0,1],[2,list]]}));
    json!({"root":root,"nodes":nodes})
}
fn request(document: Value) -> String {
    json!({"protocol":"inheritance/1","entry":{"identity":"/entry","display_path":"/entry"},"document":document}).to_string()
}
fn reply(value: Value) -> String {
    json!({"protocol":"inheritance/1","outcome":value}).to_string()
}
fn outcome(result: String) -> Value {
    let value: Value = serde_json::from_str(&result).unwrap();
    assert_eq!(value["protocol"], "inheritance/1");
    value["outcome"].clone()
}

#[test]
fn exact_owned_documents_and_callback_order_cross_the_closed_boundary() {
    let schema = schema();
    let mut calls = vec![];
    let result = traverse(
        &schema,
        &request(layer("1.0", &["a"])),
        |message, remaining| -> Result<_, ()> {
            assert!(remaining <= MAX_REPLY_BYTES);
            let message: Value = serde_json::from_str(message).unwrap();
            calls.push(message.clone());
            Ok(if message["operation"] == "canonicalize" {
                reply(json!({"status":"resolved","identity":"/a","display_path":"/written/a"}))
            } else {
                reply(json!({"status":"document","document":layer("1.0",&[])}))
            })
        },
    )
    .unwrap();
    assert_eq!(
        calls,
        [
            json!({"protocol":"inheritance/1","operation":"canonicalize","declaring":"/entry","path":"a"}),
            json!({"protocol":"inheritance/1","operation":"read","identity":"/a","display_path":"/written/a"}),
        ]
    );
    assert_eq!(
        outcome(result),
        json!({"status":"traversed","layers":[
            {"identity":"/a","display_path":"/written/a","document":layer("1.0",&[])},
            {"identity":"/entry","display_path":"/entry","document":layer("1.0",&["a"])},
        ]})
    );
}

#[test]
fn version_mismatch_has_exact_role_context_without_source_access() {
    let result = traverse(
        &schema(),
        &request(layer("0", &["unread"])),
        |_, _| -> Result<String, ()> { panic!("unexpected callback") },
    )
    .unwrap();
    assert_eq!(
        outcome(result),
        json!({"status":"invalid","source":"/entry","entry":"/entry","diagnostics":[{
            "condition":"schema_version_mismatch","path":"schema_version","requirement":"REQ-0245",
            "context":[{"name":"expected","value":{"kind":"text","value":"1.0"}},{"name":"actual","value":{"kind":"text","value":"0"}}],
        }]})
    );
}

#[test]
fn cycle_uses_canonical_names_and_does_not_read_the_entry_again() {
    let mut calls = 0;
    let result = traverse(
        &schema(),
        &request(layer("1.0", &["alias"])),
        |_, _| -> Result<_, ()> {
            calls += 1;
            assert_eq!(calls, 1);
            Ok(reply(
                json!({"status":"resolved","identity":"/entry","display_path":"/alias"}),
            ))
        },
    )
    .unwrap();
    assert_eq!(
        outcome(result),
        json!({"status":"invalid","source":"/entry","diagnostics":[{
            "condition":"inheritance_cycle","path":"parents","requirement":"REQ-0655",
            "context":[{"name":"reason","value":{"kind":"text","value":"parent_chain_returns_to_entry"}},
            {"name":"cycle","value":{"kind":"text_list","value":["/entry","/entry"]}}],
        }]})
    );
}

#[test]
fn wrong_operation_reply_and_extra_fields_fail_without_later_callbacks() {
    for response in [
        reply(json!({"status":"document","document":layer("1.0",&[])})),
        reply(json!({"status":"resolved","identity":"/a","display_path":"/a","extra":true})),
        json!({"protocol":"other","outcome":{"status":"unavailable"}}).to_string(),
    ] {
        let mut calls = 0;
        let error = traverse(
            &schema(),
            &request(layer("1.0", &["a", "later"])),
            |_, _| -> Result<_, ()> {
                calls += 1;
                Ok(response.clone())
            },
        )
        .unwrap_err();
        assert!(matches!(
            error,
            Failure::Transport(TransportError::InvalidReply)
        ));
        assert_eq!(calls, 1);
    }
}

#[test]
fn host_failures_are_opaque_and_do_not_poison_the_captured_schema() {
    let schema = schema();
    assert!(matches!(
        traverse(&schema, &request(layer("1.0", &["a"])), |_, _| Err::<
            String,
            _,
        >((
            73, "original"
        ))),
        Err(Failure::Host((73, "original")))
    ));
    assert_eq!(
        outcome(
            traverse(
                &schema,
                &request(layer("1.0", &[])),
                |_, _| -> Result<String, ()> { panic!("unexpected callback") }
            )
            .unwrap()
        )["status"],
        "traversed"
    );
}

#[test]
fn source_reply_byte_limit_is_cumulative_and_never_publishes_a_partial_graph() {
    let mut remaining_seen = vec![];
    let result = traverse(
        &schema(),
        &request(layer("1.0", &["a"])),
        |_, remaining| -> Result<_, ()> {
            remaining_seen.push(remaining);
            Ok(if remaining_seen.len() == 1 {
                let mut response =
                    reply(json!({"status":"resolved","identity":"/a","display_path":"/a"}));
                response.push_str(&" ".repeat(MAX_REPLY_BYTES / 2));
                response
            } else {
                " ".repeat(remaining + 1)
            })
        },
    )
    .unwrap();
    assert_eq!(remaining_seen.len(), 2);
    assert!(remaining_seen[1] < MAX_REPLY_BYTES / 2);
    assert_eq!(
        outcome(result),
        json!({"status":"resource_limit","phase":"inheritance","resource":"source_reply_bytes","limit":MAX_REPLY_BYTES})
    );
}

#[test]
fn missing_source_has_portable_context_and_declaring_identity() {
    let result = traverse(
        &schema(),
        &request(layer("1.0", &["missing"])),
        |_, _| -> Result<_, ()> { Ok(reply(json!({"status":"unavailable"}))) },
    )
    .unwrap();
    assert_eq!(
        outcome(result),
        json!({"status":"invalid","source":"/entry","diagnostics":[{
            "condition":"parent_not_found","path":"parents","requirement":"REQ-0654",
            "context":[{"name":"path","value":{"kind":"text","value":"missing"}}],
        }]})
    );
}

#[test]
fn malformed_requests_are_rejected_before_any_port_authority() {
    let good = request(layer("1.0", &[]));
    let mut extra: Value = serde_json::from_str(&good).unwrap();
    extra["extra"] = json!(true);
    for request in [extra.to_string(), "{".into()] {
        assert!(matches!(
            traverse(&schema(), &request, |_, _| -> Result<String, ()> {
                panic!("unexpected callback")
            }),
            Err(Failure::Transport(TransportError::InvalidRequest))
        ));
    }
}

#[test]
fn shared_independent_graph_truth_and_source_traces() {
    let sources: Vec<Vec<&str>> = include_str!("fixtures/inheritance_sources.tsv")
        .lines()
        .skip(1)
        .map(|line| line.split('\t').collect())
        .collect();
    let mut count = 0;
    for line in include_str!("fixtures/inheritance_traversal.tsv")
        .lines()
        .skip(1)
    {
        let fields: Vec<_> = line.split('\t').collect();
        let [name, schema, request, expected] = fields.as_slice() else {
            panic!("case shape")
        };
        let sources: Vec<_> = sources.iter().filter(|row| row[0] == *name).collect();
        let mut index = 0;
        let result = yamaa_adapters::inheritance_transport::interpret(
            schema,
            request,
            |message, maximum| -> Result<_, ()> {
                let source = sources[index];
                assert_eq!(message, source[1], "{name}: callback {index}");
                assert!(source[2].len() <= maximum);
                index += 1;
                Ok(source[2].to_string())
            },
        )
        .unwrap();
        assert_eq!(index, sources.len(), "{name}: missing callback");
        assert_eq!(result, *expected, "{name}: complete outcome");
        count += 1;
    }
    assert_eq!(count, 8);
}

#[test]
fn missing_parent_version_retains_both_file_identities_and_authored_context() {
    let missing = json!({"nodes":[{"kind":"text","value":"parents"},{"kind":"sequence","items":[]},{"kind":"mapping","entries":[[0,1]]}],"root":2});
    let mut calls = 0;
    let result = traverse(
        &schema(),
        &request(layer("1.0", &["a", "unread"])),
        |_, _| -> Result<_, ()> {
            calls += 1;
            Ok(if calls == 1 {
                reply(json!({"status":"resolved","identity":"/a","display_path":"/a"}))
            } else {
                assert_eq!(calls, 2);
                reply(json!({"status":"document","document":missing}))
            })
        },
    )
    .unwrap();
    assert_eq!(calls, 2);
    assert_eq!(
        outcome(result),
        json!({
            "status":"invalid","source":"/a","entry":"/entry","context_document":missing,
            "diagnostics":[{"condition":"schema_version_mismatch","path":"schema_version","requirement":"REQ-0656","context":[
                {"name":"expected","value":{"kind":"text","value":"1.0"}},
                {"name":"actual","value":{"kind":"null"}}
            ]}]
        })
    );
}
