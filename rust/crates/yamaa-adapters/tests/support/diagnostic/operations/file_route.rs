//! The file route's operations: a study directory of authored files, checked
//! or built as every host runs it, through the host-functions port's empty
//! double.
//!
//! `check` is the static check of a request (`file_application::check`):
//! what a refused preparation reports, or the compiled specification's own
//! findings, with no study input read. Each projection a refusal has, of a
//! refused capture or of a refused compilation, is one of these: the rows
//! `project_application::selection_failure` gives it.
//! `check_resources` is the same check of names relative to the study
//! directory, which the resources take as their root
//! (`file_workflow::select_with_resources`), so the paths a refusal names
//! keep that spelling; it needs the `project`. `build` prepares and runs the
//! request (`support::study::domain`) and returns the issues of its result
//! or of its refusal, with the output table and the run report a result
//! holds; each specification its `built` names is built and saved first, in
//! order. `save` builds as `build` does, then saves the result, and returns
//! its issues afterwards. Refused or not, it also returns each file of the
//! study directory the request created, changed or removed, read against the
//! directory as the case staged it, once its `built` specifications saved; a
//! refused request saves nothing, and a save that fails reports its own
//! issues, such as an output directory that does not exist. Each takes the
//! study a case stages ([`super::study`]), the `specification` and the
//! optional `project` a request names, and the `host` it runs for, `python`
//! unless written.
use super::{contents, host, path, rows, study, File, Reported, STUDY};
use crate::support::diagnostic::{
    form::{self, Outcome},
    replay::{closed, field, items, text},
};
use yamaa_core::{domain_entry::Request, json::Value};

pub(super) const FILES: &[File] = &[
    fixture!("aggregate_record_operations.json"),
    fixture!("case_item_order.json"),
    fixture!("check_keyless_donor.json"),
    fixture!("check_output_validation.json"),
    fixture!("check_row_phase_boundaries.json"),
    fixture!("check_row_scopes.json"),
    fixture!("check_unknown_reads.json"),
    fixture!("check_variable_forms.json"),
    fixture!("column_driver_bindings.json"),
    fixture!("completed_intermediates.json"),
    fixture!("csv_precision.json"),
    fixture!("dataset_json_sources.json"),
    fixture!("declaration_vectors.json"),
    fixture!("declarative_case_predicates.json"),
    fixture!("declarative_self.json"),
    fixture!("empty_child.json"),
    fixture!("file_origins.json"),
    fixture!("inheritance.json"),
    fixture!("integer_literals.json"),
    fixture!("intermediate_driver_columns.json"),
    fixture!("key_selection.json"),
    fixture!("language_header.json"),
    fixture!("layer_composition.json"),
    fixture!("local_paths.json"),
    fixture!("lookup_matched_derivations.json"),
    fixture!("lookup_model.json"),
    fixture!("missing_order.json"),
    fixture!("odm_source_reads.json"),
    fixture!("producer_links.json"),
    fixture!("project_files.json"),
    fixture!("project_run.json"),
    fixture!("rounding.json"),
    fixture!("row_derivations.json"),
    fixture!("row_intermediate_operands.json"),
    fixture!("row_scopes.json"),
    fixture!("run_report.json"),
    fixture!("scalar_operations.json"),
    fixture!("secondary_operands.json"),
    fixture!("specification_runs.json"),
    fixture!("static_declarations.json"),
    fixture!("submission.json"),
    fixture!("temporal.json"),
    fixture!("temporal_imputation.json"),
    fixture!("text_case.json"),
    fixture!("text_extract.json"),
    fixture!("text_pad.json"),
    fixture!("text_replace.json"),
    fixture!("typed_csv_sources.json"),
    fixture!("value_semantics.json"),
    fixture!("verification.json"),
    fixture!("windows.json"),
    fixture!("workflow.json"),
];

pub(super) fn run(operation: &str, input: &Value) -> Option<Reported> {
    let members = match operation {
        "check" | "check_resources" => &["specification", "project", "host"][..],
        "build" | "save" => &["built", "specification", "project", "host"][..],
        _ => return None,
    };
    closed(input, &[&STUDY[..], members].concat());
    let study = study(input);
    let host = host(input);
    let project = input.get("project").map(|name| path(&study, text(name)));
    let specification = path(&study, text(field(input, "specification")));
    let identity = crate::support::study::identity("diagnostics", &specification);
    let (issues, outcome) = match operation {
        "check" => (
            yamaa_adapters::file_application::check(
                host,
                Request {
                    specification: &specification,
                    environment: project.as_deref(),
                },
                identity,
            ),
            Outcome::default(),
        ),
        "check_resources" => {
            let root = path(&study, "");
            let selected = yamaa_adapters::file_workflow::select_with_resources(
                yamaa_adapters::file_resources::Resources::new(&root, &root, &[]).unwrap(),
                text(field(input, "specification")),
                text(field(input, "project")),
                host,
                yamaa_adapters::document::Closures::shipped().unwrap(),
            );
            let issues = match selected {
                Ok(prepared) => yamaa_adapters::specification_check::issues(prepared.run()),
                Err(error) => yamaa_adapters::project_application::selection_failure(&error),
            };
            (issues, Outcome::default())
        }
        _ => {
            for name in input.get("built").map_or(&[][..], items) {
                let built = path(&study, text(name));
                let mut domain = crate::support::study::domain(
                    host,
                    Request {
                        specification: &built,
                        environment: project.as_deref(),
                    },
                    crate::support::study::identity("diagnostics", &built),
                )
                .unwrap_or_else(|issues| panic!("{built} is refused: {issues:?}"));
                assert!(domain.issues().is_empty(), "{built}: {:?}", domain.issues());
                assert!(domain.save().unwrap(), "{built} saves");
            }
            let saves = operation == "save";
            // The directory as the case staged it, which what the request
            // did is read against, whether it is refused or not.
            let staged = saves.then(|| contents(&study));
            let (issues, outcome) = match crate::support::study::domain(
                host,
                Request {
                    specification: &specification,
                    environment: project.as_deref(),
                },
                identity,
            ) {
                Ok(mut domain) => {
                    if saves {
                        // What the save returns follows from its issues and
                        // what it did to the study directory, which the case
                        // states.
                        let _ = domain.save();
                    }
                    let outcome = Outcome {
                        output: domain.output().map(rows),
                        report: Some(rows(domain.run_report())),
                        saved: None,
                    };
                    (domain.issues(), outcome)
                }
                Err(issues) => (issues, Outcome::default()),
            };
            let saved = staged.map(|staged| form::saved(&staged, &contents(&study)));
            (issues, Outcome { saved, ..outcome })
        }
    };
    Some(Reported {
        issues,
        study: Some(study),
        outcome,
    })
}
