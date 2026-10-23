#![cfg(feature = "yaml")]
//! Streamed runs (#3697): a run whose output its consumer reads as Arrow
//! batches, each pull building what it hands out, beside the whole-table run
//! of the same specification and input.
//!
//! The expectations derive from the stream contract in the facade's `stream`
//! module and from the specification below: its output holds five records in
//! descending `ID` order (OUTPUT-11), so an engine with two-record partitions
//! hands it out as the batches 5 and 4, 3 and 2, then 1, and a whole-table
//! engine as one batch.
use std::num::NonZeroUsize;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

use arrow_array::ffi_stream::ArrowArrayStreamReader;
use arrow_array::{Array, Int64Array, RecordBatchReader, StringArray};
use yamaa::{
    CancellationToken, Engine, EngineOptions, Error, HostLanguage, MemoryResolver, Partitions,
    Plans, StreamProgress, RecordBatch,
};

use crate::support::NoFunctions;

/// Five records of four types, ordered by descending ID.
const SPEC: &str = "yamaa: \"1.0\"\ntype: spec\ndomain: TEST\nkeys: [ID]\ninput: {SRC: {path: input.csv, types: {ID: int, V: float, D: date}}}\noutput: {path: out.csv, columns: [ID, NAME, V, D], order_by: [{variable: ID, direction: desc}]}\ncolumns:\n  - {name: ID, type: int, label: Identifier, derivation: SRC.ID}\n  - {name: NAME, type: str, label: Name, derivation: SRC.NAME}\n  - {name: V, type: float, label: Value, derivation: SRC.V}\n  - {name: D, type: date, label: Day, derivation: SRC.D}\n";
const INPUT: &[u8] = b"ID,NAME,V,D\n1,alpha,1.5,2024-01-01\n2,beta,,2024-02-29\n3,gamma,2.25,\n4,delta,-0.5,2023-12-31\n5,epsilon,10,2024-03-01\n";
const PROJECT: &str = "yamaa: \"1.0\"\ntype: project\nadam: {specs: [spec.yaml]}\n";

/// The specification served from memory, with its input when `input`.
fn resolver(input: bool) -> Arc<MemoryResolver> {
    let resolver = Arc::new(MemoryResolver::new());
    for declaring in ["/", "/project.yaml"] {
        resolver.insert(declaring, "spec.yaml", "spec:1", Arc::from(SPEC.as_bytes()));
        resolver.insert(declaring, "project.yaml", "project:1", Arc::from(PROJECT.as_bytes()));
    }
    if input {
        insert_input(&resolver);
    }
    resolver
}

fn insert_input(resolver: &MemoryResolver) {
    resolver.insert("/spec.yaml", "input.csv", "input:1", Arc::from(INPUT));
}

/// An engine over `resolver`, with partitions of `rows` records when given.
fn engine(resolver: Arc<MemoryResolver>, rows: Option<usize>) -> Engine {
    let options = EngineOptions::new(HostLanguage::Python, "test".into(), "0".into())
        .with_resolver(resolver);
    Engine::new(match rows {
        Some(rows) => options.with_partitions(Partitions::new(NonZeroUsize::new(rows).unwrap())),
        None => options,
    })
}

/// The IDs and names of each batch.
fn identities(batches: &[RecordBatch]) -> Vec<Vec<(i64, String)>> {
    batches
        .iter()
        .map(|batch| {
            let ids = batch.column(0).as_any().downcast_ref::<Int64Array>().unwrap();
            let names = batch.column(1).as_any().downcast_ref::<StringArray>().unwrap();
            (0..batch.num_rows())
                .map(|row| (ids.value(row), names.value(row).to_owned()))
                .collect()
        })
        .collect()
}

/// Each batch of a stream holds the whole-table run's rows in its order, at
/// most the partitions' record count at a time, under the schema the stream
/// gave before its first pull, which is the whole-table output's. Once the
/// stream ends, the run's result is the whole-table run's: its status,
/// summary, report and saved bytes.
#[test]
fn streamed_batches_hold_the_whole_table_output_in_its_order() {
    let token = CancellationToken::new();
    let whole_resolver = resolver(true);
    let mut whole = engine(whole_resolver.clone(), None)
        .spec_run("spec.yaml", None, &token)
        .unwrap();
    let table = whole.output().unwrap().clone();
    for (rows, expected) in [
        (
            Some(2),
            vec![
                vec![(5, "epsilon"), (4, "delta")],
                vec![(3, "gamma"), (2, "beta")],
                vec![(1, "alpha")],
            ],
        ),
        (
            None,
            vec![vec![
                (5, "epsilon"),
                (4, "delta"),
                (3, "gamma"),
                (2, "beta"),
                (1, "alpha"),
            ]],
        ),
    ] {
        let label = format!("partitions of {rows:?} records");
        let streamed_resolver = resolver(true);
        let streamed = engine(streamed_resolver.clone(), rows)
            .spec_stream("spec.yaml", None, &Plans::new(), &token)
            .unwrap();
        assert_eq!(streamed.schema(), Some(table.schema()), "{label}");
        let batches = streamed.reader().collect::<Result<Vec<_>, _>>().unwrap();
        let expected = expected
            .into_iter()
            .map(|batch| {
                batch
                    .into_iter()
                    .map(|(id, name)| (id, name.to_owned()))
                    .collect()
            })
            .collect::<Vec<Vec<_>>>();
        assert_eq!(identities(&batches), expected, "{label}");
        let mut start = 0;
        for batch in &batches {
            assert_eq!(batch.schema(), *table.schema(), "{label}");
            let whole_rows = table.batches()[0].slice(start, batch.num_rows());
            assert_eq!(batch, &whole_rows, "{label}, rows from {start}");
            start += batch.num_rows();
        }
        assert_eq!(streamed.progress().unwrap(), StreamProgress::Ended, "{label}");
        let mut run = streamed.run().unwrap();
        assert!(run.output().is_none(), "{label}: a streamed output is not kept");
        assert_eq!(
            (
                run.status(),
                run.summary().to_string(),
                run.report().map(|report| report.batches().to_vec()),
            ),
            (
                whole.status(),
                whole.summary().to_string(),
                whole.report().map(|report| report.batches().to_vec()),
            ),
            "{label}"
        );
        let saved = run.save(&token).unwrap();
        assert_eq!(saved, whole.save(&token).unwrap(), "{label}");
        assert_eq!(
            streamed_resolver.published("memory:/out.csv"),
            whole_resolver.published("memory:/out.csv"),
            "{label}"
        );
    }
}

/// The run reads nothing until its first pull: an input supplied after the
/// call, before that pull, is the one the run reads.
#[test]
fn a_streamed_run_reads_its_input_at_its_first_pull() {
    let resolver = resolver(false);
    let streamed = engine(resolver.clone(), None)
        .spec_stream("spec.yaml", None, &Plans::new(), &CancellationToken::new())
        .unwrap();
    assert_eq!(streamed.progress().unwrap(), StreamProgress::Unread);
    insert_input(&resolver);
    let rows: usize = streamed
        .reader()
        .map(|batch| batch.unwrap().num_rows())
        .sum();
    assert_eq!(rows, 5);
}

/// The pull is the backpressure: the run polls its token once before each
/// batch it builds, beyond the polls of the build, and a reader released
/// after its first batch builds nothing more and cancels the run.
#[test]
fn a_consumer_that_stops_pulling_stops_production() {
    let polls = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&polls);
    let token = CancellationToken::with_probe(move || {
        counted.fetch_add(1, Ordering::SeqCst);
        false
    });
    // The polls of the build and of the first batch, counted on a stream
    // that is read whole.
    let read = engine(resolver(true), Some(1))
        .spec_stream("spec.yaml", None, &Plans::new(), &token)
        .unwrap();
    let mut reader = read.reader();
    reader.next().unwrap().unwrap();
    let first = polls.swap(0, Ordering::SeqCst);
    assert_eq!(reader.count(), 4, "the other batches");
    assert_eq!(polls.swap(0, Ordering::SeqCst), 4, "one poll per later batch");
    let streamed = engine(resolver(true), Some(1))
        .spec_stream("spec.yaml", None, &Plans::new(), &token)
        .unwrap();
    let mut reader = streamed.reader();
    reader.next().unwrap().unwrap();
    assert_eq!(polls.swap(0, Ordering::SeqCst), first, "the first batch");
    drop(reader);
    assert_eq!(polls.load(Ordering::SeqCst), 0, "nothing after the release");
    assert_eq!(streamed.progress().unwrap(), StreamProgress::Cancelled);
    assert!(matches!(streamed.run(), Err(Error::Cancelled)));
}

/// A run nobody started to read runs whole when its result is asked for,
/// keeping its output table, and a reader then reads that table, as often
/// as asked, with a cursor of its own.
#[test]
fn an_unread_run_runs_whole_when_its_result_is_asked_for() {
    let token = CancellationToken::new();
    let streamed = engine(resolver(true), Some(2))
        .spec_stream("spec.yaml", None, &Plans::new(), &token)
        .unwrap();
    let table = streamed.run().unwrap().output().unwrap().clone();
    assert_eq!(table.rows(), 5);
    for reader in 0..2 {
        let batches = streamed.reader().collect::<Result<Vec<_>, _>>().unwrap();
        assert_eq!(batches, table.batches(), "reader {reader}");
    }
}

/// A started stream belongs to the reader that started it until it ends: a
/// second reader is refused, and so is the run's result, until the first
/// reader reads the stream to its end.
#[test]
fn a_started_stream_belongs_to_its_reader_until_it_ends() {
    let streamed = engine(resolver(true), Some(2))
        .spec_stream("spec.yaml", None, &Plans::new(), &CancellationToken::new())
        .unwrap();
    let mut first = streamed.reader();
    first.next().unwrap().unwrap();
    assert_eq!(streamed.progress().unwrap(), StreamProgress::Reading);
    let mut second = streamed.reader();
    assert!(
        matches!(second.next(), Some(Err(Error::Stream { code: "stream_taken" }))),
        "a second reader"
    );
    assert!(second.next().is_none(), "the refused reader ends");
    assert!(
        matches!(streamed.run(), Err(Error::Stream { code: "stream_open" })),
        "the result while the stream is open"
    );
    assert_eq!(first.count(), 2, "the first reader's other batches");
    assert!(streamed.run().unwrap().output().is_none());
}

/// A run that can call its host, an engine with project functions running
/// a specification with a project, is built only on the thread that started
/// it: a reader's pull or the result on another thread is refused with
/// `host_thread`, starting nothing. A run that cannot call its host builds
/// on any thread.
#[test]
fn a_run_that_can_call_its_host_is_built_only_on_its_own_thread() {
    let token = CancellationToken::new();
    let host = Engine::new(
        EngineOptions::new(HostLanguage::Python, "test".into(), "0".into())
            .with_resolver(resolver(true))
            .with_functions(Arc::new(NoFunctions)),
    );
    let streamed = host
        .spec_stream("spec.yaml", Some("project.yaml"), &Plans::new(), &token)
        .unwrap();
    std::thread::scope(|scope| {
        scope.spawn(|| {
            let mut reader = streamed.reader();
            assert!(
                matches!(reader.next(), Some(Err(Error::Stream { code: "host_thread" }))),
                "a pull on another thread"
            );
            assert!(
                matches!(streamed.run(), Err(Error::Stream { code: "host_thread" })),
                "the result on another thread"
            );
        });
    });
    assert_eq!(streamed.progress().unwrap(), StreamProgress::Unread);
    assert_eq!(streamed.reader().count(), 1, "on its own thread");
    let free = engine(resolver(true), None)
        .spec_stream("spec.yaml", None, &Plans::new(), &token)
        .unwrap();
    let rows = std::thread::scope(|scope| {
        scope
            .spawn(|| free.reader().map(|batch| batch.unwrap().num_rows()).sum::<usize>())
            .join()
            .unwrap()
    });
    assert_eq!(rows, 5, "a run that cannot call its host");
}

/// Exported as an Arrow C stream, a reader gives its schema before any
/// pull, each `get_next` builds one batch, and an error ends the stream
/// with D39's text of reading the run, behind the prefixes the Arrow
/// crate's C stream export and import add.
#[test]
fn a_reader_exported_as_a_c_stream_pulls_as_the_reader_does() {
    let token = CancellationToken::new();
    let streamed = engine(resolver(true), Some(2))
        .spec_stream("spec.yaml", None, &Plans::new(), &token)
        .unwrap();
    let stream = ArrowArrayStreamReader::try_new(streamed.reader().into_stream()).unwrap();
    assert_eq!(Some(&stream.schema()), streamed.schema());
    let rows = stream
        .map(|batch| batch.unwrap().num_rows())
        .collect::<Vec<_>>();
    assert_eq!(rows, [2, 2, 1]);
    let cancelled = CancellationToken::new();
    let streamed = engine(resolver(true), Some(2))
        .spec_stream("spec.yaml", None, &Plans::new(), &cancelled)
        .unwrap();
    cancelled.cancel();
    let mut stream = ArrowArrayStreamReader::try_new(streamed.reader().into_stream()).unwrap();
    assert_eq!(
        stream.next().unwrap().unwrap_err().to_string(),
        "C Data interface error: External error: cannot read TEST: operation cancelled"
    );
}
