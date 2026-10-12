use yamaa_core::resource::has_uri_scheme;

#[test]
fn uri_grammar_preserves_local_absolute_parent_and_windows_drive_paths() {
    for path in [
        "https://example.invalid/x.csv",
        "s3:bucket/x.csv",
        "file:///tmp/x.csv",
        "git+ssh:resource",
        "a.b-c:value",
    ] {
        assert!(has_uri_scheme(path), "{path}");
    }
    for path in [
        "input.csv",
        "../input.csv",
        "/tmp/input.csv",
        "C:/input.csv",
        "C:\\input.csv",
        "1bad:value",
        ":value",
        "folder/input:name.csv",
    ] {
        assert!(!has_uri_scheme(path), "{path}");
    }
}
