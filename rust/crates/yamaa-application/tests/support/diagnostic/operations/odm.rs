//! The ODM XML reader's operation: one file of the study directory read into
//! the clinical-item table (`odm_xml::read`).
//!
//! `read_odm` reads the study file `source` names, a path relative to the
//! study directory that need not reach a file, under the read's `options`:
//! the members of `odm_items::Options` the input writes (`archive_member`,
//! `max_archive_members`, `max_expanded_bytes` and `max_xml_depth`), and the
//! default of each other. It returns the one issue a refusal reports, naming
//! the source by its path as a caller gives it (`odm_xml::Error::issue`), or
//! none and the table it read. It takes the study a case stages
//! ([`super::study`]).
//!
//! The reader is the `odm` feature's. A build without it compiles the
//! reader, whose every read fails as the build's missing reader, which
//! reports no issue and so refuses the input; the replay, which runs the
//! cases, is built only with the feature (`tests/diagnostics.rs`). Its file
//! stays declared in every build, so the cases the other seams read
//! (`replay::cases`) are the same in each.
use super::{path, rows, study, File, Reported, STUDY};
use crate::support::diagnostic::{
    form::Outcome,
    replay::{closed, field, text},
};
use std::path::Path;
use yamaa_core::{json::Value, odm_items::Options};

pub(super) const FILES: &[File] = &[fixture!("odm_reads_xml.json"), fixture!("odm_ascii.json")];

pub(super) fn run(operation: &str, input: &Value) -> Option<Reported> {
    if operation != "read_odm" {
        return None;
    }
    closed(input, &[&STUDY[..], &["source", "options"]].concat());
    let study = study(input);
    let source = path(&study, text(field(input, "source")));
    let options = input.get("options").map_or_else(Options::default, options);
    let (issues, outcome) = match yamaa_adapters::odm_xml::read(Path::new(&source), &options) {
        Ok(table) => (
            vec![],
            Outcome {
                output: Some(rows(&table)),
                ..Outcome::default()
            },
        ),
        Err(error) => {
            let issue = error
                .issue(&source)
                .unwrap_or_else(|| panic!("{error:?} reports no issue"));
            (vec![issue], Outcome::default())
        }
    };
    Some(Reported {
        issues,
        study: Some(study),
        outcome,
    })
}

/// The read's options: each member written, and the default of each other.
fn options(written: &Value) -> Options {
    let members = closed(
        written,
        &[
            "archive_member",
            "max_archive_members",
            "max_expanded_bytes",
            "max_xml_depth",
        ],
    );
    let bound = |name: &str, default: u64| {
        members.get(name).map_or(default, |value| {
            value
                .as_u64()
                .unwrap_or_else(|| panic!("{name} is a whole number, not {value:?}"))
        })
    };
    let count = |name: &str, default: usize| {
        usize::try_from(bound(name, default as u64))
            .unwrap_or_else(|_| panic!("{name} is a count this platform holds"))
    };
    let defaults = Options::default();
    Options {
        archive_member: members
            .get("archive_member")
            .map(|name| text(name).to_owned()),
        max_archive_members: count("max_archive_members", defaults.max_archive_members),
        max_expanded_bytes: bound("max_expanded_bytes", defaults.max_expanded_bytes),
        max_xml_depth: count("max_xml_depth", defaults.max_xml_depth),
    }
}
