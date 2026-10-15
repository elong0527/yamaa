#![allow(unused_imports)]
pub(crate) use std::collections::BTreeSet;
pub(crate) use yamaa_core::diagnostic::{
    lock_diagnostics, ConditionCode, LockDiagnosticError as E, LockFinding, LockOrigins,
    LockReason as R,
};
pub(crate) use yamaa_core::json::Value as V;
pub(crate) fn text(value: &str) -> V {
    V::String(value.into())
}
pub(crate) fn origins() -> LockOrigins<'static> {
    LockOrigins {
        source: "study/environment.yaml",
        lock_source: "held/uv.lock",
        written: "../uv.lock",
    }
}
pub(crate) fn finding(reason: R, expected: &[&str], actual: Option<&str>) -> LockFinding {
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
