//! Streamed calls through the safe client retain their callback context until
//! the last run, table or reader releases it, including refused and unused calls.
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use yamaa_boundary::client::{Call, Callbacks, Gate, Language};
use yamaa_boundary::{Value, BOUNDARY_VERSION};

use crate::support::{family, lifetime_study, records, serial};

struct Host(Arc<AtomicUsize>);
impl Callbacks for Host {
    type Object = ();
}
impl Drop for Host {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

fn engine() -> yamaa_boundary::client::Engine {
    let gate = Gate::new();
    let request = Value::Map(vec![
        (
            "version".into(),
            Value::Integer(i64::from(BOUNDARY_VERSION)),
        ),
        ("required".into(), Value::List(vec![])),
        ("optional".into(), Value::List(vec![])),
    ]);
    // SAFETY: this is the linked engine's negotiation entry.
    unsafe { gate.negotiate(&request, yamaa_engine::negotiate) };
    gate.pass().unwrap()
}

#[test]
fn a_stream_retains_its_callbacks_after_its_run_and_table_are_released() {
    let _serial = serial();
    let study = lifetime_study("client-stream-owned");
    let engine = engine();
    let drops = Arc::new(AtomicUsize::new(0));
    let call = Call::new(Language::None, "test", "").with_owned(Host(Arc::clone(&drops)), false);
    let run = engine
        .spec_stream(call, &study.spelled("spec.yaml"), None, 0)
        .unwrap();
    let output = engine.output(run.handle(), None).unwrap().unwrap();
    let stream = engine.export(output.handle()).unwrap();
    drop(output);
    drop(run);
    let held = drops.load(Ordering::SeqCst);
    let rows = records(stream.into_raw(), &["ID", "NAME"]);
    assert_eq!(
        (held, rows, drops.load(Ordering::SeqCst)),
        (0, family("interface")["lifetime"]["rows"].clone(), 1),
        "the final exported reader owns callbacks after both engine handles drop"
    );
}

#[test]
fn unused_and_refused_stream_calls_release_their_owned_callbacks_once() {
    let _serial = serial();
    let study = lifetime_study("client-stream-refused");
    let engine = engine();
    let drops = Arc::new(AtomicUsize::new(0));
    let call = || Call::new(Language::None, "test", "").with_owned(Host(Arc::clone(&drops)), false);
    drop(call());
    let unused = drops.load(Ordering::SeqCst);
    let plans = engine.plans().unwrap();
    let expired = plans.handle();
    drop(plans);
    let refused = engine
        .spec_stream(call(), &study.spelled("spec.yaml"), None, expired)
        .is_err();
    assert_eq!(
        (unused, refused, drops.load(Ordering::SeqCst)),
        (1, true, 2),
        "unused and refused contexts each release exactly once"
    );
}
