//! Submission boundary findings retained by a native preparation owner.
//! Resource identity and platform path comparison stay with that owner; core
//! classifies the observed declaration/relation without reading a resource.
use crate::{
    diagnostic::ConditionCode, project_environment::Submission, submission_composition::Finding,
};

pub fn standard_name(written: &str) -> Result<Submission, Finding> {
    match written {
        "sdtm" => Ok(Submission::Sdtm),
        "adam" => Ok(Submission::Adam),
        "send" => Ok(Submission::Send),
        _ => Err(Finding::cause(
            ConditionCode::DefineStandardName,
            "standard",
            written,
        )),
    }
}
pub fn required<T>(value: Option<T>, path: &str, detail: &str) -> Result<T, Finding> {
    value.ok_or_else(|| Finding::cause(ConditionCode::DefineRequiredDeclaration, path, detail))
}
/// Clock acquisition stays native. The held explicit text has the same portable
/// whole-second civil grammar as a generated Define creation attribute.
pub fn created_text(written: &str) -> Result<&str, Finding> {
    if written.len() == 19 && written.parse::<crate::temporal::DateTime>().is_ok() {
        Ok(written)
    } else {
        Err(creation_refusal("expected whole-second civil datetime"))
    }
}
/// A retained native clock refusal, after clock acquisition/format admission.
pub fn creation_refusal(detail: &str) -> Finding {
    Finding::cause(ConditionCode::DefineCreatedInvalid, "created", detail)
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TargetRelation {
    Independent,
    ArtifactOutsidePackage,
    DatasetJsonOutsidePackage,
    ArtifactCollision,
    DatasetJsonCollision,
}
/// Project the entered platform comparison, preserving the original detail
/// spelling and declaring position. No path is canonicalized or hashed here.
pub fn admit_relation(relation: TargetRelation, path: &str, detail: &str) -> Result<(), Finding> {
    let code = match relation {
        TargetRelation::Independent => return Ok(()),
        TargetRelation::ArtifactOutsidePackage => ConditionCode::DefineArtifactOutsidePackage,
        TargetRelation::DatasetJsonOutsidePackage => ConditionCode::DefineDatasetJsonOutsidePackage,
        TargetRelation::ArtifactCollision => ConditionCode::DefineArtifactCollision,
        TargetRelation::DatasetJsonCollision => ConditionCode::DefineDatasetJsonCollision,
    };
    Err(Finding::cause(code, path, detail))
}
