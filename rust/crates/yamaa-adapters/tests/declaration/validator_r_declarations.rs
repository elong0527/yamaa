//! Original R authoring declarations remain typed witnesses when R is absent.
//! This proves metadata admission only. The mandatory installed R replay owns
//! lock verification, activation, source binding, values and publication.
use std::{convert::Infallible, path::PathBuf};
use yamaa_adapters::{
    project_check,
    project_run::PreparedRun,
    project_source::{self, CapturePort, Kind, Reply, Request},
    shipped_schema,
    specification_source::Source,
};
use yamaa_core::{project_environment::LockKind, project_function::Language};

struct DeclaredLock {
    captures: usize,
}
impl CapturePort for DeclaredLock {
    type Error = Infallible;
    fn capture(&mut self, request: Request<'_>) -> Result<Reply, Self::Error> {
        assert_eq!(request.kind, Kind::Lock);
        assert_eq!(request.written, "renv.lock");
        self.captures += 1;
        // Static admission borrows only the declared lock format. This authored
        // codec fixture grants no installed-package or verification evidence.
        Ok(Reply::Lock {
            source: Source {
                identity: "typed-fixture/renv.lock".into(),
                bytes: b"{}".to_vec(),
            },
            kind: LockKind::Renv,
        })
    }
}

fn replay(name: &str, expected: Option<(&str, &str, &str)>) {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../benchmarks");
    let case = root.join(name);
    let mut lock = DeclaredLock { captures: 0 };
    let environment = project_source::prepare(
        shipped_schema::capture_environment().unwrap(),
        Source {
            identity: format!("benchmarks/{name}/r/environment.yaml"),
            bytes: std::fs::read(case.join("r/environment.yaml")).unwrap(),
        },
        Language::R,
        &mut lock,
        Default::default(),
    )
    .unwrap()
    .into_owned();
    assert_eq!(lock.captures, 1);
    assert_eq!(environment.environment().host(), Language::R);
    let document = shipped_schema::capture()
        .unwrap()
        .prepare_standalone(Source {
            identity: format!("benchmarks/{name}/spec.yaml"),
            bytes: std::fs::read(case.join("spec.yaml")).unwrap(),
        })
        .unwrap();
    let rows = match PreparedRun::prepare(document, environment) {
        Ok(run) => project_check::prepared(&run).unwrap(),
        Err(run) => project_check::rejected(&run).unwrap(),
    };
    match expected {
        None => assert!(rows.is_empty(), "{name}: {rows:?}"),
        Some((condition, requirement, path)) => {
            assert_eq!(rows.len(), 1, "{name}: {rows:?}");
            let issue = &rows[0];
            assert_eq!(issue.phase, "validation");
            assert_eq!(issue.condition, condition);
            assert_eq!(issue.requirement.as_deref(), Some(requirement));
            assert_eq!(issue.spec_paths, [path]);
            assert_eq!(
                serde_json::from_str::<serde_json::Value>(&issue.context).unwrap(),
                serde_json::json!({"argument":"x", "cause":"missing_required_argument", "function":"project_value"})
            );
        }
    }
    // No activation, source-data or publication port exists in this replay.
    assert_eq!(lock.captures, 1);
}

#[test]
fn schema_functions_r_declared_calls_are_statically_admitted() {
    replay("schema-functions", None);
}
#[test]
fn schema_non_finite_r_declared_calls_are_statically_admitted() {
    replay("schema-non-finite", None);
}
#[test]
fn adam_adsl_bmi_r_declared_calls_are_statically_admitted() {
    replay("adam-adsl-bmi", None);
}
#[test]
fn adam_advs_percentiles_r_declared_calls_are_statically_admitted() {
    replay("adam-advs-percentiles", None);
}
#[test]
fn negative_function_contract_r_preserves_the_exact_static_issue() {
    replay(
        "negative-function-contract",
        Some((
            "invalid_function_argument",
            "REQ-0700",
            "columns.RESULT.derivation.function.args",
        )),
    );
}
