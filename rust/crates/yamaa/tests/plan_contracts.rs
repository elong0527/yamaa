#![cfg(feature = "yaml")]
//! Changed contracts refuse a plan's run; nothing recompiles (#2821 step 3).
//!
//! A plan holds the metadata snapshots and compiled function contracts it was
//! compiled from. Each run first verifies every held snapshot through the
//! compile authority, before activation or any study read, and activates the
//! project functions afresh. A file that changed refuses that run with
//! `resource_path_content_changed` (FILE-10), naming the field that declared
//! it and the path as written, and leaves the plan unchanged: once the
//! original is back, the next run succeeds without a new compile.
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc, Mutex,
};

use yamaa::{
    Argument, CallError, CancellationToken, ContextValue, Engine, EngineOptions, FunctionError,
    FunctionHandle, FunctionIdentity, HostLanguage, LockContext, MemoryResolver, Plan,
    ProjectFunctions, Request, ResolverFailure, ResolverReply, ResolverRequest, ResourceResolver,
    RunRequest, Scalar, ScalarRef, Signature,
};

const SPECIFICATION: &[u8] = b"schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\ninput: {SRC: {path: input.csv, types: {ID: int}}}\noutput: {path: out.csv, columns: [ID]}\ncolumns:\n  - {name: ID, type: int, derivation: SRC.ID}\n";
const FUNCTION_SPECIFICATION: &[u8] = b"schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\ninput: {SRC: {path: input.csv, types: {ID: int}}}\noutput: {path: out.csv, columns: [ID, VALUE]}\ncolumns:\n  - {name: ID, type: int, derivation: SRC.ID}\n  - {name: VALUE, type: int, derivation: {function: {name: id, args: {x: SRC.ID}}}}\n";
const PROJECT: &[u8] = b"schema_version: '1.0'\nenvironments: {python: environment.yaml}\n";
const ENVIRONMENT: &[u8] = b"schema_version: '1.0'\nfunctions: {id: id.yaml}\n";
const FUNCTION_DEFINITION: &[u8] = b"function: program.id\ndescription: Integer identity.\nparams: [{name: x, type: int, accepts_missing: true}]\nreturns: int\nmay_return_missing: true\ntests:\n  - {id: normal, covers: [normal, boundary], args: {x: 7}, result: 7}\n  - {id: missing, covers: ['accepted-missing:x', nullable-output], args: {x: null}, result: null}\n";
const INPUT: &[u8] = b"ID\n1\n2\n";
const OUTPUT: &[u8] = b"ID,VALUE\n1,1\n2,2\n";
const SAVED: &str = "memory:/out.csv";

/// Record every request a resolver receives; a hidden object answers
/// `missing`, as a deleted file would.
#[derive(Default)]
struct Recording {
    inner: MemoryResolver,
    requests: Mutex<Vec<(String, String, String, bool)>>,
    hidden: Mutex<Option<String>>,
}
impl Recording {
    fn requests(&self) -> Vec<(String, String, String, bool)> {
        self.requests.lock().unwrap().clone()
    }
    fn hide(&self, written: Option<&str>) {
        *self.hidden.lock().unwrap() = written.map(str::to_owned);
    }
}
impl ResourceResolver for Recording {
    fn request(&self, request: ResolverRequest<'_>) -> Result<ResolverReply, ResolverFailure> {
        self.requests.lock().unwrap().push((
            request.operation.name().into(),
            request.kind.name().into(),
            request.written.into(),
            request.reference.is_some(),
        ));
        if self.hidden.lock().unwrap().as_deref() == Some(request.written) {
            return Err(ResolverFailure::from_code("missing"));
        }
        self.inner.request(request)
    }
}

/// The installed host functions: one identity callable whose parameter names
/// the test can change, as a redefined host function would.
struct Installed {
    parameters: Mutex<Vec<String>>,
    binds: AtomicUsize,
}
impl Installed {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            parameters: Mutex::new(vec!["x".into()]),
            binds: AtomicUsize::new(0),
        })
    }
    fn redefine(&self, parameters: &[&str]) {
        *self.parameters.lock().unwrap() = parameters.iter().map(|name| (*name).into()).collect();
    }
    fn binds(&self) -> usize {
        self.binds.load(Ordering::SeqCst)
    }
}
impl ProjectFunctions for Installed {
    fn verify_lock(
        &self,
        _: &LockContext<'_>,
        _: &[FunctionIdentity],
    ) -> Result<(), FunctionError> {
        panic!("no lock is declared")
    }
    fn bind(
        &self,
        identity: &FunctionIdentity,
        signature: &Signature,
    ) -> Result<FunctionHandle, FunctionError> {
        self.binds.fetch_add(1, Ordering::SeqCst);
        let installed = self.parameters.lock().unwrap();
        let declared = signature
            .parameters
            .iter()
            .map(|parameter| &parameter.name)
            .collect::<Vec<_>>();
        if declared.iter().copied().ne(installed.iter()) {
            return Err(FunctionError::Exception {
                class: "TypeError".into(),
                message: format!("{} takes {:?}", identity.call, *installed),
                payload: None,
            });
        }
        Ok(FunctionHandle::new(()))
    }
    fn invoke(&self, _: &FunctionHandle, arguments: &[Argument<'_>]) -> Result<Scalar, CallError> {
        Ok(match arguments[0].value {
            ScalarRef::Int(value) => Scalar::Int(value),
            _ => Scalar::Missing,
        })
    }
}

/// A project whose specification calls one project function: the
/// specification, the project file, the environment file it links for
/// Python and the function definition that file declares.
struct Project {
    resolver: Arc<Recording>,
    installed: Arc<Installed>,
    engine: Engine,
}
impl Project {
    fn new() -> Self {
        let resolver = Arc::new(Recording::default());
        let installed = Installed::new();
        let project = Self {
            engine: Engine::new(
                EngineOptions::new(HostLanguage::Python, "test".into(), "0".into())
                    .with_resolver(resolver.clone())
                    .with_functions(installed.clone()),
            ),
            resolver,
            installed,
        };
        for (written, _, _, _) in FILES {
            project.restore(written);
        }
        project
    }
    fn compile(&self) -> Plan {
        compile(
            &self.engine,
            Request::new("spec.yaml").with_environment("project.yaml"),
        )
    }
    /// Put back the original object version of one file.
    fn restore(&self, file: &str) {
        let (written, declaring, reference, bytes) = FILES
            .into_iter()
            .find(|(written, _, _, _)| *written == file)
            .unwrap();
        self.resolver
            .inner
            .insert(declaring, written, reference, Arc::from(bytes));
    }
    /// A new object version of one file, with other content.
    fn replace(&self, file: &str, bytes: &[u8]) {
        let (written, declaring, _, _) = FILES
            .into_iter()
            .find(|(written, _, _, _)| *written == file)
            .unwrap();
        self.resolver
            .inner
            .insert(declaring, written, "replaced:2", Arc::from(bytes));
    }
}
/// Each metadata file: written path, declaring identity, reference, bytes.
const FILES: [(&str, &str, &str, &[u8]); 4] = [
    ("spec.yaml", "/", "spec:1", FUNCTION_SPECIFICATION),
    ("project.yaml", "/", "project:1", PROJECT),
    (
        "environment.yaml",
        "/project.yaml",
        "environment:1",
        ENVIRONMENT,
    ),
    (
        "id.yaml",
        "/environment.yaml",
        "function:1",
        FUNCTION_DEFINITION,
    ),
];

fn compile(engine: &Engine, request: Request<'_>) -> Plan {
    let plan = engine.compile(&request, &CancellationToken::new()).unwrap();
    assert!(plan.report().is_empty(), "{:?}", plan.report());
    plan
}

/// Run from a fresh study that serves the input and receives the save.
fn run(plan: &Plan) -> (yamaa::Build, Arc<Recording>) {
    let study = Arc::new(Recording::default());
    study
        .inner
        .insert("/spec.yaml", "input.csv", "data:1", Arc::from(INPUT));
    let build = plan
        .run(
            &RunRequest::new().with_resolver(study.clone()),
            &CancellationToken::new(),
        )
        .unwrap();
    (build, study)
}

/// Run, save to the study and return the saved bytes.
fn saved(plan: &Plan) -> Vec<u8> {
    let (mut build, study) = run(plan);
    assert!(build.report().is_empty(), "{:?}", build.report());
    build.save().unwrap();
    study.inner.published(SAVED).unwrap().to_vec()
}

/// A run refused at its start: exactly one record of `condition` naming the
/// declaring `field` and the `path` as written, no study request, and no
/// save.
fn assert_refused(plan: &Plan, condition: &str, field: &str, path: &str) {
    let (mut build, study) = run(plan);
    assert!(build.output().is_none());
    let report = build.report();
    assert_eq!(report.len(), 1, "{report:?}");
    let refusal = &report.diagnostics[0];
    let (phase, rule) = match condition {
        "resource_path_content_changed" => ("ingest", "FILE-10"),
        "resource_path_missing" => ("validation", "FILE-7"),
        other => panic!("unexpected condition {other}"),
    };
    assert_eq!(refusal.code, condition);
    assert_eq!(refusal.phase, phase);
    assert_eq!(refusal.rule.as_deref(), Some(rule));
    assert_eq!(refusal.spec_path, [field]);
    assert_eq!(refusal.context.len(), 1, "{:?}", refusal.context);
    assert_eq!(
        refusal.context.get("path"),
        Some(&ContextValue::Text(path.into()))
    );
    assert!(study.requests().is_empty());
    assert!(build.save().is_err());
    assert!(study.inner.published(SAVED).is_none());
}

#[test]
fn an_unchanged_plan_runs_many_times_verifying_its_files_and_never_recompiling() {
    let project = Project::new();
    let plan = project.compile();
    // Compilation activates nothing and reads no study data.
    assert_eq!(project.installed.binds(), 0);
    let compiled = project.resolver.requests();
    assert!(compiled.iter().all(|(_, kind, _, _)| kind == "metadata"));
    for attempt in 1..=5 {
        let before = project.resolver.requests().len();
        assert_eq!(saved(&plan), OUTPUT);
        // Each run activates the project functions afresh.
        assert_eq!(project.installed.binds(), attempt);
        // Each run only verifies the held files against their references:
        // nothing is resolved or captured anew, so nothing recompiles.
        let mut verified = project.resolver.requests()[before..].to_vec();
        verified.sort();
        let mut expected = FILES
            .iter()
            .map(|(written, _, _, _)| {
                ("capture".into(), "metadata".into(), (*written).into(), true)
            })
            .collect::<Vec<_>>();
        expected.sort();
        assert_eq!(verified, expected);
    }
}

#[test]
fn a_changed_environment_refuses_runs_until_it_is_restored() {
    for (file, field, replacement) in [
        (
            "project.yaml",
            "environment",
            &b"schema_version: '1.0'\nenvironments: {python: environment.yaml}\nsdtm: {specs: [spec.yaml]}\n"[..],
        ),
        (
            "environment.yaml",
            "environments.python",
            &b"schema_version: '1.0'\nfunctions: {id: id.yaml}\nlock: uv.lock\n"[..],
        ),
    ] {
        let project = Project::new();
        let plan = project.compile();
        assert_eq!(saved(&plan), OUTPUT);
        project.replace(file, replacement);
        assert_refused(&plan, "resource_path_content_changed", field, file);
        assert_refused(&plan, "resource_path_content_changed", field, file);
        // The refusal precedes activation.
        assert_eq!(project.installed.binds(), 1);
        // The original is back: the same plan runs again, with no recompile.
        project.restore(file);
        assert_eq!(saved(&plan), OUTPUT);
        assert_eq!(project.installed.binds(), 2);
    }
}

#[test]
fn a_changed_function_contract_refuses_runs_until_it_is_restored() {
    let project = Project::new();
    let plan = project.compile();
    assert_eq!(saved(&plan), OUTPUT);
    // The definition now names another callable: another contract identity.
    let other = std::str::from_utf8(FUNCTION_DEFINITION)
        .unwrap()
        .replace("program.id", "program.other");
    project.replace("id.yaml", other.as_bytes());
    assert_refused(
        &plan,
        "resource_path_content_changed",
        "functions.id",
        "id.yaml",
    );
    assert_eq!(project.installed.binds(), 1);
    // A fresh compilation adopts the new contract; the plan keeps refusing.
    let recompiled = project.compile();
    assert_refused(
        &plan,
        "resource_path_content_changed",
        "functions.id",
        "id.yaml",
    );
    project.restore("id.yaml");
    assert_eq!(saved(&plan), OUTPUT);
    // The fresh plan holds the replaced definition, now changed in turn.
    assert_refused(
        &recompiled,
        "resource_path_content_changed",
        "functions.id",
        "id.yaml",
    );
}

#[test]
fn a_removed_environment_file_refuses_runs_as_missing_until_it_returns() {
    let project = Project::new();
    let plan = project.compile();
    assert_eq!(saved(&plan), OUTPUT);
    project.resolver.hide(Some("environment.yaml"));
    assert_refused(
        &plan,
        "resource_path_missing",
        "environments.python",
        "environment.yaml",
    );
    project.resolver.hide(None);
    assert_eq!(saved(&plan), OUTPUT);
}

#[test]
fn a_changed_specification_refuses_runs_until_it_is_restored() {
    let resolver = Arc::new(MemoryResolver::new());
    resolver.insert("/", "spec.yaml", "spec:1", Arc::from(SPECIFICATION));
    let engine = Engine::new(
        EngineOptions::new(HostLanguage::Python, "test".into(), "0".into())
            .with_resolver(resolver.clone()),
    );
    let plan = compile(&engine, Request::new("spec.yaml"));
    assert_eq!(saved(&plan), INPUT);
    resolver.insert(
        "/",
        "spec.yaml",
        "spec:2",
        Arc::from(&b"schema_version: '1.0'\ndomain: OTHER\n"[..]),
    );
    assert_refused(
        &plan,
        "resource_path_content_changed",
        "specification",
        "spec.yaml",
    );
    // A one-shot build compiles the replacement afresh and is not refused
    // by the plan's held snapshot.
    let replaced = engine
        .build(&Request::new("spec.yaml"), &CancellationToken::new())
        .unwrap();
    assert!(!replaced.report().is_empty());
    assert!(replaced
        .report()
        .diagnostics
        .iter()
        .all(|diagnostic| diagnostic.code != "resource_path_content_changed"));
    resolver.insert("/", "spec.yaml", "spec:1", Arc::from(SPECIFICATION));
    assert_eq!(saved(&plan), INPUT);
}

#[test]
fn a_redefined_host_function_is_refused_at_activation_until_restored() {
    let project = Project::new();
    let plan = project.compile();
    assert_eq!(saved(&plan), OUTPUT);
    assert_eq!(project.installed.binds(), 1);
    // The installed callable now reports another signature.
    project.installed.redefine(&["y"]);
    let (mut refused, study) = run(&plan);
    assert_eq!(project.installed.binds(), 2);
    assert!(refused.output().is_none());
    let first = &refused.report().diagnostics[0];
    assert_eq!(first.phase, "validation");
    assert_eq!(first.code, "project_environment_invalid");
    assert_eq!(first.rule.as_deref(), Some("FUNC-2"));
    assert_eq!(
        first.context.get("function"),
        Some(&ContextValue::Text("id".into()))
    );
    assert_eq!(
        first.context.get("call"),
        Some(&ContextValue::Text("program.id".into()))
    );
    // Activation precedes every study request.
    assert!(study.requests().is_empty());
    assert!(refused.save().is_err());
    project.installed.redefine(&["x"]);
    assert_eq!(saved(&plan), OUTPUT);
    assert_eq!(project.installed.binds(), 3);
}

/// Local files: every held file a project specification's plan was compiled
/// from refuses the run under the field that declared it, and the plan runs
/// again once the file is restored.
#[test]
fn every_changed_local_file_refuses_runs_under_its_declaring_field() {
    const BASE: &str = "schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\ninput: {P: {path: produced.csv, schema: producer.yaml}}\noutput: {path: out.csv, columns: [ID, VALUE]}\ncolumns:\n  - {name: ID, type: int, label: Identifier, derivation: P.ID}\n  - {name: VALUE, type: int, label: Value, derivation: {function: {name: id, args: {x: P.ID}}}}\n";
    const PRODUCER: &str = "schema_version: '1.0'\ndomain: PRODUCER\nkeys: [ID]\ninput: {RAW: {path: raw.csv, types: {ID: int}}}\ncolumns:\n  - {name: ID, type: int, label: Identifier, derivation: RAW.ID}\noutput: {path: produced.csv, columns: [ID]}\n";
    let root = std::env::temp_dir().join(format!("yamaa-plan-contracts-{}", std::process::id()));
    std::fs::create_dir_all(root.join("python")).unwrap();
    // The engine's spelling: no Windows verbatim prefix.
    let root = yamaa_adapters::canonical_path::canonicalize(&root).unwrap();
    let files: [(&str, &[u8]); 7] = [
        ("spec.yaml", b"schema_version: '1.0'\nparent: base.yaml\n"),
        ("base.yaml", BASE.as_bytes()),
        ("producer.yaml", PRODUCER.as_bytes()),
        ("produced.csv", INPUT),
        (
            "project.yaml",
            b"schema_version: '1.0'\nenvironments: {python: python/environment.yaml}\n",
        ),
        ("python/environment.yaml", ENVIRONMENT),
        ("python/id.yaml", FUNCTION_DEFINITION),
    ];
    for (name, bytes) in files {
        std::fs::write(root.join(name), bytes).unwrap();
    }
    let path = |name: &str| {
        let path = root.join(name).to_str().unwrap().to_owned();
        #[cfg(windows)]
        let path = path.replace('\\', "/");
        path
    };
    let installed = Installed::new();
    let engine = Engine::new(
        EngineOptions::new(HostLanguage::Python, "test".into(), "0".into())
            .with_functions(installed.clone()),
    );
    let project = path("project.yaml");
    let plan = compile(
        &engine,
        Request::new(&path("spec.yaml")).with_environment(&project),
    );
    let succeeds = || {
        let build = plan
            .run(&RunRequest::new(), &CancellationToken::new())
            .unwrap();
        assert!(build.report().is_empty(), "{:?}", build.report());
        assert_eq!(build.output().unwrap().rows(), 2);
    };
    succeeds();
    for (name, field, written) in [
        ("spec.yaml", "specification", "spec.yaml"),
        ("base.yaml", "parent", "base.yaml"),
        ("producer.yaml", "input.P.schema", "producer.yaml"),
        ("project.yaml", "environment", project.as_str()),
        (
            "python/environment.yaml",
            "environments.python",
            "python/environment.yaml",
        ),
        ("python/id.yaml", "functions.id", "id.yaml"),
    ] {
        let binds = installed.binds();
        std::fs::write(root.join(name), b"schema_version: '1.0'\n").unwrap();
        let build = plan
            .run(&RunRequest::new(), &CancellationToken::new())
            .unwrap();
        assert!(build.output().is_none());
        let report = build.report();
        assert_eq!(report.len(), 1, "{name}: {report:?}");
        let refusal = &report.diagnostics[0];
        assert_eq!(refusal.code, "resource_path_content_changed", "{name}");
        assert_eq!(refusal.phase, "ingest");
        assert_eq!(refusal.rule.as_deref(), Some("FILE-10"));
        assert_eq!(refusal.spec_path, [field], "{name}");
        assert_eq!(
            refusal.context.get("path"),
            Some(&ContextValue::Text(written.into())),
            "{name}"
        );
        assert_eq!(
            installed.binds(),
            binds,
            "{name}: refused before activation"
        );
        let (_, original) = files.iter().find(|(file, _)| *file == name).unwrap();
        std::fs::write(root.join(name), original).unwrap();
        succeeds();
    }
    assert!(!root.join("out.csv").exists());
    std::fs::remove_dir_all(root).unwrap();
}

/// A specification whose column admits only names of the study's datasets
/// holds every specification its project lists. A changed listed
/// specification refuses the run under its listing.
#[test]
fn a_changed_listed_specification_refuses_runs_under_its_listing() {
    const LB: &str = "schema_version: '1.0'\ndomain: LB\nkeys: [USUBJID, LBTESTCD]\ninput: {RAW: raw.csv}\noutput: {path: lb.csv, columns: [USUBJID, LBTESTCD]}\ncolumns:\n  - name: USUBJID\n    type: str\n    label: Unique Subject Identifier\n    derivation: RAW.USUBJID\n    submission: {core: Req, length: 20, origin: {type: Assigned, source: Sponsor}}\n  - name: LBTESTCD\n    type: str\n    label: Lab Test or Examination Short Name\n    derivation: RAW.LBTESTCD\n    submission: {core: Req, length: 8, origin: {type: Assigned, source: Sponsor}}\nsubmission: {label: Laboratory Test Results, class: FINDINGS, structure: One record per lab test per subject, repeating: true}\n";
    const ADLB: &str = "schema_version: '1.0'\ndomain: ADLB\nkeys: [USUBJID, PARAMCD]\ninput: {LB: lb.csv}\noutput: {path: adlb.csv, columns: [USUBJID, PARAMCD, SRCDOM]}\ncolumns:\n  - name: USUBJID\n    type: str\n    label: Unique Subject Identifier\n    derivation: LB.USUBJID\n    submission: {core: Req, mandatory: true, length: 20, origin: {type: Predecessor, description: LB.USUBJID}}\n  - name: PARAMCD\n    type: str\n    label: Parameter Code\n    derivation: LB.LBTESTCD\n    submission: {core: Req, mandatory: true, length: 8, origin: {type: Predecessor, description: LB.LBTESTCD}}\n  - name: SRCDOM\n    type: str\n    label: Source Data\n    derivation: {literal: LB}\n    submission: {core: Perm, mandatory: false, length: 8, inventory_vocabulary: true, origin: {type: Assigned}}\nsubmission: {label: Laboratory Analysis Dataset, class: BASIC DATA STRUCTURE, structure: One record per subject per parameter, repeating: true}\n";
    const PROJECT: &str =
        "schema_version: '1.0'\nadam:\n  specs: [adlb.yaml]\nsdtm:\n  specs: [sdtm/lb.yaml]\n";
    let root = std::env::temp_dir().join(format!(
        "yamaa-plan-contracts-inventory-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(root.join("sdtm")).unwrap();
    let root = yamaa_adapters::canonical_path::canonicalize(&root).unwrap();
    for (name, text) in [
        ("sdtm/lb.yaml", LB),
        ("adlb.yaml", ADLB),
        ("project.yaml", PROJECT),
        ("lb.csv", "USUBJID,LBTESTCD\nS01-001,GLUC\n"),
    ] {
        std::fs::write(root.join(name), text).unwrap();
    }
    let path = |name: &str| {
        let path = root.join(name).to_str().unwrap().to_owned();
        #[cfg(windows)]
        let path = path.replace('\\', "/");
        path
    };
    let engine = Engine::new(
        EngineOptions::new(HostLanguage::Python, "test".into(), "0".into())
            .with_functions(Installed::new()),
    );
    let plan = compile(
        &engine,
        Request::new(&path("adlb.yaml")).with_environment(&path("project.yaml")),
    );
    let run = || {
        plan.run(&RunRequest::new(), &CancellationToken::new())
            .unwrap()
    };
    let build = run();
    assert!(build.report().is_empty(), "{:?}", build.report());
    assert_eq!(build.output().unwrap().rows(), 1);
    std::fs::write(root.join("sdtm/lb.yaml"), LB.replace("Lab Test", "Test")).unwrap();
    let refused = run();
    let report = refused.report();
    assert_eq!(report.len(), 1, "{report:?}");
    assert_eq!(report.diagnostics[0].code, "resource_path_content_changed");
    assert_eq!(report.diagnostics[0].spec_path, ["sdtm.specs[0]"]);
    assert_eq!(
        report.diagnostics[0].context.get("path"),
        Some(&ContextValue::Text("sdtm/lb.yaml".into()))
    );
    std::fs::write(root.join("sdtm/lb.yaml"), LB).unwrap();
    assert!(run().report().is_empty());
    std::fs::remove_dir_all(root).unwrap();
}
