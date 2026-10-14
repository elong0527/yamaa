use yamaa_adapters::project_lock::{decode_renv, Error, RenvLock};
#[test]
fn uv_classification_allows_unresolved_versions_but_validates_written_identities() {
    let dynamic = b"version = 1\n[[package]]\nname = 'program'\nsource = {editable = 'program'}\n";
    assert_eq!(
        yamaa_adapters::project_lock::kind(dynamic).unwrap(),
        yamaa_core::project_environment::LockKind::Uv
    );
    for invalid in [
        "version = 1\n[[package]]\nversion = '1.0'\n",
        "version = 1\n[[package]]\nname = 'program'\nversion = 1\n",
        "version = 1\n[[package]]\nname = 'program'\nversion = ''\n",
    ] {
        assert!(yamaa_adapters::project_lock::kind(invalid.as_bytes()).is_err());
    }
}
#[test]
fn renv_metadata_preserves_exact_names_versions_and_ignores_unrelated_tool_metadata() {
    let raw=br#"{"R":{"Version":"4.6.1","Repositories":[{"Name":"CRAN","URL":"https://example.invalid"}]},"Packages":{"yamaa":{"Package":"yamaa","Version":"0.2.0","Source":"Local"},"projectbmi":{"Package":"projectbmi","Version":"1.2-0","Source":"Repository"}}}"#;
    assert_eq!(
        decode_renv(raw).unwrap(),
        RenvLock {
            runtime_version: "4.6.1".into(),
            packages: [
                ("projectbmi".into(), "1.2-0".into()),
                ("yamaa".into(), "0.2.0".into())
            ]
            .into()
        }
    );
}
#[test]
fn duplicate_keys_are_rejected_including_nested_package_version_replacement() {
    for raw in [
        r#"{"R":{"Version":"4.6.1"},"R":{"Version":"4.5.0"},"Packages":{}}"#,
        r#"{"R":{"Version":"4.6.1"},"Packages":{"yamaa":{"Version":"0.2.0","Version":"9.0.0"}}}"#,
        r#"{"R":{"Version":"4.6.1"},"Packages":{"yamaa":{"Version":"0.2.0"},"yamaa":{"Version":"9.0.0"}}}"#,
    ] {
        assert!(matches!(
            decode_renv(raw.as_bytes()),
            Err(Error::Document(_))
        ));
    }
}
#[test]
fn yaml_extensions_never_become_valid_renv_json() {
    for raw in [
        "R: {Version: '4.6.1'}\nPackages: {}",
        r#"{"R":{"Version":"4.6.1"},"Packages":{},}"#,
        r#"{"R":{"Version":NaN},"Packages":{}}"#,
    ] {
        assert!(matches!(decode_renv(raw.as_bytes()), Err(Error::Json(_))));
    }
}
#[test]
fn wrong_shapes_and_package_identity_mismatches_never_produce_partial_metadata() {
    for raw in [
        r#"{"Packages":{}}"#,
        r#"{"R":{"Version":4.6},"Packages":{}}"#,
        r#"{"R":{"Version":"4.6.1"},"Packages":[]}"#,
        r#"{"R":{"Version":"4.6.1"},"Packages":{"yamaa":{"Package":"other","Version":"0.2.0"}}}"#,
        r#"{"R":{"Version":"4.6.1"},"Packages":{"yamaa":{"Version":null}}}"#,
    ] {
        assert!(matches!(decode_renv(raw.as_bytes()), Err(Error::Shape(_))));
    }
}
#[test]
fn version_text_limits_and_nul_are_rejected_without_truncation() {
    let raw = format!(
        "{{\"R\":{{\"Version\":\"4.6.1\"}},\"Packages\":{{\"yamaa\":{{\"Version\":\"{}\"}}}}}}",
        "x".repeat(2049)
    );
    assert!(matches!(
        decode_renv(raw.as_bytes()),
        Err(Error::Limit("text"))
    ));
    assert!(matches!(
        decode_renv(br#"{"R":{"Version":"4.6.1"},"Packages":{"yamaa":{"Version":"0.2\u0000"}}}"#),
        Err(Error::Shape("Version"))
    ));
}
