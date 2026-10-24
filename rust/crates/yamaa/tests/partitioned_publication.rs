#![cfg(feature = "yaml")]
//! Partitioned publication (#3793, D264): a run divided into partitions
//! admits, encodes and saves its output one partition at a time, each
//! partition as large as the run's largest driver partition, and its saved
//! files and run report are the whole-table run's.
//!
//! Every single-specification benchmark already saves its files over
//! one-record partitions in `partitioned_corpus.rs`, so each output is
//! written one record at a time there. Here a Parquet output of several row
//! groups is saved in partitions that divide no row group of the writer's
//! 1,024 rows, and a run whose check fires in two partitions saves the
//! whole-table run report, authored from the run report's columns. An
//! output beyond the row limit is `partitioned_reading.rs`'s.
//!
//! The seam is the facade's partitioned engine mode, which neither the
//! benchmark reader nor the host fixtures run before #3794.
use std::collections::BTreeMap;
use std::num::NonZeroUsize;
use std::path::Path;

use crate::support::scratch::Scratch;
use yamaa::{CancellationToken, Engine, EngineOptions, HostLanguage, Partitions};

fn engine(partitions: Option<Partitions>) -> Engine {
    let options = EngineOptions::new(HostLanguage::Python, "yamaa".into(), "0".into());
    Engine::new(match partitions {
        Some(partitions) => options.with_partitions(partitions),
        None => options,
    })
}

/// Run `spec` on an engine over `partitions`, or whole, and save it: the
/// output's rows, and the bytes of each file the save wrote, by name.
fn saved(spec: &Path, partitions: Option<Partitions>) -> (usize, BTreeMap<String, Vec<u8>>) {
    let spec = spec.to_str().unwrap();
    let mut run = engine(partitions).spec(spec).run().unwrap();
    let rows = run
        .output()
        .unwrap_or_else(|| panic!("{spec}: {:?}", run.diagnostics()))
        .rows();
    let files = run
        .save(&CancellationToken::new())
        .unwrap_or_else(|failure| panic!("{spec}: {}", failure.message))
        .into_iter()
        .map(|path| {
            let name = Path::new(&path).file_name().unwrap().to_string_lossy();
            (name.into_owned(), std::fs::read(&path).unwrap())
        })
        .collect();
    (rows, files)
}

/// A Parquet output of 2,500 rows saved in partitions of seven records, a
/// size that divides no row group of the writer's 1,024 rows, has the
/// whole-table run's bytes, its row groups of 1,024, 1,024 and 452 rows
/// included (D160, D264). The corpus's one Parquet benchmark,
/// `schema-parquet`, has two output rows, fewer than any such size, and
/// saves its files over one-record partitions in `partitioned_corpus.rs`.
#[cfg(feature = "parquet")]
#[test]
fn a_parquet_output_saved_in_partitions_that_divide_no_row_group_has_the_whole_tables_bytes() {
    use std::fmt::Write as _;
    const SPEC: &str = "yamaa: \"1.0\"\ntype: spec\ndomain: TEST\nkeys: [ID]\ninput: {SRC: {path: input.csv, types: {ID: int, V: float}}}\noutput: {path: out.parquet, columns: [ID, NAME, V]}\ncolumns:\n  - {name: ID, type: int, derivation: SRC.ID}\n  - {name: NAME, type: str, derivation: SRC.NAME}\n  - {name: V, type: float, derivation: SRC.V}\n";
    let mut input = String::from("ID,NAME,V\n");
    for id in 1..=2_500 {
        writeln!(input, "{id},name {id},{}.5", id % 97).unwrap();
    }
    let seven = Partitions::new(NonZeroUsize::new(7).unwrap());
    let [whole, divided] = [None, Some(seven)].map(|partitions| {
        let scratch = Scratch::new("partitioned-publication-parquet");
        scratch.write("spec.yaml", SPEC);
        scratch.write("input.csv", &input);
        saved(&scratch.join("spec.yaml"), partitions)
    });
    assert_eq!(whole.0, 2_500, "the whole-table run's rows");
    assert_eq!(divided, whole, "partitions of seven records");
}

/// A range check on AGE fires for ID 1, aged 17, and ID 4, aged 101: in each
/// of the two partitions of two records. The partitioned run saves the
/// whole-table run's output and run report byte for byte, the check's row
/// counting both failures in record order. The report is written from its
/// columns: one run row counting four output rows; the input, its 28 bytes
/// read as four records; the base rows, four emitted from four read; a row
/// for each output column, none missing; and the check, violated by two of
/// the four rows it evaluated, with their keys and the column it checks.
#[test]
fn a_check_that_fires_in_two_partitions_saves_the_whole_table_report() {
    const SPEC: &str = "yamaa: \"1.0\"\ntype: spec\ndomain: TEST\nkeys: [ID]\ninput: {SRC: {path: input.csv, types: {ID: int, AGE: int}}}\noutput: {path: out.csv, columns: [ID, AGE], report: report.csv}\ncolumns:\n  - {name: ID, type: int, derivation: SRC.ID}\n  - name: AGE\n    type: int\n    derivation: SRC.AGE\n    verifications:\n      - range: {min: 18, max: 100, severity: warning}\n";
    const INPUT: &str = "ID,AGE\n1,17\n2,40\n3,50\n4,101\n";
    let report = concat!(
        "ARTIFACT,KIND,SPEC_PATH,NAME,ID,TARGET,RULE,SEVERITY,OUTCOME,CONDITION,COUNT,TOTAL,KEYS,DETAILS\n",
        "out.csv,run,output,,,,,,succeeded,,4,,,{}\n",
        "out.csv,input,input.SRC,SRC,,,,,,,4,,,\"{\"\"bytes\"\":28,\"\"path\"\":\"\"input.csv\"\",\"\"profile\"\":\"\"csv\"\"}\"\n",
        "out.csv,rows,base,SRC,,,,,,,4,4,,{}\n",
        "out.csv,column,columns.ID,ID,,,,,,,0,4,,{}\n",
        "out.csv,column,columns.AGE,AGE,,,,,,,0,4,,{}\n",
        "out.csv,check,columns.AGE.verifications[0].range,range,,AGE,CHECK-7,warning,violated,range_failed,2,4,\"[{\"\"ID\"\":1},{\"\"ID\"\":4}]\",\"{\"\"column\"\":\"\"AGE\"\"}\"\n",
    );
    let expected = BTreeMap::from([
        ("out.csv".to_owned(), INPUT.as_bytes().to_vec()),
        ("report.csv".to_owned(), report.as_bytes().to_vec()),
    ]);
    let two = Partitions::new(NonZeroUsize::new(2).unwrap());
    for (label, partitions) in [
        ("the whole-table run", None),
        ("partitions of two", Some(two)),
    ] {
        let scratch = Scratch::new("partitioned-publication-report");
        scratch.write("spec.yaml", SPEC);
        scratch.write("input.csv", INPUT);
        assert_eq!(
            saved(&scratch.join("spec.yaml"), partitions),
            (4, expected.clone()),
            "{label}"
        );
    }
}

/// An output of 10,000,002 rows, beyond the row limit, saves in partitions of
/// 1,000,000 records (D264): one keyed column copied from an input of as
/// many records, read by partition (#3792), each record its own row, so the
/// saved file has a header and 10,000,002 lines, the last two the last two
/// records' IDs. The one column keeps the run near 3 GB; the input and the
/// run take minutes unoptimized, so the test runs in the optimized
/// `release-test` profile, as the native lane runs every test.
#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "a 10,000,002-row output runs in the optimized release-test profile"
)]
fn an_output_beyond_the_row_limit_saves_a_partition_at_a_time() {
    use std::io::Write as _;
    const SPEC: &str = "yamaa: \"1.0\"\ntype: spec\ndomain: TEST\nkeys: [ID]\ninput: {SRC: {path: input.csv, types: {ID: int}}}\noutput: {path: out.csv, columns: [ID]}\ncolumns:\n  - {name: ID, type: int, derivation: SRC.ID}\n";
    let records = yamaa_core::limits::Limits::TABLE_ROWS + 2;
    let scratch = Scratch::new("partitioned-publication-limit");
    scratch.write("spec.yaml", SPEC);
    let mut input =
        std::io::BufWriter::new(std::fs::File::create(scratch.join("input.csv")).unwrap());
    writeln!(input, "ID").unwrap();
    for id in 1..=records {
        writeln!(input, "{id}").unwrap();
    }
    input.flush().unwrap();
    drop(input);
    let million = Partitions::new(NonZeroUsize::new(1_000_000).unwrap());
    let (rows, mut files) = saved(&scratch.join("spec.yaml"), Some(million));
    let saved = String::from_utf8(files.remove("out.csv").unwrap()).unwrap();
    let last = saved.lines().rev().take(2).collect::<Vec<_>>();
    assert_eq!(
        (rows, saved.lines().count(), last),
        (records, records + 1, vec!["10000002", "10000001"]),
        "the output's rows, the saved file's lines and its last two"
    );
}
