use serde_json::{json, Value};
use yamaa_adapters::column_dependency_transport::{
    analyze_column_dependencies, TransportError, MAX_REQUEST_BYTES,
};

/// Decode authored indices without consulting the implementation under test.
fn indices(text: &str) -> Vec<usize> {
    if matches!(text, "-" | "_") {
        vec![]
    } else {
        text.split(',').map(|n| n.parse().unwrap()).collect()
    }
}

/// Ordered independent truth crosses the versioned boundary without host rule evaluation.
#[test]
fn shared_column_truth_crosses_transport() {
    for line in include_str!("../../yamaa-core/tests/fixtures/column_dependencies.tsv")
        .lines()
        .skip(1)
    {
        let fields: Vec<_> = line.split('\t').collect();
        let graph: Vec<Option<Vec<usize>>> = if fields[1] == "~" {
            vec![]
        } else {
            fields[1]
                .split(';')
                .map(|edges| (edges != "-").then(|| indices(edges)))
                .collect()
        };
        let diagnostics: Vec<Value> = if fields[5] == "-" {
            vec![]
        } else {
            fields[5].split(';').map(|entry| {
                let (kind, columns) = entry.split_once(':').unwrap();
                let (condition, requirement, location) = match kind {
                    "cycle" => ("dependency_cycle", "REQ-0072", "operation"),
                    "forward" => ("forward_reference", "REQ-0071", "operation"),
                    "missing" => ("key_dependency", "REQ-0074", "declaration"),
                    "key" => ("key_dependency", "REQ-0074", "expression"),
                    _ => panic!("unknown fixture condition"),
                };
                json!({"condition":condition,"requirement":requirement,"location":location,"columns":indices(columns)})
            }).collect()
        };
        let request = json!({"protocol":"column-dependencies/1","dependencies":graph,
            "keys":indices(fields[2]),"has_rows":fields[3].parse::<bool>().unwrap()});
        let actual: Value =
            serde_json::from_str(&analyze_column_dependencies(&request.to_string()).unwrap())
                .unwrap();
        assert_eq!(
            actual,
            json!({"protocol":"column-dependencies/1","outcome":{
            "status":"complete","order":indices(fields[4]),"diagnostics":diagnostics}}),
            "{}",
            fields[0]
        );
    }
}

/// Invalid metadata, duplicate fields and lossy integers cannot reach the compiler.
#[test]
fn strict_column_request_admission() {
    let valid =
        json!({"protocol":"column-dependencies/1","dependencies":[[]],"keys":[0],"has_rows":false});
    for (field, value) in [
        ("extra", json!(null)),
        ("has_rows", json!(0)),
        ("keys", json!([true])),
        ("keys", json!([-1])),
        ("keys", json!([0.0])),
        ("dependencies", json!([[false]])),
        ("dependencies", json!([[0.0]])),
        ("dependencies", json!([[-1]])),
    ] {
        let mut request = valid.clone();
        request[field] = value;
        assert_eq!(
            analyze_column_dependencies(&request.to_string()),
            Err(TransportError::InvalidEnvelope)
        );
    }
    for field in ["protocol", "dependencies", "keys", "has_rows"] {
        let mut request = valid.clone();
        request.as_object_mut().unwrap().remove(field);
        assert_eq!(
            analyze_column_dependencies(&request.to_string()),
            Err(TransportError::InvalidEnvelope)
        );
        let duplicate = format!("{{\"{field}\":{},{}", valid[field], &valid.to_string()[1..]);
        assert_eq!(
            analyze_column_dependencies(&duplicate),
            Err(TransportError::InvalidEnvelope)
        );
    }
    for (field, value, error) in [
        (
            "protocol",
            json!("future/2"),
            TransportError::UnsupportedProtocol,
        ),
        ("keys", json!([1]), TransportError::InvalidKeys),
        ("keys", json!([0, 0]), TransportError::InvalidKeys),
        ("dependencies", json!([[1]]), TransportError::InvalidGraph),
    ] {
        let mut request = valid.clone();
        request[field] = value;
        assert_eq!(
            analyze_column_dependencies(&request.to_string()),
            Err(error)
        );
    }
    assert_eq!(
        analyze_column_dependencies(&" ".repeat(MAX_REQUEST_BYTES + 1)),
        Err(TransportError::RequestLimit)
    );
}

/// Both phases count toward admission; filtering and malformed keys cannot hide the budget.
#[test]
fn bounded_column_analysis_and_reuse() {
    for (graph, resource, limit, required) in [
        (
            json!(vec![None::<Vec<usize>>; 4097]),
            "nodes",
            "4096",
            "4097",
        ),
        (json!([vec![1; 65537], null]), "edges", "65536", "65537"),
    ] {
        let request = json!({"protocol":"column-dependencies/1","dependencies":graph,"keys":[99999],"has_rows":true});
        let actual: Value =
            serde_json::from_str(&analyze_column_dependencies(&request.to_string()).unwrap())
                .unwrap();
        assert_eq!(
            actual,
            json!({"protocol":"column-dependencies/1","outcome":{
            "status":"limit","resource":resource,"limit":limit,"required":required}})
        );
    }
    for _ in 0..2 {
        let actual: Value = serde_json::from_str(&analyze_column_dependencies(
            r#"{"protocol":"column-dependencies/1","dependencies":[null,[]],"keys":[0],"has_rows":true}"#).unwrap()).unwrap();
        assert_eq!(
            actual["outcome"],
            json!({"status":"complete","order":[1],"diagnostics":[]})
        );
    }
}
