//! Lock reporting only borrows a completed native activation rejection.
use std::{cell::RefCell, rc::Rc};
use yamaa_core::{
    diagnostic::{
        lock_diagnostics, lock_diagnostics_with_limit, LockDiagnosticError, LockFinding,
        LockOrigins, LockReason,
    },
    function_signature::{LogicalSignature, ProjectFunctionIdentity},
    project_environment::{LockKind, LockReference},
    project_function::{Case, Definition, Function, Language, Parameter},
    value::{ColumnType, Value, ValueType},
};
use yamaa_engine::{
    function_invocation::{Argument, HostError},
    project_activation::{activate, ActivationPort, Failure},
};

// No Clone, Debug or Send implementation exists for this original host payload.
struct Opaque {
    marker: Rc<u8>,
    findings: Vec<LockFinding>,
}
struct Port {
    original: Option<Opaque>,
    effects: Rc<RefCell<Vec<&'static str>>>,
}
impl ActivationPort for Port {
    type Handle = ();
    type Error = Opaque;
    fn verify_lock(
        &mut self,
        language: Language,
        lock: &LockReference,
        functions: &[ProjectFunctionIdentity],
    ) -> Result<(), Opaque> {
        assert_eq!(language, Language::Python);
        assert_eq!(lock.written, "../uv.lock");
        assert_eq!(functions.len(), 1);
        assert_eq!(functions[0].call, "program.identity");
        self.effects.borrow_mut().push("metadata/lock");
        Err(self.original.take().expect("single lock effect"))
    }
    fn bind(&mut self, _: &ProjectFunctionIdentity, _: &LogicalSignature) -> Result<(), Opaque> {
        panic!("lock rejection forbids provider binding")
    }
    fn invoke(&mut self, _: &(), _: &[Argument<'_>]) -> Result<Value, HostError<Opaque>> {
        panic!("lock rejection forbids invocation")
    }
}
fn function() -> Function {
    Function::admit(
        Language::Python,
        Definition {
            name: "identity".into(),
            function: "program.identity".into(),
            description: "Identity integer.".into(),
            params: vec![Parameter {
                name: "x".into(),
                kind: ValueType::Int,
                required: true,
                default: None,
                accepts_missing: false,
            }],
            returns: ColumnType::Int,
            may_return_missing: false,
            comparison_decimals: 4,
            tests: vec![
                Case {
                    id: "normal".into(),
                    covers: vec!["normal".into()],
                    args: vec![("x".into(), Value::Int(7))],
                    result: Value::Int(7),
                },
                Case {
                    id: "boundary".into(),
                    covers: vec!["boundary".into()],
                    args: vec![("x".into(), Value::Int(i64::MIN))],
                    result: Value::Int(i64::MIN),
                },
                Case {
                    id: "missing".into(),
                    covers: vec!["short-circuit-missing:x".into()],
                    args: vec![("x".into(), Value::Missing)],
                    result: Value::Missing,
                },
            ],
        },
    )
    .unwrap()
}
#[test]
fn non_clone_non_debug_non_send_lock_error_retains_identity_without_provider_retry() {
    let original = Opaque {
        marker: Rc::new(73),
        findings: vec![
            LockFinding {
                package: "program".into(),
                reason: LockReason::VersionMismatch,
                expected: vec!["1.0".into()],
                actual: Some("1.0.0".into()),
            },
            LockFinding {
                package: "yamaa".into(),
                reason: LockReason::PackageNotInstalled,
                expected: vec!["0.3.0".into()],
                actual: None,
            },
        ],
    };
    let address = Rc::as_ptr(&original.marker);
    let expected_address = original.findings[0].expected[0].as_ptr();
    let effects = Rc::new(RefCell::new(vec![]));
    let mut port = Port {
        original: Some(original),
        effects: Rc::clone(&effects),
    };
    let functions = [function()];
    let lock = LockReference {
        written: "../uv.lock".into(),
        kind: LockKind::Uv,
    };
    let Err(failure) = activate(Language::Python, &lock, &functions, &mut port) else {
        panic!("original lock failure")
    };
    let Failure::Lock(original) = &failure else {
        panic!("exact native failure kind")
    };
    let source = "study/environment.yaml";
    let captured = "held/uv.lock";
    let origins = LockOrigins {
        source,
        lock_source: captured,
        written: &lock.written,
    };
    let first = lock_diagnostics(Some(origins), &original.findings).unwrap();
    assert_eq!(first.len(), 2);
    assert_eq!(first[0].definition().requirement, Some("REQ-0697"));
    assert_eq!(
        lock_diagnostics_with_limit(Some(origins), &original.findings, 0),
        Err(LockDiagnosticError::Limit)
    );
    assert_eq!(
        lock_diagnostics(None, &original.findings),
        Err(LockDiagnosticError::InvalidContext)
    );
    assert_eq!(
        lock_diagnostics(Some(origins), &original.findings).unwrap(),
        first
    );
    assert_eq!(Rc::as_ptr(&original.marker), address);
    assert_eq!(Rc::strong_count(&original.marker), 1);
    assert_eq!(original.findings[0].expected[0].as_ptr(), expected_address);
    assert_eq!(*effects.borrow(), ["metadata/lock"]);
    assert!(port.original.is_none());
}
