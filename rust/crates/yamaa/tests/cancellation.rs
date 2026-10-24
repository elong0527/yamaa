#![cfg(feature = "yaml")]
//! Cooperative cancellation (#3260, #3030 D46 and D74).
//!
//! An operation polls its token when it starts, before each member of a
//! project run and after each member it builds, at every checkpoint of a
//! dataset's schedule and before each batch of project-function calls, and
//! a save polls it once, after every file is staged and before the first
//! commit. Whichever poll finds the token cancelled, the call fails as
//! `FailureKind::Cancelled` with no result and no issue, and a cancelled save
//! writes nothing. A project function that raises the host's interrupt
//! cancels the call the same way.
use std::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    Arc, Mutex,
};

use yamaa::{
    Argument, CallError, CancellationToken, Engine, EngineOptions, Failure, FailureKind,
    FunctionError, FunctionHandle, FunctionIdentity, HostLanguage, LockContext, MemoryResolver,
    ProjectFunctions, ResolverFailure, ResolverReply, ResolverRequest, ResourceResolver, Scalar,
    ScalarRef, Signature, SubmissionOptions,
};

const SPEC: &str = "yamaa: \"1.0\"\ntype: spec\ndomain: TEST\nkeys: [ID]\ninput: {SRC: {path: input.csv, key: [ID]}}\noutput: {path: out.csv, columns: [ID, NAME]}\ncolumns:\n  - {name: ID, type: str, label: Identifier, derivation: SRC.ID}\n  - {name: NAME, type: str, label: Name, derivation: SRC.NAME}\n";
const INPUT: &[u8] = b"ID,NAME\n1,alpha\n2,beta\n";
const SAVED: &str = "memory:/out.csv";
const PROJECT: &str = "yamaa: \"1.0\"\ntype: project\nadam: {specs: [spec.yaml, other.yaml]}\n";

/// A second specification like `spec`, with its own input and output.
fn other(spec: &str) -> String {
    spec.replace("domain: TEST", "domain: OTHER")
        .replace("out.csv", "other_out.csv")
        .replace("input.csv", "other.csv")
}

/// Serves files from memory and records every request with the number of
/// polls the probe had answered when it arrived.
struct Recording {
    inner: MemoryResolver,
    polls: Arc<AtomicUsize>,
    requests: Mutex<Vec<(String, String, usize)>>,
}
impl Recording {
    /// The two specifications, their inputs and the project listing them.
    fn new(polls: &Arc<AtomicUsize>, spec: &str, project: &str) -> Arc<Self> {
        let resolver = Arc::new(Self {
            inner: MemoryResolver::new(),
            polls: Arc::clone(polls),
            requests: Mutex::new(Vec::new()),
        });
        resolver.file("spec.yaml", spec.as_bytes());
        resolver.input("spec.yaml", "input.csv");
        resolver.file("other.yaml", other(spec).as_bytes());
        resolver.input("other.yaml", "other.csv");
        resolver.file("project.yaml", project.as_bytes());
        resolver
    }
    /// A file at the root, read as a call's entry, as the project lists it,
    /// and as a project run's member names it.
    fn file(&self, written: &str, bytes: &[u8]) {
        let member = format!("/{written}");
        for (declaring, name) in [("/", written), ("/project.yaml", written), ("/", &member)] {
            self.inner
                .insert(declaring, name, &format!("{written}:1"), Arc::from(bytes));
        }
    }
    /// The input `written` of the specification `spec`.
    fn input(&self, spec: &str, written: &str) {
        self.inner.insert(
            &format!("/{spec}"),
            written,
            &format!("{written}:1"),
            Arc::from(INPUT),
        );
    }
    /// The polls answered before the first request for `written`, if any.
    fn first(&self, written: &str) -> Option<usize> {
        self.requests
            .lock()
            .unwrap()
            .iter()
            .find(|(_, name, _)| name == written)
            .map(|(_, _, polls)| *polls)
    }
    fn operations(&self, operation: &str) -> usize {
        self.requests
            .lock()
            .unwrap()
            .iter()
            .filter(|(name, _, _)| name == operation)
            .count()
    }
}
impl ResourceResolver for Recording {
    fn request(&self, request: ResolverRequest<'_>) -> Result<ResolverReply, ResolverFailure> {
        self.requests.lock().unwrap().push((
            request.operation.name().to_owned(),
            request.written.to_owned(),
            self.polls.load(Ordering::SeqCst),
        ));
        self.inner.request(request)
    }
}

/// A token whose probe counts its polls and answers true from poll `from`
/// on, or never.
fn counting(polls: &Arc<AtomicUsize>, from: Option<usize>) -> CancellationToken {
    let polls = Arc::clone(polls);
    CancellationToken::with_probe(move || {
        let poll = polls.fetch_add(1, Ordering::SeqCst) + 1;
        from.is_some_and(|from| poll >= from)
    })
}

fn engine(resolver: Arc<dyn ResourceResolver>) -> Engine {
    Engine::new(
        EngineOptions::new(HostLanguage::Python, "test".into(), "0".into()).with_resolver(resolver),
    )
}

fn cancelled<T>(result: Result<T, Failure>) -> bool {
    matches!(
        result,
        Err(Failure {
            kind: FailureKind::Cancelled,
            ..
        })
    )
}

/// A token cancelled before a call fails every cancellable operation before
/// it reads anything.
#[test]
fn a_cancelled_token_fails_every_operation_before_it_reads_anything() {
    let polls = Arc::new(AtomicUsize::new(0));
    let resolver = Recording::new(&polls, SPEC, PROJECT);
    let engine = engine(resolver.clone());
    let token = CancellationToken::new();
    token.cancel();
    assert!(cancelled(
        engine.spec("spec.yaml").cancel_with(&token).check()
    ));
    assert!(cancelled(
        engine
            .spec("spec.yaml")
            .cancel_with(&token)
            .check_on("parent.yaml")
    ));
    assert!(cancelled(
        engine.spec("spec.yaml").cancel_with(&token).run()
    ));
    assert!(cancelled(
        engine.project("project.yaml").cancel_with(&token).check()
    ));
    assert!(cancelled(
        engine.project("project.yaml").cancel_with(&token).run()
    ));
    assert!(cancelled(
        engine
            .project("project.yaml")
            .cancel_with(&token)
            .submission(&SubmissionOptions::new("0").with_created("2026-01-01T00:00:00"))
    ));
    assert!(resolver.requests.lock().unwrap().is_empty());
}

/// The request of a specification a project lists polls the project
/// request's token: once that token is cancelled, so is each of its
/// operations.
#[test]
fn a_listed_specifications_request_polls_its_project_requests_token() {
    let polls = Arc::new(AtomicUsize::new(0));
    let engine = engine(Recording::new(&polls, SPEC, PROJECT));
    let token = CancellationToken::new();
    let listed = engine
        .project("project.yaml")
        .cancel_with(&token)
        .spec("TEST")
        .unwrap();
    assert_eq!(listed.run().unwrap().output().unwrap().rows(), 2);
    token.cancel();
    assert!(cancelled(listed.check()));
    assert!(cancelled(listed.check_on("parent.yaml")));
    assert!(cancelled(listed.run()));
}

/// Whichever poll of a run finds its token cancelled, the run fails as
/// cancelled and its probe is asked no more. An uncancelled run
/// polls before its member, at each checkpoint of its dataset and once its
/// member is built.
#[test]
fn a_run_cancelled_at_any_poll_returns_no_result() {
    let polls = Arc::new(AtomicUsize::new(0));
    let engine = engine(Recording::new(&polls, SPEC, PROJECT));
    let run = engine
        .spec("spec.yaml")
        .cancel_with(&counting(&polls, None))
        .run()
        .unwrap();
    assert!(run.diagnostics().is_empty(), "{:?}", run.diagnostics());
    assert_eq!(run.output().unwrap().rows(), 2);
    let total = polls.swap(0, Ordering::SeqCst);
    // The member, the seven checkpoints of the dataset's schedule and the
    // built member.
    assert_eq!(total, 9);
    for from in 1..=total {
        polls.store(0, Ordering::SeqCst);
        let token = counting(&polls, Some(from));
        assert!(
            cancelled(engine.spec("spec.yaml").cancel_with(&token).run()),
            "cancelled at poll {from}"
        );
        assert_eq!(polls.load(Ordering::SeqCst), from, "asked after {from}");
    }
}

/// A project run polls before each member: a token cancelled at the poll
/// before the second member stops the run before that member is read.
#[test]
fn a_project_run_cancelled_between_members_never_reads_the_next() {
    let polls = Arc::new(AtomicUsize::new(0));
    let resolver = Recording::new(&polls, SPEC, PROJECT);
    let runs = engine(resolver.clone())
        .project("project.yaml")
        .cancel_with(&counting(&polls, None))
        .run()
        .unwrap();
    assert_eq!(runs.names(), ["TEST", "OTHER"]);
    assert!(runs.diagnostics().is_empty(), "{:?}", runs.diagnostics());
    // The second member's specification is first read right after the poll
    // before that member.
    let before = resolver.first("other.csv").unwrap();
    assert!(resolver.first("input.csv").unwrap() < before);
    let resolver = Recording::new(&polls, SPEC, PROJECT);
    polls.store(0, Ordering::SeqCst);
    let token = counting(&polls, Some(before));
    assert!(cancelled(
        engine(resolver.clone())
            .project("project.yaml")
            .cancel_with(&token)
            .run()
    ));
    assert!(resolver.first("input.csv").is_some());
    assert_eq!(resolver.first("other.csv"), None);
    assert!(token.is_cancelled());
}

/// A save polls its token once, after every file is staged and before the
/// first commit (D74): a cancelled save removes what it staged, writes
/// nothing and keeps the result, which a later save writes whole.
#[test]
fn a_save_cancelled_after_staging_writes_nothing_and_the_result_saves_later() {
    let polls = Arc::new(AtomicUsize::new(0));
    let resolver = Recording::new(&polls, SPEC, PROJECT);
    let mut run = engine(resolver.clone()).spec("spec.yaml").run().unwrap();
    let token = counting(&polls, Some(1));
    let failure = run.save(&token).unwrap_err();
    assert_eq!(polls.load(Ordering::SeqCst), 1);
    assert!(token.is_cancelled());
    assert_eq!(failure.message, "cannot save TEST: operation cancelled");
    assert_eq!(failure.issues.rows(), 0);
    assert!(failure.diagnostics.is_empty() && failure.paths.is_empty());
    // The one file was staged, then removed; nothing was committed.
    assert_eq!(resolver.operations("stage"), 1);
    assert_eq!(resolver.operations("commit"), 0);
    assert_eq!(resolver.inner.staged_count(), 0);
    assert!(resolver.inner.published(SAVED).is_none());
    assert!(run.diagnostics().is_empty());
    // An uncancelled save is polled once and writes the file.
    polls.store(0, Ordering::SeqCst);
    assert_eq!(run.save(&counting(&polls, None)).unwrap(), [SAVED]);
    assert_eq!(polls.load(Ordering::SeqCst), 1);
    assert_eq!(resolver.inner.published(SAVED).as_deref(), Some(INPUT));
}

/// A project run's save is one save, cancelled whole.
#[test]
fn a_cancelled_project_run_save_writes_no_member() {
    let polls = Arc::new(AtomicUsize::new(0));
    let resolver = Recording::new(&polls, SPEC, PROJECT);
    let mut runs = engine(resolver.clone())
        .project("project.yaml")
        .run()
        .unwrap();
    let token = CancellationToken::new();
    token.cancel();
    let failure = runs.save(&token).unwrap_err();
    assert_eq!(
        failure.message,
        "cannot save project.yaml: operation cancelled"
    );
    assert_eq!(resolver.operations("stage"), 2);
    assert_eq!(resolver.operations("commit"), 0);
    assert_eq!(resolver.inner.staged_count(), 0);
    let saved = runs.save(&CancellationToken::new()).unwrap();
    assert_eq!(saved, [SAVED, "memory:/other_out.csv"]);
}

const FUNCTION_SPEC: &str = "yamaa: \"1.0\"\ntype: spec\ndomain: TEST\nkeys: [ID]\ninput: {SRC: {path: input.csv, key: [ID]}}\noutput: {path: out.csv, columns: [ID, VALUE]}\ncolumns:\n  - {name: ID, type: str, label: Identifier, derivation: SRC.ID}\n  - {name: VALUE, type: str, label: Value, derivation: {function: {name: echo, args: {x: SRC.NAME}}}}\n";
const FUNCTION: &[u8] = b"yamaa: \"1.0\"\ntype: function\nname: echo\ndescription: The value itself.\nparams: [{name: x, type: str}]\nreturns: str\nimplementations: {python: pkg.echo}\ntests:\n  - {id: normal, covers: [normal], args: {x: a}, result: a}\n  - {id: empty, covers: [boundary], args: {x: ''}, result: ''}\n  - {id: missing, covers: ['short-circuit-missing:x'], args: {x: null}, result: null}\n";
const FUNCTION_PROJECT: &str = "yamaa: \"1.0\"\ntype: project\nfunctions: [functions/echo.yaml]\nadam: {specs: [spec.yaml, other.yaml]}\n";

/// Installed functions: `echo` answers its argument. The dataset's rows pass
/// `alpha` and `beta`, which no function test passes; such a call either
/// raises the host's interrupt or cancels the flag the probe reads.
struct Echo {
    interrupt: bool,
    cancel: Arc<AtomicBool>,
    binds: AtomicUsize,
    rows: Mutex<Vec<String>>,
}
impl Echo {
    fn new(interrupt: bool) -> Arc<Self> {
        Arc::new(Self {
            interrupt,
            cancel: Arc::new(AtomicBool::new(false)),
            binds: AtomicUsize::new(0),
            rows: Mutex::new(Vec::new()),
        })
    }
    fn rows(&self) -> Vec<String> {
        self.rows.lock().unwrap().clone()
    }
}
impl ProjectFunctions for Echo {
    fn verify_lock(
        &self,
        _: &LockContext<'_>,
        _: &[FunctionIdentity],
    ) -> Result<(), FunctionError> {
        panic!("no lock is declared")
    }
    fn bind(&self, _: &FunctionIdentity, _: &Signature) -> Result<FunctionHandle, FunctionError> {
        self.binds.fetch_add(1, Ordering::SeqCst);
        Ok(FunctionHandle::new(()))
    }
    fn invoke(&self, _: &FunctionHandle, arguments: &[Argument<'_>]) -> Result<Scalar, CallError> {
        let ScalarRef::Str(value) = arguments[0].value else {
            return Ok(Scalar::Missing);
        };
        if value == "alpha" || value == "beta" {
            self.rows.lock().unwrap().push(value.to_owned());
            if self.interrupt {
                return Err(CallError::Raised(FunctionError::Interrupted {
                    payload: None,
                }));
            }
            self.cancel.store(true, Ordering::SeqCst);
        }
        Ok(Scalar::Str(value.to_owned()))
    }
}

/// A project whose two specifications each call `echo`, with `functions`.
fn function_study(functions: Arc<Echo>) -> (Engine, Arc<Recording>) {
    let polls = Arc::new(AtomicUsize::new(0));
    let resolver = Recording::new(&polls, FUNCTION_SPEC, FUNCTION_PROJECT);
    resolver.inner.insert(
        "/project.yaml",
        "functions/echo.yaml",
        "echo:1",
        Arc::from(FUNCTION),
    );
    let engine = Engine::new(
        EngineOptions::new(HostLanguage::Python, "test".into(), "0".into())
            .with_resolver(resolver.clone())
            .with_functions(functions),
    );
    (engine, resolver)
}

/// A project function that raises the host's interrupt cancels the call
/// (D46): the run fails as cancelled, and a project run builds no
/// later member, so no later member binds or calls project code.
#[test]
fn a_project_function_interrupt_cancels_the_call() {
    let functions = Echo::new(true);
    let (engine, _) = function_study(functions.clone());
    let token = CancellationToken::new();
    assert!(cancelled(
        engine
            .spec("spec.yaml")
            .project("project.yaml")
            .cancel_with(&token)
            .run()
    ));
    assert_eq!(functions.rows(), ["alpha"]);
    let functions = Echo::new(true);
    let (engine, resolver) = function_study(functions.clone());
    assert!(cancelled(
        engine.project("project.yaml").cancel_with(&token).run()
    ));
    assert_eq!(functions.rows(), ["alpha"]);
    assert_eq!(functions.binds.load(Ordering::SeqCst), 1);
    assert_eq!(resolver.first("other.csv"), None);
    // The interrupt cancelled the call, not the caller's token.
    assert!(!token.is_cancelled());
}

/// A call that cancels the token, as a host's probe does once its
/// interpreter has an interrupt pending, stops the run before the next row
/// calls the host.
#[test]
fn a_token_a_call_cancels_stops_the_run_before_the_next_call() {
    let functions = Echo::new(false);
    let (engine, _) = function_study(functions.clone());
    let cancel = Arc::clone(&functions.cancel);
    let token = CancellationToken::with_probe(move || cancel.load(Ordering::SeqCst));
    assert!(cancelled(
        engine
            .spec("spec.yaml")
            .project("project.yaml")
            .cancel_with(&token)
            .run()
    ));
    assert_eq!(functions.rows(), ["alpha"]);
    // Uncancelled, both rows call it.
    let functions = Echo::new(false);
    let (engine, _) = function_study(functions.clone());
    let run = engine
        .spec("spec.yaml")
        .project("project.yaml")
        .run()
        .unwrap();
    assert!(run.diagnostics().is_empty(), "{:?}", run.diagnostics());
    assert_eq!(functions.rows(), ["alpha", "beta"]);
}
