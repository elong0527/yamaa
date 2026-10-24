//! The file route's operations: a study directory of authored files, checked
//! or built as every host runs it, through the host-functions port's empty
//! double.
//!
//! `check` is the static check of a request (`file_application::check`):
//! what a refused preparation reports, or the compiled specification's own
//! findings, with no study input read. `build` prepares and runs the request
//! (`support::study::domain`) and returns the issues of its result or of its
//! refusal; each specification its `built` names is built and saved first,
//! in order. Either takes `files` (each relative path and its text),
//! `directories` (empty ones), the `specification` and the optional
//! `project` a request names, and the `host` it runs for, `python` unless
//! written.
use super::{host, path, study, File, Reported};
use crate::support::diagnostic::replay::{closed, field, items, text};
use yamaa_core::{domain_entry::Request, json::Value};

pub(super) const FILES: &[File] = &[fixture!("producer_links.json"), fixture!("submission.json")];

pub(super) fn run(operation: &str, input: &Value) -> Option<Reported> {
    let members = match operation {
        "check" => &["files", "directories", "specification", "project", "host"][..],
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
