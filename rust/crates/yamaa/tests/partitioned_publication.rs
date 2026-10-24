#![cfg(feature = "yaml")]
//! Partitioned publication (#3793, D264): a run divided into partitions
//! admits, encodes and saves its output one partition at a time, each
//! partition as large as the run's largest driver partition, and its saved
//! files and run report are the whole-table run's.
//!
//! Every single-specification benchmark already saves its files over
//! one-record partitions in `partitioned_corpus.rs`, so each output is
//! written one record at a time there. Here the benchmarks that save Parquet
//! save it in partitions of three records, which divide no row group of the
//! writer's 1,024 rows, and a run whose check fires in two partitions saves
//! the whole-table run report, authored from the run report's columns. An
//! output beyond the row limit is `partitioned_reading.rs`'s.
//!
//! The seam is the facade's partitioned engine mode, which neither the
//! benchmark reader nor the host fixtures run before #3794.
use std::collections::BTreeMap;
use std::num::NonZeroUsize;
use std::path::Path;

use crate::support::corpus::{cases, stage};
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

/// Every single-specification benchmark that saves a Parquet file saves its
/// files in partitions of three records with the whole-table run's bytes,
/// its output having more rows than a partition holds (D160, D264).
#[cfg(feature = "parquet")]
#[test]
fn a_benchmark_saves_its_parquet_files_in_partitions_that_divide_no_row_group() {
    let three = Partitions::new(NonZeroUsize::new(3).unwrap());
    let mut compared = Vec::new();
    for case in cases() {
        let expected = case.join("expected");
        let parquet = std::fs::read_dir(&expected).unwrap().any(|file| {
            file.unwrap()
                .file_name()
                .to_string_lossy()
                .ends_with(".parquet")
        });
        if !parquet || expected.join("error.yaml").is_file() {
            continue;
        }
        let name = case.file_name().unwrap().to_string_lossy().into_owned();
        let (whole, divided) = (
            Scratch::new("partitioned-publication-whole"),
            Scratch::new("partitioned-publication"),
        );
        assert!(stage(&case, &whole) && stage(&case, &divided), "{name}");
        let (rows, files) = saved(&whole.join("spec.yaml"), None);
        let (divided_rows, divided_files) = saved(&divided.join("spec.yaml"), Some(three));
        assert_eq!(
            (divided_rows > 3, divided_rows, divided_files),
            (true, rows, files),
            "{name}: partitions of three records"
        );
        compared.push(name);
    }
    assert!(
        compared.iter().any(|name| name == "schema-parquet"),
        "{compared:?}"
    );
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
