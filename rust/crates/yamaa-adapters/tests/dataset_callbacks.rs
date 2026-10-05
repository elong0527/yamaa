//! Independent shared callback trace, table and failure observations through dataset IPC.
use serde_json::Value as Json;
use std::path::PathBuf;
use yamaa_adapters::{
    dataset_transport::{DatasetTransportError as Error, PreparedDataset},
    function_transport::CallbackError,
    table_transport::table_snapshot,
};
use yamaa_core::{table::ValueRef, value::Value};
use yamaa_engine::{
    dataset::FunctionBindings,
    function_invocation::{Argument, HostError, InvocationPlan},
};

struct Callbacks {
    signatures: Vec<InvocationPlan>,
    trace: Vec<String>,
    mode: String,
}
impl FunctionBindings for Callbacks {
    type Error = CallbackError;
    /// Stable metadata has no callback effects.
    fn signature(&self, slot: usize) -> Option<&InvocationPlan> {
        self.signatures.get(slot)
    }
    /// Independent integer test functions retain every callback argument and mapped name.
    fn call(
        &mut self,
        slot: usize,
        arguments: &[Argument<'_>],
    ) -> Result<Value, HostError<CallbackError>> {
        let values = arguments
            .iter()
            .map(|a| match a.value {
                ValueRef::Int(n) => n,
                _ => panic!("unexpected argument"),
            })
            .collect::<Vec<_>>();
        let expected_names = if values.len() == 3 {
            vec!["lhs", "rhs", "scale"]
        } else {
            vec!["value"]
        };
        assert_eq!(
            arguments.iter().map(|a| a.name).collect::<Vec<_>>(),
            expected_names
        );
        self.trace.push(format!(
            "{slot}:{}",
            values
                .iter()
                .map(i64::to_string)
                .collect::<Vec<_>>()
                .join(",")
        ));
        if self.mode == "raise_second" && self.trace.len() == 2 {
            return Err(HostError::Raised(CallbackError::Exception {
                class: "FixtureError".into(),
                message: "after first effect".into(),
                truncated: false,
            }));
        }
        if self.mode == "bad_text" {
            return Ok(Value::Str("bad".into()));
        }
        if self.mode == "boolean" {
            return Ok(Value::Bool(true));
        }
        Ok(Value::Int(if values.len() == 1 {
            values[0]
        } else {
            values.iter().sum()
        }))
    }
}
/// The same authored outcomes are replayed by both installed host packages.
#[test]
fn shared_dataset_callbacks_retain_complete_observations() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/datasets");
    let cases: Vec<Json> =
        serde_json::from_str(&std::fs::read_to_string(root.join("callbacks.json")).unwrap())
            .unwrap();
    for case in cases {
        let prepared = PreparedDataset::parse(&case["request"].to_string())
            .unwrap_or_else(|error| panic!("{}: {error:?}", case["case"]));
        let source = std::fs::read(root.join(case["input"].as_str().unwrap())).unwrap();
        for _ in 0..2 {
            let mut callbacks = Callbacks {
                signatures: prepared.function_signatures().to_vec(),
                trace: vec![],
                mode: case["mode"].as_str().unwrap().into(),
            };
            let result = prepared
                .execute_sources_functions(&source, &[], &mut callbacks)
                .unwrap();
            assert_eq!(
                serde_json::from_str::<Json>(&result.outcome).unwrap(),
                case["expected"],
                "{}",
                case["case"]
            );
            assert_eq!(
                callbacks.trace.join(";"),
                case["trace"].as_str().unwrap(),
                "{}",
                case["case"]
            );
            match result.table {
                Some(table) => assert_eq!(
                    serde_json::from_str::<Json>(&table_snapshot(&table).unwrap()).unwrap(),
                    case["snapshot"],
                    "{}",
                    case["case"]
                ),
                None => assert!(case["snapshot"].is_null()),
            }
        }
        assert!(matches!(
            prepared.execute(b"invalid IPC"),
            Err(Error::FunctionBinding)
        ));
        let mut unavailable = Callbacks {
            signatures: vec![],
            trace: vec![],
            mode: "sum".into(),
        };
        assert!(matches!(
            prepared.execute_sources_functions(b"invalid IPC", &[], &mut unavailable),
            Err(Error::FunctionBinding)
        ));
        assert!(unavailable.trace.is_empty());
        let mut different = case["request"].clone();
        different["functions"][0]["identity"]["implementation_version"] =
            serde_json::json!("different");
        unavailable.signatures = PreparedDataset::parse(&different.to_string())
            .unwrap()
            .function_signatures()
            .to_vec();
        assert!(matches!(
            prepared.execute_sources_functions(b"invalid IPC", &[], &mut unavailable),
            Err(Error::FunctionBinding)
        ));
        assert!(unavailable.trace.is_empty());
    }
}

/// Read authored request data without generating expected observations from the engine.
fn request() -> Json {
    let cases: Vec<Json> =
        serde_json::from_str(include_str!("fixtures/datasets/callbacks.json")).unwrap();
    cases[0]["request"].clone()
}

/// Closed shape, references and signature budgets are rejected without reaching IPC.
#[test]
fn callback_admission_rejects_invalid_and_amplifying_requests() {
    let base = request();
    for pointer in [
        "/functions/0",
        "/columns/0/expression/function",
        "/columns/0/expression/function/arguments/0",
    ] {
        let mut altered = base.clone();
        altered
            .pointer_mut(pointer)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .insert("extra".into(), serde_json::json!(true));
        assert!(matches!(
            PreparedDataset::parse(&altered.to_string()),
            Err(Error::InvalidRequest)
        ));
    }
    for (pointer, value) in [
        ("/columns/0/expression/function/slot", serde_json::json!(64)),
        (
            "/columns/0/expression/function/arguments/0/name",
            serde_json::json!("unknown"),
        ),
        (
            "/columns/0/expression/function/arguments/0/input",
            serde_json::json!({"source":99}),
        ),
        (
            "/columns/0/expression/function/arguments/0/input",
            serde_json::json!({"column":1}),
        ),
    ] {
        let mut altered = base.clone();
        *altered.pointer_mut(pointer).unwrap() = value;
        assert!(matches!(
            PreparedDataset::parse(&altered.to_string()),
            Err(Error::InvalidPlan)
        ));
    }
    let mut too_many = base.clone();
    too_many["functions"] = serde_json::json!(vec![base["functions"][0].clone(); 65]);
    assert!(matches!(
        PreparedDataset::parse(&too_many.to_string()),
        Err(Error::RequestLimit)
    ));

    // One small declaration repeated across many call sites must not amplify defaults.
    let mut expanded = base.clone();
    expanded["functions"][0]["parameters"][2]["type"] = serde_json::json!("str");
    expanded["functions"][0]["parameters"][2]["presence"] =
        serde_json::json!({"optional":{"str":"x".repeat(100_000)}});
    for i in 2..13 {
        expanded["output"]
            .as_array_mut()
            .unwrap()
            .push(serde_json::json!({"name":format!("V{i}"),"kind":"int"}));
        let mut call = base["columns"][0].clone();
        call["column"] = serde_json::json!(i);
        call["path"] = serde_json::json!(format!("columns.V{i}.function"));
        expanded["columns"].as_array_mut().unwrap().push(call);
    }
    assert!(expanded.to_string().len() < 1_048_576);
    assert!(matches!(
        PreparedDataset::parse(&expanded.to_string()),
        Err(Error::RequestLimit)
    ));

    // Count repeated parameter structures as well as copied text/default values.
    let mut expanded = base;
    expanded["functions"][0]["parameters"]=serde_json::json!((0..256).map(|i|serde_json::json!({"name":format!("p{i}"),"host_name":format!("a{i}"),"type":"int","accepts_missing":false,"presence":{"optional":{"int":"1"}}})).collect::<Vec<_>>());
    expanded["output"] = serde_json::json!((0..33)
        .map(|i| serde_json::json!({"name":format!("V{i}"),"kind":"int"}))
        .collect::<Vec<_>>());
    let assignments=(0..33).map(|i|serde_json::json!({"column":i,"path":format!("columns.V{i}.function"),"expression":{"function":{"slot":0,"arguments":[]}}})).collect::<Vec<_>>();
    expanded["templates"] = serde_json::json!([{"mode":{"records":null},"assignments":assignments.clone()},{"mode":{"records":null},"assignments":assignments}]);
    expanded["columns"] = serde_json::json!([]);
    assert!(matches!(
        PreparedDataset::parse(&expanded.to_string()),
        Err(Error::RequestLimit)
    ));
}

/// Metadata and invocation unwind failures are contained without replay or poisoned state.
#[test]
fn callback_boundary_contains_panics_and_reuses_fresh_attempts() {
    struct Unwind {
        signatures: Vec<InvocationPlan>,
        metadata_panic: bool,
        call_panic: bool,
        calls: usize,
    }
    impl FunctionBindings for Unwind {
        type Error = CallbackError;
        fn signature(&self, slot: usize) -> Option<&InvocationPlan> {
            assert!(!self.metadata_panic, "private metadata panic");
            self.signatures.get(slot)
        }
        fn call(
            &mut self,
            _: usize,
            _: &[Argument<'_>],
        ) -> Result<Value, HostError<CallbackError>> {
            self.calls += 1;
            assert!(!self.call_panic, "private callback panic");
            Ok(Value::Int(7))
        }
    }
    let prepared = PreparedDataset::parse(&request().to_string()).unwrap();
    let mut port = Unwind {
        signatures: prepared.function_signatures().to_vec(),
        metadata_panic: true,
        call_panic: true,
        calls: 0,
    };
    assert!(matches!(
        prepared.execute_sources_functions(b"bad IPC", &[], &mut port),
        Err(Error::Internal)
    ));
    assert_eq!(port.calls, 0);
    port.metadata_panic = false;
    let input = include_bytes!("fixtures/datasets/callbacks.arrow");
    assert!(matches!(
        prepared.execute_sources_functions(input, &[], &mut port),
        Err(Error::Internal)
    ));
    assert_eq!(port.calls, 1);
    port.call_panic = false;
    assert!(prepared
        .execute_sources_functions(input, &[], &mut port)
        .unwrap()
        .table
        .is_some());
    assert_eq!(port.calls, 3);
}

/// Collected argument shapes and scopes fail at admission, before malformed IPC matters.
#[test]
fn collected_argument_wire_admission_is_closed_and_bounded() {
    let cases: Vec<Json> =
        serde_json::from_str(include_str!("fixtures/datasets/callbacks.json")).unwrap();
    let base = cases
        .iter()
        .find(|case| case["case"] == "collected_equal")
        .unwrap()["request"]
        .clone();
    let input = "/columns/0/expression/function/arguments/0/input/collect";
    let mut extra = base.clone();
    extra
        .pointer_mut(input)
        .unwrap()
        .as_object_mut()
        .unwrap()
        .insert("filter".into(), serde_json::json!(false));
    assert!(matches!(
        PreparedDataset::parse(&extra.to_string()),
        Err(Error::InvalidRequest)
    ));
    let mut invalid = base.clone();
    invalid.pointer_mut(input).unwrap()["column"] = serde_json::json!(99);
    assert!(matches!(
        PreparedDataset::parse(&invalid.to_string()),
        Err(Error::InvalidPlan)
    ));
    let mut wide = base.clone();
    wide.pointer_mut(input).unwrap()["identifier"] = serde_json::json!("x".repeat(1025));
    assert!(matches!(
        PreparedDataset::parse(&wide.to_string()),
        Err(Error::Function(
            yamaa_adapters::function_transport::FunctionTransportError::RequestLimit
        ))
    ));
    for mode in [
        serde_json::json!({"records":null}),
        serde_json::json!({"groups":[0,1,2]}),
    ] {
        let mut invalid = base.clone();
        invalid["templates"][0]["mode"] = mode;
        assert!(matches!(
            PreparedDataset::parse(&invalid.to_string()),
            Err(Error::InvalidPlan)
        ));
    }
}
