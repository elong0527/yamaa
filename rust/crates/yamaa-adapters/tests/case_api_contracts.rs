#![cfg(feature = "yaml")]
//! API seams beyond the replay's complete call parity: ordered subsequences,
//! the identity of the first issue, the original R-host comparisons, and save
//! return values. Inputs and first-issue expectations remain
//! owned by the migrated fixtures. Authored route contracts also retain the
//! original domain/preparation distinction, output and save results, staged
//! file preservation and completed bytes across repeated saves.
use crate::support::diagnostic::{form, operations, replay};
use std::{collections::BTreeSet, fs};
use yamaa_adapters::file_application;
use yamaa_core::{
    diagnostic::Diagnostic, domain_entry::Request, json::Value, project_function::Language,
};

struct Calls {
    study: crate::support::Scratch,
    checked: Vec<Diagnostic>,
    built: Result<file_application::Domain, Vec<Diagnostic>>,
}

fn check_call(
    input: &Value,
    host: Language,
    label: &str,
) -> (crate::support::Scratch, Vec<Diagnostic>) {
    assert!(input.get("project").is_none());
    assert!(input.get("built").is_none());
    let study = operations::study(input);
    let staged = operations::state(&study);
    let specification = study.identity(replay::text(replay::field(input, "specification")));
    let request = || Request {
        specification: &specification,
        environment: None,
    };
    let identity = || crate::support::study::identity("diagnostics", &specification);
    let checked = file_application::check(host, request(), identity());
    assert_eq!(
        operations::state(&study),
        staged,
        "{label}: check preserves the staged study"
    );
    (study, checked)
}

fn calls(input: &Value, host: Language, label: &str) -> Calls {
    let (study, checked) = check_call(input, host, label);
    let staged = operations::state(&study);
    let specification = study.identity(replay::text(replay::field(input, "specification")));
    let built = crate::support::study::domain(
        host,
        Request {
            specification: &specification,
            environment: None,
        },
        crate::support::study::identity("diagnostics", &specification),
    );
    assert_eq!(
        operations::state(&study),
        staged,
        "{label}: build publishes no files"
    );
    Calls {
        study,
        checked,
        built,
    }
}

#[test]
fn migrated_check_families_keep_first_issues_subsequences_and_both_hosts() {
    const FAMILIES: [&str; 3] = [
        "check_keyless_donor.json",
        "check_unknown_reads.json",
        "check_row_phase_boundaries.json",
    ];
    let mut counts = [0; 3];
    let (mut subsequences, mut first_issues, mut executions) = (0, 0, 0);
    for mut case in replay::cases() {
        let family = case.label.split_once("::").unwrap().0;
        let Some(index) = FAMILIES.iter().position(|name| *name == family) else {
            continue;
        };
        counts[index] += 1;
        let subsequence = case.operations.len() == 1;
        subsequences += usize::from(subsequence);
        assert!(
            case.operations == ["check", "build"]
                || (index == 1 && (case.operations == ["check"] || case.operations == ["build"])),
            "{}: the original call contract is equality or an unknown-read subsequence",
            case.label
        );
        let first = case.among;
        first_issues += usize::from(first);
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
            None if index == 0 => vec![Language::Python, Language::R],
            None => vec![Language::Python],
            other => panic!("{}: unknown host {other:?}", case.label),
        };
        for host in hosts {
            // The shared replay now owns complete list-operation parity.
            // This seam retains first positions, cross-case subsequences,
            // and the keyless/donor family's original R-host calls.
            let original_r_call = index == 0 && matches!(host, Language::R);
            if !(first || subsequence || original_r_call) {
                continue;
            }
            executions += 1;
            let Calls {
                study,
                checked,
                built,
            } = calls(&case.input, host, &case.label);
            let built = match built {
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
            } else {
                for operation in &case.operations {
                    let issues = if operation == "check" {
                        &checked
                    } else {
                        &built
                    };
                    replay::compare_issues(&case, issues, Some(&study), &label);
                }
            }
        }
    }
    assert_eq!(
        counts,
        [178, 474, 36],
        "every migrated case retains its API contract"
    );
    assert_eq!((subsequences, first_issues, executions), (24, 179, 381));
}

#[test]
fn lookup_model_keeps_first_findings_and_successful_save_returns() {
    let (mut count, mut first_issues, mut parity, mut saves) = (0, 0, 0, 0);
    for mut case in replay::cases() {
        if !case.label.starts_with("lookup_model.json::") {
            continue;
        }
        count += 1;
        let complete_parity = case.operations == ["check", "build"];
        // The two admitted variants called saved() without comparing bytes;
        // all 28 save cases also called saved(), which required Ok(true).
        let must_save = case.operations == ["save"]
            || matches!(
                case.label.split_once("::").unwrap().1,
                "check_admits_an_intermediate_range_value_no_row_reads"
                    | "check_admits_an_intermediate_range_value_of_the_base_driver_record"
            );
        let first = case.among;
        first_issues += usize::from(first);
        parity += usize::from(complete_parity);
        saves += usize::from(must_save);
        if !(first || must_save) {
            continue;
        }
        let Calls { study, built, .. } = calls(&case.input, Language::Python, &case.label);
        let issues = match &built {
            Ok(domain) => domain.issues(),
            Err(issues) => issues.clone(),
        };
        if first {
            assert_eq!(
                case.issues.len(),
                1,
                "{}: one authored first issue",
                case.label
            );
            assert!(!issues.is_empty(), "{}: a first issue", case.label);
            case.among = false;
            replay::compare_issues(&case, &issues[..1], Some(&study), &case.label);
        }
        if must_save {
            let mut domain = built
                .unwrap_or_else(|issues| panic!("{}: required a domain: {issues:?}", case.label));
            assert!(
                domain.issues().is_empty(),
                "{}: the original saved helper required no issues",
                case.label
            );
            let saved = domain.save();
            assert!(
                matches!(saved, Ok(true)),
                "{}: required Ok(true), got {saved:?}",
                case.label
            );
        }
    }
    assert_eq!((count, first_issues, parity, saves), (95, 4, 38, 30));
}

/// The original Rust route/save requirements; fixture inputs and expected
/// diagnostic/output bytes retain their existing owner.
fn api_routes() -> Value {
    let routes =
        yamaa_adapters::json_wire::decode(include_str!("fixtures/migrated_api_contracts.json"))
            .unwrap();
    replay::closed(&routes, &["about", "cases"]);
    routes
}

fn saved_bytes(study: &crate::support::Scratch, expected: &form::Saved, label: &str) {
    assert!(!expected.is_empty(), "{label}: authored saved files");
    for (name, bytes) in expected {
        let bytes = bytes
            .as_ref()
            .unwrap_or_else(|| panic!("{label}: an accepted save retains {name}"));
        assert_eq!(
            fs::read(study.join(name)).unwrap(),
            *bytes,
            "{label}: saved {name}"
        );
    }
}

#[test]
fn migrated_execution_cases_keep_their_authored_api_contracts() {
    let routes = api_routes();
    let contracts = replay::object(replay::field(&routes, "cases"));
    assert_eq!(
        contracts.len(),
        132,
        "every original API contract is authored"
    );
    let mut seen = BTreeSet::new();
    let (mut domains, mut preparation) = (0, 0);
    let mut checks = 0;
    let (mut accepted_saves, mut refused_saves, mut no_save) = (0, 0, 0);
    for case in replay::cases() {
        let Some(expected) = contracts.get(&case.label) else {
            continue;
        };
        assert!(seen.insert(case.label.clone()));
        replay::closed(expected, &["route", "save"]);
        let route = replay::text(replay::field(expected, "route"));
        match route {
            "domain" => domains += 1,
            "preparation" => preparation += 1,
            "check" => checks += 1,
            other => panic!("{}: unknown route {other}", case.label),
        }
        let save = match replay::field(expected, "save") {
            Value::Bool(true) => {
                accepted_saves += 1;
                Some(true)
            }
            Value::Bool(false) => {
                refused_saves += 1;
                Some(false)
            }
            Value::Null => {
                no_save += 1;
                None
            }
            other => panic!("{}: unknown save result {other:?}", case.label),
        };
        let host =
            crate::support::host(case.input.get("host").map(replay::text).unwrap_or("python"));
        if route == "check" {
            assert!(save.is_none(), "{}: check does not save", case.label);
            let (study, checked) = check_call(&case.input, host, &case.label);
            replay::compare_issues(&case, &checked, Some(&study), &case.label);
            continue;
        }
        let Calls { study, built, .. } = calls(&case.input, host, &case.label);
        let actual_route = if built.is_ok() {
            "domain"
        } else {
            "preparation"
        };
        assert_eq!(route, actual_route, "{}: authored build route", case.label);
        let issues = match &built {
            Ok(domain) => domain.issues(),
            Err(issues) => issues.clone(),
        };
        replay::compare_issues(&case, &issues, Some(&study), &case.label);
        if let Ok(mut domain) = built {
            let accepted_output = case.issues.is_empty();
            assert_eq!(
                domain.output().is_some(),
                accepted_output,
                "{}: accepted output presence",
                case.label
            );
            // The original save result is independent of a byte delta. A
            // returned refused domain also retains FailedBuild (CHECK-2).
            if let Some(save) = save.or((!accepted_output).then_some(false)) {
                let before = operations::state(&study);
                let result = domain.save();
                if save {
                    assert!(
                        matches!(result, Ok(true)),
                        "{}: expected Ok(true), got {result:?}",
                        case.label
                    );
                    let expected = case
                        .stated
                        .saved
                        .as_ref()
                        .expect("an authored positive save");
                    saved_bytes(&study, expected, &case.label);
                    // Save owns completed bytes, even after source inputs
                    // change. Expectations still come from the fixture.
                    for name in replay::object(replay::field(&case.input, "files")).keys() {
                        if name.ends_with(".csv") && !expected.contains_key(name) {
                            fs::write(study.join(name), "changed after build").unwrap();
                        }
                    }
                    let again = domain.save();
                    assert!(
                        matches!(again, Ok(true)),
                        "{}: repeated save: {again:?}",
                        case.label
                    );
                    saved_bytes(&study, expected, &case.label);
                } else {
                    assert!(
                        matches!(result, Err(file_application::FailedBuild)),
                        "{}: expected FailedBuild, got {result:?}",
                        case.label
                    );
                    assert_eq!(
                        operations::state(&study),
                        before,
                        "{}: a refused save preserves every staged path",
                        case.label
                    );
                }
            }
        }
    }
    assert_eq!(
        seen,
        contracts.keys().cloned().collect::<BTreeSet<_>>(),
        "every authored API case still exists"
    );
    assert_eq!((domains, preparation, checks), (96, 1, 35));
    assert_eq!((accepted_saves, refused_saves, no_save), (54, 19, 59));
}
