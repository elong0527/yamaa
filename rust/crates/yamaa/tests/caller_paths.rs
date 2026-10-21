//! A path the caller passes, the specification's or the project file's, is
//! the caller's own (#3321). One that reaches no entry, reaches a directory
//! or another entry that is not a regular file, or is a link fails with
//! FILE-7's or FILE-8's condition on every route, at no field, naming the
//! path as given (FILE-11), exactly as `Engine::file_type` and `lint` refuse
//! it (D125). The same holds whether the local file resolver or a caller's
//! resolver reports the kind; any other failure to read the path stays the
//! opaque engine fault.
#![cfg(feature = "yaml")]

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use yamaa::{
    CancellationToken, Diagnostic, Engine, EngineOptions, Failure, HostLanguage, MemoryResolver,
    ResolverFailure, ResolverReply, ResolverRequest, ResourceKind, ResourceResolver, Status,
};

static NEXT: AtomicUsize = AtomicUsize::new(0);

const SPEC: &str = "yamaa: \"1.0\"\ntype: spec\nmodel: SDTM\ndomain: DM\nkeys: [USUBJID]\ninput: {RAW: {path: raw.csv}}\noutput: {path: dm.csv, columns: [USUBJID]}\ncolumns:\n  - {name: USUBJID, type: str, derivation: RAW.USUBJID}\n";
const PROJECT: &str = "yamaa: \"1.0\"\ntype: project\nsdtm:\n  specs: [dm.yaml]\n";

struct Study(PathBuf);
impl Study {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "yamaa-caller-paths-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&root).unwrap();
        // The engine's spelling: no Windows verbatim prefix.
        let study = Self(yamaa_adapters::canonical_path::canonicalize(root).unwrap());
        study.write("dm.yaml", SPEC);
        study.write("project.yaml", PROJECT);
        study.write("raw.csv", "USUBJID\nS1\n");
        std::fs::create_dir(study.0.join("folder.yaml")).unwrap();
        std::fs::create_dir(study.0.join("sub")).unwrap();
        study
    }
    fn write(&self, name: &str, contents: &str) {
        std::fs::write(self.0.join(name), contents).unwrap();
    }
    fn path(&self, name: &str) -> String {
        let path = self.0.join(name).to_str().unwrap().to_owned();
        #[cfg(windows)]
        let path = path.replace('\\', "/");
        path
    }
    /// Each path that reaches no regular file, as the caller gives it, with
    /// the condition and rule that refuse it.
    fn unreadable(&self) -> Vec<(String, &'static str, &'static str)> {
        #[cfg_attr(not(unix), allow(unused_mut))]
        let mut paths = vec![
            (self.path("absent.yaml"), "resource_path_missing", "FILE-7"),
            // A missing parent directory reaches no entry either.
            (
                self.path("absent/dm.yaml"),
                "resource_path_missing",
                "FILE-7",
            ),
            // The path is named as given, dot segments included.
            (
                self.path("sub/../absent.yaml"),
                "resource_path_missing",
                "FILE-7",
            ),
            (
                self.path("folder.yaml"),
                "resource_path_not_regular_file",
                "FILE-7",
            ),
        ];
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(self.0.join("dm.yaml"), self.0.join("dm-link.yaml"))
                .unwrap();
            std::os::unix::fs::symlink(
                self.0.join("project.yaml"),
                self.0.join("project-link.yaml"),
            )
            .unwrap();
            for name in ["dm-link.yaml", "project-link.yaml"] {
                paths.push((self.path(name), "resource_path_symlink", "FILE-8"));
            }
        }
        paths
    }
}
impl Drop for Study {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn engine() -> Engine {
    Engine::new(EngineOptions::new(
        HostLanguage::Python,
        "caller-paths".into(),
        "0".into(),
    ))
}

fn engine_with(resolver: Arc<dyn ResourceResolver>) -> Engine {
    Engine::new(
        EngineOptions::new(HostLanguage::Python, "caller-paths".into(), "0".into())
            .with_resolver(resolver),
    )
}

/// An issue as the tests compare it: phase, condition, rule, spec paths and
/// context as canonical JSON.
type Row = (String, String, Option<String>, Vec<String>, String);

fn rows(diagnostics: &[Diagnostic]) -> Vec<Row> {
    diagnostics
        .iter()
        .map(|issue| {
            (
                issue.phase.clone(),
                issue.code.clone(),
                issue.rule.clone(),
                issue.spec_path.clone(),
                issue.context.to_json().to_owned(),
            )
        })
        .collect()
}

/// The one issue that refuses `path`: its file rule's condition, at no
/// field, naming the path as given.
fn unread(condition: &str, rule: &str, path: &str) -> Vec<Row> {
    vec![(
        "validation".into(),
        condition.into(),
        Some(rule.into()),
        Vec::new(),
        format!(r#"{{"path":"{path}"}}"#),
    )]
}

/// A refusal of `call` that `expected` explains, as D39 words it.
fn refused(failure: &Failure, call: &str, expected: &[Row]) {
    assert_eq!(rows(&failure.diagnostics), expected, "{call}");
    assert_eq!(
        failure.message,
        format!("{call}: {}", failure.diagnostics[0].text())
    );
    assert!(failure.paths.is_empty());
}

/// Every route that reads the specification `path` refuses it with
/// `expected`, and names it as given.
fn spec_routes(engine: &Engine, path: &str, expected: &[Row]) {
    let token = CancellationToken::new();
    let check = engine.spec_check(path, None, &token).unwrap();
    assert_eq!(rows(&check.diagnostics()), expected, "check {path}");
    assert_eq!(check.status(), Status::Failed);
    let mut run = engine.spec_run(path, None, &token).unwrap();
    assert_eq!(rows(&run.diagnostics()), expected, "run {path}");
    assert!(run.output().is_none() && run.report().is_none());
    // A run whose file was not read names the path as given.
    for _ in 0..2 {
        refused(
            &run.save().unwrap_err(),
            &format!("cannot save {path}"),
            expected,
        );
    }
    refused(
        &engine.spec_resolve(path, None).unwrap_err(),
        &format!("cannot resolve {path}"),
        expected,
    );
    // The command line reads the type first (D122): the same issue.
    refused(
        &engine.file_type(path).unwrap_err(),
        &format!("cannot read {path}"),
        expected,
    );
}

/// Every route that reads the project file `project` refuses it with
/// `expected` before any specification is read, and names it as given.
fn project_routes(engine: &Engine, project: &str, specification: &str, expected: &[Row]) {
    let token = CancellationToken::new();
    assert_eq!(
        rows(&engine.project_check(project, &token).unwrap().diagnostics()),
        expected,
        "check {project}"
    );
    let mut run = engine.project_run(project, None, &token).unwrap();
    assert_eq!(rows(&run.diagnostics()), expected, "run {project}");
    assert!(run.names().is_empty());
    assert_eq!(run.status(), Status::Failed);
    refused(
        &run.save().unwrap_err(),
        &format!("cannot save {project}"),
        expected,
    );
    refused(
        &engine.project_spec(project, "DM").unwrap_err(),
        "cannot select DM",
        expected,
    );
    let mut submission = engine
        .project_submission(
            project,
            Some("sdtm"),
            Some("2026-09-11T00:00:00"),
            "0",
            &token,
        )
        .unwrap();
    assert_eq!(rows(&submission.diagnostics()), expected);
    refused(
        &submission.save().unwrap_err(),
        &format!("cannot save {project}, section sdtm"),
        expected,
    );
    // A specification read with the project reports the project first.
    assert_eq!(
        rows(
            &engine
                .spec_check(specification, Some(project), &token)
                .unwrap()
                .diagnostics()
        ),
        expected,
        "check {specification} with {project}"
    );
    assert_eq!(
        rows(
            &engine
                .spec_run(specification, Some(project), &token)
                .unwrap()
                .diagnostics()
        ),
        expected
    );
    refused(
        &engine
            .spec_resolve(specification, Some(project))
            .unwrap_err(),
        &format!("cannot resolve {specification}"),
        expected,
    );
    refused(
        &engine.file_type(project).unwrap_err(),
        &format!("cannot read {project}"),
        expected,
    );
}

#[test]
fn a_specification_path_that_reaches_no_regular_file_fails_with_its_file_rule_on_every_route() {
    let study = Study::new();
    let engine = engine();
    for (path, condition, rule) in study.unreadable() {
        spec_routes(&engine, &path, &unread(condition, rule, &path));
    }
}

#[test]
fn a_project_path_that_reaches_no_regular_file_fails_with_its_file_rule_on_every_route() {
    let study = Study::new();
    let engine = engine();
    let specification = study.path("dm.yaml");
    for (path, condition, rule) in study.unreadable() {
        project_routes(
            &engine,
            &path,
            &specification,
            &unread(condition, rule, &path),
        );
    }
}

#[test]
fn every_kind_a_caller_resolver_reports_for_a_caller_path_raises_its_file_rule_condition() {
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
        resolver.insert_kind("/", "study/dm.yaml", kind);
        resolver.insert_kind("/", "study/project.yaml", kind);
        resolver.insert(
            "/",
            "study/valid.yaml",
            "valid:1",
            Arc::from(SPEC.as_bytes()),
        );
        let engine = engine_with(resolver);
        spec_routes(
            &engine,
            "study/dm.yaml",
            &unread(condition, rule, "study/dm.yaml"),
        );
        project_routes(
            &engine,
            "study/project.yaml",
            "study/valid.yaml",
            &unread(condition, rule, "study/project.yaml"),
        );
    }
}

/// A resolver that reports every caller path as unreadable, a failure no
/// file rule classifies.
struct Unreadable;
impl ResourceResolver for Unreadable {
    fn request(&self, _: ResolverRequest<'_>) -> Result<ResolverReply, ResolverFailure> {
        Err(ResolverFailure::from_code("unreadable"))
    }
}

#[test]
fn any_other_failure_to_read_a_caller_path_stays_the_opaque_engine_fault() {
    let engine = engine_with(Arc::new(Unreadable));
    let expected = vec![(
        "engine".to_owned(),
        "engine_rejected".to_owned(),
        None,
        Vec::new(),
        r#"{"code":"resource_unreadable","stage":"prepare"}"#.to_owned(),
    )];
    spec_routes(&engine, "dm.yaml", &expected);
    project_routes(&engine, "project.yaml", "dm.yaml", &expected);
}
