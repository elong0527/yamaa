use serde_json::{json, Value};
use yamaa_adapters::reference_transport::{
    analyze_references, compile_reference_catalog, TransportError, MAX_REQUEST_BYTES,
};

/// The same authored truth qualifies one-shot and prepared immutable catalog queries.
#[test]
fn shared_truth_and_prepared_catalog_ownership() {
    for line in include_str!("fixtures/reference_binding.tsv")
        .lines()
        .skip(1)
        .chain(include_str!("fixtures/reference_scope.tsv").lines().skip(1))
        .chain(
            include_str!("fixtures/reference_intermediate.tsv")
                .lines()
                .skip(1),
        )
        .chain(include_str!("fixtures/reference_keys.tsv").lines().skip(1))
        .chain(
            include_str!("fixtures/reference_match_values.tsv")
                .lines()
                .skip(1),
        )
        .chain(
            include_str!("fixtures/reference_relations.tsv")
                .lines()
                .skip(1),
        )
    {
        let fields: Vec<_> = line.split('\t').collect();
        let expected: Value = serde_json::from_str(fields[2]).unwrap();
        let mut request: Value = serde_json::from_str(fields[1]).unwrap();
        assert_eq!(
            analyze_references(fields[1]).unwrap(),
            fields[2],
            "{}",
            fields[0]
        );
        let catalog_json =
            json!({"protocol":"reference-catalog/1","catalog":request["catalog"]}).to_string();
        let (catalog, status) = compile_reference_catalog(&catalog_json).unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(&status).unwrap(),
            json!({"protocol":"reference-catalog/1","outcome":{"status":"complete","results":[]}})
        );
        let query =
            json!({"protocol":"reference-queries/1","queries":request["queries"]}).to_string();
        // Neither later host mutation nor release of input strings changes the compiled catalog.
        request["catalog"] = json!(null);
        drop(catalog_json);
        let catalog = catalog.unwrap();
        for _ in 0..2 {
            let actual: Value = serde_json::from_str(&catalog.analyze(&query).unwrap()).unwrap();
            assert_eq!(actual, expected, "{}", fields[0]);
        }
    }
}

/// A valid fixed catalog supports shape/admission tests without computed expectations.
fn request() -> Value {
    json!({"protocol":"reference-analysis/1","catalog":{
        "outputs":[{"name":"A","type":"int"}],"datasets":[]},
        "queries":[{"kind":"bind","name":"A"}]})
}

/// The wire preserves field-before-dependency order and rejects malformed target contexts.
#[test]
fn intermediate_queries_batch_prepared_and_rejection() {
    let mut data = request();
    data["catalog"]["datasets"] = json!([{"name":"SRC","fields":[{"name":"K","type":"str"}]}]);
    let target = json!({"source":{"kind":"dataset","name":"SRC"},"derived":["D"],"readable":[],"dependencies":["DONOR.K","OTHER.K","LATER"]});
    data["queries"] = json!([
        {"kind":"validate_intermediate","field":"D","target":target},
        {"kind":"validate_intermediate","field":"ABSENT","target":target},
        {"kind":"validate_intermediate_read","read":{"reader":"reader","target_name":"lookup","target":target,"field":"ABSENT","donor_dataset":"DONOR","visible":["K"]}}
    ]);
    let expected = json!({"protocol":"reference-analysis/1","outcome":{"status":"complete","results":[
        {"kind":"intermediate_validation","diagnostics":[]},
        {"kind":"intermediate_validation","diagnostics":[{"kind":"unknown_field"}]},
        {"kind":"intermediate_validation","diagnostics":[{"kind":"unknown_field"},{"kind":"unavailable_dependency","dependency":1},{"kind":"unavailable_dependency","dependency":2}]}
    ]}});
    assert_eq!(
        serde_json::from_str::<Value>(&analyze_references(&data.to_string()).unwrap()).unwrap(),
        expected
    );
    let (prepared, _) = compile_reference_catalog(
        &json!({"protocol":"reference-catalog/1","catalog":data["catalog"]}).to_string(),
    )
    .unwrap();
    let prepared = prepared.unwrap();
    let query = json!({"protocol":"reference-queries/1","queries":data["queries"]});
    assert_eq!(
        serde_json::from_str::<Value>(&prepared.analyze(&query.to_string()).unwrap()).unwrap(),
        expected
    );
    for (pointer, value) in [
        (
            "/queries/0/target/source",
            json!({"kind":"self","name":"SRC"}),
        ),
        (
            "/queries/0/target/source",
            json!({"kind":"dataset","name":"SRC","fields":[]}),
        ),
        ("/queries/0/target/derived", json!([false])),
        ("/queries/2/read/visible", json!(null)),
    ] {
        let mut invalid = query.clone();
        *invalid.pointer_mut(pointer).unwrap() = value;
        assert_eq!(
            prepared.analyze(&invalid.to_string()),
            Err(TransportError::InvalidEnvelope)
        );
    }
    let mut limit = query.clone();
    limit["queries"][0]["target"]["readable"] = json!(vec!["K"; 65_537]);
    let outcome: Value =
        serde_json::from_str(&prepared.analyze(&limit.to_string()).unwrap()).unwrap();
    assert_eq!(
        outcome["outcome"],
        json!({"status":"limit","resource":"intermediate_entries","limit":"65536","required":"65543"})
    );
    assert_eq!(
        serde_json::from_str::<Value>(&prepared.analyze(&query.to_string()).unwrap()).unwrap(),
        expected
    );
}

/// Strict closed shapes reject invalid types, duplicate fields and forged protocol extensions.
#[test]
fn strict_transport_admission() {
    for (pointer, value) in [
        ("/catalog/outputs/0/type", json!("bool")),
        ("/catalog/outputs/0/name", json!(1)),
        ("/queries/0/kind", json!("evaluate")),
    ] {
        let mut data = request();
        *data.pointer_mut(pointer).unwrap() = value;
        assert_eq!(
            analyze_references(&data.to_string()),
            Err(TransportError::InvalidEnvelope)
        );
    }
    let mut data = request();
    data["catalog"]["extra"] = json!({});
    assert_eq!(
        analyze_references(&data.to_string()),
        Err(TransportError::InvalidEnvelope)
    );
    for key in ["protocol", "catalog", "queries"] {
        let data = request();
        let duplicate = format!("{{\"{key}\":{},{}", data[key], &data.to_string()[1..]);
        assert_eq!(
            analyze_references(&duplicate),
            Err(TransportError::InvalidEnvelope)
        );
    }
    let mut data = request();
    data["protocol"] = json!("future/2");
    assert_eq!(
        analyze_references(&data.to_string()),
        Err(TransportError::UnsupportedProtocol)
    );
    assert_eq!(
        analyze_references(&" ".repeat(MAX_REQUEST_BYTES + 1)),
        Err(TransportError::RequestLimit)
    );
    assert!(matches!(
        compile_reference_catalog(&" ".repeat(MAX_REQUEST_BYTES + 1)),
        Err(TransportError::RequestLimit)
    ));
}

/// Omitted optional checks mean no type or phase restriction in either interface.
#[test]
fn omitted_validation_fields_preserve_wire_defaults() {
    let mut data = request();
    data["queries"] = json!([{"kind":"validate_output","name":"A","candidates":[]}]);
    let expected = json!({"protocol":"reference-analysis/1","outcome":{
        "status":"complete","results":[{"kind":"validation","diagnostic":null}]}});
    let batch: Value =
        serde_json::from_str(&analyze_references(&data.to_string()).unwrap()).unwrap();
    assert_eq!(batch, expected);

    let (catalog, _) = compile_reference_catalog(
        &json!({"protocol":"reference-catalog/1","catalog":data["catalog"]}).to_string(),
    )
    .unwrap();
    let prepared: Value = serde_json::from_str(
        &catalog
            .unwrap()
            .analyze(
                &json!({"protocol":"reference-queries/1","queries":data["queries"]}).to_string(),
            )
            .unwrap(),
    )
    .unwrap();
    assert_eq!(prepared, expected);
}

/// Metadata errors reject the whole batch; completed language outcomes are not partial output.
#[test]
fn invalid_catalog_and_context_then_fresh_query() {
    let mut data = request();
    data["catalog"]["outputs"] = json!([{"name":"A","type":"int"},{"name":"A","type":"str"}]);
    assert_eq!(
        analyze_references(&data.to_string()),
        Err(TransportError::InvalidCatalog)
    );
    let (catalog, _) = compile_reference_catalog(
        &json!({"protocol":"reference-catalog/1","catalog":request()["catalog"]}).to_string(),
    )
    .unwrap();
    let catalog = catalog.unwrap();
    for query in [
        json!({"kind":"validate_output","name":"A","expected":null,"available":[1],"candidates":[]}),
        json!({"kind":"validate_output","name":"A","expected":null,"available":null,"candidates":[0]}),
        json!({"kind":"validate_output","name":"SRC.A","expected":null,"available":null,"candidates":[]}),
    ] {
        let queries =
            json!({"protocol":"reference-queries/1","queries":[{"kind":"bind","name":"A"},query]});
        assert_eq!(
            catalog.analyze(&queries.to_string()),
            Err(TransportError::InvalidQuery)
        );
    }
    assert_eq!(
        catalog
            .analyze(r#"{"protocol":"reference-queries/1","queries":[{"kind":"bind","name":"A"}]}"#)
            .unwrap(),
        r#"{"protocol":"reference-analysis/1","outcome":{"status":"complete","results":[{"kind":"binding","binding":{"kind":"output","column":0,"type":"int"}}]}}"#
    );
}

/// Limits expose exact counts and never hand callers a partially compiled catalog.
#[test]
fn limits_are_not_language_diagnostics() {
    let large = json!({"protocol":"reference-catalog/1","catalog":{"outputs":vec![json!({"name":"A","type":"str"});4097],"datasets":[]}});
    let (catalog, outcome) = compile_reference_catalog(&large.to_string()).unwrap();
    assert!(catalog.is_none());
    assert_eq!(
        serde_json::from_str::<Value>(&outcome).unwrap(),
        json!({"protocol":"reference-catalog/1","outcome":{"status":"limit","resource":"outputs","limit":"4096","required":"4097"}})
    );
    let mut data = request();
    data["queries"] = json!(vec![json!({"kind":"bind","name":"A"}); 4097]);
    let outcome: Value =
        serde_json::from_str(&analyze_references(&data.to_string()).unwrap()).unwrap();
    assert_eq!(
        outcome["outcome"],
        json!({"status":"limit","resource":"queries","limit":"4096","required":"4097"})
    );
    data["queries"] = json!([{"kind":"bind","name":"a".repeat(65537)}]);
    let outcome: Value =
        serde_json::from_str(&analyze_references(&data.to_string()).unwrap()).unwrap();
    assert_eq!(
        outcome["outcome"],
        json!({"status":"limit","resource":"reference_bytes","limit":"65536","required":"65537"})
    );
}

/// Qualified queries reject malformed context and admit written budgets before name findings.
#[test]
fn qualified_scope_admission_and_capabilities() {
    assert_eq!(
        yamaa_adapters::reference_transport::capabilities(),
        r#"{"protocol":"reference-analysis/1","features":["binding","output_validation","qualified_validation","intermediate_validation","key_relations","match_value_typing","relation_binding"]}"#
    );
    let mut data = request();
    data["queries"] = json!([{"kind":"validate_qualified", "name":"SRC.N", "expected":null,
        "scope":{"drivers":["SRC"], "current_driver":false, "reach":"scalar", "joined":false,
            "phase":{"kind":"row","group_by":null}}}]);
    for (pointer, value) in [
        ("/queries/0/scope/reach", json!("future")),
        ("/queries/0/scope/drivers", json!([false])),
        ("/queries/0/scope/current_driver", json!("yes")),
        ("/queries/0/scope/phase", json!({"kind":"future"})),
        (
            "/queries/0/scope/phase",
            json!({"kind":"column","groups":null}),
        ),
    ] {
        let mut invalid = data.clone();
        *invalid.pointer_mut(pointer).unwrap() = value;
        assert_eq!(
            analyze_references(&invalid.to_string()),
            Err(TransportError::InvalidEnvelope)
        );
    }
    let mut invalid = data.clone();
    invalid["queries"][0]["scope"]["extra"] = json!(true);
    assert_eq!(
        analyze_references(&invalid.to_string()),
        Err(TransportError::InvalidEnvelope)
    );
    let mut invalid = data.clone();
    invalid["queries"][0]["name"] = json!("A");
    assert_eq!(
        analyze_references(&invalid.to_string()),
        Err(TransportError::InvalidQuery)
    );
    data["queries"][0]["scope"]["drivers"] = json!(vec!["SRC"; 257]);
    let result: Value =
        serde_json::from_str(&analyze_references(&data.to_string()).unwrap()).unwrap();
    assert_eq!(
        result["outcome"],
        json!({"status":"limit","resource":"scope_drivers","limit":"256","required":"257"})
    );
    data["queries"][0]["scope"]["drivers"] = json!(["SRC"]);
    let result: Value =
        serde_json::from_str(&analyze_references(&data.to_string()).unwrap()).unwrap();
    assert_eq!(
        result["outcome"],
        json!({"status":"complete","results":[{"kind":"qualified_validation","diagnostics":[{"kind":"unknown_field"}]}]})
    );
}

/// Key queries reject malformed types/fields without publishing an earlier valid result.
#[test]
fn key_query_admission_and_reuse() {
    let data = request();
    let (catalog, _) = compile_reference_catalog(
        &json!({"protocol":"reference-catalog/1","catalog":data["catalog"]}).to_string(),
    )
    .unwrap();
    let catalog = catalog.unwrap();
    for invalid in [
        json!({"kind":"comparable_types","left":"number","right":"int"}),
        json!({"kind":"comparable_types","left":null,"right":"int"}),
        json!({"kind":"comparable_types","left":"int","right":"int","coerce":true}),
        json!({"kind":"infer_keys","keys":[false],"fields":[]}),
        json!({"kind":"infer_keys","keys":["A"],"fields":[{"name":"A","type":"bool"}]}),
        json!({"kind":"infer_keys","keys":["A"],"fields":[{"name":"A","type":"int","value":1}]}),
    ] {
        assert_eq!(catalog.analyze(&json!({"protocol":"reference-queries/1","queries":[{"kind":"bind","name":"A"},invalid]}).to_string()), Err(TransportError::InvalidEnvelope));
    }
    for fields in [
        json!([{"name":"A","type":"int"},{"name":"A","type":"str"}]),
        json!([{"name":"","type":"int"}]),
    ] {
        assert_eq!(catalog.analyze(&json!({"protocol":"reference-queries/1","queries":[{"kind":"bind","name":"A"},{"kind":"infer_keys","keys":["ABSENT"],"fields":fields}]}).to_string()), Err(TransportError::InvalidQuery));
    }
    let limit: Value = serde_json::from_str(&catalog.analyze(&json!({"protocol":"reference-queries/1","queries":[{"kind":"infer_keys","keys":vec!["A"; 65537],"fields":[]}]}).to_string()).unwrap()).unwrap();
    assert_eq!(
        limit["outcome"],
        json!({"status":"limit","resource":"key_entries","limit":"65536","required":"65537"})
    );
    let recovered: Value = serde_json::from_str(&catalog.analyze(r#"{"protocol":"reference-queries/1","queries":[{"kind":"infer_keys","keys":["A"],"fields":[{"name":"A","type":"float"}]}]}"#).unwrap()).unwrap();
    assert_eq!(
        recovered["outcome"],
        json!({"status":"complete","results":[{"kind":"key_inference","inference":{"kind":"keys","keys":[0]}}]})
    );
}

/// Strict shapes reject unknown data; a late resource failure cannot expose earlier results.
#[test]
fn match_value_admission_is_atomic_and_reusable() {
    let mut data = request();
    for expression in [
        json!({"kind":"source","name":true}),
        json!({"kind":"literal","scalar":"date"}),
        json!({"kind":"literal","scalar":"int","value":1}),
        json!({"kind":"operation","name":"compute","payload":{}}),
        json!({"kind":"unresolved","name":"hidden"}),
        json!({"kind":"future"}),
    ] {
        data["queries"] = json!([{"kind":"match_value_type","expression":expression}]);
        assert_eq!(
            analyze_references(&data.to_string()),
            Err(TransportError::InvalidEnvelope)
        );
    }
    let (catalog, _) = compile_reference_catalog(
        &json!({"protocol":"reference-catalog/1","catalog":data["catalog"]}).to_string(),
    )
    .unwrap();
    let catalog = catalog.unwrap();
    data["queries"] = json!([
        {"kind":"match_value_type","expression":{"kind":"literal","scalar":"bool"}},
        {"kind":"match_value_type","expression":{"kind":"operation","name":"é".repeat(32769)}}
    ]);
    let expected = json!({"protocol":"reference-analysis/1","outcome":{"status":"limit","resource":"operation_bytes","limit":"65536","required":"65538"}});
    assert_eq!(
        serde_json::from_str::<Value>(&analyze_references(&data.to_string()).unwrap()).unwrap(),
        expected
    );
    let mut query = json!({"protocol":"reference-queries/1","queries":data["queries"]});
    assert_eq!(
        serde_json::from_str::<Value>(&catalog.analyze(&query.to_string()).unwrap()).unwrap(),
        expected
    );
    query["queries"] =
        json!([{"kind":"match_value_type","expression":{"kind":"operation","name":"compute"}}]);
    assert_eq!(
        serde_json::from_str::<Value>(&catalog.analyze(&query.to_string()).unwrap()).unwrap(),
        json!({"protocol":"reference-analysis/1","outcome":{"status":"complete","results":[{"kind":"match_value_type","result_type":"float"}]}})
    );
}

/// Whole-relation queries retain strict shapes, atomic late failure and prepared catalog reuse.
#[test]
fn relation_binding_admission_and_atomic_failure() {
    let mut data = request();
    data["catalog"]["datasets"] = json!([{"name":"EMPTY","fields":[]}]);
    for query in [
        json!({"kind":"bind_relation","name":false}),
        json!({"kind":"bind_relation","name":"EMPTY","field":"N"}),
    ] {
        data["queries"] = json!([query]);
        assert_eq!(
            analyze_references(&data.to_string()),
            Err(TransportError::InvalidEnvelope)
        );
    }
    let (catalog, _) = compile_reference_catalog(
        &json!({"protocol":"reference-catalog/1","catalog":data["catalog"]}).to_string(),
    )
    .unwrap();
    let catalog = catalog.unwrap();
    let valid = json!({"kind":"bind_relation","name":"EMPTY"});
    data["queries"] = json!([valid, {"kind":"bind_relation","name":"\u{e9}".repeat(32769)}]);
    let expected = json!({"protocol":"reference-analysis/1","outcome":{"status":"limit","resource":"reference_bytes","limit":"65536","required":"65538"}});
    assert_eq!(
        serde_json::from_str::<Value>(&analyze_references(&data.to_string()).unwrap()).unwrap(),
        expected
    );
    assert_eq!(
        serde_json::from_str::<Value>(
            &catalog
                .analyze(
                    &json!({"protocol":"reference-queries/1","queries":data["queries"]})
                        .to_string()
                )
                .unwrap()
        )
        .unwrap(),
        expected
    );
    assert_eq!(
        serde_json::from_str::<Value>(
            &catalog
                .analyze(&json!({"protocol":"reference-queries/1","queries":[valid]}).to_string())
                .unwrap()
        )
        .unwrap(),
        json!({"protocol":"reference-analysis/1","outcome":{"status":"complete","results":[{"kind":"relation_binding","dataset":0}]}})
    );
}
