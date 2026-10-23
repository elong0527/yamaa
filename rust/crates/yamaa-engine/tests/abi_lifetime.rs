//! Handle and buffer lifetime across the engine's C ABI (#3702; the design on
//! #2870, section 7, the lifetime rows): each released handle leaves the
//! census, an exported stream outlives its run, a released or other kind of
//! handle refuses and releasing twice does nothing, a busy result refuses a
//! reentrant call, every stream and buffer is released exactly once, also
//! after a failed conversion, a cancelled call makes no handle and releases
//! every root once, and a panic is contained. Each expectation comes from
//! those rows, the condition registry and the shared truth's lifetime study.
use std::cell::RefCell;

use serde_json::json;
use yamaa_boundary::abi::{self, Str};

use crate::support::{
    answer, call, census, conditions, engine, export, family, function_study, lifetime_study,
    reader, records, release, save, serial, spec_run, status, table, Answer, Host, Invoked,
};

/// The roots of a host whose call binds nothing: its context, once.
const CONTEXT_ONLY: [(&str, (usize, usize)); 3] = [
    ("context", (1, 1)),
    ("function", (0, 0)),
    ("payload", (0, 0)),
];
/// The roots of a host whose call binds `echo` once and keeps no payload.
const BOUND_ONCE: [(&str, (usize, usize)); 3] = [
    ("context", (1, 1)),
    ("function", (1, 1)),
    ("payload", (0, 0)),
];

fn done(text: &str) -> Answer<String> {
    Answer::Done(text.to_owned())
}

#[test]
fn every_released_handle_leaves_the_census() {
    let _serial = serial();
    let study = lifetime_study("handles");
    let (spec, project) = (study.spelled("spec.yaml"), study.spelled("project.yaml"));
    let (spec, project) = (Str::of(&spec), Str::of(&project));
    let engine = engine();
    let run = spec_run(&call(), &study.spelled("spec.yaml"), None, 0).done();
    let make = |kind: &str| {
        // SAFETY: the arguments are valid for the call.
        unsafe {
            match kind {
                "check" => answer(|o, r| (engine.spec_check)(&call(), spec, Str::NONE, o, r)),
                "run" => answer(|o, r| (engine.spec_run)(&call(), spec, Str::NONE, 0, o, r)),
                "project_run" => {
                    answer(|o, r| (engine.project_run)(&call(), project, Str::NONE, 0, o, r))
                }
                "submission" => answer(|o, r| {
                    (engine.project_submission)(&call(), project, Str::NONE, Str::NONE, o, r)
                }),
                "selftest" => answer(|o, r| (engine.selftest)(&call(), o, r)),
                "plans" => answer(|o, r| (engine.plans_new)(o, r)),
                _ => table(engine.output, run),
            }
        }
    };
    let before = census();
    let (mut seen, mut expected) = (Vec::new(), Vec::new());
    for kind in [
        "check",
        "run",
        "project_run",
        "submission",
        "selftest",
        "plans",
        "table",
    ] {
        let made = make(kind).done();
        let held = census();
        release(made);
        seen.push((kind, held, census()));
        expected.push((kind, before.plus(kind), before.clone()));
    }
    release(run);
    assert_eq!(seen, expected, "each kind of handle, made and released");
}

#[test]
fn an_exported_stream_outlives_its_run() {
    let _serial = serial();
    let study = lifetime_study("outlives");
    let before = census();
    let run = spec_run(&call(), &study.spelled("spec.yaml"), None, 0).done();
    let output = table(engine().output, run).done();
    let stream = export(output).done();
    release(output);
    release(run);
    let held = census();
    let rows = records(stream, &["ID", "NAME"]);
    assert_eq!(
        (held, rows, census()),
        (
            before.streams(1),
            family("interface")["lifetime"]["rows"].clone(),
            before
        ),
        "a stream exported from a run's output, read after the run and its table are released"
    );
}

#[test]
fn a_released_handle_refuses_as_expired() {
    let _serial = serial();
    let study = lifetime_study("expired");
    let run = spec_run(&call(), &study.spelled("spec.yaml"), None, 0).done();
    let output = table(engine().output, run).done();
    release(run);
    release(output);
    let seen = [
        status(run, None).map(|_| ()),
        table(engine().output, run).map(|_| ()),
        save(&call(), run).map(|_| ()),
        export(output).map(|_| ()),
    ];
    let expired = Answer::Boundary("expired native handle".to_owned());
    assert_eq!(
        seen,
        [expired.clone(), expired.clone(), expired.clone(), expired],
        "the status, output and save of a released run, and the export of a released table"
    );
}

#[test]
fn a_handle_of_another_kind_refuses() {
    let _serial = serial();
    let study = lifetime_study("kinds");
    let spec = study.spelled("spec.yaml");
    let engine = engine();
    // SAFETY: the arguments are valid for the call.
    let plans = answer(|out, refusal| unsafe { (engine.plans_new)(out, refusal) }).done();
    let run = spec_run(&call(), &spec, None, 0).done();
    let output = table(engine.output, run).done();
    let seen = [
        status(plans, None).map(|_| ()),
        // SAFETY: the arguments are valid for the call.
        answer(|out, refusal| unsafe { (engine.rows)(run, out, refusal) }).map(|_| ()),
        spec_run(&call(), &spec, None, output).map(|_| ()),
        status(run, Some("TEST")).map(|_| ()),
    ];
    for handle in [plans, run, output] {
        release(handle);
    }
    let refused = |reason: &str| Answer::Boundary(reason.to_owned());
    assert_eq!(
        seen,
        [
            refused("status does not take a plans handle"),
            refused("rows does not take a run handle"),
            refused("spec_run does not take a table handle"),
            refused("a run handle has no members"),
        ],
        "a plans holder's status, a run's rows, a table as a run's plans and a run's member"
    );
}

#[test]
fn releasing_twice_is_harmless() {
    let _serial = serial();
    let study = lifetime_study("twice");
    let spec = study.spelled("spec.yaml");
    let before = census();
    let first = spec_run(&call(), &spec, None, 0).done();
    let once = release(first);
    // The next run takes the released slot, under a new generation, which
    // the stale handle does not name.
    let second = spec_run(&call(), &spec, None, 0).done();
    let again = [release(first), release(first), release(0)];
    let kept = (status(second, None), census());
    release(second);
    assert_eq!(
        (once, again, kept, census()),
        (
            abi::DONE,
            [abi::DONE; 3],
            (done("succeeded"), before.plus("run")),
            before
        ),
        "a run released, its stale handle released twice more and 0 released"
    );
}

#[test]
fn a_reentrant_call_into_a_busy_owner_refuses() {
    let _serial = serial();
    let study = lifetime_study("busy");
    let spec = study.spelled("spec.yaml");
    let first = spec_run(&call(), &spec, None, 0).done();
    let second = spec_run(&call(), &spec, None, 0).done();
    let seen = RefCell::new(Vec::new());
    let host = Host::new().probing(|| {
        seen.borrow_mut()
            .push((status(first, None), status(second, None)));
        abi::GO_ON
    });
    let saved = save(&host.call(), first);
    let roots = host.roots();
    drop(host);
    for handle in [first, second] {
        release(handle);
    }
    let busy = Answer::Boundary("run handle is in use or poisoned".to_owned());
    assert_eq!(
        (saved, seen.into_inner(), roots),
        (
            Answer::Done(vec![study.spelled("output.csv")]),
            // The save polls once before its first commit, holding the run
            // (D74), and the entry once more as it returns (D186).
            vec![
                (busy, done("succeeded")),
                (done("succeeded"), done("succeeded"))
            ],
            CONTEXT_ONLY,
        ),
        "a save whose probe asks the status of the run it saves and of another run"
    );
}

#[test]
fn every_exported_stream_is_released_exactly_once() {
    let _serial = serial();
    let study = lifetime_study("streams");
    let run = spec_run(&call(), &study.spelled("spec.yaml"), None, 0).done();
    let output = table(engine().output, run).done();
    let before = census();
    // Three exports of one table, each with its own cursor: one read to its
    // end, one released after its schema, one released unread.
    let (read, early) = (export(output).done(), export(output).done());
    let mut unread = export(output).done();
    let held = census();
    let rows = records(read, &["ID", "NAME"]);
    drop(reader(early));
    let release_unread = unread.release.expect("an exported stream is live");
    // SAFETY: the holder releases its stream once.
    unsafe { release_unread(&mut unread) };
    let after = (unread.release.is_none(), census());
    for handle in [output, run] {
        release(handle);
    }
    assert_eq!(
        (held, rows, after),
        (
            before.streams(3),
            family("interface")["lifetime"]["rows"].clone(),
            (true, before)
        ),
        "three streams of one table: read whole, released early and released unread"
    );
}

#[test]
fn a_failed_conversion_releases_every_buffer() {
    let _serial = serial();
    let before = census();
    // An integer beyond 2^53, which no R double holds: the host refuses the
    // value as R does, and releases what it read.
    let study = lifetime_study("conversion");
    study.write("input.csv", "ID,NAME\n9007199254740993,alpha\n");
    let run = spec_run(&call(), &study.spelled("spec.yaml"), None, 0).done();
    let output = table(engine().output, run).done();
    let mut stream = reader(export(output).done());
    let batch = stream.next().unwrap().unwrap();
    let id = batch.column_by_name("ID").unwrap().as_any();
    let id = id
        .downcast_ref::<arrow_array::Int64Array>()
        .unwrap()
        .value(0);
    let refused = id.unsigned_abs() > 1 << 53;
    drop((batch, stream));
    for handle in [output, run] {
        release(handle);
    }
    let after_table = census();
    // A project function's text that is not UTF-8: the engine refuses the
    // result (FUNC-9) and releases the reply it read and every root.
    let study = function_study("reply");
    let host = Host::new().invoking(|argument| match argument {
        "alpha" => Invoked::Bytes(b"\xff"),
        _ => Invoked::Echo,
    });
    let project = study.spelled("project.yaml");
    let run = spec_run(&host.call(), &study.spelled("spec.yaml"), Some(&project), 0).done();
    let refusal = (status(run, None), conditions(run));
    release(run);
    assert_eq!(
        (
            refused,
            after_table,
            refusal,
            host.replies(),
            host.roots(),
            census()
        ),
        (
            true,
            before.clone(),
            (
                done("failed"),
                json!([{"severity": "error", "phase": "derivation", "condition": "invalid_function_result", "rule": "FUNC-9"}]),
            ),
            // The function's tests pass `a` and empty text, and the first
            // row `alpha`, whose failure stops the run (FUNC-8, FUNC-9).
            (3, 3),
            BOUND_ONCE,
            before,
        ),
        "a host refusing an integer beyond 2^53, and a function answering text that is not UTF-8"
    );
}

#[test]
fn a_cancelled_call_creates_no_handle_and_releases_every_root_once() {
    let _serial = serial();
    let study = function_study("cancelled");
    let (spec, project) = (study.spelled("spec.yaml"), study.spelled("project.yaml"));
    let before = census();
    // The probe cancels the run at its first poll, before it reads anything.
    let probed = Host::new().probing(|| abi::CANCEL);
    let by_probe = spec_run(&probed.call(), &spec, Some(&project), 0);
    // A project function raises the host's interrupt on the first row.
    let interrupted = Host::new().invoking(|argument| match argument {
        "alpha" => Invoked::Interrupt,
        _ => Invoked::Echo,
    });
    let by_function = spec_run(&interrupted.call(), &spec, Some(&project), 0);
    let cancelled = census();
    // The probe cancels a save before its first commit: it writes nothing,
    // and the run saves whole later (D74).
    let running = Host::new();
    let run = spec_run(&running.call(), &spec, Some(&project), 0).done();
    let saving = Host::new().probing(|| abi::CANCEL);
    let by_save = save(&saving.call(), run);
    let unsaved = (study.join("out.csv").exists(), status(run, None));
    let saved = save(&call(), run);
    let written = std::fs::read_to_string(study.join("out.csv")).unwrap();
    release(run);
    let roots = [
        probed.roots(),
        interrupted.roots(),
        running.roots(),
        saving.roots(),
    ];
    assert_eq!(
        (
            by_probe,
            by_function,
            cancelled,
            by_save,
            unsaved,
            saved,
            written,
            roots,
            census()
        ),
        (
            Answer::Cancelled,
            Answer::Cancelled,
            before.clone(),
            Answer::Cancelled,
            (false, done("succeeded")),
            Answer::Done(vec![study.spelled("out.csv")]),
            "ID,VALUE\n1,alpha\n2,beta\n".to_owned(),
            [
                CONTEXT_ONLY,
                [
                    ("context", (1, 1)),
                    ("function", (1, 1)),
                    ("payload", (1, 1))
                ],
                BOUND_ONCE,
                CONTEXT_ONLY,
            ],
            before,
        ),
        "a run cancelled by its probe and by its function's interrupt, and a save cancelled \
         by its probe, with the roots each call's host handed over"
    );
}

#[test]
fn a_panicking_operation_is_a_boundary_refusal() {
    let _serial = serial();
    let study = lifetime_study("panic");
    let before = census();
    // A probe that answers outside the protocol makes the call panic, which
    // its entry contains.
    let host = Host::new().probing(|| 7);
    let answered = spec_run(&host.call(), &study.spelled("spec.yaml"), None, 0);
    let reason = "internal engine failure: the host's probe answered 7, which is neither 0 nor 1";
    assert_eq!(
        (answered, host.roots(), census()),
        (Answer::Boundary(reason.to_owned()), CONTEXT_ONLY, before),
        "a run whose probe answers 7"
    );
}

#[test]
fn a_panicking_callback_is_contained() {
    let _serial = serial();
    let study = function_study("callback");
    let before = census();
    // The host contains its code's panic and answers it as a failure of its
    // own: an opaque failure, which projects no facts, so the run fails as
    // `engine_rejected`, which the condition registry gives no rule, and the
    // engine goes on.
    let host = Host::new().invoking(|argument| match argument {
        "alpha" => panic!("the host's code panicked"),
        _ => Invoked::Echo,
    });
    let project = study.spelled("project.yaml");
    let run = spec_run(&host.call(), &study.spelled("spec.yaml"), Some(&project), 0).done();
    let failed = (status(run, None), conditions(run));
    release(run);
    assert_eq!(
        (failed, host.roots(), census()),
        (
            (
                done("failed"),
                json!([{"severity": "error", "phase": "engine", "condition": "engine_rejected", "rule": null}]),
            ),
            BOUND_ONCE,
            before,
        ),
        "a run whose project function's host code panics on its first row"
    );
}
