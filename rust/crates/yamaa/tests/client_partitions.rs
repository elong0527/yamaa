//! The partitions every client runs over by default (#3794, D264): each
//! binding and the command line construct their engine with key-bounded
//! partitions over core's `Limits::PARTITION_KEYS` leading keys, of at most
//! `Limits::PARTITION_ROWS` records, with the whole-table fallback. The
//! clients depend on the facade alone, which has no such constant, so each
//! spells the two values as `PARTITIONS`. The seam is the clients' sources,
//! which no facade call reaches: this reads each one's `PARTITIONS`.
use yamaa_core::limits::Limits;

/// Each client's `PARTITIONS` holds core's values, and its engine takes
/// them with the fallback.
#[test]
fn every_client_runs_over_the_default_partitions() {
    let crates = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    for client in [
        "yamaa-python/src/api_entry.rs",
        "yamaa-r/src/api_entry.rs",
        "yamaa-cli/src/commands.rs",
    ] {
        let source = std::fs::read_to_string(crates.join(client)).unwrap();
        let declared = source
            .split_once("const PARTITIONS: [usize; 2] = [")
            .and_then(|(_, rest)| rest.split_once("];"))
            .map(|(values, _)| {
                values
                    .split(',')
                    .map(|value| value.trim().replace('_', "").parse::<usize>().unwrap())
                    .collect::<Vec<_>>()
            });
        let engine = "Partitions::by_keys(keys, rows).with_whole_table_fallback()";
        assert_eq!(
            (declared, source.contains(engine)),
            (
                Some(vec![Limits::PARTITION_KEYS, Limits::PARTITION_ROWS]),
                true
            ),
            "{client}"
        );
    }
}

/// A host's `threads` argument (#4193, D-next on #4184) is a positive whole
/// number, which R may give as a whole double, or missing for the default;
/// a number beyond what the platform counts saturates. The seam is the
/// facade's one judgement, which the Python and R bindings and the command
/// line each reach with their own spelling of the value.
#[test]
fn a_host_threads_argument_is_a_positive_whole_number_or_missing() {
    use std::num::NonZeroUsize;
    use yamaa::{ContextValue as V, Partitions};
    let base = Partitions::new(NonZeroUsize::MIN);
    let threads = |count: usize| Some(base.with_threads(NonZeroUsize::new(count).unwrap()));
    let cases = [
        ("missing", V::Null, Some(base)),
        ("one", V::Int(1), threads(1)),
        ("four", V::Int(4), threads(4)),
        ("a whole double", V::Float(2.0), threads(2)),
        (
            "an integer beyond 64 bits",
            V::Wide("18446744073709551616".into()),
            threads(usize::MAX),
        ),
        ("zero", V::Int(0), None),
        ("negative", V::Int(-1), None),
        ("a zero double", V::Float(0.0), None),
        ("a fraction", V::Float(1.5), None),
        ("not a number", V::Float(f64::NAN), None),
        ("infinite", V::Float(f64::INFINITY), None),
        (
            "a negative integer beyond 64 bits",
            V::Wide("-18446744073709551616".into()),
            None,
        ),
        ("a Boolean", V::Bool(true), None),
        ("text", V::Text("4".into()), None),
    ];
    for (label, value, expected) in cases {
        assert_eq!(base.with_host_threads(&value), expected, "{label}");
    }
}
