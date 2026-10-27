//! The define route's operation: one submission section of a project
//! composed as both hosts compose it (`file_define::Define`), then saved.
//!
//! `define` composes the `section` (`sdtm`, `adam` or `send`) the project
//! file `project` declares, for the `host` it runs for, `python` unless
//! written, at the creation time `created` and with the installed version
//! `0.3.0` every case gives, then saves the composition, as `save` saves a
//! build. Each specification its `built` names is first built and saved
//! alone, with no project, in order, as a run of that specification saves
//! its artifact and its run report, which the composition then reads back
//! (PROJECT-15). It returns the issues afterwards, so a save that fails
//! reports its own, and each file of the study directory the save created,
//! changed or removed, read against the directory as the case staged it,
//! once its `built` specifications saved; a refused composition saves
//! nothing. It takes the study a case stages ([`super::study`]).
use super::{build_first, contents, host, path, section, study, File, Reported, STUDY};
use crate::support::diagnostic::{
    form::{self, Outcome},
    replay::{closed, field, items, text},
};
use yamaa_application::file_define::Define;
use yamaa_core::json::Value;

pub(super) const FILES: &[File] = &[
    fixture!("shared_define.json"),
    fixture!("define_versions.json"),
];

pub(super) fn run(operation: &str, input: &Value) -> Option<Reported> {
    if operation != "define" {
        return None;
    }
    closed(
        input,
        &[
            &STUDY[..],
            &["built", "project", "section", "created", "host"],
        ]
        .concat(),
    );
    let study = study(input);
    build_first(
        &study,
        input.get("built").map_or(&[][..], items),
        None,
        host(input),
    );
    let staged = contents(&study);
    let mut define = Define::prepare(
        &path(&study, text(field(input, "project"))),
        section(field(input, "section")),
        Some(text(field(input, "created"))),
        "0.3.0",
        Some(host(input)),
        None,
    );
    // What the save returns follows from its issues and what it did to the
    // study directory, which the case states.
    let _ = define.save();
    let saved = form::saved(&staged, &contents(&study));
    Some(Reported {
        issues: define.issues().to_vec(),
        study: Some(study),
        outcome: Outcome {
            saved: Some(saved),
            ..Outcome::default()
        },
    })
}
