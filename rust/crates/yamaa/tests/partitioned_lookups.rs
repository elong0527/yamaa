#![cfg(feature = "yaml")]
//! Partitioned lookups (#4191): a specification whose other inputs are read
//! only through lookups reads them whole, before any partition, and reads
//! its driver input by partition, so the row limit bounds each partition of
//! the driver input and each other input whole (LIMIT-3). A lookup into
//! another input is partition-safe: no partition of the driver input
//! produces or changes a record of it. A lookup into the driver input, and
//! an intermediate that derives values from its records, still needs the
//! whole table.
//!
//! Each case states the whole-table output by hand. The seam is the
//! facade's partitioned engine mode, which neither the benchmark reader nor
//! the host fixtures run, and an input beyond the row limit, which no
//! fixture can hold; `partitioned_corpus.rs` runs every benchmark that reads
//! another input the same way.
use std::num::NonZeroUsize;

use crate::support::{issues, Scratch};
use yamaa::{CancellationToken, Engine, EngineOptions, HostLanguage, Partitions};

/// Run `spec` over `src` and `adsl` on an engine with `partitions`, or
/// whole, and save it: the saved output's bytes, or every issue of a run
/// with no output.
fn saved(
    spec: &str,
    src: &str,
    adsl: &str,
    partitions: Option<Partitions>,
) -> Result<String, String> {
    let scratch = Scratch::new("partitioned-lookups");
    scratch.write("spec.yaml", spec);
    scratch.write("src.csv", src);
    scratch.write("adsl.csv", adsl);
    let options = EngineOptions::new(HostLanguage::Python, "yamaa".into(), "0".into());
    let engine = Engine::new(match partitions {
        Some(partitions) => options.with_partitions(partitions),
        None => options,
    });
    let mut run = engine
        .spec(scratch.join("spec.yaml").to_str().unwrap())
        .run()
        .unwrap();
    if run.output().is_none() {
        return Err(issues(&run.diagnostics()));
    }
    run.save(&CancellationToken::new()).unwrap();
    Ok(std::fs::read_to_string(scratch.join("out.csv")).unwrap())
}

/// A specification over the driver input SRC and the input ADSL, declared
/// as `input`, that copies each record's ID and SUBJ and derives ARM by
/// `arm`, with `intermediates`, if any.
fn spec(input: &str, intermediates: &str, arm: &str) -> String {
    format!(
        "yamaa: \"1.0\"\ntype: spec\ndomain: TEST\nkeys: [ID]\nbase: SRC\ninput:\n{input}output: {{path: out.csv, columns: [ID, SUBJ, ARM]}}\n{intermediates}columns:\n  - {{name: ID, type: int, derivation: SRC.ID}}\n  - {{name: SUBJ, type: str, derivation: SRC.SUBJ}}\n  - {{name: ARM, type: str, derivation: {arm}}}\n"
    )
}

/// SRC, then ADSL.
const INPUTS: &str =
    "  SRC: {path: src.csv, types: {ID: int}}\n  ADSL: {path: adsl.csv, key: [SUBJ]}\n";
/// Three records of two subjects.
const SRC: &str = "ID,SUBJ\n1,a\n2,a\n3,b\n";
/// Each subject's arm.
const ADSL: &str = "SUBJ,ARM\na,P\nb,T\n";

/// The refusal of a step that needs the whole table, at `path`.
fn refused(path: &str) -> String {
    format!(
        r#"Error engine engine_rejected None ["{path}"] {{"code":"partition_global","step":"execute"}} None None"#
    )
}

/// Each lookup runs over one-record partitions with no fallback: a lookup
/// into ADSL builds the whole-table output divided, wherever ADSL is
/// declared, and a lookup the whole table must answer is refused at the
/// read of the column that reads it, its derivation's `source`, and builds
/// the whole-table output under the fallback:
/// - ADSL read by its declared key, declared after SRC or before it, and
///   through an intermediate keyed on ADSL, give each record its subject's
///   arm, P, P and T;
/// - an intermediate keyed on ADSL that derives the arm from ADSL's records
///   is refused, since each partition would derive them again;
/// - an intermediate over SRC keyed by ID is refused, since its records
///   span every partition; ARM reads the record's own SUBJ, a, a and b.
#[test]
fn a_lookup_into_another_input_divides_and_a_lookup_into_the_driver_input_does_not() {
    let one = Partitions::new(NonZeroUsize::MIN);
    let keyed = Partitions::by_keys(NonZeroUsize::MIN, NonZeroUsize::MIN);
    let arms = "ID,SUBJ,ARM\n1,a,P\n2,a,P\n3,b,T\n";
    let adsl_first =
        "  ADSL: {path: adsl.csv, key: [SUBJ]}\n  SRC: {path: src.csv, types: {ID: int}}\n";
    let named = "  SRC: {path: src.csv, types: {ID: int}}\n  ADSL: adsl.csv\n";
    // A label, the specification, the path of the step that refuses the
    // partitions and the whole-table output.
    let cases: [(&str, String, Option<&str>, &str); 5] = [
        (
            "ADSL read by its declared key",
            spec(INPUTS, "", "ADSL.ARM"),
            None,
            arms,
        ),
        (
            "ADSL declared before SRC",
            spec(adsl_first, "", "ADSL.ARM"),
            None,
            arms,
        ),
        (
            "an intermediate keyed on ADSL",
            spec(
                named,
                "intermediates:\n  - {id: SBJ, dataset: ADSL, key: [SUBJ]}\n",
                "SBJ.ARM",
            ),
            None,
            arms,
        ),
        (
            "an intermediate keyed on ADSL that derives the arm",
            spec(
                named,
                "intermediates:\n  - id: SBJ\n    dataset: ADSL\n    key: [SUBJ]\n    derivations: {ARMD: ADSL.ARM}\n",
                "SBJ.ARMD",
            ),
            Some("columns.ARM.derivation.source"),
            arms,
        ),
        (
            "an intermediate over SRC keyed by ID",
            spec(
                named,
                "intermediates:\n  - {id: OWN, dataset: SRC, key: [ID]}\n",
                "OWN.SUBJ",
            ),
            Some("columns.ARM.derivation.source"),
            "ID,SUBJ,ARM\n1,a,a\n2,a,a\n3,b,b\n",
        ),
    ];
    for (label, spec, refusal, output) in cases {
        assert_eq!(
            saved(&spec, SRC, ADSL, None),
            Ok(output.to_owned()),
            "{label}: the whole-table run"
        );
        for (division, partitions) in [("one record", one), ("one key", keyed)] {
            let expected = match refusal {
                Some(path) => Err(refused(path)),
                None => Ok(output.to_owned()),
            };
            assert_eq!(
                saved(&spec, SRC, ADSL, Some(partitions)),
                expected,
                "{label}: partitions of {division}"
            );
            assert_eq!(
                saved(
                    &spec,
                    SRC,
                    ADSL,
                    Some(partitions.with_whole_table_fallback())
                ),
                Ok(output.to_owned()),
                "{label}: partitions of {division} with the fallback"
            );
        }
    }
}

/// A driver input of 10,000,002 records, beyond the row limit, beside an
/// input of two subjects it reads by key, runs in partitions of 1,000,000
/// records (LIMIT-3): ADSL is admitted whole, each partition of SRC on its
/// own, and every record its own row, the odd IDs subject a's, arm P, the
/// even IDs subject b's, arm T. The saved file has a header and 10,000,002
/// lines, the last two the last two records'. The input and the run take
/// minutes unoptimized, so the test runs in the optimized `release-test`
/// profile, as the native lane runs every test, and no other test beyond
/// the row limit runs beside it (`support::beyond_the_row_limit`).
#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "a 10,000,002-record driver input runs in the optimized release-test profile"
)]
fn a_driver_input_beyond_the_row_limit_beside_a_lookup_input_runs_by_partition() {
    use std::io::Write as _;
    let _alone = crate::support::beyond_the_row_limit();
    let records = yamaa_core::limits::Limits::TABLE_ROWS + 2;
    let scratch = Scratch::new("partitioned-lookups-limit");
    scratch.write("spec.yaml", spec(INPUTS, "", "ADSL.ARM"));
    scratch.write("adsl.csv", ADSL);
    let mut src = std::io::BufWriter::new(std::fs::File::create(scratch.join("src.csv")).unwrap());
    writeln!(src, "ID,SUBJ").unwrap();
    for id in 1..=records {
        writeln!(src, "{id},{}", if id % 2 == 1 { "a" } else { "b" }).unwrap();
    }
    src.flush().unwrap();
    drop(src);
    let options = EngineOptions::new(HostLanguage::Python, "yamaa".into(), "0".into());
    let million = Partitions::new(NonZeroUsize::new(1_000_000).unwrap());
    let mut run = Engine::new(options.with_partitions(million))
        .spec(scratch.join("spec.yaml").to_str().unwrap())
        .run()
        .unwrap();
    let rows = run
        .output()
        .unwrap_or_else(|| panic!("{}", issues(&run.diagnostics())))
        .rows();
    run.save(&CancellationToken::new()).unwrap();
    let saved = std::fs::read_to_string(scratch.join("out.csv")).unwrap();
    let last = saved.lines().rev().take(2).collect::<Vec<_>>();
    assert_eq!(
        (rows, saved.lines().count(), last),
        (records, records + 1, vec!["10000002,b,T", "10000001,a,P"]),
        "the output's rows, the saved file's lines and its last two"
    );
}
