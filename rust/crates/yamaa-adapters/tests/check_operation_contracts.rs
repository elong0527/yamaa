#![cfg(feature = "yaml")]
//! API seams the partial diagnostic replay cannot express: complete agreement
//! between check and build (or the original ordered subsequence), and the
//! identity of their first issue. Inputs and first-issue expectations remain
//! owned by the migrated fixtures from #3847 and #3848.
use crate::support::diagnostic::{operations, replay};
use yamaa_adapters::file_application;
use yamaa_core::{domain_entry::Request, project_function::Language};

#[test]
fn migrated_check_families_preserve_call_parity_and_first_issue() {
    const FAMILIES: [&str; 3] = [
        "check_keyless_donor.json",
        "check_unknown_reads.json",
        "check_row_phase_boundaries.json",
    ];
    let mut counts = [0; 3];
    for mut case in replay::cases() {
        let family = case.label.split_once("::").unwrap().0;
        let Some(index) = FAMILIES.iter().position(|name| *name == family) else {
            continue;
        };
        counts[index] += 1;
        let subsequence = case.operations.len() == 1;
        assert!(
            case.operations == ["check", "build"]
                || (index == 1 && (case.operations == ["check"] || case.operations == ["build"])),
            "{}: the original call contract is equality or an unknown-read subsequence",
            case.label
        );
        let first = case.among;
        if first {
            assert_eq!(
                case.issues.len(),
                1,
                "{}: one authored first issue",
                case.label
            );
            case.among = false;
        }
        let hosts = match case.input.get("host").map(replay::text) {
            Some("python") => vec![Language::Python],
            Some("r") => vec![Language::R],
            None => vec![Language::Python, Language::R],
            other => panic!("{}: unknown host {other:?}", case.label),
        };
        for host in hosts {
            assert!(case.input.get("project").is_none());
            assert!(case.input.get("built").is_none());
            let study = operations::study(&case.input);
            let specification =
                study.identity(replay::text(replay::field(&case.input, "specification")));
            let request = || Request {
                specification: &specification,
                environment: None,
            };
            let identity = || crate::support::study::identity("diagnostics", &specification);
            let checked = file_application::check(host, request(), identity());
            let built = match crate::support::study::domain(host, request(), identity()) {
                Ok(domain) => domain.issues(),
                Err(issues) => issues,
            };
            let checked_rows = crate::support::rows(&checked);
            let built_rows = crate::support::rows(&built);
            let label = format!("{} ({host:?})", case.label);
            if subsequence {
                let mut remaining = built_rows.iter();
                assert!(
                    checked_rows.iter().all(|row| remaining.any(|other| other == row)),
                    "{label}: complete check rows {checked_rows:?} remain an ordered subsequence of build rows {built_rows:?}"
                );
            } else {
                assert_eq!(checked_rows, built_rows, "{label}: complete call parity");
            }
            if first {
                assert!(
                    !checked.is_empty() && !built.is_empty(),
                    "{label}: a first issue"
                );
                replay::compare_issues(&case, &checked[..1], Some(&study), &label);
                replay::compare_issues(&case, &built[..1], Some(&study), &label);
            }
        }
    }
    assert_eq!(
        counts,
        [178, 474, 36],
        "every migrated case retains its API contract"
    );
}
