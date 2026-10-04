use serde_json::{json, Value as Json};
use yamaa_adapters::function_transport::*;
use yamaa_core::value::Value;
use yamaa_engine::function_invocation::{Argument, FunctionPort, HostError};

/// One independently specified admitted request, without runtime artifact claims.
fn request() -> Json {
    json!({"protocol":"function/1", "identity":{"name":"sample", "contract_version":"1", "implementation_version":"2", "call":"artifact.sample"},
        "parameters":[{"name":"x","host_name":"host_x","type":"int","accepts_missing":false,"presence":{"required":null}}],
        "returns":"int","may_return_missing":false,"arguments":[{"name":"x","value":{"int":"9007199254740993"}}]})
}
struct Port {
    calls: usize,
    value: Option<Value>,
    panic: bool,
    error: Option<CallbackError>,
}
impl FunctionPort for Port {
    type Error = CallbackError;
    /// One counted effect precedes the supplied scalar, host error or unwind.
    fn call(&mut self, arguments: &[Argument<'_>]) -> Result<Value, HostError<Self::Error>> {
        self.calls += 1;
        assert_eq!(arguments[0].name, "host_x");
        assert_eq!(
            arguments[0].value,
            yamaa_core::table::ValueRef::Int(9007199254740993)
        );
        assert!(!self.panic, "private panic payload");
        if let Some(error) = self.error.take() {
            return Err(HostError::Raised(error));
        }
        Ok(self.value.take().unwrap_or(Value::Int(7)))
    }
}
/// Fresh state makes repeated calls and failure effects observable.
fn port() -> Port {
    Port {
        calls: 0,
        value: None,
        panic: false,
        error: None,
    }
}
/// Assert rejection category without requiring admitted plans to implement Debug.
fn rejected(text: &str, expected: FunctionTransportError) {
    assert_eq!(PreparedInvocation::parse(text).err(), Some(expected));
}

/// Exact wire truth includes scalar types and portable failure identity/context.
#[test]
fn values_and_conditions_have_independent_wire_truth() {
    let prepared = PreparedInvocation::parse(&request().to_string()).unwrap();
    assert_eq!(prepared.host_names().collect::<Vec<_>>(), ["host_x"]);
    let mut port = port();
    assert_eq!(prepared.invoke(&mut port).unwrap(), "{\"protocol\":\"function/1\",\"outcome\":{\"status\":\"value\",\"value\":{\"int\":\"7\"}}}");
    port.value = Some(Value::Str("wrong".into()));
    let actual: Json = serde_json::from_str(&prepared.invoke(&mut port).unwrap()).unwrap();
    assert_eq!(
        actual,
        json!({"protocol":"function/1","outcome":{"status":"condition","diagnostic":{
        "phase":"derivation","condition":"invalid_function_result","requirement":"REQ-0702","applicable_handler":null,
        "context":{"function":"sample","contract_version":"1","implementation_version":"2","expected":"int","actual":"str"}}}})
    );
    port.error = Some(CallbackError::Exception {
        class: "ValueError".into(),
        message: "boom".into(),
        truncated: false,
    });
    let actual: Json = serde_json::from_str(&prepared.invoke(&mut port).unwrap()).unwrap();
    assert_eq!(
        actual["outcome"]["diagnostic"],
        json!({"phase":"derivation","condition":"function_call_failed","requirement":"REQ-0701",
        "applicable_handler":null,"context":{"function":"sample","contract_version":"1","implementation_version":"2","call":"artifact.sample","host_error":"ValueError","host_message":"boom"}})
    );
    assert_eq!(port.calls, 3);
}

/// Transport admits every argument, including unused or short-circuited entries.
#[test]
fn strict_shape_scalar_and_signature_rejection_precedes_callbacks() {
    use FunctionTransportError::*;
    let base = request();
    for text in ["", "null", "[]", "{}", "{", "{} {}"] {
        rejected(text, InvalidRequest);
    }
    let text = base.to_string();
    rejected(
        &text.replacen(
            "\"protocol\":",
            "\"protocol\":\"function/1\",\"protocol\":",
            1,
        ),
        InvalidRequest,
    );
    for (path, replacement, error) in [
        ("/protocol", json!("future/1"), UnsupportedProtocol),
        ("/returns", json!("bool"), InvalidSignature),
        ("/parameters/0/type", json!("decimal"), InvalidRequest),
        ("/parameters/0/name", json!("bad-name"), InvalidSignature),
        (
            "/parameters/0/presence",
            json!({"optional":{"float":"3ff0000000000000"}}),
            InvalidSignature,
        ),
        (
            "/parameters/0/presence",
            json!({"optional":{"missing":null}}),
            InvalidSignature,
        ),
        (
            "/parameters/0/presence",
            json!({"required":null,"optional":{"int":"1"}}),
            InvalidRequest,
        ),
        (
            "/arguments/0/value",
            json!({"int":"9223372036854775808"}),
            InvalidScalar,
        ),
        ("/arguments/0/value", json!({"int":1}), InvalidRequest),
        ("/arguments/0/value", json!({"int":"01"}), InvalidScalar),
    ] {
        let mut altered = base.clone();
        *altered.pointer_mut(path).unwrap() = replacement;
        rejected(&altered.to_string(), error);
    }
    for path in ["", "/identity", "/parameters/0", "/arguments/0"] {
        let mut altered = base.clone();
        altered
            .pointer_mut(path)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .insert("extra".into(), json!(true));
        rejected(&altered.to_string(), InvalidRequest);
    }
    for field in ["parameters", "arguments"] {
        let mut altered = base.clone();
        let item = altered[field][0].clone();
        altered[field].as_array_mut().unwrap().push(item);
        rejected(
            &altered.to_string(),
            if field == "parameters" {
                InvalidSignature
            } else {
                InvalidRequest
            },
        );
    }
}

/// Fixed admission budgets do not depend on untrusted caller configuration.
#[test]
fn request_budgets_cover_bytes_counts_and_names() {
    use FunctionTransportError::*;
    rejected(&" ".repeat(MAX_REQUEST_BYTES + 1), RequestLimit);
    for field in ["parameters", "arguments"] {
        let mut altered = request();
        let item = altered[field][0].clone();
        altered[field] = json!(vec![item; 257]);
        rejected(&altered.to_string(), RequestLimit);
    }
    for path in [
        "/identity/name",
        "/identity/call",
        "/parameters/0/name",
        "/parameters/0/host_name",
        "/arguments/0/name",
    ] {
        let mut altered = request();
        *altered.pointer_mut(path).unwrap() = json!("x".repeat(1025));
        rejected(&altered.to_string(), RequestLimit);
    }
}

/// Missing skips and unknown-name failure never reach the host port.
#[test]
fn pre_call_outcomes_have_zero_host_effects() {
    let mut request = request();
    request["arguments"][0]["value"] = json!({"missing":null});
    let mut port = port();
    assert_eq!(PreparedInvocation::parse(&request.to_string()).unwrap().invoke(&mut port).unwrap(),
        "{\"protocol\":\"function/1\",\"outcome\":{\"status\":\"value\",\"value\":{\"missing\":null}}}");
    request["arguments"]
        .as_array_mut()
        .unwrap()
        .push(json!({"name":"unknown","value":{"int":"1"}}));
    let result: Json = serde_json::from_str(
        &PreparedInvocation::parse(&request.to_string())
            .unwrap()
            .invoke(&mut port)
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        result["outcome"]["diagnostic"]["context"]["unknown"],
        json!(["unknown"])
    );
    assert_eq!(port.calls, 0);
}

/// Panic and output failures retain prior effects; subsequent calls do not retry them.
#[test]
fn contain_unwind_and_output_limits_without_replaying_effects() {
    let mut request = request();
    request["returns"] = json!("str");
    let prepared = PreparedInvocation::parse(&request.to_string()).unwrap();
    let mut port = port();
    port.panic = true;
    assert_eq!(
        prepared.invoke(&mut port),
        Err(FunctionTransportError::Internal)
    );
    assert_eq!(port.calls, 1);
    port.panic = false;
    port.value = Some(Value::Str("x".repeat(MAX_RESULT_BYTES + 1)));
    assert_eq!(
        prepared.invoke(&mut port),
        Err(FunctionTransportError::OutputLimit)
    );
    port.value = Some(Value::Str("\0".repeat(MAX_RESULT_BYTES)));
    let output = prepared.invoke(&mut port).unwrap();
    assert!(output.len() > MAX_RESULT_BYTES * 6);
    assert!(output.len() <= MAX_OUTPUT_BYTES);
    port.error = Some(CallbackError::Exception {
        class: "Failure".into(),
        message: "x".repeat(MAX_ERROR_BYTES + 1),
        truncated: false,
    });
    assert_eq!(
        prepared.invoke(&mut port),
        Err(FunctionTransportError::OutputLimit)
    );
    port.value = Some(Value::Str("recovered".into()));
    assert!(prepared.invoke(&mut port).unwrap().contains("recovered"));
    assert_eq!(port.calls, 5);
}
