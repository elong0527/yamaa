#![cfg(feature = "yaml")]
//! The cancel-or-fail by phase matrix (#3697, the contract in its comment of
//! 2026-10-23): one test per cell of the before-start, mid-partition and
//! publication rows, on a partitioned engine over a memory resolver. The
//! mid-stream row comes with streaming (step 4).
//!
//! Each cell asserts its end state whole: the outcome and its identity
//! (cancelled, or the first issue's phase, condition and paths), what names
//! the run (the summary's or the save's subject), the targets that hold
//! published bytes, the stage and commit requests the resolver saw, and the
//! staged objects left behind, which are never any.
use std::num::NonZeroUsize;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc, Mutex,
};

use yamaa::{
    Argument, CallError, CancellationToken, Engine, EngineOptions, Error, Failure, FunctionError,
    FunctionHandle, FunctionIdentity, HostLanguage, LockContext, MemoryResolver, Partitions,
    ProjectFunctions, ResolverFailure, ResolverReply, ResolverRequest, ResourceResolver, Run,
    Scalar, ScalarRef, Signature,
};

/// A partition-safe specification: W converts V to an integer, and the run
/// declares its report file.
const SPEC: &str = "yamaa: \"1.0\"\ntype: spec\ndomain: TEST\nkeys: [ID]\ninput: {SRC: {path: input.csv, types: {ID: int, V: float}}}\noutput: {path: out.csv, columns: [ID, W], report: report.csv}\ncolumns:\n  - {name: ID, type: int, label: Identifier, derivation: SRC.ID}\n  - {name: W, type: int, label: Whole, derivation: SRC.V}\n";
/// Three records whose V converts.
const INPUT: &[u8] = b"ID,V\n1,1\n2,2\n3,3\n";
/// Three records; the third partition's V, 3.5, does not convert.
const FAILING: &[u8] = b"ID,V\n1,1\n2,2\n3,3.5\n";
/// A specification whose RN numbers the records: a window, which needs the
/// whole table.
const WINDOW: &str = "yamaa: \"1.0\"\ntype: spec\ndomain: TEST\nkeys: [ID]\ninput: {SRC: {path: input.csv, types: {ID: int, V: float}}}\noutput: {path: out.csv, columns: [ID, RN], report: report.csv}\ncolumns:\n  - {name: ID, type: int, label: Identifier, derivation: SRC.ID}\n  - {name: RN, type: int, label: Number, derivation: {row_number: {window: {order_by: [ID]}}}}\n";
const OUTPUT: &str = "memory:/out.csv";
const REPORT: &str = "memory:/report.csv";

/// Serves the specification and its input from memory, records every stage
/// and commit, and fails one operation on one written path when asked.
struct Recording {
    inner: MemoryResolver,
    requests: Mutex<Vec<(String, String)>>,
    refuse: Option<(&'static str, &'static str)>,
}
impl Recording {
    fn new(spec: &str, input: &[u8], refuse: Option<(&'static str, &'static str)>) -> Arc<Self> {
        let resolver = Arc::new(Self {
            inner: MemoryResolver::new(),
            requests: Mutex::new(Vec::new()),
            refuse,
        });
        resolver
            .inner
            .insert("/", "spec.yaml", "spec:1", Arc::from(spec.as_bytes()));
        resolver
            .inner
            .insert("/spec.yaml", "input.csv", "input:1", Arc::from(input));
        resolver
    }
    fn count(&self, operation: &str) -> usize {
        self.requests
            .lock()
            .unwrap()
            .iter()
            .filter(|(name, _)| name == operation)
            .count()
    }
}
impl ResourceResolver for Recording {
    fn request(&self, request: ResolverRequest<'_>) -> Result<ResolverReply, ResolverFailure> {
        let operation = request.operation.name();
        self.requests
            .lock()
            .unwrap()
            .push((operation.to_owned(), request.written.to_owned()));
        if self.refuse == Some((operation, request.written)) {
            return Err(ResolverFailure::from_code("publication"));
        }
        self.inner.request(request)
    }
}

/// An engine that runs every build one record per partition.
fn engine(resolver: Arc<Recording>, fallback: bool) -> Engine {
    let partitions = Partitions::new(NonZeroUsize::MIN);
    Engine::new(
        EngineOptions::new(HostLanguage::Python, "test".into(), "0".into())
            .with_resolver(resolver)
            .with_partitions(if fallback {
                partitions.with_whole_table_fallback()
            } else {
                partitions
            }),
    )
}

/// The end state of one cell.
#[derive(Debug, PartialEq)]
struct EndState {
    /// `cancelled`, or the first issue's phase, condition and spec paths.
    outcome: String,
    /// What names the run: the subject of a summary or a save's failure
    /// text, through its first colon; a cancelled call has none.
    named: String,
    /// The targets that hold published bytes.
    published: Vec<&'static str>,
    /// The stage and commit requests the resolver saw.
    staged: usize,
    committed: usize,
    /// The staged objects left behind.
    left: usize,
}

fn subject(text: &str) -> String {
    text.split_inclusive(':').next().unwrap_or("").to_owned()
}

fn first_issue(diagnostics: &[yamaa::Diagnostic]) -> String {
    let first = diagnostics.first().expect("an issue");
    format!("{} {} {:?}", first.phase, first.code, first.spec_path)
}

fn end_state(resolver: &Recording, outcome: String, named: String) -> EndState {
    EndState {
        outcome,
        named,
        published: [OUTPUT, REPORT]
            .into_iter()
            .filter(|target| resolver.inner.published(target).is_some())
            .collect(),
        staged: resolver.count("stage"),
        committed: resolver.count("commit"),
        left: resolver.inner.staged_count(),
    }
}

/// The end state of a failed run and of its save.
fn failed_and_saved(resolver: &Recording, mut run: Run) -> (EndState, EndState) {
    let built = end_state(
        resolver,
        first_issue(&run.diagnostics()),
        subject(&run.summary().to_string()),
    );
    let failure: Failure = run.save(&CancellationToken::new()).unwrap_err();
    assert_eq!(failure.paths, [REPORT], "the files the failed save wrote");
    let saved = end_state(
        resolver,
        first_issue(&failure.diagnostics),
        subject(&failure.message),
    );
    (built, saved)
}

/// Before start, cancelled: a token cancelled before the call, or found at
/// the run's first poll, ends the call with no result and no issue, and
/// nothing is staged or published.
#[test]
fn before_start_a_cancelled_run_returns_no_result_and_publishes_nothing() {
    for (label, cancel_before) in [("before the call", true), ("at the first poll", false)] {
        let resolver = Recording::new(SPEC, INPUT, None);
        let polls = Arc::new(AtomicUsize::new(0));
        let counted = Arc::clone(&polls);
        let token = CancellationToken::with_probe(move || {
            counted.fetch_add(1, Ordering::SeqCst);
            true
        });
        if cancel_before {
            token.cancel();
        }
        let result = engine(Arc::clone(&resolver), false).spec_run("spec.yaml", None, &token);
        assert!(matches!(result, Err(Error::Cancelled)), "{label}");
        assert_eq!(
            end_state(&resolver, "cancelled".into(), String::new()),
            EndState {
                outcome: "cancelled".into(),
                named: String::new(),
                published: vec![],
                staged: 0,
                committed: 0,
                left: 0,
            },
            "{label}"
        );
        assert_eq!(
            polls.load(Ordering::SeqCst),
            usize::from(!cancel_before),
            "probe polls {label}"
        );
    }
}

/// Before start, failed: a step that needs the whole table refuses the
/// partitioned run before derivation, as `engine_rejected` at the window
/// node, `columns.RN.derivation.row_number`, as the negative window
/// benchmarks name it. The failed run has no output, and its save writes
/// only the report file and fails with the same issue; both name the run.
#[test]
fn before_start_a_refused_partitioned_run_publishes_only_its_report() {
    let resolver = Recording::new(WINDOW, INPUT, None);
    let run = engine(Arc::clone(&resolver), false)
        .spec_run("spec.yaml", None, &CancellationToken::new())
        .unwrap();
    assert!(run.output().is_none());
    let issue = &run.diagnostics()[0];
    assert_eq!(
        (issue.context.to_json(), run.diagnostics().len()),
        (r#"{"code":"partition_global","step":"execute"}"#, 1),
        "the refusal's context"
    );
    let outcome = "engine engine_rejected [\"columns.RN.derivation.row_number\"]".to_owned();
    assert_eq!(
        failed_and_saved(&resolver, run),
        (
            EndState {
                outcome: outcome.clone(),
                named: "yamaa run of TEST:".into(),
                published: vec![],
                staged: 0,
                committed: 0,
                left: 0,
            },
            EndState {
                outcome,
                named: "cannot save TEST:".into(),
                published: vec![REPORT],
                staged: 1,
                committed: 1,
                left: 0,
            },
        )
    );
}

/// Mid-partition, cancelled: whichever poll of a partitioned run finds its
/// token cancelled, at a checkpoint or between two partitions, the call
/// returns no result and no issue, and nothing is staged or published. An
/// uncancelled partitioned run polls more often than a whole-table run: once
/// before each partition of a divided unit after its first.
#[test]
fn mid_partition_a_cancelled_run_returns_no_result_and_publishes_nothing() {
    let polls = Arc::new(AtomicUsize::new(0));
    let counting = |from: Option<usize>| {
        let polls = Arc::clone(&polls);
        CancellationToken::with_probe(move || {
            let poll = polls.fetch_add(1, Ordering::SeqCst) + 1;
            from.is_some_and(|from| poll >= from)
        })
    };
    let whole = Engine::new(
        EngineOptions::new(HostLanguage::Python, "test".into(), "0".into())
            .with_resolver(Recording::new(SPEC, INPUT, None)),
    );
    whole.spec_run("spec.yaml", None, &counting(None)).unwrap();
    let whole_polls = polls.swap(0, Ordering::SeqCst);
    engine(Recording::new(SPEC, INPUT, None), false)
        .spec_run("spec.yaml", None, &counting(None))
        .unwrap();
    let total = polls.swap(0, Ordering::SeqCst);
    // Without rows the specification builds the key grain: its key
    // derivation and its column W each divide into three partitions, with
    // two polls between them.
    assert_eq!(total, whole_polls + 4, "polls between partitions");
    for from in 1..=total {
        let resolver = Recording::new(SPEC, INPUT, None);
        polls.store(0, Ordering::SeqCst);
        let result =
            engine(Arc::clone(&resolver), false).spec_run("spec.yaml", None, &counting(Some(from)));
        assert!(matches!(result, Err(Error::Cancelled)), "poll {from}");
        assert_eq!(
            end_state(&resolver, "cancelled".into(), String::new()),
            EndState {
                outcome: "cancelled".into(),
                named: String::new(),
                published: vec![],
                staged: 0,
                committed: 0,
                left: 0,
            },
            "cancelled at poll {from}"
        );
    }
}

/// A specification whose VALUE calls the project function `echo`.
const FUNCTION_SPEC: &str = "yamaa: \"1.0\"\ntype: spec\ndomain: TEST\nkeys: [ID]\ninput: {SRC: {path: input.csv, key: [ID]}}\noutput: {path: out.csv, columns: [ID, VALUE]}\ncolumns:\n  - {name: ID, type: str, label: Identifier, derivation: SRC.ID}\n  - {name: VALUE, type: str, label: Value, derivation: {function: {name: echo, args: {x: SRC.NAME}}}}\n";
const FUNCTION: &[u8] = b"yamaa: \"1.0\"\ntype: function\nname: echo\ndescription: The value itself.\nparams: [{name: x, type: str}]\nreturns: str\nimplementations: {python: pkg.echo}\ntests:\n  - {id: normal, covers: [normal], args: {x: a}, result: a}\n  - {id: empty, covers: [boundary], args: {x: ''}, result: ''}\n  - {id: missing, covers: ['short-circuit-missing:x'], args: {x: null}, result: null}\n";
const FUNCTION_PROJECT: &str =
    "yamaa: \"1.0\"\ntype: project\nfunctions: [functions/echo.yaml]\nadam: {specs: [spec.yaml]}\n";

/// `echo` answers its tests' arguments and raises the host's interrupt on
/// the first data row, `alpha`; it counts the data rows it is called with.
struct Interrupting {
    calls: AtomicUsize,
}
impl ProjectFunctions for Interrupting {
    fn verify_lock(
        &self,
        _: &LockContext<'_>,
        _: &[FunctionIdentity],
    ) -> Result<(), FunctionError> {
        panic!("no lock is declared")
    }
    fn bind(&self, _: &FunctionIdentity, _: &Signature) -> Result<FunctionHandle, FunctionError> {
        Ok(FunctionHandle::new(()))
    }
    fn invoke(&self, _: &FunctionHandle, arguments: &[Argument<'_>]) -> Result<Scalar, CallError> {
        match arguments[0].value {
            ScalarRef::Str(value) if value == "alpha" || value == "beta" => {
                self.calls.fetch_add(1, Ordering::SeqCst);
                Err(CallError::Raised(FunctionError::Interrupted {
                    payload: None,
                }))
            }
            ScalarRef::Str(value) => Ok(Scalar::Str(value.to_owned())),
            _ => Ok(Scalar::Missing),
        }
    }
}

/// Mid-partition, cancelled by a callback: a project function needs the
/// whole table, so it runs only under the fallback, and its interrupt on
/// the first data row cancels the call there (D46): no result and no issue,
/// no later row is called, and nothing is staged or published.
#[test]
fn mid_partition_a_project_function_interrupt_cancels_the_call() {
    let resolver = Recording::new(FUNCTION_SPEC, b"ID,NAME\n1,alpha\n2,beta\n", None);
    for (name, bytes) in [
        ("project.yaml", FUNCTION_PROJECT.as_bytes()),
        ("functions/echo.yaml", FUNCTION),
    ] {
        resolver
            .inner
            .insert("/", name, &format!("{name}:1"), Arc::from(bytes));
        resolver.inner.insert(
            "/project.yaml",
            name,
            &format!("{name}:1"),
            Arc::from(bytes),
        );
    }
    resolver.inner.insert(
        "/project.yaml",
        "spec.yaml",
        "spec:1",
        Arc::from(FUNCTION_SPEC.as_bytes()),
    );
    let functions = Arc::new(Interrupting {
        calls: AtomicUsize::new(0),
    });
    let engine = Engine::new(
        EngineOptions::new(HostLanguage::Python, "test".into(), "0".into())
            .with_resolver(resolver.clone())
            .with_functions(functions.clone())
            .with_partitions(Partitions::new(NonZeroUsize::MIN).with_whole_table_fallback()),
    );
    let token = CancellationToken::new();
    let result = engine.spec_run("spec.yaml", Some("project.yaml"), &token);
    assert!(matches!(result, Err(Error::Cancelled)));
    assert_eq!(
        functions.calls.load(Ordering::SeqCst),
        1,
        "data rows called"
    );
    assert_eq!(
        end_state(&resolver, "cancelled".into(), String::new()),
        EndState {
            outcome: "cancelled".into(),
            named: String::new(),
            published: vec![],
            staged: 0,
            committed: 0,
            left: 0,
        }
    );
    // The interrupt cancelled the call, not the caller's token.
    assert!(!token.is_cancelled());
}

/// Mid-partition, failed: the third partition's V does not convert, so the
/// run fails as the whole-table run does, with `conversion_failed` at W in
/// the `convert` phase, and no output. Its save writes only the report file.
#[test]
fn mid_partition_a_failed_run_publishes_only_its_report() {
    let resolver = Recording::new(SPEC, FAILING, None);
    let run = engine(Arc::clone(&resolver), false)
        .spec_run("spec.yaml", None, &CancellationToken::new())
        .unwrap();
    assert!(run.output().is_none());
    let outcome = "convert conversion_failed [\"columns.W\"]".to_owned();
    assert_eq!(
        failed_and_saved(&resolver, run),
        (
            EndState {
                outcome: outcome.clone(),
                named: "yamaa run of TEST:".into(),
                published: vec![],
                staged: 0,
                committed: 0,
                left: 0,
            },
            EndState {
                outcome,
                named: "cannot save TEST:".into(),
                published: vec![REPORT],
                staged: 1,
                committed: 1,
                left: 0,
            },
        )
    );
}

/// During publication, cancelled: the save's one poll comes after both files
/// are staged and before the first commit (D74). Both staged files are
/// removed, nothing is committed, and the save fails with D39's text; the
/// result saves whole later.
#[test]
fn during_publication_a_cancelled_save_removes_what_it_staged() {
    let resolver = Recording::new(SPEC, INPUT, None);
    let mut run = engine(Arc::clone(&resolver), false)
        .spec_run("spec.yaml", None, &CancellationToken::new())
        .unwrap();
    let token = CancellationToken::new();
    token.cancel();
    let failure = run.save(&token).unwrap_err();
    assert_eq!(
        (
            failure.message.as_str(),
            failure.diagnostics.len(),
            failure.paths.len()
        ),
        ("cannot save TEST: operation cancelled", 0, 0),
        "the cancelled save"
    );
    assert_eq!(
        end_state(&resolver, "cancelled".into(), subject(&failure.message)),
        EndState {
            outcome: "cancelled".into(),
            named: "cannot save TEST:".into(),
            published: vec![],
            staged: 2,
            committed: 0,
            left: 0,
        }
    );
    assert_eq!(
        run.save(&CancellationToken::new()).unwrap(),
        [REPORT, OUTPUT],
        "the later save"
    );
}

/// During publication, failed before the first commit: the output cannot be
/// staged, so the staged report is removed and no target is touched; the
/// save fails at the output's path with no files written.
#[test]
fn during_publication_a_file_that_cannot_be_staged_publishes_nothing() {
    let resolver = Recording::new(SPEC, INPUT, Some(("stage", "out.csv")));
    let mut run = engine(Arc::clone(&resolver), false)
        .spec_run("spec.yaml", None, &CancellationToken::new())
        .unwrap();
    let failure = run.save(&CancellationToken::new()).unwrap_err();
    assert!(failure.paths.is_empty(), "{:?}", failure.paths);
    assert_eq!(
        end_state(
            &resolver,
            first_issue(&failure.diagnostics),
            subject(&failure.message)
        ),
        EndState {
            outcome: "engine engine_rejected [\"output.path\"]".into(),
            named: "cannot save TEST:".into(),
            published: vec![],
            staged: 2,
            committed: 0,
            left: 0,
        }
    );
}

/// During publication, failed while committing: the report, committed
/// first, stays replaced whole; the output keeps no bytes, its staged file
/// is removed, and the save fails at the output's path, listing the report
/// as written (PROJECT-12).
#[test]
fn during_publication_a_failed_commit_keeps_only_the_files_before_it() {
    let resolver = Recording::new(SPEC, INPUT, Some(("commit", "out.csv")));
    let mut run = engine(Arc::clone(&resolver), false)
        .spec_run("spec.yaml", None, &CancellationToken::new())
        .unwrap();
    let failure = run.save(&CancellationToken::new()).unwrap_err();
    assert_eq!(failure.paths, [REPORT], "the files written before the stop");
    assert_eq!(
        end_state(
            &resolver,
            first_issue(&failure.diagnostics),
            subject(&failure.message)
        ),
        EndState {
            outcome: "engine engine_rejected [\"output.path\"]".into(),
            named: "cannot save TEST:".into(),
            published: vec![REPORT],
            staged: 2,
            committed: 2,
            left: 0,
        }
    );
}
