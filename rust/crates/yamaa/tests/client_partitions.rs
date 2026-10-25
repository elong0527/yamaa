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
