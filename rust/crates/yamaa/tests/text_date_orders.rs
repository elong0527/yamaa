#![cfg(feature = "yaml")]
//! Text date orders of the run report (CHECK-27) where the engine may
//! compare more, or fewer, texts than the rows of a run ask it to: the
//! report names what text order decided where a row asked, and nothing else.
//!
//! The seam is how the engine asks. It may answer a lookup's filter for
//! every donor record at once, matched or not; complete every partition of
//! a window, or filter the whole range of an output aggregate, whatever rows
//! read them; and let a second column take the records a first column's
//! selection chose. A run may also divide its driver input, and fail after
//! it derived. None of these may show in the report. A benchmark states one
//! spelling of one specification, and no fixture reader compares two
//! spellings or two divisions of it, so each case here states by hand the
//! rows every spelling and division must report.
use std::num::NonZeroUsize;

use crate::support::Scratch;
use yamaa::{CancellationToken, Engine, EngineOptions, HostLanguage, Partitions, Status};

/// A specification of domain TEST with `keys`, over `input`, that saves its
/// output and its run report: the `output` columns, each declared in
/// `columns`, with `intermediates`, if any, and the `tail` of `output`.
fn spec(keys: &str, input: &str, intermediates: &str, output: &str, columns: &str) -> String {
    format!(
        "yamaa: \"1.0\"\ntype: spec\ndomain: TEST\nkeys: [{keys}]\ninput:\n{input}output: {{path: out.csv, report: checks.csv, columns: [{output}]}}\n{intermediates}columns:\n{columns}"
    )
}

/// What a run saved: its output, or the condition, rule and paths of its
/// first issue when it built none, and the `check` rows of its run report.
type Saved = (Result<String, String>, Vec<String>);

/// Run `spec` over `files` on an engine with `partitions`, or whole, and
/// save it.
fn saved(spec: &str, files: &[(&str, &str)], partitions: Option<Partitions>) -> Saved {
    let scratch = Scratch::new("text-date-orders");
    scratch.write("spec.yaml", spec);
    for (name, bytes) in files {
        scratch.write(name, bytes);
    }
    let options = EngineOptions::new(HostLanguage::Python, "yamaa".into(), "0".into());
    let engine = Engine::new(match partitions {
        Some(partitions) => options.with_partitions(partitions),
        None => options,
    });
    let mut run = engine
        .spec(scratch.join("spec.yaml").to_str().unwrap())
        .run()
        .unwrap();
    // A failed run's save is refused once it has written the run report.
    let written = run.save(&CancellationToken::new());
    assert_eq!(
        written.is_ok(),
        run.status() == Status::Succeeded,
        "the save"
    );
    let output = match run.status() {
        Status::Succeeded => Ok(std::fs::read_to_string(scratch.join("out.csv")).unwrap()),
        _ => {
            let issues = run.diagnostics();
            let first = issues.first().expect("a failed run has an issue");
            Err(format!(
                "{} {:?} {:?}",
                first.code, first.rule, first.spec_path
            ))
        }
    };
    let checks = std::fs::read_to_string(scratch.join("checks.csv"))
        .unwrap()
        .lines()
        .filter(|line| line.starts_with("out.csv,check,"))
        .map(str::to_owned)
        .collect();
    (output, checks)
}

/// The warning of the place at `path`: one violated unit, and the least
/// pair text order decided there.
fn warning(path: &str, shorter: &str, longer: &str) -> String {
    format!(
        "out.csv,check,{path},text_date_order,,,CHECK-27,warning,violated,text_date_order,1,1,[{{}}],\"{{\"\"longer\"\":\"\"{longer}\"\",\"\"shorter\"\":\"\"{shorter}\"\"}}\""
    )
}

/// One sample each of three subjects.
const PC: &str = "USUBJID,PCSEQ\nS1,1\nS2,1\nS3,1\n";

/// The samples, then the doses, which a lookup reads by subject.
const SAMPLES: &str = "  PC: {path: pc.csv, types: {PCSEQ: int}}\n  EX: {path: ex.csv, key: [USUBJID], types: {EXSEQ: int}}\n";

/// A specification over the samples that copies each sample's subject and
/// sequence number and derives `derived`, the names after them in `output`.
fn samples(intermediates: &str, output: &str, derived: &str) -> String {
    spec(
        "USUBJID, PCSEQ",
        &format!("{SAMPLES}base: PC\n"),
        intermediates,
        &format!("USUBJID, PCSEQ, {output}"),
        &format!(
            "  - {{name: USUBJID, type: str, derivation: PC.USUBJID}}\n  - {{name: PCSEQ, type: int, derivation: PC.PCSEQ}}\n{derived}"
        ),
    )
}

/// One dose each of four subjects, its start and end as collected date
/// text. S0 has no sample, so no row matches its dose: its start begins its
/// end, the least such pair of the file, and its sequence number of 0 fails
/// a division. S1's dose is recorded to the minute at both ends. The starts
/// of S2's and S3's doses, recorded without a time, begin their ends; S3's
/// pair, the lesser, is the last record's, which only the last partition of
/// a divided run reads.
const ONE_DOSE: &str = "USUBJID,EXSEQ,EXSTDTC,EXENDTC\n\
    S0,0,2025-01-01,2025-01-01T08:00\n\
    S1,1,2025-03-10T08:00,2025-03-10T09:00\n\
    S2,1,2025-03-12,2025-03-12T09:00\n\
    S3,1,2025-03-11,2025-03-11T07:30\n";

/// Each sample's dose start, DOSE, through a named lookup narrowed by
/// `filter`.
fn named(filter: &str) -> String {
    samples(
        &format!(
            "intermediates:\n  - {{id: DOSE, dataset: EX, key: [USUBJID], filter: \"{filter}\"}}\n"
        ),
        "DOSE",
        "  - {name: DOSE, type: str, derivation: DOSE.EXSTDTC}\n",
    )
}

/// The same lookup as the structured source of the column.
fn inline(filter: &str) -> String {
    samples(
        "",
        "DOSE",
        &format!(
            "  - {{name: DOSE, type: str, derivation: {{source: {{variable: EX.EXSTDTC, filter: \"{filter}\"}}}}}}\n"
        ),
    )
}

/// Every sample takes its subject's one dose, whose start text does not
/// follow its end text, and the filter's warning names S3's pair: S0's dose
/// matches no sample, so the filter is never asked about it (LOOKUP-8),
/// neither where text order decides between its start and its end nor where
/// its sequence number divides by zero. That holds for the lookup named or
/// written as the column's source, for the division written before the
/// comparison or after it, and for a run over the whole input or over
/// partitions of one sample.
#[test]
fn a_lookup_filter_reports_what_text_order_decided_on_matched_records_alone() {
    let ordered = "EX.EXSTDTC <= EX.EXENDTC";
    let divided = "10 / EX.EXSEQ > 0";
    let output = "USUBJID,PCSEQ,DOSE\nS1,1,2025-03-10T08:00\nS2,1,2025-03-12\nS3,1,2025-03-11\n";
    let lookup = "intermediates.DOSE.filter";
    let source = "columns.DOSE.derivation.source.filter";
    // A label, the specification and the path of its filter.
    let cases: [(&str, String, &str); 5] = [
        ("a named lookup", named(ordered), lookup),
        (
            "a named lookup that compares, then divides",
            named(&format!("{ordered} AND {divided}")),
            lookup,
        ),
        (
            "a named lookup that divides, then compares",
            named(&format!("{divided} AND {ordered}")),
            lookup,
        ),
        ("a structured source", inline(ordered), source),
        (
            "a structured source that compares, then divides",
            inline(&format!("{ordered} AND {divided}")),
            source,
        ),
    ];
    let files = [("pc.csv", PC), ("ex.csv", ONE_DOSE)];
    let one = Partitions::new(NonZeroUsize::MIN);
    for (label, spec, path) in cases {
        let expected = (
            Ok(output.to_owned()),
            vec![warning(path, "2025-03-11", "2025-03-11T07:30")],
        );
        for (division, partitions) in [("the whole input", None), ("one record", Some(one))] {
            assert_eq!(
                saved(&spec, &files, partitions),
                expected,
                "{label}: partitions of {division}"
            );
        }
    }
}

/// Two columns read the last dose of each sample's subject by one
/// selection, ordered by the start text: FIRST its start and SECOND its
/// sequence number. S1's two doses are recorded to the minute, and S3 has
/// one dose. One of S2's doses has no time and begins the other's text, so
/// text order decides between them, in favour of the timed dose, at the
/// `order_by` of each column: a column that takes the records another
/// column's selection chose compares nothing for itself, and still reports
/// at its own path, over the whole input and over partitions of one sample,
/// where no two rows share a selection.
#[test]
fn a_selection_two_columns_share_reports_at_the_order_of_each() {
    let doses = "USUBJID,EXSEQ,EXSTDTC\n\
        S1,1,2025-03-10T08:00\n\
        S1,2,2025-03-11T08:00\n\
        S2,1,2025-03-12T06:00\n\
        S2,2,2025-03-12\n\
        S3,1,2025-03-13T07:00\n";
    let last = |variable: &str| {
        format!("{{source: {{variable: EX.{variable}, order_by: [EX.EXSTDTC], keep: last}}}}")
    };
    let spec = samples(
        "",
        "FIRST, SECOND",
        &format!(
            "  - {{name: FIRST, type: str, derivation: {}}}\n  - {{name: SECOND, type: int, derivation: {}}}\n",
            last("EXSTDTC"),
            last("EXSEQ")
        ),
    );
    let files = [("pc.csv", PC), ("ex.csv", doses)];
    let expected = (
        Ok("USUBJID,PCSEQ,FIRST,SECOND\n\
            S1,1,2025-03-11T08:00,2\n\
            S2,1,2025-03-12T06:00,1\n\
            S3,1,2025-03-13T07:00,1\n"
            .to_owned()),
        ["FIRST", "SECOND"]
            .map(|column| {
                warning(
                    &format!("columns.{column}.derivation.source.order_by"),
                    "2025-03-12",
                    "2025-03-12T06:00",
                )
            })
            .to_vec(),
    );
    let one = Partitions::new(NonZeroUsize::MIN);
    for (division, partitions) in [("the whole input", None), ("one record", Some(one))] {
        assert_eq!(
            saved(&spec, &files, partitions),
            expected,
            "partitions of {division}"
        );
    }
}

/// Two records each of groups A and B: a date and a divisor. A's dates do
/// not begin the bound `2025-03-11T08:00`; B's first date does, and B's
/// second date follows the bound. B's divisor of 0 fails a division.
const GROUPS: &str = "G,SEQ,D,K\n\
    A,1,2025-03-10,1\n\
    A,2,2025-03-09,1\n\
    B,1,2025-03-11,0\n\
    B,2,2025-03-12,0\n";

/// A specification over the groups whose N is `operation` for the records
/// of `group` and missing for the others.
fn grouped(group: &str, operation: &str) -> String {
    spec(
        "G, SEQ",
        "  T: {path: t.csv, types: {SEQ: int, K: int}}\n",
        "",
        "G, SEQ, D, K, N",
        &format!(
            "  - {{name: G, type: str, derivation: T.G}}\n  - {{name: SEQ, type: int, derivation: T.SEQ}}\n  - {{name: D, type: str, derivation: T.D}}\n  - {{name: K, type: int, derivation: T.K}}\n  - name: N\n    type: int\n    derivation:\n      case:\n        - when: \"G = '{group}'\"\n          then: {operation}\n        - otherwise: null\n"
        ),
    )
}

/// A window and an output aggregate, each over the records of one group
/// that its filter keeps, are read by the rows of one group alone, and
/// report what text order decided in that group's partition:
/// - read by A, whose dates are both before the bound and neither begins
///   it, each numbers or counts A's two records and reports nothing, B's
///   date that begins the bound unread; the same holds of a filter that
///   also divides by K, which B's records would fail;
/// - read by B, each keeps B's first record, whose date begins the bound
///   and so sorts before it, and not its second, and reports that pair.
///
/// The whole input is one batch, which completes every partition of a
/// window and filters the whole range of an aggregate; partitions of one
/// group hold nothing of the other group. Both report the same.
#[test]
fn a_window_and_an_aggregate_report_what_text_order_decided_where_a_row_read() {
    let before = "D < '2025-03-11T08:00'";
    let window = |filter: &str| {
        format!(
            "{{row_number: {{window: {{group_by: [G], order_by: [SEQ], filter: \"{filter}\"}}}}}}"
        )
    };
    let aggregate = |filter: &str| {
        format!("{{aggregate: {{group_by: [G], filter: \"{filter}\", expr: \"COUNT(SEQ)\"}}}}")
    };
    let output = |a: [&str; 2], b: [&str; 2]| {
        format!(
            "G,SEQ,D,K,N\nA,1,2025-03-10,1,{}\nA,2,2025-03-09,1,{}\nB,1,2025-03-11,0,{}\nB,2,2025-03-12,0,{}\n",
            a[0], a[1], b[0], b[1]
        )
    };
    let decided = |path: &str| vec![warning(path, "2025-03-11", "2025-03-11T08:00")];
    let window_path = "columns.N.derivation.case[0].then.row_number.window.filter";
    let aggregate_path = "columns.N.derivation.case[0].then.aggregate.filter";
    // A label, the specification, its output and its check rows.
    let cases: [(&str, String, String, Vec<String>); 6] = [
        (
            "a window A reads",
            grouped("A", &window(before)),
            output(["1", "2"], ["", ""]),
            Vec::new(),
        ),
        (
            "a window A reads, whose filter divides",
            grouped("A", &window(&format!("{before} AND 1 / K > 0"))),
            output(["1", "2"], ["", ""]),
            Vec::new(),
        ),
        (
            "an aggregate A reads",
            grouped("A", &aggregate(before)),
            output(["2", "2"], ["", ""]),
            Vec::new(),
        ),
        (
            "an aggregate A reads, whose filter divides",
            grouped("A", &aggregate(&format!("{before} AND 1 / K > 0"))),
            output(["2", "2"], ["", ""]),
            Vec::new(),
        ),
        (
            "a window B reads",
            grouped("B", &window(before)),
            output(["", ""], ["1", ""]),
            decided(window_path),
        ),
        (
            "an aggregate B reads",
            grouped("B", &aggregate(before)),
            output(["", ""], ["1", "1"]),
            decided(aggregate_path),
        ),
    ];
    let files = [("t.csv", GROUPS)];
    let keyed = Partitions::by_keys(NonZeroUsize::MIN, NonZeroUsize::MIN);
    for (label, spec, output, checks) in cases {
        let expected = (Ok(output), checks);
        for (division, partitions) in [("the whole input", None), ("one group", Some(keyed))] {
            assert_eq!(
                saved(&spec, &files, partitions),
                expected,
                "{label}: partitions of {division}"
            );
        }
    }
}

/// A run that derives its rows and then fails, here on an `output.decimals`
/// beyond 18 (CSV-6), reports no text date order, as it reports no column:
/// the same lookup over the same doses that warns in a successful run.
#[test]
fn a_run_that_fails_after_deriving_reports_no_text_date_order() {
    let spec = named("EX.EXSTDTC <= EX.EXENDTC")
        .replace("report: checks.csv,", "report: checks.csv, decimals: 19,");
    assert_eq!(
        saved(&spec, &[("pc.csv", PC), ("ex.csv", ONE_DOSE)], None),
        (
            Err(r#"invalid_field_type Some("CSV-6") ["output.decimals"]"#.to_owned()),
            Vec::new()
        ),
    );
}
