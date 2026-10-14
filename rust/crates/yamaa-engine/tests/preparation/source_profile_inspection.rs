use yamaa_core::{
    resource::ResourceFailure,
    specification::{PreflightFinding, PrepareError, UnsupportedFeature},
};
use yamaa_engine::source_profile_inspection;

fn unknown() -> PrepareError {
    PrepareError::Invalid(vec![PreflightFinding::UnknownSourceProfile {
        dataset: "SRC".into(),
        path: "input.unknown".into(),
    }])
}

#[test]
fn physical_kind_refines_unknown_profile_without_byte_authority() {
    for (failure, condition) in [
        (
            ResourceFailure::NotRegularFile,
            "resource_path_not_regular_file",
        ),
        (ResourceFailure::Symlink, "resource_path_symlink"),
        (ResourceFailure::Missing, "source_profile_unknown"),
    ] {
        let mut inspected = Vec::new();
        let error = source_profile_inspection::refine(unknown(), |dataset, path| {
            inspected.push((dataset.to_owned(), path.to_owned()));
            Some((failure, "../input.unknown".into()))
        });
        assert_eq!(inspected, [("SRC".into(), "input.unknown".into())]);
        let PrepareError::Invalid(findings) = error else {
            panic!("invalid findings")
        };
        let issue = findings[0].diagnostic();
        assert_eq!(issue.definition().condition, condition);
        assert_eq!(issue.spec_paths, ["input.SRC.path"]);
    }
    let PrepareError::Invalid(findings) = source_profile_inspection::refine(unknown(), |_, _| None)
    else {
        panic!("invalid findings")
    };
    assert_eq!(
        findings[0].diagnostic().definition().condition,
        "source_profile_unknown"
    );
}

#[test]
fn unsupported_and_other_static_findings_do_not_enter_inspection() {
    let unsupported = PrepareError::Unsupported(vec![UnsupportedFeature {
        operation: "future_operation".into(),
        path: "columns.X".into(),
    }]);
    assert!(matches!(
        source_profile_inspection::refine(unsupported, |_, _| panic!(
            "unsupported entered study metadata"
        )),
        PrepareError::Unsupported(_)
    ));
    let original = PreflightFinding::UndeclaredKey {
        position: 0,
        column: "ID".into(),
    };
    let expected = original.diagnostic();
    let PrepareError::Invalid(findings) =
        source_profile_inspection::refine(PrepareError::Invalid(vec![original]), |_, _| {
            panic!("unrelated finding entered study metadata")
        })
    else {
        panic!("invalid findings")
    };
    assert_eq!(findings[0].diagnostic(), expected);
}
