#![cfg(feature = "yaml")]
//! API seams beyond the replay's complete call parity: ordered subsequences,
//! the identity of the first issue, the original R-host comparisons, and save
//! return values. Inputs and first-issue expectations remain
//! owned by the migrated fixtures. Authored route contracts also retain the
//! original domain/preparation distinction, output and save results, staged
//! file preservation and completed bytes across repeated saves, the declared
//! report a refused save publishes alone, and the calls a scripted
//! host-functions port received, in order
//! (`fixtures/function_call_contracts.json`).
use crate::support::diagnostic::{form, operations, replay};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
};
use yamaa_application::file_application;
use yamaa_core::{
    diagnostic::Diagnostic, domain_entry::Request, json::Value, project_function::Language,
};

fn resource_contracts() -> Value {
    yamaa_adapters::json_wire::decode(include_str!("fixtures/resource_api_contracts.json")).unwrap()
}

struct Calls {
    study: crate::support::Scratch,
    checked: Vec<Diagnostic>,
    built: Result<file_application::Domain, Vec<Diagnostic>>,
}

/// The static check of a case's input, with the project it names if any,
/// which leaves the staged study as it was.
fn check_call(
    input: &Value,
    host: Language,
    label: &str,
) -> (crate::support::Scratch, Vec<Diagnostic>) {
    assert!(input.get("built").is_none());
    let study = operations::study(input);
    let staged = operations::state(&study);
    let specification = study.identity(replay::text(replay::field(input, "specification")));
    let project = input
        .get("project")
        .map(|name| study.identity(replay::text(name)));
    let request = || Request {
        specification: &specification,
        environment: project.as_deref(),
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

/// The check and the build of a case's input, each with the project it
/// names if any; the build publishes nothing.
fn calls(input: &Value, host: Language, label: &str) -> Calls {
    let (study, checked) = check_call(input, host, label);
    let staged = operations::state(&study);
    let specification = study.identity(replay::text(replay::field(input, "specification")));
    let project = input
        .get("project")
        .map(|name| study.identity(replay::text(name)));
    let built = crate::support::study::domain(
        host,
        Request {
            specification: &specification,
            environment: project.as_deref(),
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
    const FAMILIES: [&str; 4] = [
        "check_keyless_donor.json",
        "check_unknown_reads.json",
        "check_row_phase_boundaries.json",
        "check_row_scopes.json",
    ];
    let mut counts = [0; 4];
    let (mut subsequences, mut first_issues, mut executions) = (0, 0, 0);
    let mut row_phase_saves = 0;
    let mut single_routes = 0;
    for mut case in replay::cases() {
        let family = case.label.split_once("::").unwrap().0;
        let Some(index) = FAMILIES.iter().position(|name| *name == family) else {
            continue;
        };
        // These migrated execution cases called one route at a time;
        // the two list-operation cases still prove complete call parity.
        if index == 3 && case.operations.len() == 1 {
            single_routes += 1;
            continue;
        }
        counts[index] += 1;
        // The save cases moved from execution_row_phase_reads.rs keep their
        // route and save results in migrated_api_contracts.json; their tests
        // built and saved for the Python host alone, and called no check.
        if index == 2 && case.operations == ["save"] {
            row_phase_saves += 1;
            continue;
        }
        let subsequence = case.operations.len() == 1;
        subsequences += usize::from(subsequence);
        assert!(
            case.operations == ["check", "build"]
                || (index == 1 && (case.operations == ["check"] || case.operations == ["build"])),
            "{}: the original call contract is equality or an unknown-read subsequence",
            case.label
        );
        // An `among` row is the issue its test read first, which this seam
        // places; the replay places a `prefix` case's rows itself.
        let first = case.order == form::Order::Among;
        first_issues += usize::from(first);
        if first {
            assert_eq!(
                case.issues.len(),
                1,
                "{}: one authored first issue",
                case.label
            );
            case.order = form::Order::Prefix;
        }
        // The host is read only for a project's environment, functions or
        // lock, and no check case names a project, so each case is stated
        // once and the replay runs it for the default host. The tests of the
        // first three families also called the R host, as this seam does.
        assert!(
            case.input.get("host").is_none(),
            "{}: a check case is stated once, for every host",
            case.label
        );
        let hosts: &[Language] = match index {
            0..=2 => &[Language::Python, Language::R],
            _ => &[Language::Python],
        };
        for &host in hosts {
            // The shared replay now owns complete list-operation parity.
            // This seam retains first positions, cross-case subsequences,
            // and the original R-host calls.
            let original_r_call = matches!(host, Language::R);
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
                replay::compare_issues(&case, &checked, Some(&study), &label);
                replay::compare_issues(&case, &built, Some(&study), &label);
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
        [176, 237, 33, 89],
        "every migrated case retains its API contract"
    );
    assert_eq!(
        row_phase_saves, 15,
        "the save cases moved from execution_row_phase_reads.rs"
    );
    assert_eq!((subsequences, first_issues, executions), (12, 162, 605));
    assert_eq!(single_routes, 22, "every moved single-route row-scope case");
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
        // every save case without issues also called saved() or unwrapped
        // save(), which required Ok(true). A refused save's contract is in
        // migrated_api_contracts.json.
        let must_save = (case.operations == ["save"] && case.issues.is_empty())
            || matches!(
                case.label.split_once("::").unwrap().1,
                "check_admits_an_intermediate_range_value_no_row_reads"
                    | "check_admits_an_intermediate_range_value_of_the_base_driver_record"
            );
        let first = case.order == form::Order::Among;
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
            case.order = form::Order::Prefix;
            replay::compare_issues(&case, &issues, Some(&study), &case.label);
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
    assert_eq!((count, first_issues, parity, saves), (123, 4, 38, 43));
}

/// Seam: the R-host calls of the tests that moved into `case_item_order.json`
/// and `check_output_validation.json`, which ran each specification for the
/// Python and the R host. The host is read only for a project's environment,
/// functions or lock, and no case of the two names one, so the replay states
/// each specification once, for the default host. This seam makes the R
/// calls those tests made: the check and the build publish nothing, each
/// operation a case names reports its rows, and a save returns `Ok(true)` and
/// writes the bytes its case states. The lookup-placeholder case moved from
/// the two-host check family to `completed_intermediates.json` keeps its R
/// build here; Q2 removes only its old check finding (#3805).
#[test]
fn moved_two_host_families_keep_their_r_host_calls() {
    const SELECTED: [&str; 3] = [
        "case_item_order.json",
        "check_output_validation.json",
        "completed_intermediates.json::a_lookup_placeholder_of_another_lookup_s_field_is_typed",
    ];
    let mut counts = [0; 3];
    for case in replay::cases() {
        let family = case.label.split_once("::").unwrap().0;
        let Some(index) = SELECTED
            .iter()
            .position(|name| *name == family || *name == case.label)
        else {
            continue;
        };
        counts[index] += 1;
        assert!(
            case.input.get("host").is_none() && case.input.get("project").is_none(),
            "{}: a case of these families is stated once, for every host",
            case.label
        );
        let label = format!("{} (R)", case.label);
        let Calls {
            study,
            checked,
            mut built,
        } = calls(&case.input, Language::R, &label);
        for operation in &case.operations {
            match operation.as_str() {
                "check" => replay::compare_issues(&case, &checked, Some(&study), &label),
                "build" => {
                    let issues = match &built {
                        Ok(domain) => domain.issues(),
                        Err(issues) => issues.clone(),
                    };
                    replay::compare_issues(&case, &issues, Some(&study), &label);
                }
                "save" => {
                    let domain = built
                        .as_mut()
                        .unwrap_or_else(|issues| panic!("{label}: a save builds: {issues:?}"));
                    replay::compare_issues(&case, &domain.issues(), Some(&study), &label);
                    let saved = domain.save();
                    assert!(
                        matches!(saved, Ok(true)),
                        "{label}: expected Ok(true), got {saved:?}"
                    );
                    saved_bytes(&study, case.stated.saved.as_ref().unwrap(), &label);
                }
                other => panic!("{label}: no R call for {other}"),
            }
        }
    }
    assert_eq!(
        counts,
        [10, 67, 1],
        "every moved two-host case makes its R calls"
    );
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

fn saved_bytes(
    study: &crate::support::Scratch,
    expected: &BTreeMap<String, form::Expected>,
    label: &str,
) {
    assert!(!expected.is_empty(), "{label}: authored saved files");
    for (name, stated) in expected {
        assert!(
            *stated != form::Expected::Removed,
            "{label}: an accepted save retains {name}"
        );
        let bytes = fs::read(study.join(name)).unwrap();
        stated.compare(name, Some(&bytes), &format!("{label}: saved"));
    }
}

/// Read the published CSV, comparing the same authored report rows as the
/// replay. CSV writes missing cells empty; other scalar cells retain their
/// text. This checks the saved artifact independently of the in-memory report.
fn saved_report_rows(bytes: &[u8], expected: &BTreeMap<String, Vec<Value>>, label: &str) {
    let report = yamaa_adapters::csv_source::parse(bytes).expect("a saved report is CSV");
    let kind_at = report.names.iter().position(|name| name == "KIND").unwrap();
    for (kind, rows) in expected {
        let columns = rows[0]
            .as_array()
            .unwrap()
            .iter()
            .map(|name| {
                report
                    .names
                    .iter()
                    .position(|column| column == replay::text(name))
                    .unwrap()
            })
            .collect::<Vec<_>>();
        let actual = report
            .records
            .iter()
            .filter(|row| row[kind_at].as_deref() == Some(kind.as_str()))
            .map(|row| {
                columns
                    .iter()
                    .map(|&at| row[at].clone().unwrap_or_default())
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        let stated = rows[1..]
            .iter()
            .map(|row| {
                row.as_array()
                    .unwrap()
                    .iter()
                    .map(|cell| match cell {
                        Value::Null => String::new(),
                        Value::String(text) => text.clone(),
                        other => yamaa_adapters::json_wire::encode(other),
                    })
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        assert_eq!(actual, stated, "{label}: the saved report's {kind} rows");
    }
}

/// A migrated build case whose original test saved only to read artifacts.
/// Output bytes come from that test's literal; report rows stay in the replay.
fn saved_build_artifacts(study: &crate::support::Scratch, case: &replay::Case, expected: &Value) {
    assert!(
        case.stated.report.is_some(),
        "{}: authored report",
        case.label
    );
    assert_eq!(
        case.operations,
        ["build"],
        "{}: build-only replay",
        case.label
    );
    if let Some(report) = expected.get("publishes") {
        let files = replay::object(replay::field(expected, "saved"));
        assert!(!files.is_empty(), "{}: authored saved output", case.label);
        for (name, text) in files {
            assert_eq!(
                fs::read(study.join(name)).unwrap(),
                replay::text(text).as_bytes(),
                "{}: saved {name}",
                case.label
            );
        }
        saved_report_rows(
            &fs::read(study.join(replay::text(report))).unwrap(),
            case.stated.report.as_ref().unwrap(),
            &case.label,
        );
    }
}

#[test]
fn migrated_execution_cases_keep_their_authored_api_contracts() {
    let routes = api_routes();
    let contracts = replay::object(replay::field(&routes, "cases"));
    assert_eq!(
        contracts.len(),
        692,
        "every original API contract is authored"
    );
    let mut seen = BTreeSet::new();
    let (mut domains, mut preparation, mut either) = (0, 0, 0);
    let mut checks = 0;
    let (mut accepted_saves, mut refused_saves, mut no_save) = (0, 0, 0);
    let mut published_reports = 0;
    for case in replay::cases() {
        let Some(expected) = contracts.get(&case.label) else {
            continue;
        };
        assert!(seen.insert(case.label.clone()));
        replay::closed(expected, &["route", "save", "publishes", "saved"]);
        let route = replay::text(replay::field(expected, "route"));
        match route {
            "domain" => domains += 1,
            "preparation" => preparation += 1,
            "either" => either += 1,
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
        // A build case whose test read a saved report states its rows in
        // the replay, and names the published file here.
        let publishes = expected.get("publishes").map(replay::text);
        if publishes.is_some() {
            published_reports += 1;
            assert!(
                save.is_some()
                    && case.operations == ["build"]
                    && case.stated.report.is_some()
                    && case.stated.saved.is_none(),
                "{}: a build case that states its saved report's rows publishes it",
                case.label
            );
        }
        assert!(
            expected.get("saved").is_none() || (save == Some(true) && publishes.is_some()),
            "{}: authored output accompanies an accepted save and report",
            case.label
        );
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
        // A test that accepted either outcome pinned no route.
        if route != "either" {
            assert_eq!(route, actual_route, "{}: authored build route", case.label);
        }
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
                let staged = operations::contents(&study);
                let rows = crate::support::rows(&domain.issues());
                let result = domain.save();
                if save {
                    assert!(
                        matches!(result, Ok(true)),
                        "{}: expected Ok(true), got {result:?}",
                        case.label
                    );
                    // A case whose own operation saves states the bytes; a
                    // build case's test asserted the Ok(true) alone.
                    if let Some(expected) = case.stated.saved.as_ref() {
                        saved_bytes(&study, expected, &case.label);
                        // Save owns completed bytes, even after source inputs
                        // change. Expectations still come from the fixture.
                        // A benchmark's staged files are not the input's own.
                        let files = case.input.get("files").map(replay::object);
                        for name in files.into_iter().flat_map(|files| files.keys()) {
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
                        saved_build_artifacts(&study, &case, expected);
                    }
                } else {
                    // A failed build refuses every save alike (CHECK-2): each
                    // of two saves returns FailedBuild and leaves the build's
                    // complete issue rows as they were. The first publishes
                    // only the declared report (OUTPUT-10), the bytes its
                    // case states as saved, whose whole delta the replay
                    // compares; without one, every staged path stays. The
                    // second leaves the study as the first left it.
                    let refused = |result: Result<bool, file_application::FailedBuild>,
                                   domain: &file_application::Domain,
                                   attempt: &str| {
                        assert!(
                            matches!(result, Err(file_application::FailedBuild)),
                            "{}: {attempt} refused save: expected FailedBuild, got {result:?}",
                            case.label
                        );
                        assert_eq!(
                            crate::support::rows(&domain.issues()),
                            rows,
                            "{}: the {attempt} refused save keeps the build's issues",
                            case.label
                        );
                    };
                    refused(result, &domain, "first");
                    match case.stated.saved.as_ref().filter(|saved| !saved.is_empty()) {
                        Some(report) => saved_bytes(&study, report, &case.label),
                        // The report whose rows the build case states is
                        // the one file the refused save publishes.
                        None => match publishes {
                            Some(report) => {
                                let published = form::saved(&staged, &operations::contents(&study));
                                assert!(
                                    published.len() == 1
                                        && matches!(published.get(report), Some(Some(_))),
                                    "{}: a refused save publishes {report} alone, not {:?}",
                                    case.label,
                                    published.keys().collect::<Vec<_>>()
                                );
                                saved_report_rows(
                                    published[report].as_ref().unwrap(),
                                    case.stated.report.as_ref().unwrap(),
                                    &case.label,
                                );
                            }
                            None => assert_eq!(
                                operations::state(&study),
                                before,
                                "{}: a refused save preserves every staged path",
                                case.label
                            ),
                        },
                    }
                    let published = operations::state(&study);
                    let again = domain.save();
                    refused(again, &domain, "repeated");
                    assert_eq!(
                        operations::state(&study),
                        published,
                        "{}: repeated refused save preserves the report and every staged path",
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
    assert_eq!((domains, preparation, either, checks), (456, 53, 73, 110));
    assert_eq!((accepted_saves, refused_saves, no_save), (295, 98, 299));
    assert_eq!(published_reports, 5, "every published report is authored");
}

/// Seam: the composition's save return, held artifact order, capture counter
/// and absent directories are independent of the replay's saved-file delta.
/// A case's `built` specifications are built and saved first, as the replay's
/// `define` builds them, and a refused composition plans no file to save.
#[test]
fn migrated_define_cases_keep_save_artifact_and_target_contracts() {
    let authored = resource_contracts();
    let contracts = replay::object(replay::field(&authored, "define"));
    let mut seen = BTreeSet::new();
    let (mut accepted, mut refused, mut artifact_contracts) = (0, 0, 0);
    let mut ordered_artifacts = 0;
    for case in replay::cases() {
        let Some(expected) = contracts.get(&case.label) else {
            continue;
        };
        assert!(seen.insert(case.label.clone()));
        replay::closed(
            expected,
            &["save", "empty_artifacts", "artifacts", "absent"],
        );
        let study = operations::study(&case.input);
        let host =
            crate::support::host(case.input.get("host").map(replay::text).unwrap_or("python"));
        operations::build_first(
            &study,
            case.input.get("built").map_or(&[][..], replay::items),
            None,
            host,
        );
        let staged = operations::state(&study);
        let project = study.identity(replay::text(replay::field(&case.input, "project")));
        let section = operations::section(replay::field(&case.input, "section"));
        let mut define = yamaa_application::file_define::Define::prepare(
            &project,
            section,
            Some(replay::text(replay::field(&case.input, "created"))),
            "0.3.0",
            Some(host),
            None,
        );
        assert_eq!(
            operations::state(&study),
            staged,
            "{}: preparation publishes no files or directories",
            case.label
        );
        if let Some(empty) = expected.get("empty_artifacts") {
            artifact_contracts += 1;
            assert_eq!(
                &Value::Bool(define.artifacts().len() == 0),
                empty,
                "{}: the authored held-artifact contract",
                case.label
            );
        }
        if let Some(artifacts) = expected.get("artifacts") {
            ordered_artifacts += 1;
            assert_eq!(
                define
                    .artifacts()
                    .map(|(path, _, _)| path)
                    .collect::<Vec<_>>(),
                replay::items(artifacts)
                    .iter()
                    .map(replay::text)
                    .collect::<Vec<_>>(),
                "{}: the authored held-artifact order and cardinality",
                case.label
            );
        }
        let reads = define.capture_reads();
        let planned = define.plan().is_some();
        let result = define.save();
        match replay::field(expected, "save") {
            Value::Bool(true) => {
                accepted += 1;
                assert!(
                    matches!(result, Ok(true)),
                    "{}: expected Ok(true), got {result:?}",
                    case.label
                );
                saved_bytes(&study, case.stated.saved.as_ref().unwrap(), &case.label);
            }
            Value::Bool(false) => {
                refused += 1;
                assert!(
                    !planned,
                    "{}: a refused composition plans no file to save",
                    case.label
                );
                assert!(
                    result.is_err(),
                    "{}: refused composition must fail its save, got {result:?}",
                    case.label
                );
                assert_eq!(
                    operations::state(&study),
                    staged,
                    "{}: refused save preserves every staged path",
                    case.label
                );
            }
            other => panic!("{}: unknown save contract {other:?}", case.label),
        }
        assert_eq!(
            define.capture_reads(),
            reads,
            "{}: save reads no source again",
            case.label
        );
        for path in replay::items(replay::field(expected, "absent")) {
            let path = replay::text(path);
            assert!(
                !study.join(path).exists(),
                "{}: {path} remains absent, including empty directories",
                case.label
            );
        }
    }
    assert_eq!(
        seen,
        contracts.keys().cloned().collect(),
        "every define API case remains present"
    );
    assert_eq!(
        (accepted, refused, artifact_contracts, ordered_artifacts),
        (11, 29, 3, 5)
    );
}

/// Seam: a refused written path has no accepted output and cannot publish,
/// independently of the diagnostic projection or the save's byte delta.
#[test]
fn migrated_written_paths_keep_output_and_publication_contracts() {
    let authored = resource_contracts();
    let contracts = replay::items(replay::field(&authored, "written_paths"))
        .iter()
        .map(|name| replay::text(name).to_owned())
        .collect::<BTreeSet<_>>();
    let mut seen = BTreeSet::new();
    for case in replay::cases() {
        if !contracts.contains(&case.label) {
            continue;
        }
        assert!(seen.insert(case.label.clone()));
        let Calls { study, built, .. } = calls(&case.input, Language::Python, &case.label);
        let staged = operations::state(&study);
        if let Ok(mut domain) = built {
            assert!(
                domain.output().is_none(),
                "{}: refused path has no accepted output",
                case.label
            );
            let saved = domain.save();
            assert!(
                matches!(saved, Err(file_application::FailedBuild)),
                "{}: refused domain save: {saved:?}",
                case.label
            );
        }
        assert_eq!(
            operations::state(&study),
            staged,
            "{}: refusal publishes no path",
            case.label
        );
        assert!(
            !study.join("entry/output.csv").exists(),
            "{}: refused build leaves output absent",
            case.label
        );
    }
    assert_eq!(
        seen, contracts,
        "every written-path API case remains present"
    );
    assert_eq!(seen.len(), 9);
}

/// Seam: the calls a scripted host-functions port received, and where the
/// run started reading its study input, which no issue row, table or saved
/// file states (`operations::calls`).
#[test]
fn host_function_cases_keep_the_calls_their_port_received() {
    let authored =
        yamaa_adapters::json_wire::decode(include_str!("fixtures/function_call_contracts.json"))
            .unwrap();
    replay::closed(&authored, &["about", "cases"]);
    let contracts = replay::object(replay::field(&authored, "cases"));
    let mut seen = BTreeSet::new();
    for case in replay::cases() {
        let host = case
            .operations
            .iter()
            .any(|operation| operation.starts_with("host_"));
        let Some(expected) = contracts.get(&case.label) else {
            assert!(
                !host,
                "{}: a scripted host's case states its calls",
                case.label
            );
            continue;
        };
        assert!(seen.insert(case.label.clone()));
        let expected = replay::items(expected)
            .iter()
            .map(|entry| replay::text(entry).to_owned())
            .collect::<Vec<_>>();
        for operation in &case.operations {
            assert_eq!(
                operations::calls(operation, &case.input),
                expected,
                "{} ({operation}): the calls its port received",
                case.label
            );
        }
    }
    assert_eq!(
        seen,
        contracts.keys().cloned().collect(),
        "every case with authored calls remains present"
    );
    assert_eq!(seen.len(), 51);
}

/// Seam: the R-host calls of the language-header tests whose cases build,
/// read a project's listings or compose a submission. The host is read only
/// for a project's environment, functions or lock, so each case is stated
/// once and the replay runs it for the default host; those tests also called
/// the R host, so this seam runs each operation of those cases for R and
/// compares what it reports with the case's rows.
#[test]
fn language_header_cases_keep_their_r_host_calls() {
    let (mut cases, mut calls) = (0, 0);
    for case in replay::cases() {
        if !case.label.starts_with("language_header.json::")
            || !case.operations.iter().any(|operation| {
                matches!(
                    operation.as_str(),
                    "build" | "define" | "listing_failure" | "listing_step"
                )
            })
        {
            continue;
        }
        assert!(
            case.input.get("host").is_none(),
            "{}: a case is stated once, for every host",
            case.label
        );
        cases += 1;
        let mut input = case.input.clone();
        input["host"] = Value::String("r".into());
        for operation in &case.operations {
            calls += 1;
            let reported = operations::run(operation, &input);
            replay::compare_issues(
                &case,
                &reported.issues,
                reported.study.as_ref(),
                &format!("{} ({operation}, R)", case.label),
            );
        }
    }
    assert_eq!((cases, calls), (16, 29));
}
