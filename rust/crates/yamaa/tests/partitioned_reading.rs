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
//! captured file. The output is admitted, encoded and saved one partition
//! at a time too (#3793), and its saved file holds a line for every
//! subject, the last ones the subset run's.
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

/// The lines of the file at `path` and its last `count` lines, read without
/// holding the file.
fn tail(path: &str, count: usize) -> (usize, Vec<String>) {
    use std::io::BufRead as _;
    let mut lines = 0;
    let mut last = std::collections::VecDeque::with_capacity(count + 1);
    for line in std::io::BufReader::new(std::fs::File::open(path).unwrap()).lines() {
        lines += 1;
        last.push_back(line.unwrap());
        if last.len() > count {
            last.pop_front();
        }
    }
    (lines, last.into())
}

/// The output file a run's save wrote: the last path it returns, after the
/// run report a specification may declare (OUTPUT-10).
fn saved_output(run: &mut yamaa::Run) -> String {
    let token = yamaa::CancellationToken::new();
    run.save(&token).unwrap().pop().unwrap()
}

/// Run each partition-safe shape over `records` generated
/// driver records, in partitions of `rows`: for its first and its last
/// subjects, the output holds the rows a whole-table run of only their
/// records gives, and its saved file holds a header and a line for every
/// subject, the last ones those of the whole-table run's saved file.
fn check_shapes(records: usize, rows_per_partition: usize) {
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
        let mut run = engine(Some(partitions)).spec(&spec).run().unwrap();
        let output = run.output().unwrap_or_else(|| {
            panic!("{case}: {:?}", run.diagnostics());
        });
        assert_eq!(output.rows(), subjects, "{case}");
        // The whole-table run of the first and the last subjects alone.
        let subset = Scratch::new("partitioned-reading-subset");
        let edges = (1..=EDGE).chain(subjects - EDGE + 1..=subjects);
        let subset_spec = stage(&subset, &shape, edges);
        let mut whole = engine(None).spec(&subset_spec).run().unwrap();
        let expected = whole
            .output()
            .unwrap_or_else(|| panic!("{case}: the subset's {:?}", whole.diagnostics()));
        assert_eq!(output.schema(), expected.schema(), "{case}");
        assert_eq!(
            rows(output.batches(), (0..EDGE).chain(subjects - EDGE..subjects)),
            rows(expected.batches(), 0..2 * EDGE),
            "{case}"
        );
        let (_, expected) = tail(&saved_output(&mut whole), EDGE);
        assert_eq!(
            tail(&saved_output(&mut run), EDGE),
            (subjects + 1, expected),
            "{case}: the saved file"
        );
    }
}

/// Inputs of 10,002 driver records in partitions of 1,000, which every lane
/// runs: each partition is read on its own and the output is the
/// whole-table run's.
#[test]
fn every_partition_safe_shape_reads_its_input_by_partition() {
    check_shapes(10_002, 1_000);
}

/// An input of 10,000,002 driver records, beyond the row limit, in
/// partitions of 1,000,000 (#3792): every shape runs, each with a row for
/// every record included (#3870). The run holds its output once, a column
/// at a time in the executor's buffers, beside its host view and its encoded
/// bytes, and its observations encode the output only when they are
/// serialized, so a native runner holds it. The input and the run take
/// several gigabytes and minutes unoptimized, so the test runs in the
/// optimized `release-test` profile, as the native lane runs every test, and
/// no other test beyond the row limit runs beside it
/// (`support::beyond_the_row_limit`).
#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "10,000,002-record inputs run in the optimized release-test profile"
)]
fn every_partition_safe_shape_reads_a_driver_input_beyond_the_row_limit_by_partition() {
    let _alone = crate::support::beyond_the_row_limit();
    check_shapes(yamaa_core::limits::Limits::TABLE_ROWS + 2, 1_000_000);
}

/// A subject-keyed window over 10,000,001 records, beyond the row limit,
/// runs over key-bounded partitions of at most 1,000,000 records (#3870):
/// each subject's ten records, SEQ 1 to 10 in order, number 1 to 10, and
/// the last record is a subject of its own. The output holds a row for
/// every record, in input order, and its saved file a header and a line for
/// each, the last three those of the subjects 00999999 and 01000000. It runs
/// in the optimized `release-test` profile, alone, as the test above does.
#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "10,000,001-record inputs run in the optimized release-test profile"
)]
fn a_subject_keyed_window_beyond_the_row_limit_runs_by_key_bounded_partition() {
    const SPEC: &str = "yamaa: \"1.0\"\ntype: spec\ndomain: TEST\nkeys: [ID, SEQ]\ninput: {SRC: {path: input.csv, types: {SEQ: int}}}\noutput: {path: out.csv, columns: [ID, SEQ, RN]}\ncolumns:\n  - {name: ID, type: str, derivation: SRC.ID}\n  - {name: SEQ, type: int, derivation: SRC.SEQ}\n  - {name: RN, type: int, derivation: {row_number: {window: {group_by: [ID], order_by: [SEQ]}}}}\n";
    let _alone = crate::support::beyond_the_row_limit();
    let records = yamaa_core::limits::Limits::TABLE_ROWS + 1;
    let scratch = Scratch::new("partitioned-reading-window");
    scratch.write("spec.yaml", SPEC);
    let mut file =
        std::io::BufWriter::new(std::fs::File::create(scratch.join("input.csv")).unwrap());
    writeln!(file, "ID,SEQ").unwrap();
    for record in 0..records {
        writeln!(file, "{:08},{}", record / 10, record % 10 + 1).unwrap();
    }
    file.flush().unwrap();
    drop(file);
    let spec = scratch.join("spec.yaml");
    let partitions = Partitions::by_keys(NonZeroUsize::MIN, NonZeroUsize::new(1_000_000).unwrap());
    let mut run = engine(Some(partitions))
        .spec(spec.to_str().unwrap())
        .run()
        .unwrap();
    let rows = run
        .output()
        .unwrap_or_else(|| panic!("{:?}", run.diagnostics()))
        .rows();
    assert_eq!(rows, records, "a row for every record");
    assert_eq!(
        tail(&saved_output(&mut run), 3),
        (
            records + 1,
            vec![
                "00999999,9,9".to_owned(),
                "00999999,10,10".to_owned(),
                "01000000,1,1".to_owned(),
            ]
        ),
        "the saved file"
    );
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

/// The observations of a run that read its input by partition are the
/// whole-table run's, the source's records included: the run holds no copy
/// of them, and they are decoded again from the held bytes, a partition at
/// a time, when the observations are serialized (#3830).
#[test]
fn a_partitioned_runs_observations_hold_the_whole_reads_source_records() {
    let partitions = Partitions::new(NonZeroUsize::new(7).unwrap());
    for shape in shapes() {
        let scratch = Scratch::new("partitioned-reading-observations");
        let spec = stage(&scratch, &shape, 1..=25);
        let divided = engine(Some(partitions)).spec(&spec).run().unwrap();
        let whole = engine(None).spec(&spec).run().unwrap();
        assert_eq!(
            divided.observations(),
            whole.observations(),
            "{}",
            shape.benchmark
        );
    }
}

/// A required ODM field missing from records in two partitions refuses the
/// partitioned run with the whole read's one finding: the field, its first
/// missing record, the header being record 1, and the number of records it
/// is missing from, every partition's (NAME-9, #3830). The fifth and the
/// seventh records, the second and the fourth data records, lack a subject
/// key, in the first and the second partitions of two records.
#[test]
fn an_odm_field_missing_in_two_partitions_is_the_whole_reads_one_finding() {
    let spec = "yamaa: \"1.0\"\ntype: spec\ndomain: TEST\nkeys: [ID]\ninput:\n  ODM: input.csv\noutput: {path: out.csv, columns: [ID, AGE]}\ncolumns:\n  - {name: ID, type: str, derivation: ODM.SubjectKey}\n  - {name: AGE, type: str, derivation: {odm: ODM.IT.AGE}}\n";
    let mut csv = String::from(
        "StudyOID,MetaDataVersionOID,SubjectKey,StudyEventOID,StudyEventRepeatKey,FormOID,FormRepeatKey,ItemGroupOID,ItemGroupRepeatKey,ItemOID,Value\n",
    );
    for (subject, key) in ["1", "", "3", "", "5"].into_iter().enumerate() {
        writeln!(csv, "S,M,{key},E,,F,,G,,IT.AGE,{}", 30 + subject).unwrap();
    }
    let scratch = Scratch::new("partitioned-reading-odm");
    scratch.write("spec.yaml", spec);
    scratch.write("input.csv", csv.as_bytes());
    let spec = scratch.join("spec.yaml");
    let spec = spec.to_str().unwrap();
    let partitions = Partitions::new(NonZeroUsize::new(2).unwrap());
    let divided = engine(Some(partitions)).spec(spec).run().unwrap();
    let whole = engine(None).spec(spec).run().unwrap();
    assert_eq!(divided.diagnostics(), whole.diagnostics(), "the issues");
    let issues: Vec<(String, String, Vec<String>, String)> = divided
        .diagnostics()
        .iter()
        .map(|issue| {
            (
                issue.phase.as_str().to_owned(),
                issue.code.as_str().to_owned(),
                issue.spec_path.clone(),
                issue.context.to_json().to_owned(),
            )
        })
        .collect();
    assert_eq!(
        issues,
        vec![(
            "ingest".to_owned(),
            "odm_schema_value_missing".to_owned(),
            vec!["input.ODM.path".to_owned()],
            r#"{"dataset":"ODM","field":"SubjectKey","record":3,"records":2}"#.to_owned(),
        )],
        "the partitioned run's issues"
    );
}
