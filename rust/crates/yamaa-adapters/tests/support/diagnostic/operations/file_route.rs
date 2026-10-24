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
//! or of its refusal; each specification its `built` names is built and
//! saved first, in order. Each takes `files` (each relative path and its
//! text), `directories` (empty ones), the `specification` and the optional
//! `project` a request names, and the `host` it runs for, `python` unless
//! written.
use super::{host, path, study, File, Reported};
use crate::support::diagnostic::replay::{closed, field, items, text};
use yamaa_core::{domain_entry::Request, json::Value};

pub(super) const FILES: &[File] = &[
    fixture!("language_header.json"),
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
        "build" => &[
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
    let issues = match operation {
        "check" => yamaa_adapters::file_application::check(
            host,
            Request {
                specification: &specification,
                environment: project.as_deref(),
            },
            identity,
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
            match selected {
                Ok(prepared) => yamaa_adapters::specification_check::issues(prepared.run()),
                Err(error) => yamaa_adapters::project_application::selection_failure(&error),
            }
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
            match crate::support::study::domain(
                host,
                Request {
                    specification: &specification,
                    environment: project.as_deref(),
                },
                identity,
            ) {
                Ok(domain) => domain.issues(),
                Err(issues) => issues,
            }
        }
    };
    Some(Reported {
        issues,
        study: Some(study),
    })
}
