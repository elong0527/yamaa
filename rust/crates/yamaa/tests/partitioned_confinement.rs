#![cfg(feature = "yaml")]
//! The confinement gate (#3830): the records one step reads together must
//! sit in one partition, so a partitioned run whose partitions cannot keep
//! them together is refused before derivation as `partition_global`, and its
//! whole-table fallback runs it whole. Consecutive partitions end only
//! between runs of the input fields the plan's grouped row templates share,
//! so a plan they confine divides. A run's measured observations name how it
//! divided its driver input (#3794).
//!
//! Each case states the whole-table output by hand: a plan the gate refuses
//! builds it under the fallback, and a plan it admits builds it divided. The
//! seam is the facade's partitioned engine mode, which neither the benchmark
//! reader nor the host fixtures run.
use std::num::NonZeroUsize;

use crate::support::{issues, Scratch};
use yamaa::{CancellationToken, Engine, EngineOptions, HostLanguage, Partitions};

/// Run `spec` over `input` on an engine with `partitions`, or whole, and
/// save it: the saved output's bytes, or every issue of a run with no output.
fn saved(spec: &str, input: &str, partitions: Option<Partitions>) -> Result<String, String> {
    let scratch = Scratch::new("partitioned-confinement");
    scratch.write("spec.yaml", spec);
    scratch.write("input.csv", input);
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

/// The refusal of a step that needs the whole table, at `path`.
fn refused(path: &str) -> String {
    format!(
        r#"Error engine engine_rejected None ["{path}"] {{"code":"partition_global","step":"execute"}} None None"#
    )
}

/// The rows `kept` builds from the records an intermediate keeps, those
/// whose V is above 1: b and c.
const DRIVEN: &str = "yamaa: \"1.0\"\ntype: spec\ndomain: TEST\nkeys: [ID]\ninput: {SRC: {path: input.csv, types: {V: int}}}\noutput: {path: out.csv, columns: [ID, V]}\nintermediates:\n  - {id: KEPT, dataset: SRC, filter: \"SRC.V > 1\"}\ncolumns:\n  - {name: ID, type: str}\n  - {name: V, type: int}\nrows:\n  - id: kept\n    dataset: KEPT\n    derivations: {ID: KEPT.ID, V: KEPT.V}\n";
/// Every record, beside an intermediate over the input whose IDs are
/// verified unique.
const VERIFIED: &str = "yamaa: \"1.0\"\ntype: spec\ndomain: TEST\nkeys: [ID]\ninput: {SRC: {path: input.csv, types: {V: int}}}\noutput: {path: out.csv, columns: [ID, V]}\nintermediates:\n  - id: SEEN\n    dataset: SRC\n    key: [ID]\n    verifications:\n      - unique: [ID]\ncolumns:\n  - {name: ID, type: str, derivation: SRC.ID}\n  - {name: V, type: int, derivation: SRC.V}\n";
/// Three records with distinct IDs.
const RECORDS: &str = "ID,V\na,1\nb,2\nc,3\n";
/// Every record, keyed by ID and N, then a row per subject with N 0: a
/// template grouped by ID, which copies the key ID but not the key N.
const SUBJECTS: &str = "yamaa: \"1.0\"\ntype: spec\ndomain: TEST\nkeys: [ID, N]\ninput: {SRC: {path: input.csv, types: {N: int}}}\noutput: {path: out.csv, columns: [ID, N]}\ncolumns:\n  - {name: ID, type: str, derivation: SRC.ID}\n  - {name: N, type: int}\nrows:\n  - id: records\n    derivations: {N: SRC.N}\n  - id: subjects\n    group_by: [SRC.ID]\n    derivations: {N: \"0\"}\n";
/// Two subjects' records, grouped by subject.
const VISITS: &str = "ID,N\na,1\na,2\nb,1\n";
/// A row per subject with N 1, then a row per site with N 2: two grouped
/// templates whose group fields, ID and SITE, share none.
const SITES: &str = "yamaa: \"1.0\"\ntype: spec\ndomain: TEST\nkeys: [K, N]\ninput: {SRC: input.csv}\noutput: {path: out.csv, columns: [K, N]}\ncolumns:\n  - {name: K, type: str}\n  - {name: N, type: int}\nrows:\n  - id: subjects\n    group_by: [SRC.ID]\n    derivations: {K: SRC.ID, N: \"1\"}\n  - id: sites\n    group_by: [SRC.SITE]\n    derivations: {K: SRC.SITE, N: \"2\"}\n";
/// The subjects grouped by subject, but not by site: s1 recurs after s2.
const ENROLMENT: &str = "ID,SITE\na,s1\na,s1\nb,s2\nc,s1\n";

/// Each refused plan is refused at the step whose records no partition
/// keeps together, under one-record or one-subject partitions, and builds
/// the whole-table output under the fallback; each admitted plan builds it
/// divided:
/// - rows driven by an intermediate over the driver input, whose records
///   every partition holds some of, are refused at the intermediate;
/// - an intermediate over the driver input verified unique, whose check
///   reads every record, is refused at the intermediate;
/// - a template grouped by ID is admitted by partitions of one ID each, and
///   refused at its `group_by` by partitions over ID and N, since a
///   subject's records hold several values of N;
/// - consecutive partitions cut between runs of ID admit the template
///   grouped by ID alone, and refuse the template grouped by SITE beside it
///   at its `group_by`, since the two templates share no field to cut by.
#[test]
fn a_step_whose_records_no_partition_keeps_together_is_refused_unless_it_falls_back() {
    let one = NonZeroUsize::MIN;
    let two = NonZeroUsize::new(2).unwrap();
    let subjects_only = SUBJECTS.replace("  - id: records\n    derivations: {N: SRC.N}\n", "");
    // A label, the specification, its input, the partitions, the path of
    // the step that refuses them and the whole-table output.
    type Case<'s> = (
        &'s str,
        &'s str,
        &'s str,
        Partitions,
        Option<&'s str>,
        &'s str,
    );
    let cases: [Case<'_>; 6] = [
        (
            "rows driven by an intermediate over the input",
            DRIVEN,
            RECORDS,
            Partitions::new(one),
            Some("intermediates.KEPT"),
            "ID,V\nb,2\nc,3\n",
        ),
        (
            "an intermediate over the input verified unique",
            VERIFIED,
            RECORDS,
            Partitions::new(one),
            Some("intermediates.SEEN"),
            "ID,V\na,1\nb,2\nc,3\n",
        ),
        (
            "a template grouped by ID, by the key ID",
            SUBJECTS,
            VISITS,
            Partitions::by_keys(one, one),
            None,
            "ID,N\na,1\na,2\nb,1\na,0\nb,0\n",
        ),
        (
            "a template grouped by ID, by the keys ID and N",
            SUBJECTS,
            VISITS,
            Partitions::by_keys(two, one),
            Some("rows.subjects.group_by"),
            "ID,N\na,1\na,2\nb,1\na,0\nb,0\n",
        ),
        (
            "a template grouped by ID, consecutive",
            &subjects_only,
            VISITS,
            Partitions::new(one),
            None,
            "ID,N\na,0\nb,0\n",
        ),
        (
            "templates grouped by ID and by SITE, consecutive",
            SITES,
            ENROLMENT,
            Partitions::new(one),
            Some("rows.sites.group_by"),
            "K,N\na,1\nb,1\nc,1\ns1,2\ns2,2\n",
        ),
    ];
    for (label, spec, input, partitions, refusal, output) in cases {
        assert_eq!(
            saved(spec, input, None),
            Ok(output.to_owned()),
            "{label}, whole"
        );
        assert_eq!(
            saved(spec, input, Some(partitions)),
            refusal.map_or(Ok(output.to_owned()), |path| Err(refused(path))),
            "{label}"
        );
        if refusal.is_some() {
            assert_eq!(
                saved(spec, input, Some(partitions.with_whole_table_fallback())),
                Ok(output.to_owned()),
                "{label}, with the fallback"
            );
        }
    }
}

/// A run's measured observations name how it divided its driver input
/// (#3794): the strategy, the leading keys its partitions keep whole, the
/// most records each holds and how many partitions it ran. Two subjects'
/// records, a, a and b, run whole on an engine without partitions; in two
/// partitions, cut between the runs of ID, under consecutive partitions of
/// one record and key-bounded ones over the key ID; and whole again under the
/// fallback when key-bounded partitions over ID and N refuse the template
/// grouped by ID.
#[test]
fn a_run_names_how_it_divided_its_driver_input_in_its_measured_observations() {
    let one = NonZeroUsize::MIN;
    let two = NonZeroUsize::new(2).unwrap();
    let subjects_only = SUBJECTS.replace("  - id: records\n    derivations: {N: SRC.N}\n", "");
    let whole = r#"{"count":1,"keys":0,"rows":null,"strategy":"whole_table"}"#;
    let cases: [(&str, &str, Option<Partitions>, &str); 4] = [
        ("without partitions", SUBJECTS, None, whole),
        (
            "consecutive partitions",
            &subjects_only,
            Some(Partitions::new(one)),
            r#"{"count":2,"keys":0,"rows":1,"strategy":"consecutive"}"#,
        ),
        (
            "key-bounded partitions",
            SUBJECTS,
            Some(Partitions::by_keys(one, one)),
            r#"{"count":2,"keys":1,"rows":1,"strategy":"key_bounded"}"#,
        ),
        (
            "the fallback",
            SUBJECTS,
            Some(Partitions::by_keys(two, one).with_whole_table_fallback()),
            whole,
        ),
    ];
    for (label, spec, partitions, division) in cases {
        let scratch = Scratch::new("partitioned-division");
        scratch.write("spec.yaml", spec);
        scratch.write("input.csv", VISITS);
        let options = EngineOptions::new(HostLanguage::Python, "yamaa".into(), "0".into());
        let engine = Engine::new(match partitions {
            Some(partitions) => options.with_partitions(partitions),
            None => options,
        });
        let run = engine
            .spec(scratch.join("spec.yaml").to_str().unwrap())
            .run()
            .unwrap();
        // The measurements close the observations, so their record is the
        // last one named.
        let observed = run.measured_observations();
        let start = observed.rfind("\"partitions\":").unwrap() + "\"partitions\":".len();
        let record = &observed[start..];
        assert_eq!(&record[..=record.find('}').unwrap()], division, "{label}");
    }
}
