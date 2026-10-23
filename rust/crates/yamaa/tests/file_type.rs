//! `Engine::file_type` reads the type a file declares, once, through the
//! engine's resolver (#3030 D122). Only a 1.x file declares a type: a file
//! that declares `schema_version` is refused as `unsupported_language_version`
//! at `schema_version` (SCHEMA-2). A 1.x header is refused as a call reading
//! its type refuses it, and a path that reaches no regular file as any read
//! refuses it (FILE-7, FILE-8, FILE-11), each as `cannot read <path>` (D39).
#![cfg(feature = "yaml")]

use std::sync::{Arc, Mutex};

use yamaa::{
    CancellationToken, Engine, EngineOptions, Failure, FileType, MemoryResolver, ResolverFailure,
    ResolverKind, ResolverOperation, ResolverReply, ResolverRequest, ResourceKind,
    ResourceResolver,
};

struct Study(crate::support::Scratch);
impl Study {
    fn new() -> Self {
        let root = crate::support::Scratch::new("file-type");
        Self(root)
    }
    fn write(&self, name: &str, contents: &str) -> String {
        std::fs::write(self.0.join(name), contents).unwrap();
        self.path(name)
    }
    fn path(&self, name: &str) -> String {
        let path = self.0.join(name).to_str().unwrap().to_owned();
        #[cfg(windows)]
        let path = path.replace('\\', "/");
        path
    }
}

/// An engine that claims no host language, as the command line's does.
fn engine() -> Engine {
    Engine::new(EngineOptions::neutral("file-type-test".into(), "0".into()))
}

fn engine_with(resolver: Arc<dyn ResourceResolver>) -> Engine {
    Engine::new(EngineOptions::neutral("file-type-test".into(), "0".into()).with_resolver(resolver))
}

/// The one issue of a refusal: its condition, rule, spec paths and context
/// as canonical JSON, after checking the refusal names the call and that
/// issue (D39) and its table holds exactly that row.
fn refused(failure: &Failure, path: &str) -> (String, Option<String>, Vec<String>, String) {
    let [issue] = failure.diagnostics.as_slice() else {
        panic!("one issue: {failure:?}")
    };
    assert_eq!(
        failure.message,
        format!("cannot read {path}: {}", issue.text())
    );
    assert_eq!(failure.issues.rows(), 1);
    assert!(failure.paths.is_empty());
    assert_eq!(issue.phase, "validation");
    (
        issue.code.clone(),
        issue.rule.clone(),
        issue.spec_path.clone(),
        issue.context.to_json().to_owned(),
    )
}

fn issue(
    condition: &str,
    rule: &str,
    path: &str,
    context: &str,
) -> (String, Option<String>, Vec<String>, String) {
    (
        condition.into(),
        Some(rule.into()),
        vec![path.into()],
        context.into(),
    )
}

/// A path refusal: its file rule's condition, at no field, naming `path`.
fn unread(
    condition: &str,
    rule: &str,
    path: &str,
) -> (String, Option<String>, Vec<String>, String) {
    (
        condition.into(),
        Some(rule.into()),
        Vec::new(),
        format!(r#"{{"path":"{path}"}}"#),
    )
}

#[test]
fn each_type_a_1_x_file_declares_is_read_from_its_header() {
    let study = Study::new();
    let engine = engine();
    for file_type in [
        FileType::Spec,
        FileType::Project,
        FileType::Codelists,
        FileType::Function,
    ] {
        // Only the header is read: the rest of the file is not checked.
        let path = study.write(
            "file.yaml",
            &format!(
                "yamaa: \"1.0\"\ntype: {}\nunchecked: [1, 2]\n",
                file_type.as_str()
            ),
        );
        assert_eq!(engine.file_type(&path).unwrap(), file_type);
    }
}

#[test]
fn a_file_that_declares_schema_version_has_no_type() {
    let study = Study::new();
    let engine = engine();
    for text in [
        "schema_version: '1.0'\ndomain: DM\n",
        // `yamaa` beside `schema_version` keeps the file a 0.5 file (D75).
        "schema_version: '1.0'\nyamaa: \"1.0\"\ntype: spec\ndomain: DM\n",
        // Whatever version it names.
        "schema_version: '2.0'\n",
    ] {
        let path = study.write("dm.yaml", text);
        let failure = engine.file_type(&path).unwrap_err();
        assert_eq!(
            refused(&failure, &path),
            issue(
                "unsupported_language_version",
                "SCHEMA-2",
                "schema_version",
                r#"{"language_versions":["1.0"],"version":"0.5"}"#,
            ),
            "{text}"
        );
        assert_eq!(
            failure.diagnostics[0].message,
            "this engine does not read language version 0.5"
        );
    }
}

#[test]
fn a_1_x_header_is_refused_as_a_call_reading_its_type_refuses_it() {
    let study = Study::new();
    let engine = engine();
    // A newer version is refused at `yamaa`, as a check of the file is.
    let path = study.write("dm.yaml", "yamaa: \"1.1\"\ntype: spec\ndomain: DM\n");
    let failure = engine.file_type(&path).unwrap_err();
    let newer = issue(
        "unsupported_language_version",
        "SCHEMA-2",
        "yamaa",
        r#"{"language_versions":["1.0"],"version":"1.1"}"#,
    );
    assert_eq!(refused(&failure, &path), newer);
    let check = engine
        .spec_check(&path, None, &CancellationToken::new())
        .unwrap()
        .diagnostics();
    assert_eq!(
        (
            check[0].code.clone(),
            check[0].rule.clone(),
            check[0].spec_path.clone(),
            check[0].context.to_json().to_owned()
        ),
        newer
    );
    // A malformed header is refused at its field, naming the class of the
    // type the file declares, or none when it declares none it knows.
    for (text, expected) in [
        (
            "type: project\n",
            issue(
                "missing_required_field",
                "SCHEMA-13",
                "yamaa",
                r#"{"class":"project_class","field":"yamaa"}"#,
            ),
        ),
        (
            "domain: DM\n",
            issue(
                "missing_required_field",
                "SCHEMA-13",
                "yamaa",
                r#"{"field":"yamaa"}"#,
            ),
        ),
        (
            "yamaa: \"1.0\"\n",
            issue(
                "missing_required_field",
                "SCHEMA-13",
                "type",
                r#"{"field":"type"}"#,
            ),
        ),
        (
            "yamaa: 1.0\ntype: spec\n",
            issue(
                "invalid_field_type",
                "SCHEMA-12",
                "yamaa",
                r#"{"actual":"float","expected":"language_version"}"#,
            ),
        ),
        (
            "- yamaa\n",
            issue(
                "invalid_field_type",
                "SCHEMA-12",
                "$",
                r#"{"actual":"sequence","expected":"non-empty dict"}"#,
            ),
        ),
    ] {
        let path = study.write("file.yaml", text);
        let failure = engine.file_type(&path).unwrap_err();
        assert_eq!(refused(&failure, &path), expected, "{text}");
    }
    // A type naming none of the four is unexpected where any is expected.
    let path = study.write("file.yaml", "yamaa: \"1.0\"\ntype: environment\n");
    let failure = engine.file_type(&path).unwrap_err();
    assert_eq!(
        refused(&failure, &path),
        issue(
            "unexpected_file_type",
            "SCHEMA-2",
            "type",
            r#"{"expected":["spec","project","codelists","function"],"type":"environment"}"#,
        )
    );
    // Text that is not YAML is refused as the entry of a call is.
    let path = study.write("file.yaml", "yamaa: [\n");
    let failure = engine.file_type(&path).unwrap_err();
    let (condition, rule, paths, _) = refused(&failure, &path);
    assert_eq!(
        (condition.as_str(), rule.as_deref(), paths),
        ("invalid_yaml", Some("SCHEMA-3"), vec!["$".to_owned()])
    );
}

#[test]
fn a_path_that_reaches_no_regular_file_is_refused_as_any_read() {
    let study = Study::new();
    let engine = engine();
    for path in [study.path("missing.yaml"), study.path("missing/dm.yaml")] {
        let failure = engine.file_type(&path).unwrap_err();
        assert_eq!(
            refused(&failure, &path),
            unread("resource_path_missing", "FILE-7", &path)
        );
        assert_eq!(
            failure.message,
            format!("cannot read {path}: {path} does not exist [resource_path_missing, FILE-7]")
        );
    }
    std::fs::create_dir(study.0.join("directory.yaml")).unwrap();
    let path = study.path("directory.yaml");
    let failure = engine.file_type(&path).unwrap_err();
    assert_eq!(
        refused(&failure, &path),
        unread("resource_path_not_regular_file", "FILE-7", &path)
    );
    #[cfg(unix)]
    {
        let target = study.write("target.yaml", "yamaa: \"1.0\"\ntype: spec\n");
        std::os::unix::fs::symlink(&target, study.0.join("link.yaml")).unwrap();
        let path = study.path("link.yaml");
        let failure = engine.file_type(&path).unwrap_err();
        assert_eq!(
            refused(&failure, &path),
            unread("resource_path_symlink", "FILE-8", &path)
        );
    }
}

/// A resolver that records every request it serves.
struct Recording {
    inner: MemoryResolver,
    requests: Mutex<Vec<(ResolverOperation, ResolverKind, String, String)>>,
}

impl ResourceResolver for Recording {
    fn request(&self, request: ResolverRequest<'_>) -> Result<ResolverReply, ResolverFailure> {
        self.requests.lock().unwrap().push((
            request.operation,
            request.kind,
            request.declaring.to_owned(),
            request.written.to_owned(),
        ));
        self.inner.request(request)
    }
}

#[test]
fn a_caller_resolver_serves_the_file_in_one_read() {
    let recording = Arc::new(Recording {
        inner: MemoryResolver::new(),
        requests: Mutex::new(Vec::new()),
    });
    recording.inner.insert(
        "/",
        "study/project.yaml",
        "project:1",
        Arc::from(&b"yamaa: \"1.0\"\ntype: project\n"[..]),
    );
    let engine = engine_with(recording.clone());
    assert_eq!(
        engine.file_type("study/project.yaml").unwrap(),
        FileType::Project
    );
    assert_eq!(
        *recording.requests.lock().unwrap(),
        [(
            ResolverOperation::Capture,
            ResolverKind::Metadata,
            "/".to_owned(),
            "study/project.yaml".to_owned()
        )]
    );
}

#[test]
fn every_kind_a_resolver_reports_raises_its_file_rule_condition() {
    for (kind, condition, rule) in [
        (ResourceKind::Missing, "resource_path_missing", "FILE-7"),
        (
            ResourceKind::Directory,
            "resource_path_not_regular_file",
            "FILE-7",
        ),
        (
            ResourceKind::Other,
            "resource_path_not_regular_file",
            "FILE-7",
        ),
        (
            ResourceKind::SymbolicLink,
            "resource_path_symlink",
            "FILE-8",
        ),
        (ResourceKind::Junction, "resource_path_symlink", "FILE-8"),
        (
            ResourceKind::ReparsePoint,
            "resource_path_symlink",
            "FILE-8",
        ),
    ] {
        let resolver = Arc::new(MemoryResolver::new());
        resolver.insert_kind("/", "dm.yaml", kind);
        let failure = engine_with(resolver).file_type("dm.yaml").unwrap_err();
        assert_eq!(
            refused(&failure, "dm.yaml"),
            unread(condition, rule, "dm.yaml"),
            "{kind:?}"
        );
    }
}
