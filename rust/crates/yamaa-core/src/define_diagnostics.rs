//! Submission boundary findings retained by a native preparation owner.
//! Resource identity and platform path comparison stay with that owner; core
//! classifies the observed declaration/relation without reading a resource.
//! The findings only the adapters' Define-XML and transport encoders observe
//! are made here too, so each keeps one owner of its cause, path and context.
use crate::json::Value as JsonValue;
use crate::{
    diagnostic::ConditionCode,
    submission_composition::{Dataset, Finding},
};

/// Clock acquisition stays native. The held explicit text has the same portable
/// whole-second civil grammar as a generated Define creation attribute.
pub fn created_text(written: &str) -> Result<&str, Finding> {
    if written.len() == 19 && written.parse::<crate::temporal::DateTime>().is_ok() {
        Ok(written)
    } else {
        Err(creation_refusal(
            "expected whole-second civil datetime",
            JsonValue::String(written.into()),
        ))
    }
}
/// A retained native clock refusal, after clock acquisition/format admission,
/// naming the refused `value`: the text given, or the clock's reading, null
/// when the clock gave none (DEFINE-3).
pub fn creation_refusal(detail: &str, value: JsonValue) -> Finding {
    Finding::cause(ConditionCode::DefineCreatedInvalid, "created", detail).with("value", value)
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TargetRelation {
    Independent,
    ArtifactCollision,
    TransportCollision,
}
/// Project the entered platform comparison, preserving the original detail
/// spelling and declaring position. No path is canonicalized or hashed here.
/// `target` is the colliding file as written, which the finding names as its
/// `path` (JSON-2).
pub fn admit_relation(
    relation: TargetRelation,
    path: &str,
    target: &str,
    detail: &str,
) -> Result<(), Finding> {
    let code = match relation {
        TargetRelation::Independent => return Ok(()),
        TargetRelation::ArtifactCollision => ConditionCode::DefineArtifactCollision,
        TargetRelation::TransportCollision => ConditionCode::DefineTransportCollision,
    };
    Err(Finding::cause(code, path, detail).with("path", target))
}
/// DEFINE-16: the listing at `path` publishes the artifact of `dataset`, its
/// `output.path` as written, outside the directory of the document whose
/// path the project writes as `define`, so no leaf can name it without a
/// parent traversal.
pub fn artifact_outside_document(
    path: &str,
    dataset: &str,
    artifact: &str,
    define: &str,
) -> Finding {
    let mut finding = Finding::cause(
        ConditionCode::DefineArtifactOutsidePackage,
        path,
        "artifact must be inside the Define package",
    );
    for (key, text) in [
        ("dataset", dataset),
        ("artifact", artifact),
        ("define", define),
    ] {
        finding
            .context
            .insert(key.into(), JsonValue::String(text.into()));
    }
    finding
}
/// DEFINE-24: generated text at `path` holds a carriage return or a tab, or an
/// attribute value holds a line feed: `character`, which the finding names as
/// `carriage_return`, `tab` or `line_feed`.
pub fn xml_whitespace(path: &str, character: char) -> Finding {
    let whitespace = match character {
        '\r' => "carriage_return",
        '\t' => "tab",
        _ => "line_feed",
    };
    Finding::cause(
        ConditionCode::SubmissionXmlWhitespace,
        path,
        "forbidden XML whitespace",
    )
    .with("whitespace", whitespace)
}
/// DEFINE-25: the generated document at `path` would break the selected Define-XML
/// schema `facet` names, such as a forbidden character or an unresolved
/// generated reference; `value` is the value that breaks it.
pub fn xml_invalid(path: &str, facet: &str, value: JsonValue) -> Finding {
    Finding::cause(
        ConditionCode::SubmissionXmlInvalid,
        path,
        &format!("schema_violation: {facet}"),
    )
    .with("value", value)
}
/// XPT-10: the transport file of `dataset` would hold `bytes`, more than the
/// 5,000,000,000 bytes one file carries.
pub fn xpt_file_too_large(dataset: &Dataset, bytes: u64) -> Finding {
    let mut finding = transport_file_finding(
        ConditionCode::XptFileTooLarge,
        dataset,
        "transport file larger than 5000000000 bytes",
    );
    finding
        .context
        .insert("bytes".into(), JsonValue::from(bytes));
    finding
}
/// XPT-10: the last row of `dataset`, `row`, encodes to blanks only, which a
/// reader cannot tell from the final record's padding.
pub fn xpt_last_row_blank(dataset: &Dataset, row: usize) -> Finding {
    let mut finding = transport_file_finding(
        ConditionCode::XptLastRowBlank,
        dataset,
        "the last row encodes to blanks only",
    );
    finding.context.insert("row".into(), JsonValue::from(row));
    finding
}
/// A refusal of a whole transport file, at the dataset's published artifact.
fn transport_file_finding(code: ConditionCode, dataset: &Dataset, detail: &str) -> Finding {
    let mut finding = Finding::cause(code, &format!("datasets.{}.artifact", dataset.id()), detail);
    for (key, text) in [("dataset", dataset.id()), ("artifact", dataset.artifact())] {
        finding
            .context
            .insert(key.into(), JsonValue::String(text.into()));
    }
    finding
}
