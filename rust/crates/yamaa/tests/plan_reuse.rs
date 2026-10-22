#![cfg(feature = "yaml")]
//! Host lazy objects reuse what their first run compiled (#2821 steps 4-5).
//!
//! A [`Plans`] holder keeps the plan each specification's first run compiled.
//! A later run reuses it once the run-start check finds every file it was
//! compiled from unchanged, and compiles again when one changed, so no run is
//! stale. A run's private measurements say how it got its plan: `compiled`,
//! `refused` or `reused`; a compiling run's also start with the `preparation`
//! phase. The host suites and the conformance repeat pass read the same.
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc, Mutex,
};

use yamaa::{
    Argument, CallError, CancellationToken, Engine, EngineOptions, FunctionError, FunctionHandle,
    FunctionIdentity, HostLanguage, LockContext, MemoryResolver, Plans, ProjectFunctions,
    ResolverFailure, ResolverReply, ResolverRequest, ResourceResolver, Run, Scalar, ScalarRef,
    Signature,
};

const SPECIFICATION: &str = "schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\ninput: {SRC: {path: input.csv, types: {ID: int}}}\noutput: {path: out.csv, columns: [ID, VALUE]}\ncolumns:\n  - {name: ID, type: int, derivation: SRC.ID}\n  - {name: VALUE, type: int, derivation: {function: {name: f, args: {x: SRC.ID}}}}\n";
const PROJECT: &str = "schema_version: '1.0'\nenvironments: {python: environment.yaml}\n";
const ENVIRONMENT: &str = "schema_version: '1.0'\nfunctions: {f: f.yaml}\n";
const IDENTITY: &str = "function: program.same\ndescription: Integer identity.\nparams: [{name: x, type: int, accepts_missing: true}]\nreturns: int\nmay_return_missing: true\ntests:\n  - {id: normal, covers: [normal, boundary], args: {x: 7}, result: 7}\n  - {id: missing, covers: ['accepted-missing:x', nullable-output], args: {x: null}, result: null}\n";
const INPUT: &str = "ID\n1\n2\n";
const SAME: &str = "ID,VALUE\n1,1\n2,2\n";
const DOUBLED: &str = "ID,VALUE\n1,2\n2,4\n";

/// The doubling contract: another callable, with tests of its own.
fn doubling() -> String {
    IDENTITY
        .replace("program.same", "program.double")
        .replace("Integer identity.", "Twice the integer.")
        .replace("result: 7", "result: 14")
}

/// Installed host functions: `program.same` returns its argument and
/// `program.double` twice it. Binds are counted per installed port.
#[derive(Default)]
struct Installed {
    binds: AtomicUsize,
    nested: Mutex<Option<Box<dyn Fn() + Send>>>,
}
impl Installed {
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
        _: &Signature,
    ) -> Result<FunctionHandle, FunctionError> {
        self.binds.fetch_add(1, Ordering::SeqCst);
        Ok(FunctionHandle::new(identity.call.clone()))
    }
    fn invoke(
        &self,
        handle: &FunctionHandle,
        arguments: &[Argument<'_>],
    ) -> Result<Scalar, CallError> {
        // Take the nested run before calling it: it binds and invokes too.
        let nested = self.nested.lock().unwrap().take();
        if let Some(nested) = nested {
            nested();
        }
        let factor = handle
            .with(|call: &String| if call == "program.double" { 2 } else { 1 })
            .unwrap();
        Ok(match arguments[0].value {
            ScalarRef::Int(value) => Scalar::Int(value * factor),
            _ => Scalar::Missing,
        })
    }
}

/// Record whether each metadata request resolves or captures anew (a
/// compilation) or verifies a held reference (a reuse).
#[derive(Default)]
struct Recording {
    inner: MemoryResolver,
    fresh: AtomicUsize,
    verified: AtomicUsize,
}
impl Recording {
    fn take(&self) -> (usize, usize) {
        (
            self.fresh.swap(0, Ordering::SeqCst),
            self.verified.swap(0, Ordering::SeqCst),
        )
    }
}
impl ResourceResolver for Recording {
    fn request(&self, request: ResolverRequest<'_>) -> Result<ResolverReply, ResolverFailure> {
        if request.kind.name() == "metadata" {
            let counter = if request.reference.is_some() {
                &self.verified
            } else {
                &self.fresh
            };
            counter.fetch_add(1, Ordering::SeqCst);
        }
        self.inner.request(request)
    }
}

fn engine(resolver: Option<Arc<dyn ResourceResolver>>, installed: &Arc<Installed>) -> Engine {
    let mut options = EngineOptions::new(HostLanguage::Python, "test".into(), "0".into())
        .with_functions(installed.clone());
    if let Some(resolver) = resolver {
        options = options.with_resolver(resolver);
    }
    Engine::new(options)
}

/// How the run got its plan, from its private measurements.
fn plan_of(run: &Run) -> &'static str {
    let observed = run.build().observations(true).unwrap();
    let statuses = ["compiled", "refused", "reused"]
        .into_iter()
        .filter(|status| observed.contains(&format!("\"plan\":\"{status}\"")))
        .collect::<Vec<_>>();
    assert_eq!(statuses.len(), 1, "{observed}");
    statuses[0]
}

/// Whether the run compiled: its plan status, and its measurements include
/// the `preparation` phase.
fn compiled(run: &Run) -> bool {
    let preparation = run
        .build()
        .observations(true)
        .unwrap()
        .contains("\"phase\":\"preparation\"");
    assert_eq!(preparation, plan_of(run) != "reused");
    preparation
}

/// The saved output bytes of a run through a memory resolver.
fn saved(run: &mut Run, resolver: &Recording) -> String {
    assert_eq!(run.issues().rows(), 0, "{:?}", run.build().report());
    run.save().unwrap();
    let bytes = resolver.inner.published("memory:/out.csv").unwrap();
    String::from_utf8(bytes.to_vec()).unwrap()
}

/// A memory study: the specification, project file, environment file and
/// function definition, plus the input.
fn memory_study() -> Arc<Recording> {
    let resolver = Arc::new(Recording::default());
    for (declaring, written, text) in [
        ("/", "spec.yaml", SPECIFICATION),
        ("/", "project.yaml", PROJECT),
        ("/project.yaml", "environment.yaml", ENVIRONMENT),
        ("/environment.yaml", "f.yaml", IDENTITY),
        ("/spec.yaml", "input.csv", INPUT),
    ] {
        resolver.inner.insert(
            declaring,
            written,
            &format!("{written}:1"),
            Arc::from(text.as_bytes()),
        );
    }
    resolver
}

fn run(engine: &Engine, plans: &Plans) -> Run {
    engine
        .spec_run_reusing(
            "spec.yaml",
            Some("project.yaml"),
            plans,
            &CancellationToken::new(),
        )
        .unwrap()
}

#[test]
fn a_holder_is_send_and_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<Plans>();
}

#[test]
fn later_runs_reuse_the_plan_the_first_run_compiled() {
    let resolver = memory_study();
    let installed = Arc::new(Installed::default());
    let engine = engine(Some(resolver.clone()), &installed);
    let plans = Plans::new();
    let mut first = run(&engine, &plans);
    assert_eq!(plan_of(&first), "compiled");
    assert!(compiled(&first));
    assert_eq!(saved(&mut first, &resolver), SAME);
    let (fresh, _) = resolver.take();
    assert!(fresh > 0);
    for attempt in 2..=4 {
        let mut again = run(&engine, &plans);
        assert_eq!(plan_of(&again), "reused");
        assert!(!compiled(&again));
        assert_eq!(saved(&mut again, &resolver), SAME);
        // Nothing is resolved or captured anew; each of the four held files
        // is verified once against its reference.
        assert_eq!(resolver.take(), (0, 4));
        // Each run still activates the project function afresh.
        assert_eq!(installed.binds(), attempt);
    }
    // A one-shot run, and a run through another holder, compile.
    let one_shot = engine
        .spec_run("spec.yaml", Some("project.yaml"), &CancellationToken::new())
        .unwrap();
    assert!(compiled(&one_shot));
    assert!(compiled(&run(&engine, &Plans::new())));
}

#[test]
fn a_changed_specification_compiles_again_and_runs_the_change() {
    let resolver = memory_study();
    let installed = Arc::new(Installed::default());
    let engine = engine(Some(resolver.clone()), &installed);
    let plans = Plans::new();
    assert_eq!(saved(&mut run(&engine, &plans), &resolver), SAME);
    let changed = SPECIFICATION.replace("columns: [ID, VALUE]", "columns: [VALUE, ID]");
    resolver.inner.insert(
        "/",
        "spec.yaml",
        "spec.yaml:2",
        Arc::from(changed.as_bytes()),
    );
    let mut recompiled = run(&engine, &plans);
    assert!(compiled(&recompiled));
    assert_eq!(saved(&mut recompiled, &resolver), "VALUE,ID\n1,1\n2,2\n");
    // The new plan is kept in turn.
    assert!(!compiled(&run(&engine, &plans)));
}

#[test]
fn a_changed_environment_or_function_contract_compiles_again_and_runs_the_change() {
    let resolver = memory_study();
    let installed = Arc::new(Installed::default());
    let engine = engine(Some(resolver.clone()), &installed);
    let plans = Plans::new();
    assert_eq!(saved(&mut run(&engine, &plans), &resolver), SAME);
    // The function definition now names the doubling callable.
    resolver.inner.insert(
        "/environment.yaml",
        "f.yaml",
        "f.yaml:2",
        Arc::from(doubling().as_bytes()),
    );
    let mut recompiled = run(&engine, &plans);
    assert!(compiled(&recompiled));
    assert_eq!(saved(&mut recompiled, &resolver), DOUBLED);
    assert!(!compiled(&run(&engine, &plans)));
    // The environment file now declares the identity contract inline.
    let inline = format!(
        "schema_version: '1.0'\nfunctions:\n  f:\n{}",
        IDENTITY
            .lines()
            .map(|line| format!("    {line}\n"))
            .collect::<String>()
    );
    resolver.inner.insert(
        "/project.yaml",
        "environment.yaml",
        "environment.yaml:2",
        Arc::from(inline.as_bytes()),
    );
    let mut recompiled = run(&engine, &plans);
    assert!(compiled(&recompiled));
    assert_eq!(saved(&mut recompiled, &resolver), SAME);
}

#[test]
fn a_refused_compilation_keeps_nothing_and_the_next_run_compiles() {
    let resolver = memory_study();
    let installed = Arc::new(Installed::default());
    let engine = engine(Some(resolver.clone()), &installed);
    let plans = Plans::new();
    assert_eq!(saved(&mut run(&engine, &plans), &resolver), SAME);
    resolver.inner.insert(
        "/",
        "spec.yaml",
        "spec.yaml:2",
        Arc::from(&b"schema_version: '1.0'\ndomain: TEST\n"[..]),
    );
    let refused = run(&engine, &plans);
    assert_eq!(plan_of(&refused), "refused");
    assert!(refused.output().is_none());
    assert!(refused.issues().rows() > 0);
    // The original is back: the next run compiles it again rather than
    // reusing the refusal or the plan the refusal replaced.
    resolver.inner.insert(
        "/",
        "spec.yaml",
        "spec.yaml:1",
        Arc::from(SPECIFICATION.as_bytes()),
    );
    let mut fixed = run(&engine, &plans);
    assert!(compiled(&fixed));
    assert_eq!(saved(&mut fixed, &resolver), SAME);
}

#[test]
fn a_kept_plan_runs_with_the_calling_engines_functions() {
    let resolver = memory_study();
    let first = Arc::new(Installed::default());
    let second = Arc::new(Installed::default());
    let plans = Plans::new();
    let compiling = engine(Some(resolver.clone()), &first);
    assert_eq!(saved(&mut run(&compiling, &plans), &resolver), SAME);
    // Another engine over the same resolver reuses the plan, binding through
    // its own functions, as each host call passes its own.
    let calling = engine(Some(resolver.clone()), &second);
    assert!(!compiled(&run(&calling, &plans)));
    assert_eq!((first.binds(), second.binds()), (1, 1));
    // An engine over another resolver compiles anew.
    let other = engine(Some(memory_study()), &second);
    assert!(compiled(&run(&other, &plans)));
}

#[test]
fn a_kept_plan_holds_no_project_functions_of_the_compiling_call() {
    let resolver = memory_study();
    let installed = Arc::new(Installed::default());
    let plans = Plans::new();
    let compiling = engine(Some(resolver.clone()), &installed);
    assert_eq!(saved(&mut run(&compiling, &plans), &resolver), SAME);
    drop(compiling);
    // The holder keeps the plan, but not the functions it was compiled with.
    assert_eq!(Arc::strong_count(&installed), 1);
    let calling = engine(Some(resolver.clone()), &installed);
    let mut again = run(&calling, &plans);
    assert_eq!(plan_of(&again), "reused");
    assert_eq!(saved(&mut again, &resolver), SAME);
}

#[test]
fn a_run_started_from_a_project_function_reuses_the_same_holder() {
    let resolver = memory_study();
    let installed = Arc::new(Installed::default());
    let engine = Arc::new(engine(Some(resolver.clone()), &installed));
    let plans = Arc::new(Plans::new());
    assert!(compiled(&run(&engine, &plans)));
    let nested = Arc::new(Mutex::new(None));
    *installed.nested.lock().unwrap() = Some(Box::new({
        let (engine, plans, nested) = (engine.clone(), plans.clone(), nested.clone());
        move || {
            let inner = run(&engine, &plans);
            *nested.lock().unwrap() = Some(compiled(&inner));
        }
    }));
    let outer = run(&engine, &plans);
    assert_eq!(outer.issues().rows(), 0);
    assert_eq!(*nested.lock().unwrap(), Some(false));
}

/// A local study for project runs: DM, then ADSL reading DM through its
/// `schema` link.
struct Study(crate::support::Scratch);
impl Study {
    /// An empty directory for a test of its own.
    fn empty(name: &str) -> Self {
        Self(crate::support::Scratch::new(&format!("plan-reuse-{name}")))
    }
    fn new(name: &str) -> Self {
        let study = Self::empty(name);
        study.write("raw/dm.csv", "USUBJID,AGE\nS1,30\nS2,41\n");
        study.write("sdtm/dm.yaml", "schema_version: '1.0'\ndomain: DM\nkeys: [USUBJID]\ninput: {RAW: {path: ../raw/dm.csv, types: {AGE: int}}}\ncolumns:\n  - {name: USUBJID, type: str, label: Subject, derivation: RAW.USUBJID}\n  - {name: AGE, type: int, label: Age, derivation: RAW.AGE}\noutput: {path: ../out/dm.csv, columns: [USUBJID, AGE]}\n");
        study.write("adam/adsl.yaml", "schema_version: '1.0'\ndomain: ADSL\nkeys: [USUBJID]\ninput: {DM: {path: ../out/dm.csv, schema: ../sdtm/dm.yaml}}\ncolumns:\n  - {name: USUBJID, type: str, label: Subject, derivation: DM.USUBJID}\n  - {name: AGE, type: int, label: Age, derivation: DM.AGE}\noutput: {path: ../out/adsl.csv, columns: [USUBJID, AGE]}\n");
        study.write(
            "project.yaml",
            "schema_version: '1.0'\nsdtm:\n  specs: [sdtm/dm.yaml]\nadam:\n  specs: [adam/adsl.yaml]\n",
        );
        study
    }
    fn write(&self, name: &str, text: &str) {
        let path = self.0.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    fn path(&self, name: &str) -> String {
        let path = self.0.join(name).to_str().unwrap().to_owned();
        #[cfg(windows)]
        let path = path.replace('\\', "/");
        path
    }
}

/// Run the whole project; return each specification's output CSV and whether
/// its run compiled.
fn project_run(engine: &Engine, study: &Study, plans: &Plans) -> Vec<(String, String, bool)> {
    let runs = engine
        .project_run_reusing(
            &study.path("project.yaml"),
            None,
            plans,
            &CancellationToken::new(),
        )
        .unwrap();
    assert_eq!(runs.issues().rows(), 0);
    runs.names()
        .into_iter()
        .map(|name| {
            let run = runs.get(&name).unwrap();
            let output = run.output().unwrap();
            (name, format!("{} rows", output.rows()), compiled(run))
        })
        .collect()
}

#[test]
fn project_runs_reuse_each_specifications_plan_and_compile_only_what_changed() {
    let study = Study::new("project");
    let installed = Arc::new(Installed::default());
    let engine = engine(None, &installed);
    let plans = Plans::new();
    let rows = |compiled: [bool; 2]| {
        vec![
            ("DM".to_owned(), "2 rows".to_owned(), compiled[0]),
            ("ADSL".to_owned(), "2 rows".to_owned(), compiled[1]),
        ]
    };
    assert_eq!(project_run(&engine, &study, &plans), rows([true, true]));
    assert_eq!(project_run(&engine, &study, &plans), rows([false, false]));
    // ADSL's own specification changed: only ADSL compiles again.
    study.write(
        "adam/adsl.yaml",
        &std::fs::read_to_string(study.0.join("adam/adsl.yaml"))
            .unwrap()
            .replace("label: Age", "label: Age in Years"),
    );
    assert_eq!(project_run(&engine, &study, &plans), rows([false, true]));
    // DM changed: DM compiles, and so does ADSL, whose plan holds DM's
    // specification through its `schema` link.
    study.write(
        "sdtm/dm.yaml",
        &std::fs::read_to_string(study.0.join("sdtm/dm.yaml"))
            .unwrap()
            .replace("label: Subject", "label: Subject Identifier"),
    );
    assert_eq!(project_run(&engine, &study, &plans), rows([true, true]));
    // The project file changed: every specification compiles again.
    study.write(
        "project.yaml",
        "schema_version: '1.0'\nadam:\n  specs: [adam/adsl.yaml]\nsdtm:\n  specs: [sdtm/dm.yaml]\n",
    );
    assert_eq!(project_run(&engine, &study, &plans), rows([true, true]));
    assert_eq!(project_run(&engine, &study, &plans), rows([false, false]));
}

/// Symbolic links need privileges on Windows, so this test runs on Unix only,
/// as the host suites' link tests do. Its specification is declared inside it,
/// so a platform that skips the test does not compile that text unused.
#[cfg(unix)]
#[test]
fn a_path_through_a_retargeted_link_compiles_the_study_it_now_names() {
    /// A specification without project functions: its output repeats its input.
    const PLAIN: &str = "schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\ninput: {SRC: {path: input.csv, types: {ID: int}}}\noutput: {path: out.csv, columns: [ID]}\ncolumns:\n  - {name: ID, type: int, derivation: SRC.ID}\n";
    let study = Study::empty("link");
    for (name, input) in [("a", "ID\n1\n2\n"), ("b", "ID\n1\n2\n3\n")] {
        study.write(&format!("{name}/spec.yaml"), PLAIN);
        study.write(&format!("{name}/input.csv"), input);
    }
    let link = study.0.join("study");
    std::os::unix::fs::symlink(study.0.join("a"), &link).unwrap();
    let installed = Arc::new(Installed::default());
    let engine = engine(None, &installed);
    let plans = Plans::new();
    let specification = study.path("study/spec.yaml");
    let rows = |plans: &Plans| {
        let run = engine
            .spec_run_reusing(&specification, None, plans, &CancellationToken::new())
            .unwrap();
        (plan_of(&run), run.output().unwrap().rows())
    };
    assert_eq!(rows(&plans), ("compiled", 2));
    assert_eq!(rows(&plans), ("reused", 2));
    // The same path now names study B, whose files are unchanged too.
    std::fs::remove_file(&link).unwrap();
    std::os::unix::fs::symlink(study.0.join("b"), &link).unwrap();
    assert_eq!(rows(&plans), ("compiled", 3));
    assert_eq!(rows(&plans), ("reused", 3));
}
