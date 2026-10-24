#![cfg(all(feature = "yaml", feature = "arrow"))]
//! Partitioned input reading (#3792): on an engine with partitions, a CSV
//! driver input of a partition-safe specification is read a range of
//! records at a time, each partition admitted on its own, so the row limit
//! bounds a partition rather than the input.
//!
//! Each partition-safe shape of #3697's corpus (`partitioned_corpus.rs`)
//! runs over an input the test generates in a scratch directory, and its
//! output is compared with a whole-table run over a subset of the same
//! records: the first and the last subjects. Nothing is compared with a
//! captured file. A shape whose output has a row for every record still
//! refuses beyond the row limit at its output, which this issue leaves
//! whole: the refusal names the output, not the input.
//!
//! The seam is the facade's partitioned engine mode, which neither the
//! benchmark reader nor the host fixtures run before #3794, and inputs
//! beyond the row limit, which no fixture can hold.
use std::fmt::Write as _;
use std::io::Write as _;
use std::num::NonZeroUsize;
use std::path::Path;

use arrow_array::RecordBatch;

use crate::support::scratch::Scratch;
use yamaa::{Engine, EngineOptions, HostLanguage, Partitions};

/// One partition-safe shape of the corpus: the benchmark whose
/// specification runs, the input file it reads with its header, and the
/// records of one subject.
struct Shape {
    benchmark: &'static str,
    input: &'static str,
    header: &'static str,
    /// The records each subject has.
    records: usize,
    /// Append the records of subject `subject`, counted from 1.
    write: fn(usize, &mut String),
}

fn shapes() -> [Shape; 3] {
    [
        Shape {
            benchmark: "adam-adsl-analysis-age",
            input: "input/dm.csv",
            header: "STUDYID,USUBJID,BRTHDT,RANDDT",
            records: 1,
            write: |subject, out| {
                let (year, month, day) = (1950 + subject % 50, 1 + subject % 12, 1 + subject % 28);
                writeln!(out, "S1,{subject:08},{year}-{month:02}-{day:02},2020-06-15").unwrap();
            },
        },
        Shape {
            benchmark: "schema-adsl-age-quality",
            input: "input/dm.csv",
            header: "STUDYID,USUBJID,AGE",
            records: 1,
            write: |subject, out| writeln!(out, "S1,{subject:08},{}", 10 + subject % 100).unwrap(),
        },
        // The three items the specification maps, and seven it reads past,
        // as an extract of a study's items holds them.
        Shape {
            benchmark: "sdtm-dm-basic",
            input: "input/odm.csv",
            header: "StudyOID,SubjectKey,ItemOID,Value",
            records: 10,
            write: |subject, out| {
                let sex = ["Male", "Female", "Unknown"][subject % 3];
                let arm = ["Placebo", "Vitamin D3"][subject % 2];
                writeln!(out, "S1,{subject:08},IT.DM.SEX,{sex}").unwrap();
                writeln!(out, "S1,{subject:08},IT.DM.AGE,{}", 20 + subject % 60).unwrap();
                writeln!(out, "S1,{subject:08},IT.DM.ARM,{arm}").unwrap();
                for item in [
                    "RACE", "ETHNIC", "COUNTRY", "SITEID", "BRTHDTC", "RFSTDTC", "DTHFL",
                ] {
                    writeln!(out, "S1,{subject:08},IT.DM.{item},{}", subject % 7).unwrap();
                }
            },
        },
    ]
}

fn benchmarks() -> &'static Path {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../../benchmarks"))
}

/// Write `shape`'s input of `subjects` into `directory` beside its
/// specification, and return the specification's path.
fn stage(directory: &Path, shape: &Shape, subjects: impl Iterator<Item = usize>) -> String {
    let spec = directory.join("spec.yaml");
    std::fs::copy(benchmarks().join(shape.benchmark).join("spec.yaml"), &spec).unwrap();
    let path = directory.join(shape.input);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let mut file = std::io::BufWriter::new(std::fs::File::create(path).unwrap());
    writeln!(file, "{}", shape.header).unwrap();
    let mut records = String::new();
    for subject in subjects {
        (shape.write)(subject, &mut records);
        if records.len() > 1 << 20 {
            file.write_all(records.as_bytes()).unwrap();
            records.clear();
        }
    }
    file.write_all(records.as_bytes()).unwrap();
    file.flush().unwrap();
    spec.to_str().unwrap().to_owned()
}

fn engine(partitions: Option<Partitions>) -> Engine {
    let options = EngineOptions::new(HostLanguage::Python, "yamaa".into(), "0".into());
    Engine::new(match partitions {
        Some(partitions) => options.with_partitions(partitions),
        None => options,
    })
}

/// The rows `rows` of `batches`, one batch each.
fn rows(batches: &[RecordBatch], rows: impl Iterator<Item = usize>) -> Vec<RecordBatch> {
    let mut starts = Vec::with_capacity(batches.len());
    let mut start = 0;
    for batch in batches {
        starts.push(start);
        start += batch.num_rows();
    }
    rows.map(|row| {
        let batch = starts.partition_point(|&start| start <= row) - 1;
        batches[batch].slice(row - starts[batch], 1)
    })
    .collect()
}

/// The first issue of a run that refused, as its phase, condition, spec
/// paths and context.
type Issue = (String, String, Vec<String>, String);

/// Run every partition-safe shape over `records` generated driver records,
/// in partitions of `rows`. A shape whose output fits the row limit gives,
/// for its first and its last subjects, the rows a whole-table run of only
/// their records gives; one whose output does not fit refuses at its output.
fn check_every_shape(records: usize, rows_per_partition: usize) {
    const EDGE: usize = 3;
    let partitions = Partitions::new(NonZeroUsize::new(rows_per_partition).unwrap());
    for shape in shapes() {
        let subjects = records.div_ceil(shape.records);
        let case = format!(
            "{} over {} records",
            shape.benchmark,
            subjects * shape.records
        );
        let scratch = Scratch::new("partitioned-reading");
        let spec = stage(&scratch, &shape, 1..=subjects);
        let run = engine(Some(partitions)).spec(&spec).run().unwrap();
        if subjects > yamaa_core::limits::Limits::TABLE_ROWS {
            // One output row per subject, beyond the row limit: the input is
            // admitted a partition at a time, and the output refuses whole.
            let first = run.diagnostics().first().map(|issue| -> Issue {
                (
                    issue.phase.as_str().to_owned(),
                    issue.code.as_str().to_owned(),
                    issue.spec_path.clone(),
                    issue.context.to_json().to_owned(),
                )
            });
            let expected: Issue = (
                "engine".into(),
                "engine_rejected".into(),
                Vec::new(),
                format!(
                    r#"{{"actual":{subjects},"code":"execution_limit","limit":10000000,"resource":"output_rows","step":"execute"}}"#
                ),
            );
            assert_eq!(first, Some(expected), "{case}");
            continue;
        }
        let output = run.output().unwrap_or_else(|| {
            panic!("{case}: {:?}", run.diagnostics());
        });
        assert_eq!(output.rows(), subjects, "{case}");
        // The whole-table run of the first and the last subjects alone.
        let subset = Scratch::new("partitioned-reading-subset");
        let edges = (1..=EDGE).chain(subjects - EDGE + 1..=subjects);
        let subset_spec = stage(&subset, &shape, edges);
        let whole = engine(None).spec(&subset_spec).run().unwrap();
        let expected = whole
            .output()
            .unwrap_or_else(|| panic!("{case}: the subset's {:?}", whole.diagnostics()));
        assert_eq!(output.schema(), expected.schema(), "{case}");
        assert_eq!(
            rows(output.batches(), (0..EDGE).chain(subjects - EDGE..subjects)),
            rows(expected.batches(), 0..2 * EDGE),
            "{case}"
        );
    }
}

/// Inputs of 10,002 driver records in partitions of 1,000, which every lane
/// runs: each partition is read on its own and the output is the
/// whole-table run's.
#[test]
fn every_partition_safe_shape_reads_its_input_by_partition() {
    check_every_shape(10_002, 1_000);
}

/// Inputs of 10,000,002 driver records, beyond the row limit, in partitions
/// of 1,000,000 (#3792): the grouped shape runs, and the shapes with a row
/// for every record refuse at their output. The inputs and the runs take
/// several gigabytes and minutes unoptimized, so the test runs in the
/// optimized `release-test` profile, as the native lane runs every test.
#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "10,000,002-record inputs run in the optimized release-test profile"
)]
fn a_driver_input_beyond_the_row_limit_runs_by_partition_on_every_partition_safe_shape() {
    check_every_shape(yamaa_core::limits::Limits::TABLE_ROWS + 2, 1_000_000);
}

/// A defect in a record past the first partition of two refuses the
/// partitioned run with the whole-table run's issues, context included:
/// the record counts the header, so the fifth record is the fourth data
/// record, in the second partition.
#[test]
fn a_malformed_record_past_the_first_partition_reports_the_whole_table_runs_issues() {
    let spec = "yamaa: \"1.0\"\ntype: spec\ndomain: TEST\nkeys: [ID]\ninput:\n  SRC: {path: input.csv, types: {N: int}}\noutput: {path: out.csv, columns: [ID, N]}\ncolumns:\n  - {name: ID, type: str, derivation: SRC.ID}\n  - {name: N, type: int, derivation: SRC.N}\n";
    let csv = |condition, rule, field| {
        (
            condition,
            Some(rule),
            "input.SRC.path",
            format!(r#"{{"dataset":"SRC","field":{field},"path":"input.csv","record":5}}"#),
        )
    };
    for (input, (condition, rule, path, context)) in [
        (
            &b"ID,N\n1,1\n2,2\n3,3\n4,\"4\n"[..],
            csv("source_quote_unterminated", "CSV-16", 2),
        ),
        (
            b"ID,N\n1,1\n2,2\n3,3\n4,4,4\n",
            csv("source_record_width", "CSV-16", 3),
        ),
        (
            b"ID,N\n1,1\n2,2\n3,3\n4,\xff\n",
            csv("invalid_text", "CSV-10", 2),
        ),
        (
            b"ID,N\n1,1\n2,2\n3,3\n4,four\n",
            (
                "field_parse_failed",
                Some("INPUT-7"),
                "input.SRC.types.N",
                r#"{"dataset":"SRC","field":"N","type":"int","value":"four"}"#.to_owned(),
            ),
        ),
    ] {
        let scratch = Scratch::new("partitioned-reading-defect");
        scratch.write("spec.yaml", spec);
        scratch.write("input.csv", input);
        let spec = scratch.join("spec.yaml");
        let spec = spec.to_str().unwrap();
        let partitions = Partitions::new(NonZeroUsize::new(2).unwrap());
        let divided = engine(Some(partitions)).spec(spec).run().unwrap();
        let whole = engine(None).spec(spec).run().unwrap();
        assert_eq!(divided.diagnostics(), whole.diagnostics(), "{condition}");
        let issues = divided.diagnostics();
        let first = issues.first().unwrap();
        assert_eq!(
            (
                first.phase.as_str(),
                first.code.as_str(),
                first.rule.as_deref(),
                first.spec_path.clone(),
                first.context.to_json()
            ),
            (
                "ingest",
                condition,
                rule,
                vec![path.to_owned()],
                context.as_str()
            ),
            "{condition}"
        );
    }
}
