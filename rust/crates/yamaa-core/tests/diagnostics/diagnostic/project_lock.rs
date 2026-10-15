use serde_json::Value as V;
use std::collections::BTreeSet;
use yamaa_core::diagnostic::{
    lock_diagnostics, ConditionCode, LockDiagnosticError as E, LockFinding, LockOrigins,
    LockReason as R,
};
fn text(value: &str) -> V {
    V::String(value.into())
}
fn origins() -> LockOrigins<'static> {
    LockOrigins {
        source: "study/environment.yaml",
        lock_source: "held/uv.lock",
        written: "../uv.lock",
    }
}
fn finding(reason: R, expected: &[&str], actual: Option<&str>) -> LockFinding {
    LockFinding {
        package: "program".into(),
        reason,
        expected: expected.iter().map(|v| (*v).into()).collect(),
        actual: actual.map(str::to_owned),
    }
}
pub fn reached() -> BTreeSet<ConditionCode> {
    let observations = [
        (
            R::DistributionNotIdentified,
            "distribution_not_identified",
            vec![],
            None,
        ),
        (R::VersionNotLocked, "version_not_locked", vec![], None),
        (
            R::AmbiguousLockVersion,
            "ambiguous_lock_version",
            vec!["2.0", "1.0", "2.0"],
            None,
        ),
        (
            R::InvalidLockedVersion,
            "invalid_locked_version",
            vec!["invalid\0é"],
            None,
        ),
        (
            R::PackageNotInstalled,
            "package_not_installed",
            vec!["1.0"],
            None,
        ),
        (
            R::InvalidInstalledVersion,
            "invalid_installed_version",
            vec!["1.0"],
            Some("bad\0🙂"),
        ),
        (
            R::VersionMismatch,
            "version_mismatch",
            vec!["1.0"],
            Some("1.0.0"),
        ),
    ];
    let findings = observations
        .iter()
        .map(|(reason, _, expected, actual)| finding(*reason, expected, *actual))
        .collect::<Vec<_>>();
    let first_expected = findings[2].expected[0].as_ptr();
    let actual_address = findings[5].actual.as_ref().unwrap().as_ptr();
    let diagnostics = lock_diagnostics(Some(origins()), &findings).unwrap();
    let mut reached = BTreeSet::new();
    for (diagnostic, (reason, token, expected, actual)) in diagnostics.iter().zip(observations) {
        assert_eq!(
            (
                diagnostic.definition().phase,
                diagnostic.definition().condition,
                diagnostic.definition().requirement
            ),
            ("validation", "runtime_artifact_mismatch", Some("REQ-0697"))
        );
        assert_eq!(diagnostic.spec_paths, ["lock"]);
        assert_eq!(diagnostic.source_span, None);
        assert_eq!(diagnostic.operand_route, None);
        assert_eq!(
            diagnostic.context,
            [
                ("source".into(), text("study/environment.yaml")),
                ("entry".into(), text("study/environment.yaml")),
                ("lock_source".into(), text("held/uv.lock")),
                ("lock".into(), text("../uv.lock")),
                ("package".into(), text("program")),
                ("reason".into(), text(token)),
                (
                    "expected".into(),
                    V::Array(expected.into_iter().map(text).collect())
                ),
                ("actual".into(), actual.map_or(V::Null, text)),
            ]
            .into()
        );
        assert_eq!(R::parse(token), Some(reason));
        assert_eq!(reason.as_str(), token);
        reached.insert(diagnostic.code);
    }
    assert_eq!(
        lock_diagnostics(Some(origins()), &findings).unwrap(),
        diagnostics
    );
    assert_eq!(findings[2].expected[0].as_ptr(), first_expected);
    assert_eq!(
        findings[5].actual.as_ref().unwrap().as_ptr(),
        actual_address
    );
    assert!(R::parse("unclassified runtime error").is_none());
    reached
}
#[test]
fn every_classified_reason_preserves_independent_complete_context_and_order() {
    assert_eq!(reached(), [ConditionCode::ProjectLockMismatch].into());
}
#[test]
fn spelling_missing_and_empty_facts_are_preserved_without_version_comparison() {
    let records = lock_diagnostics(
        Some(LockOrigins {
            source: "é\0🙂/environment.yaml",
            lock_source: "captured-lock",
            written: "../declared/uv.lock",
        }),
        &[
            finding(R::InvalidInstalledVersion, &["1.0"], None),
            finding(R::InvalidLockedVersion, &[], None),
            finding(R::VersionMismatch, &[" 1.0 "], Some(" 1.0 ")),
            finding(R::VersionMismatch, &[""], Some("")),
        ],
    )
    .unwrap();
    assert_eq!(records[0].context["actual"], V::Null);
    assert_eq!(records[1].context["expected"], V::Array(vec![]));
    assert_eq!(
        records[2].context["expected"],
        V::Array(vec![text(" 1.0 ")])
    );
    assert_eq!(records[2].context["actual"], text(" 1.0 "));
    assert_eq!(records[3].context["actual"], text(""));
    assert_eq!(records[0].context["source"], text("é\0🙂/environment.yaml"));
    assert_eq!(records[0].context["entry"], records[0].context["source"]);
    assert_eq!(records[0].context["lock"], text("../declared/uv.lock"));
}
#[test]
fn contradictory_shapes_refuse_the_complete_report() {
    assert_eq!(lock_diagnostics(None, &[]).unwrap(), vec![]);
    let first = || finding(R::VersionMismatch, &["1.0"], Some("2.0"));
    assert_eq!(lock_diagnostics(None, &[first()]), Err(E::InvalidContext));
    for bad in [
        finding(R::DistributionNotIdentified, &["1"], None),
        finding(R::VersionNotLocked, &[], Some("1")),
        finding(R::AmbiguousLockVersion, &["1"], None),
        finding(R::InvalidLockedVersion, &["1", "2"], None),
        finding(R::PackageNotInstalled, &["1"], Some("2")),
        finding(R::InvalidInstalledVersion, &[], Some("2")),
        finding(R::VersionMismatch, &["1"], None),
        LockFinding {
            package: "bad\0package".into(),
            ..first()
        },
        LockFinding {
            package: String::new(),
            ..first()
        },
    ] {
        assert_eq!(
            lock_diagnostics(Some(origins()), &[first(), bad]),
            Err(E::InvalidContext)
        );
    }
}

#[test]
fn long_lock_facts_and_all_ordered_versions_remain_complete() {
    let source = "\0".repeat(65_537);
    let package = "p".repeat(2049);
    let actual = "é".repeat(1025);
    let records = [LockFinding {
        package: package.clone(),
        reason: R::VersionMismatch,
        expected: vec!["1".into()],
        actual: Some(actual.clone()),
    }];
    let report = lock_diagnostics(
        Some(LockOrigins {
            source: &source,
            ..origins()
        }),
        &records,
    )
    .unwrap();
    assert_eq!(
        report,
        [yamaa_core::diagnostic::Diagnostic {
            code: ConditionCode::ProjectLockMismatch,
            spec_paths: vec!["lock".into()],
            context: [
                ("source".into(), text(&source)),
                ("entry".into(), text(&source)),
                ("lock_source".into(), text("held/uv.lock")),
                ("lock".into(), text("../uv.lock")),
                ("package".into(), text(&package)),
                ("reason".into(), text("version_mismatch")),
                ("expected".into(), V::Array(vec![text("1")])),
                ("actual".into(), text(&actual)),
            ]
            .into(),
            source_span: None,
            operand_route: None,
        }]
    );
    let many = (0..2050)
        .map(|_| finding(R::VersionNotLocked, &[], None))
        .collect::<Vec<_>>();
    let projected = lock_diagnostics(Some(origins()), &many).unwrap();
    assert_eq!(projected.len(), 2050);
    assert!(projected
        .iter()
        .all(|diagnostic| diagnostic.context["reason"] == text("version_not_locked")));
    let versions = [LockFinding {
        package: "program".into(),
        reason: R::AmbiguousLockVersion,
        expected: vec!["1".into(); 65_537],
        actual: None,
    }];
    let projected = lock_diagnostics(Some(origins()), &versions).unwrap();
    assert_eq!(
        projected[0].context["expected"],
        V::Array(vec![text("1"); 65_537])
    );
    assert_eq!(projected[0].context["actual"], V::Null);
}
