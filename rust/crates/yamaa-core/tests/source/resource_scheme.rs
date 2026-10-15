use yamaa_core::resource::{has_uri_scheme, written_path_failure, ResourceFailure};

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

#[test]
fn written_form_checks_keep_normative_order_and_allow_root_dot_and_parent_paths() {
    for (path, expected) in [
        ("file:input\\x.csv", ResourceFailure::UriScheme),
        ("input\\x.csv/", ResourceFailure::Backslash),
        ("C:\\input.csv", ResourceFailure::Backslash),
        ("input//x.csv", ResourceFailure::EmptySegment),
        ("input/x.csv/", ResourceFailure::EmptySegment),
        ("", ResourceFailure::EmptySegment),
        ("/", ResourceFailure::EmptySegment),
        ("//input.csv", ResourceFailure::EmptySegment),
        ("C:/", ResourceFailure::EmptySegment),
        ("C://input.csv", ResourceFailure::EmptySegment),
    ] {
        assert_eq!(written_path_failure(path), Some(expected), "{path}");
    }
    for path in [
        "input.csv",
        "./input.csv",
        "input/../input.csv",
        "../input.csv",
        "/tmp/../input.csv",
        "C:/input.csv",
        "C:/../input.csv",
        "folder/input:name.csv",
    ] {
        assert_eq!(written_path_failure(path), None, "{path}");
    }
}
