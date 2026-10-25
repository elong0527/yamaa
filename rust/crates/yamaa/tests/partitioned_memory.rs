#![cfg(feature = "yaml")]
//! A partitioned run's measured memory (#3830): it holds one decoded
//! partition of its driver input at a time, so the memory it takes beyond
//! the input's bytes follows the partition, not the input.
//!
//! The shape is one #3763 admits: a row per subject, whose aggregates the
//! subject's records reduce, over key-bounded partitions of whole subjects.
//! Its output is a row per thousand records, so what a run holds besides the
//! input's bytes is the partition it decodes and runs. The suite's allocator
//! counts the bytes the run's thread holds (`support::peak`), and the run's
//! token reads them at every poll: from the admission poll on, once the
//! input's bytes are held, to the run's end, the most it holds beyond them
//! is the run's peak.
//!
//! A run over ten times the records or more, in partitions of the same
//! size, peaks below twice the peak of the run over one partition. A run
//! that held every partition, or any table of the whole input, would hold
//! ten partitions' tables at once, and the six text fields every record
//! carries, which the specification never reads, make one partition's
//! table a large share of what a run over one partition holds: ten of them
//! are more than that run's peak twice over. The larger input of the second
//! test is beyond the row limit, more than a whole-table run admits.
//!
//! The seam is the facade's partitioned engine mode with an allocator that
//! counts, which neither the benchmark reader nor the host fixtures have.
use std::io::Write as _;
use std::num::NonZeroUsize;
use std::sync::{Arc, Mutex};

use crate::support::{peak, Scratch};
use yamaa::{CancellationToken, Engine, EngineOptions, HostLanguage, Partitions};

/// A row per subject: the records it has and the total of their AVAL.
const SPEC: &str = "yamaa: \"1.0\"\ntype: spec\ndomain: TEST\nkeys: [STUDYID, USUBJID]\ninput: {LB: {path: input.csv, types: {SEQ: int, AVAL: float}}}\noutput: {path: out.csv, columns: [STUDYID, USUBJID, RECORDS, TOTAL]}\ncolumns:\n  - {name: STUDYID, type: str, derivation: LB.STUDYID}\n  - {name: USUBJID, type: str, derivation: LB.USUBJID}\n  - {name: RECORDS, type: int}\n  - {name: TOTAL, type: float}\nrows:\n  - id: subjects\n    group_by: [LB.STUDYID, LB.USUBJID]\n    derivations:\n      RECORDS: {aggregate: \"COUNT(LB.SEQ)\"}\n      TOTAL: {aggregate: \"SUM(LB.AVAL)\"}\n";

/// The records each subject has.
const RECORDS: usize = 1_000;

/// Text fields the specification never reads, which every partition's
/// table holds.
const NOTES: [&str; 6] = ["NOTE1", "NOTE2", "NOTE3", "NOTE4", "NOTE5", "NOTE6"];

/// Write `records` records, a thousand for each subject, beside the
/// specification in `scratch`: the specification's path and the input's
/// bytes.
fn stage(scratch: &Scratch, records: usize) -> (String, usize) {
    scratch.write("spec.yaml", SPEC);
    let path = scratch.join("input.csv");
    let mut file = std::io::BufWriter::new(std::fs::File::create(&path).unwrap());
    writeln!(file, "STUDYID,USUBJID,SEQ,AVAL,{}", NOTES.join(",")).unwrap();
    for record in 0..records {
        write!(file, "S1,{:08},{},1.5", record / RECORDS, record % RECORDS).unwrap();
        for note in NOTES {
            write!(file, ",{note} {:04}", record % 9_973).unwrap();
        }
        writeln!(file).unwrap();
    }
    file.flush().unwrap();
    drop(file);
    let bytes = std::fs::metadata(&path).unwrap().len();
    let spec = scratch.join("spec.yaml");
    (
        spec.to_str().unwrap().to_owned(),
        usize::try_from(bytes).unwrap(),
    )
}

/// Run the specification over `records` records in key-bounded partitions
/// of at most `rows` records: the most bytes the run held beyond the
/// input's, from its admission poll to its end.
fn measured(records: usize, rows: usize) -> isize {
    let scratch = Scratch::new("partitioned-memory");
    let (spec, bytes) = stage(&scratch, records);
    let bytes = isize::try_from(bytes).unwrap();
    let partitions = Partitions::by_keys(
        NonZeroUsize::new(2).unwrap(),
        NonZeroUsize::new(rows).unwrap(),
    );
    let engine = Engine::new(
        EngineOptions::new(HostLanguage::Python, "yamaa".into(), "0".into())
            .with_partitions(partitions),
    );
    // Each poll ends a count: the bytes held then, and the most held since
    // the poll before.
    let polls = Arc::new(Mutex::new(Vec::new()));
    let seen = Arc::clone(&polls);
    let token = CancellationToken::with_probe(move || {
        seen.lock().unwrap().push((peak::live(), peak::take_peak()));
        false
    });
    peak::take_peak();
    let run = engine.spec(&spec).cancel_with(&token).run().unwrap();
    let last = peak::take_peak();
    let rows_out = run
        .output()
        .unwrap_or_else(|| panic!("{records} records: {:?}", run.diagnostics()))
        .rows();
    assert_eq!(
        rows_out,
        records / RECORDS,
        "{records} records: a row per subject"
    );
    drop(run);
    let polls = polls.lock().unwrap();
    // The admission poll is the first to find the input's bytes held.
    let admitted = polls
        .iter()
        .position(|&(live, _)| live >= bytes)
        .unwrap_or_else(|| panic!("{records} records: no poll after the input was read"));
    polls[admitted + 1..]
        .iter()
        .map(|&(_, peak)| peak)
        .chain(std::iter::once(last))
        .max()
        .unwrap()
        - bytes
}

/// Ten times the records of one partition, in partitions of the same size,
/// against the one partition: 10,000 and 100,000 records in partitions of
/// 10,000, which every lane runs.
#[test]
fn a_run_over_ten_partitions_holds_no_more_than_one_partition_at_a_time() {
    let one = measured(10_000, 10_000);
    let ten = measured(100_000, 10_000);
    assert!(
        ten < 2 * one,
        "ten partitions peak at {ten} bytes, one at {one}"
    );
}

/// The same over 10,001,000 records, beyond the row limit, and over a
/// million: partitions of a million records, each of whole subjects and
/// within the limit, the last of a thousand.
/// The input and the runs take minutes unoptimized, so the test runs in the
/// optimized `release-test` profile, as the native lane runs every test, and
/// no other test beyond the row limit runs beside it
/// (`support::beyond_the_row_limit`).
#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "10,001,000-record inputs run in the optimized release-test profile"
)]
fn a_run_over_an_input_beyond_whole_holds_no_more_than_one_partition_at_a_time() {
    let _alone = crate::support::beyond_the_row_limit();
    let one = measured(1_000_000, 1_000_000);
    let ten = measured(yamaa_core::limits::Limits::TABLE_ROWS + RECORDS, 1_000_000);
    assert!(
        ten < 2 * one,
        "ten partitions peak at {ten} bytes, one at {one}"
    );
}
