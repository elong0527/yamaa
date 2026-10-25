//! The preparation's operation: a specification read through the shipped
//! schema, as every route reads it before it compiles anything.
//!
//! `prepare_specification` prepares the `specification` a case stages
//! ([`super::study`]) with the closures the package ships
//! (`shipped_schema::prepare`): its layers are admitted, composed and
//! lowered, each parent read beside the file that declares it, by its
//! canonical path (`support::inheritance::Files`). It returns the findings a
//! refused preparation reports (`inheritance_failure_ref`), naming the layer
//! each is in and the entry, or none when the preparation admits the
//! specification. Nothing is compiled, no study input is read and nothing is
//! saved. A refusal that is not the language findings of the specification,
//! such as a fault, fails the reader rather than the case.
use super::{path, study, File, Reported, STUDY};
use crate::support::diagnostic::{
    form::Outcome,
    replay::{closed, field, text},
};
use yamaa_adapters::{
    shipped_schema,
    specification_diagnostics::{inheritance_failure_ref, Rejection},
    specification_source::Source,
};
use yamaa_core::json::Value;

pub(super) const FILES: &[File] = &[fixture!("one_spelling.json")];

pub(super) fn run(operation: &str, input: &Value) -> Option<Reported> {
    if operation != "prepare_specification" {
        return None;
    }
    closed(input, &[&STUDY[..], &["specification"]].concat());
    let study = study(input);
    let entry = path(&study, text(field(input, "specification")));
    let prepared = shipped_schema::prepare(
        Source {
            identity: entry.clone(),
            bytes: std::fs::read(&entry).unwrap(),
        },
        entry.clone(),
        &mut crate::support::inheritance::Files,
    );
    let issues = match prepared {
        Ok(_) => Vec::new(),
        Err(error) => match inheritance_failure_ref(&error) {
            Ok(Rejection::Invalid(findings)) => findings,
            other => panic!("{entry}: expected the specification's findings, got {other:?}"),
        },
    };
    Some(Reported {
        issues,
        study: Some(study),
        outcome: Outcome::default(),
    })
}
