#![cfg(feature = "yaml")]
//! The cancel-or-fail by phase matrix (#3697, the contract in its comment of
//! 2026-10-23): one test per cell of the before-start, mid-partition,
//! mid-stream and publication rows, on a partitioned engine over a memory
//! resolver. A save writes a partitioned run's output one partition at a
//! time (#3793), so its publication row covers each partition. The mid-stream cells read a streamed run's output one record per
//! batch (`SpecRequest::stream`). The before-start and mid-partition cells
//! hold for key-bounded partitions too (#3763), each over a specification
//! that the strategy divides by its subjects or refuses.
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

use yamaa::host::functions::{
    Argument, CallError, FunctionError, FunctionHandle, FunctionIdentity, LockContext, Signature,
};
use yamaa::host::resolver::{ResolverFailure, ResolverReply, ResolverRequest};
use yamaa::scalar::{Scalar, ScalarRef};
use yamaa::{
    CancellationToken, Engine, EngineOptions, Failure, FailureKind, HostLanguage, MemoryResolver,
    Partitions, ProjectFunctions, ResourceResolver, Run,
};

use crate::support::issues;

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
/// A specification keyed by subject and sequence whose RN numbers each
/// subject's records: a window grouped by the leading key, ID.
const SUBJECTS: &str = "yamaa: \"1.0\"\ntype: spec\ndomain: TEST\nkeys: [ID, SEQ]\ninput: {SRC: {path: input.csv, types: {SEQ: int, V: float}}}\noutput: {path: out.csv, columns: [ID, SEQ, RN, W], report: report.csv}\ncolumns:\n  - {name: ID, type: str, label: Subject, derivation: SRC.ID}\n  - {name: SEQ, type: int, label: Sequence, derivation: SRC.SEQ}\n  - {name: RN, type: int, label: Number, derivation: {row_number: {window: {group_by: [ID], order_by: [SEQ]}}}}\n  - {name: W, type: int, label: Whole, derivation: SRC.V}\n";
/// Three subjects' records, grouped by subject, whose V converts.
const VISITS: &[u8] = b"ID,SEQ,V\na,1,1\na,2,2\nb,1,3\nc,1,4\n";
/// The same records; the third subject's V, 4.5, does not convert.
const FAILING_VISITS: &[u8] = b"ID,SEQ,V\na,1,1\na,2,2\nb,1,3\nc,1,4.5\n";
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
    /// The requests of `operation` for the path written as `written`.
    fn requested(&self, operation: &str, written: &str) -> usize {
        self.requests
            .lock()
            .unwrap()
            .iter()
            .filter(|(name, path)| name == operation && path == written)
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
    partitioned(
        resolver,
        if fallback {
            partitions.with_whole_table_fallback()
        } else {
            partitions
        },
    )
}

/// An engine that runs every build over `partitions`.
fn partitioned(resolver: Arc<Recording>, partitions: Partitions) -> Engine {
    Engine::new(
        EngineOptions::new(HostLanguage::Python, "test".into(), "0".into())
            .with_resolver(resolver)
            .with_partitions(partitions),
    )
}

/// Consecutive partitions of one record, and key-bounded partitions of one
/// subject each, over the leading declared key (#3763).
fn strategies() -> [(&'static str, Partitions); 2] {
    [
        ("consecutive partitions", Partitions::new(NonZeroUsize::MIN)),
        (
            "key-bounded partitions",
            Partitions::by_keys(NonZeroUsize::MIN, NonZeroUsize::MIN),
        ),
    ]
}

/// The end state of one cell.
#[derive(Debug, PartialEq)]
struct EndState {
    /// `cancelled`, or every issue whole.
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
fn failed_and_saved(resolver: &Recording, run: &mut Run) -> (EndState, EndState) {
    let built = end_state(
        resolver,
        issues(&run.diagnostics()),
        subject(&run.summary().to_string()),
    );
    let failure: Failure = run.save(&CancellationToken::new()).unwrap_err();
    assert_eq!(failure.paths, [REPORT], "the files the failed save wrote");
    let saved = end_state(
        resolver,
        issues(&failure.diagnostics),
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
        let result = engine(Arc::clone(&resolver), false)
            .spec("spec.yaml")
            .cancel_with(&token)
            .run();
        assert!(
            matches!(
                result,
                Err(Failure {
                    kind: FailureKind::Cancelled,
                    ..
                })
            ),
            "{label}"
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
/// benchmarks name it. Under key-bounded partitions the window, which
/// groups by nothing, still omits the partition key ID, so the refusal and
/// its identity are the same (#3763). The failed run has no output, and its
/// save writes only the report file and fails with the same issue; both name
/// the run.
#[test]
fn before_start_a_refused_partitioned_run_publishes_only_its_report() {
    for (label, partitions) in strategies() {
        let resolver = Recording::new(WINDOW, INPUT, None);
        let mut run = partitioned(Arc::clone(&resolver), partitions)
            .spec("spec.yaml")
            .run()
            .unwrap();
        assert!(run.output().is_none(), "{label}");
        let outcome = r#"Error engine engine_rejected None ["columns.RN.derivation.row_number"] {"code":"partition_global","step":"execute"} None None"#.to_owned();
        assert_eq!(
            failed_and_saved(&resolver, &mut run),
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
            ),
            "{label}"
        );
    }
}

/// Mid-partition, cancelled: whichever poll of a partitioned run finds its
/// token cancelled, at a checkpoint or between two partitions, the call
/// returns no result and no issue, and nothing is staged or published. An
/// uncancelled partitioned run on one thread polls more often than a
/// whole-table run: each partition's run polls at every checkpoint of its
/// derivation (#3830). On more threads the calling thread polls instead,
/// as often as it waits (#4193), which `yamaa-core` proves at each poll.
/// Both specifications have no rows, so each builds the key grain. Over
/// three one-record partitions, SPEC's four derivation checkpoints, its
/// intermediates', its first key's, its key grain's and its column W's,
/// are polled twice more. Over key-bounded partitions of one subject each
/// (#3763), SUBJECTS' five, its window RN's included, are polled twice more.
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
    let [consecutive, key_bounded] = strategies();
    for ((label, partitions), spec, input, between) in [
        (consecutive, SPEC, INPUT, 8),
        (key_bounded, SUBJECTS, VISITS, 10),
    ] {
        let partitions = partitions.with_threads(NonZeroUsize::MIN);
        polls.store(0, Ordering::SeqCst);
        let whole = Engine::new(
            EngineOptions::new(HostLanguage::Python, "test".into(), "0".into())
                .with_resolver(Recording::new(spec, input, None)),
        );
        whole
            .spec("spec.yaml")
            .cancel_with(&counting(None))
            .run()
            .unwrap();
        let whole_polls = polls.swap(0, Ordering::SeqCst);
        partitioned(Recording::new(spec, input, None), partitions)
            .spec("spec.yaml")
            .cancel_with(&counting(None))
            .run()
            .unwrap();
        let total = polls.swap(0, Ordering::SeqCst);
        assert_eq!(
            total,
            whole_polls + between,
            "{label}: polls between partitions"
        );
        for from in 1..=total {
            let resolver = Recording::new(spec, input, None);
            polls.store(0, Ordering::SeqCst);
            let result = partitioned(Arc::clone(&resolver), partitions)
                .spec("spec.yaml")
                .cancel_with(&counting(Some(from)))
                .run();
            assert!(
                matches!(
                    result,
                    Err(Failure {
                        kind: FailureKind::Cancelled,
                        ..
                    })
                ),
                "{label}: poll {from}"
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
                },
                "{label}: cancelled at poll {from}"
            );
        }
    }
}

/// A specification whose VALUE calls the project function `echo`.
const FUNCTION_SPEC: &str = "yamaa: \"1.0\"\ntype: spec\ndomain: TEST\nkeys: [ID]\ninput: {SRC: {path: input.csv, key: [ID]}}\noutput: {path: out.csv, columns: [ID, VALUE]}\ncolumns:\n  - {name: ID, type: str, label: Identifier, derivation: SRC.ID}\n  - {name: VALUE, type: str, label: Value, derivation: {function: {name: echo, args: {x: SRC.NAME}}}}\n";
/// A specification whose rows ECHOED drives: an intermediate whose
/// derivation ECHO calls the project function `echo`.
const DRIVER_SPEC: &str = "yamaa: \"1.0\"\ntype: spec\ndomain: TEST\nkeys: [ID]\ninput: {SRC: {path: input.csv, key: [ID]}}\noutput: {path: out.csv, columns: [ID, VALUE]}\nintermediates:\n  - id: ECHOED\n    dataset: SRC\n    derivations:\n      ECHO: {function: {name: echo, args: {x: SRC.NAME}}}\ncolumns:\n  - {name: ID, type: str, label: Identifier}\n  - {name: VALUE, type: str, label: Value}\nrows:\n  - id: echoed\n    dataset: ECHOED\n    derivations:\n      ID: ECHOED.ID\n      VALUE: ECHOED.ECHO\n";
const FUNCTION: &[u8] = b"yamaa: \"1.0\"\ntype: function\nname: echo\ndescription: The value itself.\nparams: [{name: x, type: str}]\nreturns: str\nimplementations: {python: pkg.echo}\ntests:\n  - {id: normal, covers: [normal], args: {x: a}, result: a}\n  - {id: empty, covers: [boundary], args: {x: ''}, result: ''}\n  - {id: missing, covers: ['short-circuit-missing:x'], args: {x: null}, result: null}\n";
const FUNCTION_PROJECT: &str =
    "yamaa: \"1.0\"\ntype: project\nfunctions: [functions/echo.yaml]\nadam: {specs: [spec.yaml]}\n";
/// The data rows' names, which no function test passes.
const NAMES: &[u8] = b"ID,NAME\n1,alpha\n2,beta\n";

/// `echo` answers its argument; on a data row, `alpha` or `beta`, it counts
/// the call and, when asked to, raises the host's interrupt instead.
struct Echo {
    interrupt: bool,
    calls: AtomicUsize,
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
        Ok(FunctionHandle::new(()))
    }
    fn invoke(&self, _: &FunctionHandle, arguments: &[Argument<'_>]) -> Result<Scalar, CallError> {
        let ScalarRef::Str(value) = arguments[0].value else {
            return Ok(Scalar::Missing);
        };
        if value == "alpha" || value == "beta" {
            self.calls.fetch_add(1, Ordering::SeqCst);
            if self.interrupt {
                return Err(CallError::Raised(FunctionError::Interrupted {
                    payload: None,
                }));
            }
        }
        Ok(Scalar::Str(value.to_owned()))
    }
}

/// A project listing `spec` and the function file of `echo`, served from
/// memory, and a partitioned engine with `functions` over it.
fn function_study(
    spec: &str,
    functions: Arc<Echo>,
    partitions: Partitions,
) -> (Engine, Arc<Recording>) {
    let resolver = Recording::new(spec, NAMES, None);
    for (name, bytes) in [
        ("project.yaml", FUNCTION_PROJECT.as_bytes()),
        ("functions/echo.yaml", FUNCTION),
        ("spec.yaml", spec.as_bytes()),
    ] {
        for declaring in ["/", "/project.yaml"] {
            resolver
                .inner
                .insert(declaring, name, &format!("{name}:1"), Arc::from(bytes));
        }
    }
    let engine = Engine::new(
        EngineOptions::new(HostLanguage::Python, "test".into(), "0".into())
            .with_resolver(resolver.clone())
            .with_functions(functions)
            .with_partitions(partitions),
    );
    (engine, resolver)
}

/// Before start, failed, at a driver: a template driven by an intermediate
/// whose derivation calls a project function needs the whole table, as the
/// call does. Without the fallback the partitioned run is refused at the
/// intermediate before any call; with it, the donor calls `echo` once per
/// record and the run builds its output.
#[test]
fn before_start_a_driver_whose_donor_calls_a_function_is_refused_before_any_call() {
    let one = Partitions::new(NonZeroUsize::MIN);
    let refused = r#"Error engine engine_rejected None ["intermediates.ECHOED"] {"code":"partition_global","step":"execute"} None None"#;
    for (partitions, outcome, calls, rows) in [
        (one, refused.to_owned(), 0, None),
        (
            one.with_whole_table_fallback(),
            "succeeded".to_owned(),
            2,
            Some(2),
        ),
    ] {
        let functions = Arc::new(Echo {
            interrupt: false,
            calls: AtomicUsize::new(0),
        });
        let (engine, resolver) = function_study(DRIVER_SPEC, functions.clone(), partitions);
        let run = engine
            .spec("spec.yaml")
            .project("project.yaml")
            .run()
            .unwrap();
        let built = if run.diagnostics().is_empty() {
            "succeeded".to_owned()
        } else {
            issues(&run.diagnostics())
        };
        assert_eq!(
            (
                built,
                functions.calls.load(Ordering::SeqCst),
                run.output().map(|output| output.rows()),
                resolver.count("stage"),
            ),
            (outcome, calls, rows, 0),
            "{partitions:?}"
        );
    }
}

/// Mid-partition, cancelled by a callback: a project function needs the
/// whole table, so it runs only under the fallback, and its interrupt on
/// the first data row cancels the call there (D46): no result and no issue,
/// no later row is called, and nothing is staged or published.
#[test]
fn mid_partition_a_project_function_interrupt_cancels_the_call() {
    let functions = Arc::new(Echo {
        interrupt: true,
        calls: AtomicUsize::new(0),
    });
    let (engine, resolver) = function_study(
        FUNCTION_SPEC,
        functions.clone(),
        Partitions::new(NonZeroUsize::MIN).with_whole_table_fallback(),
    );
    let token = CancellationToken::new();
    let result = engine
        .spec("spec.yaml")
        .project("project.yaml")
        .cancel_with(&token)
        .run();
    assert!(matches!(
        result,
        Err(Failure {
            kind: FailureKind::Cancelled,
            ..
        })
    ));
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
/// the `convert` phase, and no output: under consecutive partitions at the
/// third record's 3.5, and under key-bounded partitions (#3763) at the
/// third subject's 4.5. Its save writes only the report file.
#[test]
fn mid_partition_a_failed_run_publishes_only_its_report() {
    let [consecutive, key_bounded] = strategies();
    for ((label, partitions), spec, input, context) in [
        (
            consecutive,
            SPEC,
            FAILING,
            r#"{"from":"float","keys":[{"ID":3}],"to":"int","value":3.5}"#,
        ),
        (
            key_bounded,
            SUBJECTS,
            FAILING_VISITS,
            r#"{"from":"float","keys":[{"ID":"c","SEQ":1}],"to":"int","value":4.5}"#,
        ),
    ] {
        let resolver = Recording::new(spec, input, None);
        let mut run = partitioned(Arc::clone(&resolver), partitions)
            .spec("spec.yaml")
            .run()
            .unwrap();
        assert!(run.output().is_none(), "{label}");
        let outcome = format!(
            r#"Error convert conversion_failed Some("VALUE-7") ["columns.W"] {context} None None"#
        );
        assert_eq!(
            failed_and_saved(&resolver, &mut run),
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
            ),
            "{label}"
        );
    }
}

/// A streamed run of `SPEC` over `input` on a one-record partitioned engine,
/// whose output is read one record per batch.
fn streamed(resolver: &Arc<Recording>, token: &CancellationToken) -> yamaa::Streamed {
    engine(Arc::clone(resolver), false)
        .spec("spec.yaml")
        .cancel_with(token)
        .stream()
        .unwrap()
}

/// The end state of a cancelled call.
const CANCELLED: fn() -> EndState = || EndState {
    outcome: "cancelled".into(),
    named: String::new(),
    published: vec![],
    staged: 0,
    committed: 0,
    left: 0,
};

/// Mid-stream, cancelled: a token found cancelled before the first pull or
/// between two batches, or the reader released after its first batch, ends
/// production. The pull fails with no issue, no further batch is built, and
/// the run has no result, so nothing is staged or published. A token
/// cancelled before the first pull ends the run before it reads its input.
#[test]
fn mid_stream_a_cancelled_run_builds_nothing_more_and_publishes_nothing() {
    for (label, batches_before, released, input_read) in [
        ("before the first pull", 0, false, 0),
        ("between batches", 1, false, 1),
        ("by releasing the reader", 1, true, 1),
    ] {
        let resolver = Recording::new(SPEC, INPUT, None);
        let token = CancellationToken::new();
        let streamed = streamed(&resolver, &token);
        let mut reader = streamed.reader();
        let built = reader
            .by_ref()
            .take(batches_before)
            .map(|batch| batch.unwrap().num_rows())
            .collect::<Vec<_>>();
        assert_eq!(built, vec![1; batches_before], "{label}");
        let ended = if released {
            drop(reader);
            None
        } else {
            token.cancel();
            let ended = reader.next();
            assert!(reader.next().is_none(), "{label}: no later batch");
            ended
        };
        assert!(
            matches!(
                ended,
                None | Some(Err(Failure {
                    kind: FailureKind::Cancelled,
                    ..
                }))
            ),
            "{label}: {ended:?}"
        );
        assert!(
            matches!(
                streamed.run(),
                Err(Failure {
                    kind: FailureKind::Cancelled,
                    ..
                })
            ),
            "{label}: no result"
        );
        assert_eq!(
            (
                end_state(&resolver, "cancelled".into(), String::new()),
                resolver.requested("capture", "input.csv"),
            ),
            (CANCELLED(), input_read),
            "{label}"
        );
    }
}

/// Mid-stream, cancelled by a callback: under the fallback the stream's
/// first pull builds the run, and a project function's interrupt on the
/// first data row cancels it there (D46): the pull fails with no issue, no
/// later row is called, the run has no result, and nothing is staged or
/// published.
#[test]
fn mid_stream_a_project_function_interrupt_cancels_the_run() {
    let functions = Arc::new(Echo {
        interrupt: true,
        calls: AtomicUsize::new(0),
    });
    let (engine, resolver) = function_study(
        FUNCTION_SPEC,
        functions.clone(),
        Partitions::new(NonZeroUsize::MIN).with_whole_table_fallback(),
    );
    let token = CancellationToken::new();
    let streamed = engine
        .spec("spec.yaml")
        .project("project.yaml")
        .cancel_with(&token)
        .stream()
        .unwrap();
    let mut reader = streamed.reader();
    assert!(matches!(
        reader.next(),
        Some(Err(Failure {
            kind: FailureKind::Cancelled,
            ..
        }))
    ));
    assert!(reader.next().is_none(), "no later batch");
    assert!(matches!(
        streamed.run(),
        Err(Failure {
            kind: FailureKind::Cancelled,
            ..
        })
    ));
    assert_eq!(
        (
            functions.calls.load(Ordering::SeqCst),
            end_state(&resolver, "cancelled".into(), String::new()),
        ),
        (1, CANCELLED()),
    );
    assert!(!token.is_cancelled(), "the interrupt is not the token");
}

/// Mid-stream, failed: the third record's V does not convert, so no batch
/// can be built, and the stream's first pull ends it with the run's issue,
/// the whole-table run's `conversion_failed` at W. No later batch is built;
/// the run has the issue and no output, and its save writes only the report
/// file.
#[test]
fn mid_stream_a_failed_run_ends_its_stream_with_its_issue() {
    let resolver = Recording::new(SPEC, FAILING, None);
    let streamed = streamed(&resolver, &CancellationToken::new());
    let mut reader = streamed.reader();
    let Some(Err(
        failure @ Failure {
            kind: FailureKind::Rejected,
            ..
        },
    )) = reader.next()
    else {
        panic!("the stream ends with the run's issue");
    };
    assert!(reader.next().is_none(), "no later batch");
    let outcome = r#"Error convert conversion_failed Some("VALUE-7") ["columns.W"] {"from":"float","keys":[{"ID":3}],"to":"int","value":3.5} None None"#.to_owned();
    assert_eq!(issues(&failure.diagnostics), outcome, "the stream's error");
    let mut run = streamed.run().unwrap();
    assert!(run.output().is_none());
    assert_eq!(
        failed_and_saved(&resolver, &mut run),
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

/// During publication, cancelled (#3793): the save writes the output one
/// partition at a time, and its three one-record partitions give it three
/// polls, one before each partition after the first, then D74's after both
/// files are staged and before the first commit. Whichever poll finds the
/// token cancelled, nothing is committed or left staged, and the save fails
/// with D39's text: before the second or a later partition, the staged
/// report is removed and the output, which the resolver stages whole, was
/// never staged; at the last poll both staged files are removed. The result
/// saves whole later.
#[test]
fn during_publication_a_cancelled_save_removes_what_it_staged() {
    let polls = Arc::new(AtomicUsize::new(0));
    let cancelling = |from: Option<usize>| {
        let polls = Arc::clone(&polls);
        CancellationToken::with_probe(move || {
            let poll = polls.fetch_add(1, Ordering::SeqCst) + 1;
            from.is_some_and(|from| poll >= from)
        })
    };
    let resolver = Recording::new(SPEC, INPUT, None);
    let mut run = engine(Arc::clone(&resolver), false)
        .spec("spec.yaml")
        .run()
        .unwrap();
    run.save(&cancelling(None)).unwrap();
    assert_eq!(polls.swap(0, Ordering::SeqCst), 3, "the polls of a save");
    for (label, from, staged) in [
        ("before the output's second partition", 1, 1),
        ("before its third partition", 2, 1),
        ("after every file is staged", 3, 2),
    ] {
        let resolver = Recording::new(SPEC, INPUT, None);
        let mut run = engine(Arc::clone(&resolver), false)
            .spec("spec.yaml")
            .run()
            .unwrap();
        polls.store(0, Ordering::SeqCst);
        let failure = run.save(&cancelling(Some(from))).unwrap_err();
        assert_eq!(
            (
                failure.message.as_str(),
                failure.diagnostics.len(),
                failure.paths.len()
            ),
            ("cannot save TEST: operation cancelled", 0, 0),
            "the save cancelled {label}"
        );
        assert_eq!(
            end_state(&resolver, "cancelled".into(), subject(&failure.message)),
            EndState {
                outcome: "cancelled".into(),
                named: "cannot save TEST:".into(),
                published: vec![],
                staged,
                committed: 0,
                left: 0,
            },
            "cancelled {label}"
        );
        assert_eq!(
            run.save(&CancellationToken::new()).unwrap(),
            [REPORT, OUTPUT],
            "the later save, cancelled {label}"
        );
    }
}

/// During publication, failed before the first commit: the output cannot be
/// staged, so the staged report is removed and no target is touched; the
/// save fails at the output's path with no files written.
#[test]
fn during_publication_a_file_that_cannot_be_staged_publishes_nothing() {
    let resolver = Recording::new(SPEC, INPUT, Some(("stage", "out.csv")));
    let mut run = engine(Arc::clone(&resolver), false)
        .spec("spec.yaml")
        .run()
        .unwrap();
    let failure = run.save(&CancellationToken::new()).unwrap_err();
    assert!(failure.paths.is_empty(), "{:?}", failure.paths);
    assert_eq!(
        end_state(
            &resolver,
            issues(&failure.diagnostics),
            subject(&failure.message)
        ),
        EndState {
            outcome: r#"Error engine engine_rejected None ["output.path"] {"code":"host_publication_failure","completed":0,"entered":2,"path":"out.csv","step":"output"} None None"#.into(),
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
        .spec("spec.yaml")
        .run()
        .unwrap();
    let failure = run.save(&CancellationToken::new()).unwrap_err();
    assert_eq!(failure.paths, [REPORT], "the files written before the stop");
    assert_eq!(
        end_state(
            &resolver,
            issues(&failure.diagnostics),
            subject(&failure.message)
        ),
        EndState {
            outcome: r#"Error engine engine_rejected None ["output.path"] {"code":"host_publication_failure","completed":1,"entered":2,"path":"out.csv","step":"output"} None None"#.into(),
            named: "cannot save TEST:".into(),
            published: vec![REPORT],
            staged: 2,
            committed: 2,
            left: 0,
        }
    );
}
