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
//! its issues afterwards, with the files the save created or changed; a
//! refused request saves nothing. Each takes `files` (each relative path and
//! its text), `directories` (empty ones), the `specification` and the
//! optional `project` a request names, and the `host` it runs for, `python`
//! unless written.
use super::{contents, host, path, rows, study, File, Reported};
use crate::support::diagnostic::{
    form::{Files, Outcome},
    replay::{closed, field, items, text},
};
use yamaa_core::{domain_entry::Request, json::Value};

pub(super) const FILES: &[File] = &[
    fixture!("declarative_case_predicates.json"),
    fixture!("language_header.json"),
    fixture!("lookup_matched_derivations.json"),
    fixture!("producer_links.json"),
    fixture!("project_run.json"),
    fixture!("submission.json"),
    fixture!("workflow.json"),
];

pub(super) fn run(operation: &str, input: &Value) -> Option<Reported> {
    let members = match operation {
        "check" | "check_resources" => {
            &["files", "directories", "specification", "project", "host"][..]
        }
        "build" | "save" => &[
            "files",
            "directories",
            "built",
            "specification",
            "project",
            "host",
        ][..],
        _ => return None,
    };
    closed(input, members);
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
            match crate::support::study::domain(
                host,
                Request {
                    specification: &specification,
                    environment: project.as_deref(),
                },
                identity,
            ) {
                Ok(mut domain) => {
                    // What the save returns follows from its issues and the
                    // files it wrote, which the case states.
                    let saved = saves.then(|| {
                        let before = contents(&study);
                        let _ = domain.save();
                        let mut saved = contents(&study);
                        saved.retain(|name, bytes| before.get(name) != Some(bytes));
                        saved
                    });
                    let outcome = Outcome {
                        output: domain.output().map(rows),
                        report: Some(rows(domain.run_report())),
                        saved,
                    };
                    (domain.issues(), outcome)
                }
                Err(issues) => {
                    let saved = saves.then(Files::new);
                    (
                        issues,
                        Outcome {
                            saved,
                            ..Outcome::default()
                        },
                    )
                }
            }
        }
    };
    Some(Reported {
        issues,
        study: Some(study),
        outcome,
    })
}
