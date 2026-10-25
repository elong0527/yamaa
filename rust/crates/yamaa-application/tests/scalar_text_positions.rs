#![cfg(feature = "yaml")]
//! The positional text verbs, `str_substr` and `str_length` (TEXT-13,
//! TEXT-14), replayed from the shared host truth that the Python and R suites
//! read too.
use crate::support::study::{identity, issue_row, Study};
use yamaa_adapters::file_application;
use yamaa_core::{
    diagnostic::Diagnostic, domain_entry::Request, json::Value, project_function::Language,
};

/// The issue rows of `issues`, as the shared truth writes them.
fn rows(issues: &[Diagnostic]) -> Vec<Value> {
    issues.iter().map(issue_row).collect()
}

/// Each case of the `text_positions` group: its check before any input
/// exists, then its run, with the authored issue rows, saved CSV and count of
/// `missing` answers.
#[test]
fn positional_text_cases_match_the_shared_host_truth() {
    let truth: Value =
        yamaa_adapters::json_wire::decode(include_str!("../../../tests/host-fixtures/cases.json"))
            .unwrap();
    for case in truth["families"]["original-specifications"]["text_positions"]
        .as_array()
        .unwrap()
    {
        let name = case["name"].as_str().unwrap();
        let expected = case["issues"].as_array().map_or(&[][..], Vec::as_slice);
        let study = Study::new("text-positions");
        study.write("spec.yaml", case["spec"].as_str().unwrap());
        let specification = study.path("spec.yaml");
        let checked = file_application::check(
            Language::Python,
            Request {
                specification: &specification,
                environment: None,
            },
            identity(name, &specification),
        );
        assert_eq!(rows(&checked), expected, "{name}: the check");
        for (path, text) in case["inputs"].as_object().unwrap() {
            study.write(path, text.as_str().unwrap());
        }
        match study.run("spec.yaml") {
            Ok(mut domain) => {
                assert_eq!(rows(&domain.issues()), expected, "{name}: the run");
                let missing: u64 = domain.observations()["handler_counts"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter(|count| count["handler"] == "missing")
                    .map(|count| count["count"].as_u64().unwrap())
                    .sum();
                assert_eq!(
                    missing,
                    case["missing_count"].as_u64().unwrap(),
                    "{name}: the missing answers"
                );
                match case["csv"].as_str() {
                    Some(csv) => {
                        assert!(domain.save().unwrap(), "{name}: the save");
                        assert_eq!(study.saved("result.csv"), csv.as_bytes(), "{name}");
                    }
                    None => assert!(domain.output().is_none(), "{name}: no output"),
                }
            }
            Err(refused) => assert_eq!(rows(&refused), expected, "{name}: the run"),
        }
    }
}
