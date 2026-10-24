//! A save walks to each target when it publishes (#3030 D189). A directory on
//! the way that does not exist, one that is not a directory, a target that is
//! a directory, and a link on the way or at the target are refused with
//! FILE-7's or FILE-8's condition, decided while publishing so under
//! `output`, at the field that declares the file and naming it as written; a
//! table saved to a path its caller names is refused at no field, naming the
//! path as given (D134). Nothing is written, and a later save retries.
#![cfg(feature = "yaml")]

use std::path::Path;
use std::sync::Arc;

use yamaa::host::resolver::{
    ResolverFailure, ResolverOperation, ResolverReply, ResolverRequest, ResourceKind,
};
use yamaa::{
    CancellationToken, Diagnostic, Engine, EngineOptions, Failure, HostLanguage, MemoryResolver,
    ResourceResolver, SubmissionOptions,
};

struct Study(crate::support::Scratch);
impl Study {
    fn new() -> Self {
        let root = crate::support::Scratch::new("save-targets");
        let study = Self(root);
        study.write("raw.csv", "USUBJID\nS1\n");
        study
    }
    fn write(&self, name: &str, contents: &str) {
        let path = self.0.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, contents).unwrap();
    }
    fn path(&self, name: &str) -> String {
        let path = self.0.join(name).to_str().unwrap().to_owned();
        #[cfg(windows)]
        let path = path.replace('\\', "/");
        path
    }
    /// A specification whose output is `output`, and whose report file is
    /// `report` when given, written as `spec.yaml`.
    fn specification(&self, output: &str, report: Option<&str>) -> String {
        let report = report.map_or_else(String::new, |report| format!(", report: {report}"));
        self.write(
            "spec.yaml",
            &format!(
                "yamaa: \"1.0\"\ntype: spec\nmodel: SDTM\ndomain: DM\nkeys: [USUBJID]\ninput: {{RAW: raw.csv}}\noutput: {{path: {output}, columns: [USUBJID]{report}}}\ncolumns:\n  - {{name: USUBJID, type: str, derivation: RAW.USUBJID}}\n"
            ),
        );
        self.path("spec.yaml")
    }
}

fn engine() -> Engine {
    Engine::new(EngineOptions::new(
        HostLanguage::Python,
        "save-targets".into(),
        "0".into(),
    ))
}

/// An issue as these tests compare it: phase, condition, rule, spec paths
/// and context as canonical JSON.
type Row = (String, String, Option<String>, Vec<String>, String);

fn rows(diagnostics: &[Diagnostic]) -> Vec<Row> {
    diagnostics
        .iter()
        .map(|issue| {
            (
                issue.phase.as_str().to_owned(),
                issue.code.as_str().to_owned(),
                issue.rule.clone(),
                issue.spec_path.clone(),
                issue.context.to_json().to_owned(),
            )
        })
        .collect()
}

/// The walk's refusal of the file `path` names: its file rule's condition
/// under `output`, at `field` when a file declares it.
fn unreached(condition: &str, rule: &str, field: Option<&str>, path: &str) -> Row {
    (
        "output".into(),
        condition.into(),
        Some(rule.into()),
        field.map(String::from).into_iter().collect(),
        format!(r#"{{"path":"{path}"}}"#),
    )
}

/// D39's refusal of a save of `subject` that `expected` explains, having
/// written nothing.
fn refused(failure: &Failure, subject: &str, expected: &[Row]) {
    assert_eq!(rows(&failure.diagnostics), expected, "{}", failure.message);
    assert_eq!(
        failure.message,
        format!("cannot save {subject}: {}", failure.diagnostics[0].text())
    );
    assert!(failure.paths.is_empty());
}

#[test]
fn a_save_refuses_a_target_its_walk_cannot_reach_at_the_field_that_declares_it() {
    let study = Study::new();
    std::fs::create_dir(study.0.join("folder.csv")).unwrap();
    std::fs::create_dir(study.0.join("present")).unwrap();
    #[cfg_attr(not(unix), allow(unused_mut))]
    let mut cases = vec![
        // A directory that does not exist, whichever directory of the way.
        ("adam/dm.csv", "resource_path_missing", "FILE-7"),
        ("present/absent/dm.csv", "resource_path_missing", "FILE-7"),
        // A regular file where a directory belongs.
        ("raw.csv/dm.csv", "resource_path_not_regular_file", "FILE-7"),
        // A directory where the file belongs.
        ("folder.csv", "resource_path_not_regular_file", "FILE-7"),
    ];
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(study.0.join("present"), study.0.join("linked")).unwrap();
        std::os::unix::fs::symlink(study.0.join("raw.csv"), study.0.join("link.csv")).unwrap();
        cases.extend([
            ("linked/dm.csv", "resource_path_symlink", "FILE-8"),
            ("link.csv", "resource_path_symlink", "FILE-8"),
        ]);
    }
    let token = CancellationToken::new();
    for (output, condition, rule) in cases {
        let spec = study.specification(output, None);
        let mut run = engine().spec(&spec).cancel_with(&token).run().unwrap();
        assert!(run.output().is_some(), "{output}");
        let expected = [unreached(condition, rule, Some("output.path"), output)];
        for _ in 0..2 {
            refused(
                &run.save(&CancellationToken::new()).unwrap_err(),
                "DM",
                &expected,
            );
        }
        assert_eq!(rows(&run.diagnostics()), expected, "{output}");
    }
    assert!(std::fs::read_dir(study.0.join("present"))
        .unwrap()
        .next()
        .is_none());
    assert!(std::fs::read_dir(study.0.join("folder.csv"))
        .unwrap()
        .next()
        .is_none());
}

#[test]
fn the_report_file_is_refused_at_its_own_field_and_a_later_save_retries() {
    let study = Study::new();
    let spec = study.specification("out/dm.csv", Some("checks/report.csv"));
    let mut run = engine().spec(&spec).run().unwrap();
    // The report file comes first (OUTPUT-10).
    refused(
        &run.save(&CancellationToken::new()).unwrap_err(),
        "DM",
        &[unreached(
            "resource_path_missing",
            "FILE-7",
            Some("output.report"),
            "checks/report.csv",
        )],
    );
    std::fs::create_dir(study.0.join("checks")).unwrap();
    refused(
        &run.save(&CancellationToken::new()).unwrap_err(),
        "DM",
        &[unreached(
            "resource_path_missing",
            "FILE-7",
            Some("output.path"),
            "out/dm.csv",
        )],
    );
    // Nothing was replaced: the save writes every file or none (D74).
    assert!(!study.0.join("checks/report.csv").exists());
    std::fs::create_dir(study.0.join("out")).unwrap();
    assert_eq!(
        run.save(&CancellationToken::new()).unwrap(),
        [study.path("checks/report.csv"), study.path("out/dm.csv")]
    );
    assert!(run.diagnostics().is_empty());
}

#[test]
fn a_project_run_save_names_the_member_whose_target_it_cannot_reach() {
    let study = Study::new();
    study.specification("adam/dm.csv", None);
    study.write(
        "project.yaml",
        "yamaa: \"1.0\"\ntype: project\nsdtm:\n  specs: [spec.yaml]\n",
    );
    let project = study.path("project.yaml");
    let mut run = engine().project(&project).run().unwrap();
    let failure = run.save(&CancellationToken::new()).unwrap_err();
    let [issue] = failure.diagnostics.as_slice() else {
        panic!("one issue: {failure:?}")
    };
    assert_eq!(
        (
            issue.phase.as_str(),
            issue.code.as_str(),
            issue.rule.as_deref()
        ),
        ("output", "resource_path_missing", Some("FILE-7"))
    );
    assert_eq!(issue.spec_path, ["output.path"]);
    assert_eq!(
        issue.context.get("path"),
        Some(&yamaa::ContextValue::Text("adam/dm.csv".into()))
    );
    assert!(failure
        .message
        .starts_with(&format!("cannot save {project}: ")));
}

#[test]
fn a_submission_save_refuses_its_files_at_the_section_define() {
    let study = Study::new();
    let fixture =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../benchmarks/sdtm-dm-metadata");
    for name in ["spec.yaml", "project.yaml"] {
        std::fs::copy(fixture.join(name), study.0.join(name)).unwrap();
    }
    std::fs::create_dir(study.0.join("input")).unwrap();
    std::fs::copy(
        fixture.join("input/dm_raw.csv"),
        study.0.join("input/dm_raw.csv"),
    )
    .unwrap();
    std::fs::copy(fixture.join("expected/dm.csv"), study.0.join("dm.csv")).unwrap();
    let mut submission = engine()
        .project(&study.path("project.yaml"))
        .section("sdtm")
        .submission(&SubmissionOptions::new("0.4.0").with_created("2026-09-11T00:00:00"))
        .unwrap();
    assert!(submission.diagnostics().is_empty());
    // A directory where the section's first file, its transport file, goes
    // (JSON-14). The section's `define` places every file of the section.
    std::fs::create_dir(study.0.join("dm.json")).unwrap();
    refused(
        &submission.save(&CancellationToken::new()).unwrap_err(),
        &format!("{}, section sdtm", study.path("project.yaml")),
        &[unreached(
            "resource_path_not_regular_file",
            "FILE-7",
            Some("sdtm.define"),
            "dm.json",
        )],
    );
    assert!(!study.0.join("define.xml").exists());
}

#[test]
fn a_table_saved_to_a_path_its_caller_names_is_refused_at_no_field() {
    let study = Study::new();
    let spec = study.specification("dm.csv", None);
    let engine = engine();
    let run = engine.spec(&spec).run().unwrap();
    let table = run.output().unwrap();
    std::fs::create_dir(study.0.join("folder.csv")).unwrap();
    #[cfg_attr(not(unix), allow(unused_mut))]
    let mut cases = vec![
        (
            study.path("absent/dm.csv"),
            "resource_path_missing",
            "FILE-7",
        ),
        (
            study.path("folder.csv"),
            "resource_path_not_regular_file",
            "FILE-7",
        ),
    ];
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(study.0.join("raw.csv"), study.0.join("link.csv")).unwrap();
        cases.push((study.path("link.csv"), "resource_path_symlink", "FILE-8"));
    }
    for (out, condition, rule) in cases {
        refused(
            &engine.save_table(table, &out).unwrap_err(),
            &out,
            &[unreached(condition, rule, None, &out)],
        );
    }
    assert!(!study.0.join("absent").exists());
}

/// A memory resolver whose target replies report `kind` for `written`.
struct Refusing {
    inner: MemoryResolver,
    written: &'static str,
    kind: ResourceKind,
}
impl ResourceResolver for Refusing {
    fn request(&self, request: ResolverRequest<'_>) -> Result<ResolverReply, ResolverFailure> {
        if request.operation == ResolverOperation::Target && request.written == self.written {
            return Err(ResolverFailure::from_kind(self.kind));
        }
        self.inner.request(request)
    }
}

#[test]
fn a_kind_a_caller_resolver_reports_for_a_target_is_refused_as_the_walk_refuses_it() {
    const SPEC: &[u8] = b"yamaa: \"1.0\"\ntype: spec\nmodel: SDTM\ndomain: DM\nkeys: [USUBJID]\ninput: {RAW: raw.csv}\noutput: {path: out/dm.csv, columns: [USUBJID]}\ncolumns:\n  - {name: USUBJID, type: str, derivation: RAW.USUBJID}\n";
    for (kind, condition, rule) in [
        (ResourceKind::Missing, "resource_path_missing", "FILE-7"),
        (
            ResourceKind::Directory,
            "resource_path_not_regular_file",
            "FILE-7",
        ),
        (ResourceKind::Junction, "resource_path_symlink", "FILE-8"),
    ] {
        let inner = MemoryResolver::new();
        inner.insert("/", "spec.yaml", "spec:1", Arc::from(SPEC));
        inner.insert(
            "/spec.yaml",
            "raw.csv",
            "raw:1",
            Arc::from(&b"USUBJID\nS1\n"[..]),
        );
        let engine = Engine::new(
            EngineOptions::new(HostLanguage::Python, "save-targets".into(), "0".into())
                .with_resolver(Arc::new(Refusing {
                    inner,
                    written: "out/dm.csv",
                    kind,
                })),
        );
        let mut run = engine.spec("spec.yaml").run().unwrap();
        let expected = [unreached(
            condition,
            rule,
            Some("output.path"),
            "out/dm.csv",
        )];
        refused(
            &run.save(&CancellationToken::new()).unwrap_err(),
            "DM",
            &expected,
        );
        // A table saved through the resolver names the path as given.
        let out = "out/dm.csv";
        refused(
            &engine.save_table(run.output().unwrap(), out).unwrap_err(),
            out,
            &[unreached(condition, rule, None, out)],
        );
    }
}
