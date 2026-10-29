#![cfg(all(feature = "yaml", feature = "arrow"))]
//! The progress port and per-phase timings (#3762): the events an engine's
//! progress callback receives, what a callback that fails does to its
//! result, a callback that runs the engine again, and the timings every
//! result carries. These are seams the benchmark readers cannot reach: the
//! readers compare saved files, and neither the events nor the timings are
//! saved (the memory route runs every benchmark with a callback, so the
//! saved bytes are proven unchanged there).
//!
//! The expected events of `adam-adsl-analysis-age` are derived from the
//! engine's documented geometry: a run without project functions passes
//! preparation, input, admission, derivation, output keys, verification and
//! output, each started and ended once (`yamaa::progress`); its
//! specification reads no record with another, so partitions of three
//! records cut its eight `dm.csv` records into three partitions of 3, 3 and
//! 2, entered in derivation; and a stream of it on that engine hands out its
//! eight output rows in batches of at most three, after its output phase.
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use crate::support::Scratch;
use yamaa::host::functions::FunctionError;
use yamaa::{
    CancellationToken, Engine, EngineOptions, HostLanguage, MemoryResolver, Partitions, Plans,
    ProgressEvent, ProgressKind, ProgressPhase, Status, SubmissionOptions,
};

/// One event as the tests compare it: name, phase, kind, outcome, number
/// and rows.
type Heard = (
    String,
    ProgressPhase,
    ProgressKind,
    Option<Status>,
    Option<usize>,
    Option<usize>,
);

fn benchmark(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../benchmarks")
        .join(name)
}

/// An engine that keeps every event it reports in `heard`.
fn hearing(heard: &Arc<Mutex<Vec<Heard>>>, partitions: Option<Partitions>) -> Engine {
    let kept = Arc::clone(heard);
    let options = EngineOptions::new(HostLanguage::Python, "test".into(), "0".into())
        .with_progress(move |event: &ProgressEvent| {
            kept.lock().unwrap().push((
                event.name.clone(),
                event.phase,
                event.kind,
                event.outcome,
                event.number,
                event.rows,
            ));
            Ok(())
        });
    Engine::new(match partitions {
        Some(partitions) => options.with_partitions(partitions),
        None => options,
    })
}

fn start(name: &str, phase: ProgressPhase) -> Heard {
    (
        name.to_owned(),
        phase,
        ProgressKind::Start,
        None,
        None,
        None,
    )
}

fn end(name: &str, phase: ProgressPhase) -> Heard {
    let succeeded = Some(Status::Succeeded);
    (
        name.to_owned(),
        phase,
        ProgressKind::End,
        succeeded,
        None,
        None,
    )
}

fn numbered(name: &str, kind: ProgressKind, number: usize, rows: usize) -> Heard {
    let phase = match kind {
        ProgressKind::Partition => ProgressPhase::Derivation,
        _ => ProgressPhase::Output,
    };
    (name.to_owned(), phase, kind, None, Some(number), Some(rows))
}

/// Each phase started and ended in order, with `inside` heard between the
/// start and the end of derivation, then `after`.
fn phases(name: &str, inside: &[Heard], after: &[Heard]) -> Vec<Heard> {
    use ProgressPhase::*;
    let mut events = Vec::new();
    for phase in [
        Preparation,
        Input,
        Admission,
        Derivation,
        OutputKeys,
        Verification,
        Output,
    ] {
        events.push(start(name, phase));
        if phase == Derivation {
            events.extend_from_slice(inside);
        }
        events.push(end(name, phase));
    }
    events.extend_from_slice(after);
    events
}

#[test]
fn a_run_reports_each_phase_partition_and_batch_in_order() {
    let spec = benchmark("adam-adsl-analysis-age").join("spec.yaml");
    let path = spec.to_str().unwrap();
    let thirds = Partitions::new(NonZeroUsize::new(3).unwrap());
    let partitions = [(1, 3), (2, 3), (3, 2)]
        .map(|(number, rows)| numbered(path, ProgressKind::Partition, number, rows));
    let batches = [(1, 3), (2, 3), (3, 2)]
        .map(|(number, rows)| numbered(path, ProgressKind::Batch, number, rows));
    for (partitioned, streamed, expected, label) in [
        (None, false, phases(path, &[], &[]), "a whole-table run"),
        (
            Some(thirds),
            false,
            phases(path, &partitions, &[]),
            "a run in partitions of three records",
        ),
        (
            Some(thirds),
            true,
            phases(path, &partitions, &batches),
            "a stream in partitions of three records",
        ),
    ] {
        let heard = Arc::new(Mutex::new(Vec::new()));
        let engine = hearing(&heard, partitioned);
        let status = if streamed {
            let stream = engine.spec(path).stream().unwrap();
            let rows: usize = stream.reader().map(|batch| batch.unwrap().num_rows()).sum();
            assert_eq!(rows, 8, "{label}: the rows streamed");
            let run = stream.run().unwrap();
            run.status()
        } else {
            engine.spec(path).run().unwrap().status()
        };
        assert_eq!(status, Status::Succeeded, "{label}");
        assert_eq!(*heard.lock().unwrap(), expected, "{label}: the events");
    }
}

/// A specification and its input, served from memory, whose save writes
/// `out.csv` and `report.csv`.
fn served() -> Arc<MemoryResolver> {
    let resolver = Arc::new(MemoryResolver::new());
    resolver.insert(
        "/",
        "spec.yaml",
        "spec:1",
        Arc::from(
            &b"yamaa: \"1.0\"\ntype: spec\ndomain: TEST\nkeys: [ID]\ninput: {SRC: input.csv}\noutput: {path: out.csv, report: report.csv, columns: [ID]}\ncolumns:\n  - {name: ID, type: str, derivation: SRC.ID}\n"[..],
        ),
    );
    resolver.insert(
        "/spec.yaml",
        "input.csv",
        "input:1",
        Arc::from(&b"ID\n1\n2\n"[..]),
    );
    resolver
}

/// The one issue of an operation whose progress callback failed at `step`.
fn progress_failure(step: &str, panics: bool) -> String {
    let (class, message) = if panics {
        ("panic", "the progress callback panics")
    } else {
        ("protocol", "host protocol violation")
    };
    format!(
        "Error engine engine_rejected None [] {{\"code\":\"host_progress_failure\",\"host_error\":\"{class}\",\"host_message\":\"{message}\",\"step\":\"{step}\"}} None None"
    )
}

#[test]
fn a_callback_that_panics_or_fails_fails_the_run_which_saves_nothing() {
    for (panics, label) in [
        (true, "a callback that panics"),
        (false, "a callback that fails"),
    ] {
        let resolver = served();
        let calls = Arc::new(AtomicUsize::new(0));
        let counted = Arc::clone(&calls);
        let engine = Engine::new(
            EngineOptions::new(HostLanguage::Python, "test".into(), "0".into())
                .with_resolver(Arc::clone(&resolver) as Arc<dyn yamaa::ResourceResolver>)
                .with_progress(move |_: &ProgressEvent| {
                    counted.fetch_add(1, Ordering::SeqCst);
                    if panics {
                        panic!("the progress callback panics");
                    }
                    Err(FunctionError::Protocol)
                }),
        );
        let mut run = engine.spec("spec.yaml").run().unwrap();
        assert_eq!(
            calls.load(Ordering::SeqCst),
            1,
            "{label}: called once, then no more"
        );
        assert_eq!(
            (
                run.status(),
                run.output().is_none(),
                run.report().is_none(),
                crate::support::issues(&run.diagnostics()),
            ),
            (
                Status::Failed,
                true,
                true,
                progress_failure("build", panics)
            ),
            "{label}: the run"
        );
        let refused = run.save(&CancellationToken::new()).unwrap_err();
        assert_eq!(
            (
                refused.paths,
                resolver.published("memory:/out.csv").is_none(),
                resolver.published("memory:/report.csv").is_none(),
            ),
            (Vec::<String>::new(), true, true),
            "{label}: the save writes nothing"
        );
        let check = engine.spec("spec.yaml").check().unwrap();
        assert_eq!(
            (check.status(), crate::support::issues(&check.diagnostics())),
            (Status::Failed, progress_failure("check", panics)),
            "{label}: the check"
        );
    }
}

#[test]
fn a_callback_may_run_the_engine_again_with_the_same_holder() {
    let resolver = served();
    let engine_slot: Arc<Mutex<Option<Engine>>> = Arc::new(Mutex::new(None));
    let plans = Arc::new(Plans::new());
    let again = Arc::new(AtomicBool::new(false));
    let inner = Arc::new(Mutex::new(None));
    let (slot, holder, once, kept) = (
        Arc::clone(&engine_slot),
        Arc::clone(&plans),
        Arc::clone(&again),
        Arc::clone(&inner),
    );
    let engine = Engine::new(
        EngineOptions::new(HostLanguage::Python, "test".into(), "0".into())
            .with_resolver(Arc::clone(&resolver) as Arc<dyn yamaa::ResourceResolver>)
            .with_progress(move |event: &ProgressEvent| {
                // In the outer run's derivation, run the same specification
                // through the same engine and holder, once.
                if event.phase == ProgressPhase::Derivation && !once.swap(true, Ordering::SeqCst) {
                    let engine = slot.lock().unwrap().clone().expect("the engine");
                    let run = engine.spec("spec.yaml").reusing(&holder).run().unwrap();
                    *kept.lock().unwrap() = Some(run.status());
                }
                Ok(())
            }),
    );
    *engine_slot.lock().unwrap() = Some(engine.clone());
    let outer = engine.spec("spec.yaml").reusing(&plans).run().unwrap();
    // Break the cycle between the engine and its callback.
    engine_slot.lock().unwrap().take();
    assert_eq!(
        (outer.status(), *inner.lock().unwrap()),
        (Status::Succeeded, Some(Status::Succeeded)),
        "the outer run and the run its callback started"
    );
}

#[test]
fn every_result_carries_the_phases_it_ran() {
    use ProgressPhase::*;
    let case = benchmark("adam-adsl-xpt");
    let study = Scratch::new("progress-timings");
    for file in ["project.yaml", "spec.yaml", "input/adsl_raw.csv"] {
        study.write(file, std::fs::read(case.join(file)).unwrap());
    }
    let project = study.join("project.yaml");
    let project = project.to_str().unwrap();
    let spec = study.join("spec.yaml");
    let spec = spec.to_str().unwrap();
    let heard = Arc::new(Mutex::new(Vec::new()));
    let engine = hearing(&heard, None);
    let run_phases = [
        Preparation,
        Input,
        Admission,
        Derivation,
        OutputKeys,
        Verification,
        Output,
    ];
    let listed = |timings: yamaa::Timings| {
        timings
            .entries()
            .iter()
            .map(|timing| (timing.name.clone(), timing.phase, timing.outcome))
            .collect::<Vec<_>>()
    };
    let succeeded = |name: &str, phases: &[ProgressPhase]| {
        phases
            .iter()
            .map(|phase| (name.to_owned(), *phase, Some(Status::Succeeded)))
            .collect::<Vec<_>>()
    };
    let check = engine.spec(spec).project(project).check().unwrap();
    assert_eq!(
        listed(check.timings()),
        succeeded(spec, &[Preparation]),
        "a check"
    );
    let run = engine.spec(spec).project(project).run().unwrap();
    assert_eq!(listed(run.timings()), succeeded(spec, &run_phases), "a run");
    let mut runs = engine.project(project).run().unwrap();
    assert_eq!(
        listed(runs.timings()),
        succeeded("ADSL", &run_phases),
        "a project run"
    );
    let project_check = engine.project(project).check().unwrap();
    assert_eq!(
        listed(project_check.timings()),
        succeeded("ADSL", &[Preparation]),
        "a project check"
    );
    runs.save(&CancellationToken::new()).unwrap();
    let submission = engine
        .project(project)
        .submission(&SubmissionOptions::new(yamaa::VERSION).with_created("2026-09-11T00:00:00"))
        .unwrap();
    assert_eq!(submission.status(), Status::Succeeded, "the submission");
    assert_eq!(
        listed(submission.timings()),
        succeeded("adam", &[Composition]),
        "a submission"
    );
    // Each timing ends no earlier than it starts, and the table holds one row
    // per timing with the timing columns.
    let timings = runs.timings();
    let table = timings.table();
    assert_eq!(
        (
            table.rows(),
            table
                .columns()
                .iter()
                .map(|field| field.name().as_str())
                .collect::<Vec<_>>()
        ),
        (run_phases.len(), yamaa::TIMING_COLUMNS.to_vec()),
        "the project run's timings table"
    );
    assert!(
        timings
            .entries()
            .windows(2)
            .all(|pair| pair[0].started <= pair[1].started),
        "the timings are in the order the phases started"
    );
}
