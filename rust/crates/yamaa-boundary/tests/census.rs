//! The census of live native handles: each handle counts under its kind
//! while its `Live` is held, and the kinds come in the order of the shared
//! truth's `interface.census_kinds`, which both hosts' accessors return.
use yamaa_boundary::{Census, HandleKind};

use crate::support::{family, texts};

static CENSUS: Census = Census::new();

fn counts() -> Vec<(&'static str, usize)> {
    CENSUS
        .counts()
        .into_iter()
        .map(|(kind, count)| (kind.as_str(), count))
        .collect()
}

#[test]
fn each_live_handle_counts_until_it_drops() {
    let kinds: Vec<&str> = HandleKind::ALL.iter().map(|kind| kind.as_str()).collect();
    assert_eq!(
        kinds,
        texts(family("interface"), "census_kinds"),
        "the census kinds, in the truth's order"
    );
    let run = CENSUS.hold(HandleKind::Run);
    let tables = [
        CENSUS.hold(HandleKind::Table),
        CENSUS.hold(HandleKind::Table),
    ];
    let held = counts();
    drop(run);
    let one_dropped = counts();
    drop(tables);
    assert_eq!(
        [held, one_dropped, counts()],
        [
            vec![
                ("check", 0),
                ("run", 1),
                ("project_run", 0),
                ("submission", 0),
                ("lint", 0),
                ("selftest", 0),
                ("plans", 0),
                ("table", 2),
            ],
            vec![
                ("check", 0),
                ("run", 0),
                ("project_run", 0),
                ("submission", 0),
                ("lint", 0),
                ("selftest", 0),
                ("plans", 0),
                ("table", 2),
            ],
            vec![
                ("check", 0),
                ("run", 0),
                ("project_run", 0),
                ("submission", 0),
                ("lint", 0),
                ("selftest", 0),
                ("plans", 0),
                ("table", 0),
            ],
        ],
        "the census with a run and two tables held, after the run drops, and after the \
         tables drop"
    );
}
