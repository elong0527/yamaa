#![cfg(any(unix, windows))]
#[path = "support/project_schemas.rs"]
mod schemas;
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};
use yamaa_adapters::{
    file_project::{Error, FileProject},
    file_resources::{Error as ResourceError, Resources},
    project_run::BoundaryFailure,
    project_source::{CaptureFailure, Failure as EnvironmentFailure, Origin},
};
use yamaa_core::{
    function_signature::{LogicalSignature, ProjectFunctionIdentity},
    project_environment::{LockKind, LockReference},
    project_function::Language,
    table::{TableAccess, ValueRef},
    value::Value,
};
use yamaa_engine::{
    function_invocation::{Argument, HostError},
    project_activation::{ActivationPort, Failure, LockObservation},
    specification_run::PortError,
};

static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Study(PathBuf);
impl Study {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "yamaa-file-project-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        for name in ["env", "entry", "parent", "outside"] {
            fs::create_dir(path.join(name)).unwrap();
        }
        let study = Self(fs::canonicalize(path).unwrap());
        study.write("env/environment.yaml", "schema_version: '1.0'\nlanguage: python\nlock: ../uv.lock\nfunctions: {id: id.yaml}\ncodelists: [ct.yaml]\n");
        study.write("uv.lock", "version = 1\n");
        study.write("env/id.yaml", "function: program.id\ndescription: Integer identity.\nparams: [{name: x, type: int, accepts_missing: true}]\nreturns: int\nmay_return_missing: true\ntests:\n  - {id: normal, covers: [normal, boundary], args: {x: 7}, result: 7}\n  - {id: missing, covers: ['accepted-missing:x', nullable-output], args: {x: null}, result: null}\n");
        study.write(
            "env/ct.yaml",
            "codelists: [{id: SEX, name: Sex, items: [{value: F}, {value: M}]}]\n",
        );
        study.write("entry/domain.yaml", DOMAIN);
        study
    }
    fn text(&self, name: &str) -> String {
        self.0.join(name).to_str().unwrap().into()
    }
    fn write(&self, name: &str, bytes: &str) {
        fs::write(self.0.join(name), bytes).unwrap();
    }
    fn prepare(&self, root: &str, roots: &[String]) -> Result<FileProject, Error> {
        FileProject::prepare(
            Resources::new(root, &self.text("entry"), roots).unwrap(),
            "domain.yaml",
            "../env/environment.yaml",
            Language::Python,
            schemas::domain_schema(),
            schemas::environment_schema(),
        )
    }
}
impl Drop for Study {
    fn drop(&mut self) {
        if let Err(error) = fs::remove_dir_all(&self.0) {
            if !std::thread::panicking() {
                panic!(
                    "failed to remove study directory {}: {error}",
                    self.0.display()
                );
            }
        }
    }
}
const DOMAIN: &str = "schema_version: '1.0'\ndomain: TEST\ninput: {SRC: {path: input.csv, types: {ID: int}}}\nkeys: [ID]\ncolumns:\n  - {name: ID, type: int, derivation: SRC.ID}\n  - {name: VALUE, type: int, derivation: {function: {name: id, args: {x: SRC.ID}}}}\noutput: {path: output.csv, columns: [ID, VALUE]}\n";

#[derive(Default)]
struct Activation {
    trace: Vec<&'static str>,
    reject: bool,
    mismatch: bool,
}
impl ActivationPort for Activation {
    type Handle = ();
    type Error = &'static str;
    fn verify_lock(
        &mut self,
        _: Language,
        lock: &LockReference,
        functions: &[ProjectFunctionIdentity],
    ) -> Result<(), Self::Error> {
        assert_eq!(lock.kind, LockKind::Uv);
        assert_eq!(lock.written, "../uv.lock");
        assert_eq!(functions[0].call, "program.id");
        self.trace.push("lock");
        if self.reject {
            Err("original lock rejection")
        } else {
            Ok(())
        }
    }
    fn bind(
        &mut self,
        _: &ProjectFunctionIdentity,
        _: &LogicalSignature,
    ) -> Result<(), Self::Error> {
        self.trace.push("bind");
        Ok(())
    }
    fn invoke(&mut self, _: &(), args: &[Argument<'_>]) -> Result<Value, HostError<Self::Error>> {
        self.trace.push("call");
        let [Argument { name: "x", value }] = args else {
            panic!("exact named argument")
        };
        match value {
            ValueRef::Int(n) => Ok(Value::Int(if self.mismatch { n + 1 } else { *n })),
            ValueRef::Missing => Ok(Value::Missing),
            _ => panic!("integer or admitted missing"),
        }
    }
}

#[test]
fn project_report_uses_held_external_function_origins_after_metadata_files_change() {
    let study = Study::new();
    // Resource identities use portable separators and omit the Windows verbatim
    // prefix; the host path passed to preparation keeps its native spelling.
    let identity = |name| {
        let path = study.text(name);
        #[cfg(windows)]
        let path = path.trim_start_matches(r"\\?\").replace('\\', "/");
        path
    };
    let mut project = study.prepare(&study.text(""), &[]).unwrap();
    let reads = project.capture_reads();
    let mut activation = Activation {
        mismatch: true,
        ..Default::default()
    };
    let attempt = project.build(&mut activation);
    let run = project.retained_run();
    study.write("env/id.yaml", "changed after preparation");
    study.write("env/environment.yaml", "changed root after preparation");
    drop(project);
    let result = yamaa_adapters::project_report::build_result(
        &run,
        &attempt,
        yamaa_adapters::specification_report::Identity {
            runtime: "test",
            runtime_version: "1",
            engine_version: "0.1.0",
            example: "external-project",
            specification: "domain.yaml",
            base_directory: "entry",
        },
        &[],
    )
    .unwrap();
    assert_eq!(result.issues().len(), 1);
    assert_eq!(result.issues()[0].spec_paths, ["tests[0]"]);
    let context: serde_json::Value = serde_json::from_str(&result.issues()[0].context).unwrap();
    assert_eq!(context["source"], identity("env/id.yaml"));
    assert_eq!(context["entry"], identity("env/environment.yaml"));
    assert_eq!(context["environment_path"], "functions.id");
    assert_eq!(context["actual"], 8);
    assert_eq!(context["expected"], 7);
    assert_eq!(result.observations()["source_reads"], serde_json::json!([]));
    assert_eq!(activation.trace, ["lock", "bind", "call", "call"]);
    assert_eq!(reads, 5);
    assert_eq!(
        run.captured_environment().captures()[0]
            .document
            .source()
            .identity,
        identity("env/id.yaml")
    );
}

#[test]
fn native_project_retains_metadata_and_reuses_exact_snapshots_with_fresh_activation() {
    let study = Study::new();
    let mut project = study.prepare(&study.text(""), &[]).unwrap();
    assert_eq!(project.capture_reads(), 5);
    let captures = project.run().captured_environment();
    assert_eq!(
        captures.root().source().bytes,
        fs::read(study.0.join("env/environment.yaml")).unwrap()
    );
    assert_eq!(captures.lock().unwrap().source.bytes, b"version = 1\n");
    assert_eq!(captures.captures().len(), 2);
    assert_eq!(captures.captures()[0].origin, Origin::Function("id".into()));
    assert!(captures.captures()[0]
        .document
        .source()
        .identity
        .ends_with("/env/id.yaml"));
    assert!(project.run().check_diagnostics().is_empty());
    let root_bytes = captures.root().source().bytes.as_ptr();
    let lock_bytes = captures.lock().unwrap().source.bytes.as_ptr();
    let definition = project.run().environment().functions()[0]
        .definition()
        .tests
        .as_ptr();
    // Metadata bytes belong to the prepared capability, independently of later
    // filesystem changes. Study bytes are first captured only after activation.
    study.write("env/environment.yaml", "[changed");
    study.write("env/id.yaml", "[changed");
    study.write("uv.lock", "changed");
    study.write("entry/domain.yaml", "[changed");
    study.write("entry/input.csv", "ID\n1\n");
    let mut activation = Activation::default();
    let first = project.build(&mut activation);
    assert!(first.boundary.is_ok());
    assert_eq!(first.activation.lock, LockObservation::Verified);
    assert_eq!(project.capture_reads(), 6);
    assert_eq!(
        first
            .dataset
            .result
            .as_ref()
            .unwrap()
            .result
            .as_ref()
            .unwrap()
            .dataset
            .rows(),
        &[vec![Value::Int(1), Value::Int(1)]]
    );
    let second = project.build(&mut activation);
    assert!(second.boundary.is_ok());
    assert_eq!(project.capture_reads(), 6);
    assert_eq!(second.dataset.sources[0].read.snapshots_created, Some(0));
    assert!(std::sync::Arc::ptr_eq(
        first.dataset.sources[0].snapshot.as_ref().unwrap(),
        second.dataset.sources[0].snapshot.as_ref().unwrap()
    ));
    assert_eq!(
        second.dataset.sources[0]
            .table
            .as_ref()
            .unwrap()
            .cell(0, 0)
            .unwrap(),
        ValueRef::Int(1)
    );
    assert_eq!(
        activation.trace,
        ["lock", "bind", "call", "call", "call", "lock", "bind", "call", "call", "call"]
    );
    assert_eq!(
        project
            .run()
            .captured_environment()
            .root()
            .source()
            .bytes
            .as_ptr(),
        root_bytes
    );
    assert_eq!(
        project
            .run()
            .captured_environment()
            .lock()
            .unwrap()
            .source
            .bytes
            .as_ptr(),
        lock_bytes
    );
    assert_eq!(
        project.run().environment().functions()[0]
            .definition()
            .tests
            .as_ptr(),
        definition
    );
}

#[test]
fn native_project_activation_rejection_precedes_even_a_missing_study_file() {
    let study = Study::new();
    let mut project = study.prepare(&study.text(""), &[]).unwrap();
    let mut activation = Activation {
        reject: true,
        ..Default::default()
    };
    let attempt = project.build(&mut activation);
    assert!(matches!(
        attempt.boundary,
        Err(BoundaryFailure::Activation(Failure::Lock(
            "original lock rejection"
        )))
    ));
    assert!(attempt.dataset.sources.is_empty());
    assert!(matches!(attempt.dataset.result, Err(PortError::Incomplete)));
    assert_eq!(project.capture_reads(), 5);
    assert_eq!(activation.trace, ["lock"]);
}

#[test]
fn installed_port_factory_borrows_the_exact_prepared_lock_on_each_build() {
    struct Borrowed<'a> {
        lock: &'a [u8],
        expected: *const u8,
        inner: Activation,
    }
    impl ActivationPort for Borrowed<'_> {
        type Handle = ();
        type Error = &'static str;
        fn verify_lock(
            &mut self,
            language: Language,
            lock: &LockReference,
            functions: &[ProjectFunctionIdentity],
        ) -> Result<(), Self::Error> {
            assert_eq!(self.lock.as_ptr(), self.expected);
            assert_eq!(self.lock, b"version = 1\n");
            self.inner.verify_lock(language, lock, functions)
        }
        fn bind(
            &mut self,
            identity: &ProjectFunctionIdentity,
            signature: &LogicalSignature,
        ) -> Result<(), Self::Error> {
            self.inner.bind(identity, signature)
        }
        fn invoke(
            &mut self,
            handle: &(),
            args: &[Argument<'_>],
        ) -> Result<Value, HostError<Self::Error>> {
            self.inner.invoke(handle, args)
        }
    }
    let study = Study::new();
    let mut project = study.prepare(&study.text(""), &[]).unwrap();
    let expected = project
        .run()
        .captured_environment()
        .lock()
        .unwrap()
        .source
        .bytes
        .as_ptr();
    study.write("entry/input.csv", "ID\n1\n");
    let mut constructed = 0;
    for _ in 0..2 {
        let attempt = project.build_with(|run| {
            constructed += 1;
            Borrowed {
                lock: &run.captured_environment().lock().unwrap().source.bytes,
                expected,
                inner: Activation::default(),
            }
        });
        assert!(attempt.boundary.is_ok());
        assert_eq!(attempt.activation.lock, LockObservation::Verified);
        assert!(attempt.dataset.result.unwrap().result.is_ok());
    }
    assert_eq!(constructed, 2);
    assert_eq!(project.capture_reads(), 6);
}

#[test]
fn native_project_keeps_approved_roots_for_metadata_and_study_without_expanding_them() {
    let study = Study::new();
    // The selected entry root does not authorize its sibling environment. The
    // same explicit approved fallback used by ordinary builds makes it available.
    let Err(Error::Resource(ResourceError::OutsideRoots)) =
        study.prepare(&study.text("entry"), &[])
    else {
        panic!("environment outside roots")
    };
    let mut project = study
        .prepare(&study.text("entry"), &[study.text("")])
        .unwrap();
    study.write("input.csv", "ID\n1\n");
    let attempt = project.build(&mut Activation::default());
    assert!(attempt.boundary.is_ok());
    assert!(attempt.dataset.result.unwrap().result.is_ok());
    assert!(attempt.dataset.sources[0].read.source.path == "input.csv");

    study.write("env/environment.yaml", "schema_version: '1.0'\nlanguage: python\nlock: ../uv.lock\nfunctions: {id: ../../unapproved.yaml}\n");
    let Err(Error::Environment(EnvironmentFailure::Rejected(rejected))) =
        study.prepare(&study.text(""), &[])
    else {
        panic!("unapproved definition")
    };
    assert_eq!(rejected.sources.len(), 1);
    assert_eq!(rejected.sources[0].written, "../../unapproved.yaml");
    assert!(matches!(
        rejected.sources[0].error,
        CaptureFailure::Port(ResourceError::OutsideRoots)
    ));
    assert!(rejected.lock.is_some());
}

#[test]
fn native_project_inheritance_retains_parent_authorship_and_selected_entry_source_paths() {
    let study = Study::new();
    study.write(
        "parent/base.yaml",
        DOMAIN.replace("input.csv", "data.csv").as_str(),
    );
    study.write("parent/data.csv", "ID\n1\n");
    study.write(
        "entry/domain.yaml",
        "schema_version: '1.0'\nparents: ../parent/base.yaml\n",
    );
    let mut project = study.prepare(&study.text(""), &[]).unwrap();
    assert_eq!(project.capture_reads(), 6);
    let document = project.run().document();
    assert_eq!(document.parents().len(), 1);
    assert_eq!(document.written_source_path("SRC"), Some("data.csv"));
    assert_eq!(project.run().compiled().source().path, "../parent/data.csv");
    let attempt = project.build(&mut Activation::default());
    assert!(attempt.boundary.is_ok());
    assert!(attempt.dataset.result.unwrap().result.is_ok());
    assert_eq!(project.capture_reads(), 7);
}
