use std::sync::Arc;
use yamaa_adapters::{
    project_lock::{Finding, Reason},
    project_lock_diagnostics::{issues, issues_with_limit, Error},
    project_source::{self, CapturePort, CapturedEnvironment, Kind, Reply, Request},
    specification_source::{CapturedSchema, Source},
};
use yamaa_core::{project_environment::LockKind, project_function::Language};

struct Capture {
    calls: usize,
}
impl CapturePort for Capture {
    type Error = std::convert::Infallible;
    fn capture(&mut self, request: Request<'_>) -> Result<Reply, Self::Error> {
        assert_eq!(request.kind, Kind::Lock);
        assert_eq!(request.written, "../uv.lock");
        self.calls += 1;
        Ok(Reply::Lock {
            source: Source {
                identity: "held/uv.lock".into(),
                bytes: b"version = 1\n".to_vec(),
            },
            kind: LockKind::Uv,
        })
    }
}
fn environment(locked: bool) -> (CapturedEnvironment, Capture) {
    let schema = CapturedSchema::admit_root(
        vec![
            Source {
                identity: "schema_environment.yaml".into(),
                bytes: include_bytes!("fixtures/environment-candidate/schema_environment.yaml")
                    .to_vec(),
            },
            Source {
                identity: "schema_shared.yaml".into(),
                bytes: include_bytes!("fixtures/environment-candidate/schema_shared.yaml").to_vec(),
            },
        ],
        0,
        "environment_class",
        Default::default(),
    )
    .unwrap();
    let mut capture = Capture { calls: 0 };
    let source = Source {
        identity: "study/environment.yaml".into(),
        bytes: if locked {
            b"schema_version: '1.0'\nlanguage: python\nlock: ../uv.lock\n".to_vec()
        } else {
            b"schema_version: '1.0'\n".to_vec()
        },
    };
    let prepared = project_source::prepare(
        Arc::clone(&schema),
        source,
        Language::Python,
        &mut capture,
        Default::default(),
    )
    .unwrap();
    let (_, provenance) = prepared.into_owned().into_parts();
    (provenance, capture)
}
fn finding(reason: Reason, expected: &[&str], actual: Option<&str>) -> Finding {
    Finding {
        package: "program".into(),
        reason,
        expected: expected.iter().map(|version| (*version).into()).collect(),
        actual: actual.map(str::to_owned),
    }
}

#[test]
fn complete_classified_host_facts_preserve_order_and_held_lock_origins_without_reads() {
    let (environment, capture) = environment(true);
    let root = environment.root().source().bytes.as_ptr();
    let lock = environment.lock().unwrap().source.bytes.as_ptr();
    let facts = [
        finding(Reason::DistributionNotIdentified, &[], None),
        finding(Reason::VersionNotLocked, &[], None),
        finding(Reason::AmbiguousLockVersion, &["1.0", "2.0"], None),
        finding(Reason::InvalidLockedVersion, &["invalid\0é"], None),
        finding(Reason::PackageNotInstalled, &["1.0"], None),
        finding(Reason::InvalidInstalledVersion, &["1.0"], Some("bad\0🙂")),
        finding(Reason::VersionMismatch, &["1.0"], Some("2.0")),
    ];
    let records = issues(&environment, &facts).unwrap();
    assert_eq!(records.len(), 7);
    let expected = [
        (
            "distribution_not_identified",
            serde_json::json!([]),
            serde_json::Value::Null,
        ),
        (
            "version_not_locked",
            serde_json::json!([]),
            serde_json::Value::Null,
        ),
        (
            "ambiguous_lock_version",
            serde_json::json!(["1.0", "2.0"]),
            serde_json::Value::Null,
        ),
        (
            "invalid_locked_version",
            serde_json::json!(["invalid\0é"]),
            serde_json::Value::Null,
        ),
        (
            "package_not_installed",
            serde_json::json!(["1.0"]),
            serde_json::Value::Null,
        ),
        (
            "invalid_installed_version",
            serde_json::json!(["1.0"]),
            serde_json::json!("bad\0🙂"),
        ),
        (
            "version_mismatch",
            serde_json::json!(["1.0"]),
            serde_json::json!("2.0"),
        ),
    ];
    for (record, (reason, versions, actual)) in records.iter().zip(expected) {
        assert_eq!(record.phase, "validation");
        assert_eq!(record.condition, "project_environment_invalid");
        assert_eq!(record.requirement.as_deref(), Some("REQ-0695"));
        assert_eq!(record.spec_paths, ["lock"]);
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&record.context).unwrap(),
            serde_json::json!({"source":"study/environment.yaml", "entry":"study/environment.yaml",
                "lock_source":"held/uv.lock", "lock":"../uv.lock", "package":"program",
                "reason":reason, "expected":versions, "actual":actual})
        );
    }
    assert_eq!(issues(&environment, &facts).unwrap(), records);
    assert_eq!(capture.calls, 1);
    assert_eq!(environment.root().source().bytes.as_ptr(), root);
    assert_eq!(environment.lock().unwrap().source.bytes.as_ptr(), lock);
}

#[test]
fn projection_preserves_host_version_spelling_and_does_not_reimplement_comparison() {
    let (environment, _) = environment(true);
    let facts = [
        finding(Reason::InvalidInstalledVersion, &["1.0"], None),
        finding(Reason::InvalidLockedVersion, &[], None),
        finding(Reason::VersionMismatch, &["1.0"], Some("1.0.0")),
    ];
    let records = issues(&environment, &facts).unwrap();
    assert_eq!(records.len(), 3);
    let context: serde_json::Value = serde_json::from_str(&records[2].context).unwrap();
    assert_eq!(context["expected"], serde_json::json!(["1.0"]));
    assert_eq!(context["actual"], "1.0.0");
}

#[test]
fn absent_lock_and_contradictory_facts_refuse_the_complete_projection() {
    let (unlocked, _) = environment(false);
    assert!(issues(&unlocked, &[]).unwrap().is_empty());
    let fact = finding(Reason::VersionMismatch, &["1.0"], Some("2.0"));
    assert_eq!(issues(&unlocked, &[fact]), Err(Error::InvalidContext));
    let (environment, _) = environment(true);
    for bad in [
        finding(Reason::VersionMismatch, &["1.0"], None),
        finding(Reason::VersionNotLocked, &["1.0"], None),
        finding(Reason::AmbiguousLockVersion, &["1.0"], None),
        finding(Reason::PackageNotInstalled, &["1.0"], Some("2.0")),
    ] {
        let first = finding(Reason::VersionMismatch, &["1.0"], Some("2.0"));
        assert_eq!(
            issues(&environment, &[first, bad]),
            Err(Error::InvalidContext)
        );
    }
    assert!(Reason::parse("unclassified runtime error").is_none());
}

#[test]
fn cumulative_escaped_text_and_utf8_limits_are_atomic() {
    let (environment, _) = environment(true);
    let first = finding(Reason::VersionMismatch, &["1.0"], Some("2.0"));
    assert_eq!(
        issues_with_limit(&environment, &[first], 3_000)
            .unwrap()
            .len(),
        1
    );
    let first = finding(Reason::VersionMismatch, &["1.0"], Some("2.0"));
    let second = finding(Reason::VersionMismatch, &["1.0"], Some("2.0"));
    assert_eq!(
        issues_with_limit(&environment, &[first, second], 3_000),
        Err(Error::Limit)
    );
    let too_wide = "é".repeat(1_025);
    assert_eq!(
        issues(
            &environment,
            &[finding(
                Reason::InvalidInstalledVersion,
                &["1.0"],
                Some(&too_wide)
            )]
        ),
        Err(Error::Limit)
    );
    let facts = (0..2_050)
        .map(|_| finding(Reason::VersionNotLocked, &[], None))
        .collect::<Vec<_>>();
    assert_eq!(issues(&environment, &facts), Err(Error::Limit));
}
