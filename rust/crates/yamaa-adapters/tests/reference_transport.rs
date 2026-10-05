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
        r#"{"protocol":"reference-analysis/1","features":["binding","output_validation","qualified_validation"]}"#
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
