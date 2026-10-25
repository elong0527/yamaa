#![cfg(all(feature = "yaml", feature = "parquet"))]
//! The oracle of #3805 over the file route: the seam between the checks
//! pass, which still reports `check()`, and binding over the declared input
//! schemas, which will. Every case of `tests/fixtures/diagnostics/`, every
//! benchmark entry and every `diagnostic-parity` and `project-parity` case
//! of `rust/tests/host-fixtures/cases.json` makes its calls while the engine
//! records, for each specification it prepares, both implementations'
//! complete rows; they must agree but for the differences #3805 records,
//! each listed with its class. The oracle goes with the pass.
use std::path::Path;

use crate::support::{
    benchmarks,
    diagnostic::{operations, replay},
    study, Scratch,
};
use yamaa_core::{
    domain_entry::Request,
    json::Value,
    project_function::Language,
    specification::check_oracle::{self, Class, Comparison, Difference},
};

/// Check the specification at `path`, with the environment at
/// `environment`, recording what each specification it prepares compares.
fn check(
    host: Language,
    path: &Path,
    environment: Option<&Path>,
    label: &str,
    recorded: &mut Vec<(String, Comparison)>,
) {
    let path = path.to_string_lossy();
    let environment = environment.map(|file| file.to_string_lossy());
    let request = Request {
        specification: &path,
        environment: environment.as_deref(),
    };
    let (_, comparisons) = check_oracle::record(|| {
        yamaa_adapters::file_application::check(host, request, study::identity(label, &path))
    });
    recorded.extend(
        comparisons
            .into_iter()
            .map(|comparison| (label.to_owned(), comparison)),
    );
}

fn judged(recorded: &[(String, Comparison)], differences: &[Difference]) {
    if let Err(failure) = check_oracle::judge(recorded, differences) {
        panic!("{failure}");
    }
}

/// The differences of this crate's cases, as #3805 records them.
const CASE_DIFFERENCES: &[Difference] = &[
    // An aggregate range with neither bound (LOOKUP-9).
    Difference {
        case: "aggregate_record_operations.json::an_aggregate_range_with_neither_bound_is_refused",
        domain: "result",
        class: Class::Q3,
    },
    // A lookup's template placeholder of another lookup's field: binding
    // takes the field untyped, as its catalog does not type it, and
    // execution refuses its type (TEXT-1).
    Difference {
        case: "check_keyless_donor.json::a_lookup_placeholder_of_another_lookup_s_field_is_typed",
        domain: "out",
        class: Class::Q2,
    },
    // A contextual key whose value a derivation types as `str` (LOOKUP-9).
    Difference {
        case: "completed_intermediates.json::a_contextual_key_rejects_a_known_field_of_another_type",
        domain: "out",
        class: Class::Q3,
    },
    // A SELF derivation's read of a column no declaration names
    // (LOOKUP-15), bare and qualified.
    Difference {
        case: "completed_intermediates.json::a_self_derivation_rejects_an_unknown_field",
        domain: "out",
        class: Class::Q3,
    },
    Difference {
        case: "completed_intermediates.json::a_self_derivation_rejects_an_unknown_qualified_field",
        domain: "out",
        class: Class::Q3,
    },
    // `NULLIF` over a declared int and float as a shift's days (TIME-14).
    Difference {
        case: "completed_intermediates.json::nullif_over_an_int_and_a_float_checks_clean_as_integer_days",
        domain: "out",
        class: Class::Q3,
    },
    Difference {
        case: "completed_intermediates.json::nullif_over_an_int_and_a_float_is_refused_as_integer_days_when_bound",
        domain: "out",
        class: Class::Q3,
    },
    // A SELF derivation named as a completed column: binding's
    // `duplicate_derivation`, which `run()` reports, replaces the pass's
    // `unknown_field` (LOOKUP-15).
    Difference {
        case: "completed_intermediates.json::self_derivations_cannot_shadow_completed_stored_fields",
        domain: "out",
        class: Class::Q3,
    },
    // A bare name in a correlated filter, which the donor's `types` states,
    // with its qualifier suggested (LOOKUP-7), and the read of the lookup
    // that follows (NAME-3).
    Difference {
        case: "declarative_case_predicates.json::a_bare_stored_field_in_a_correlated_filter_suggests_its_qualifier",
        domain: "result",
        class: Class::Q3,
    },
    // A read of a field a lookup's `columns` hides (LOOKUP-11).
    Difference {
        case: "declarative_case_predicates.json::a_hidden_read_has_the_complete_original_reference_finding_and_no_output",
        domain: "result",
        class: Class::Q3,
    },
    // A row template's read of a SELF lookup's column the column phase
    // completes (LOOKUP-11).
    Difference {
        case: "declarative_self.json::column_only_donor_is_unavailable_to_later_templates",
        domain: "result",
        class: Class::Q3,
    },
    // A column's read of a field its driving intermediate's `columns` leaves
    // out (NAME-3, ROW-2).
    Difference {
        case: "intermediate_driver_columns.json::columns_list_limits_the_driver_fields",
        domain: "out",
        class: Class::Q3,
    },
    // A range value of the intermediate driving the rows (unsupported).
    Difference {
        case: "lookup_model.json::a_range_value_naming_a_field_its_driver_intermediate_derives_is_unsupported",
        domain: "out",
        class: Class::Q3,
    },
    // A grouped row aggregate over its driver's bare field (LOOKUP-18): the
    // pass also reports a cycle through the column (COLUMN-6), which neither
    // binding nor `run()` reports.
    Difference {
        case: "specification_runs.json::aggregate_grammar_findings_follow_typed_ingestion_and_retain_original_path_for_a_bare_driver_field",
        domain: "adlb",
        class: Class::Q2,
    },
    Difference {
        case: "specification_runs.json::aggregate_grammar_findings_follow_typed_ingestion_and_retain_original_path_for_a_bare_driver_field_after_an_unparsed_float",
        domain: "adlb",
        class: Class::Q2,
    },
    // A read of a field a named driver's projection hides (NAME-3, LOOKUP-11).
    Difference {
        case: "row_scopes.json::a_named_driver_s_projection_hides_a_field_from_a_derivation",
        domain: "out",
        class: Class::Q3,
    },
    Difference {
        case: "row_scopes.json::a_named_driver_s_projection_hides_a_field_from_a_group_key",
        domain: "out",
        class: Class::Q3,
    },
    // A row template's date operation over an undeclared field of a
    // delimited input, `str` by INPUT-5 (TIME-1).
    Difference {
        case: "temporal.json::a_missing_input_precedes_a_row_template_date_diff_type_error",
        domain: "result",
        class: Class::Q3,
    },
    Difference {
        case: "temporal.json::a_missing_input_precedes_a_row_template_study_day_type_error",
        domain: "result",
        class: Class::Q3,
    },
    Difference {
        case: "temporal.json::a_missing_input_precedes_a_row_template_to_epoch_day_type_error",
        domain: "result",
        class: Class::Q3,
    },
    Difference {
        case: "temporal.json::a_row_template_date_diff_refuses_text_operands",
        domain: "result",
        class: Class::Q3,
    },
    Difference {
        case: "temporal.json::a_row_template_study_day_refuses_text_operands",
        domain: "result",
        class: Class::Q3,
    },
    Difference {
        case: "temporal.json::a_row_template_to_epoch_day_refuses_a_text_operand",
        domain: "result",
        class: Class::Q3,
    },
];

#[test]
fn binding_reports_what_the_pass_reports_for_every_case() {
    let mut recorded = Vec::new();
    for case in replay::cases() {
        for operation in &case.operations {
            let (_, comparisons) = check_oracle::record(|| operations::run(operation, &case.input));
            recorded.extend(
                comparisons
                    .into_iter()
                    .map(|comparison| (case.label.clone(), comparison)),
            );
        }
    }
    judged(&recorded, CASE_DIFFERENCES);
}

/// The differences of the benchmark entries, as #3805 records them.
const BENCHMARK_DIFFERENCES: &[Difference] = &[];

/// The entries of the benchmark at `case`, each host's in its order, an
/// entry R repeats from Python once: its host, specification and
/// environment. A benchmark without `entries` has its one specification
/// for Python, under its project when it has one.
fn entries(case: &Path) -> Vec<(Language, String, Option<String>)> {
    let metadata = benchmarks::yaml(&case.join("benchmark.yaml"));
    let project = case
        .join("project.yaml")
        .is_file()
        .then(|| "project.yaml".to_owned());
    let mut entries = Vec::new();
    let Some(listed) = metadata["entries"].as_object() else {
        let mut specifications: Vec<_> = std::fs::read_dir(case)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|file| file.starts_with("spec") && file.ends_with(".yaml"))
            .collect();
        specifications.sort();
        for specification in specifications {
            entries.push((Language::Python, specification, project.clone()));
        }
        return entries;
    };
    for (host, language) in [("python", Language::Python), ("r", Language::R)] {
        for entry in listed
            .get(host)
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let specification = entry["specification"].as_str().unwrap().to_owned();
            let environment = match entry.get("environment") {
                None => project.clone(),
                Some(Value::Null) => None,
                Some(file) => Some(file.as_str().unwrap().to_owned()),
            };
            let repeated = entries
                .iter()
                .any(|(_, other, at)| *other == specification && *at == environment);
            if !repeated {
                entries.push((language, specification, environment));
            }
        }
    }
    entries
}

#[test]
fn binding_reports_what_the_pass_reports_for_every_benchmark_entry() {
    let mut recorded = Vec::new();
    for case in benchmarks::all() {
        let name = case.file_name().unwrap().to_string_lossy().into_owned();
        for (host, specification, environment) in entries(&case) {
            let label = format!("{name}/{specification}");
            let environment = environment.map(|file| case.join(file));
            check(
                host,
                &case.join(&specification),
                environment.as_deref(),
                &label,
                &mut recorded,
            );
        }
    }
    judged(&recorded, BENCHMARK_DIFFERENCES);
}

/// The differences of the parity cases, as #3805 records them.
const PARITY_DIFFERENCES: &[Difference] = &[
    // An aggregate range with neither bound (LOOKUP-9), and an aggregate
    // key that pairs nothing (LOOKUP-5).
    Difference {
        case: "diagnostic-parity::aggregate_between_without_bound",
        domain: "output",
        class: Class::Q3,
    },
    Difference {
        case: "diagnostic-parity::aggregate_empty_key_list",
        domain: "output",
        class: Class::Q3,
    },
    Difference {
        case: "diagnostic-parity::aggregate_empty_key_mapping",
        domain: "output",
        class: Class::Q3,
    },
];

/// Write each file of `files`, a map of relative paths to their text, under
/// `root`.
fn write(root: &Path, files: &Value) {
    for (path, text) in files.as_object().into_iter().flatten() {
        let path = root.join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text.as_str().unwrap()).unwrap();
    }
}

#[test]
fn binding_reports_what_the_pass_reports_for_every_parity_case() {
    let truth: Value =
        yamaa_adapters::json_wire::decode(include_str!("../../../tests/host-fixtures/cases.json"))
            .unwrap();
    let mut recorded = Vec::new();
    for case in truth["families"]["diagnostic-parity"]["cases"]
        .as_array()
        .unwrap()
    {
        // A case without a specification has nothing to prepare.
        let Some(specification) = case["specification_yaml"].as_str() else {
            continue;
        };
        let label = format!("diagnostic-parity::{}", case["name"].as_str().unwrap());
        let root = Scratch::new("check-oracle");
        std::fs::write(root.join("spec.yaml"), specification).unwrap();
        if let Some(input) = case["input_csv"].as_str() {
            std::fs::write(root.join("input.csv"), input).unwrap();
        }
        check(
            Language::Python,
            &root.join("spec.yaml"),
            None,
            &label,
            &mut recorded,
        );
    }
    for case in truth["families"]["project-parity"]["cases"]
        .as_array()
        .unwrap()
    {
        let label = format!("project-parity::{}", case["name"].as_str().unwrap());
        let root = Scratch::new("check-oracle");
        write(&root, &case["files"]);
        write(&root, &case["host_files"]["python"]);
        let project = root.join("project.yaml");
        let environment = project.is_file().then_some(project.as_path());
        let mut specifications: Vec<_> = case["files"]
            .as_object()
            .unwrap()
            .iter()
            .filter(|(path, text)| {
                path.ends_with(".yaml")
                    && text
                        .as_str()
                        .is_some_and(|text| text.contains("\ntype: spec\n"))
            })
            .map(|(path, _)| path.clone())
            .collect();
        specifications.sort();
        for specification in specifications {
            check(
                Language::Python,
                &root.join(&specification),
                environment,
                &format!("{label}/{specification}"),
                &mut recorded,
            );
        }
    }
    judged(&recorded, PARITY_DIFFERENCES);
}
