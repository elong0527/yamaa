//! The findings and the creation time of a Define composition. The adapters'
//! Define service composes, renders and publishes one submission section; it
//! reports the resources and the publication it observes through these
//! findings, and records the creation time a composition is given or reads
//! once from a clock.
use crate::json::Value as JsonValue;
use crate::{
    application_issue::{Fault, Step},
    diagnostic::{ConditionCode, Diagnostic},
    submission_composition::Finding,
};

/// Two artifacts of one composition whose written paths name one physical
/// target (JSON-2).
pub fn shared_target(earlier: &str, later: &str) -> Finding {
    crate::define_diagnostics::admit_relation(
        crate::define_diagnostics::TargetRelation::TransportCollision,
        "define.targets",
        later,
        &format!("{earlier} and {later} name one physical publication target"),
    )
    .unwrap_err()
}
/// The creation time a composition records (DEFINE-3): `created` when the
/// caller gives one, admitted as the document's creation time, or else the
/// reading of `clock`, called once, as a whole-second UTC civil time. An
/// invalid `created` is refused before the clock is read.
pub fn created_time(
    created: Option<&str>,
    clock: impl FnOnce() -> Option<u64>,
) -> Result<String, Finding> {
    if let Some(created) = created {
        return crate::define_diagnostics::created_text(created).map(str::to_owned);
    }
    let seconds = clock().ok_or_else(|| {
        crate::define_diagnostics::creation_refusal("UTC clock before epoch", JsonValue::Null)
    })?;
    utc_time(seconds).ok_or_else(|| {
        crate::define_diagnostics::creation_refusal(
            "UTC clock outside supported calendar",
            JsonValue::from(seconds),
        )
    })
}

/// The creation time a composition records for a UTC clock reading of
/// `seconds` after the epoch when `created` is omitted (DEFINE-3): the
/// whole-second civil datetime `YYYY-MM-DDTHH:MM:SS`, or `None` past the
/// year 9999.
pub fn utc_time(seconds: u64) -> Option<String> {
    // The civil date of the elapsed UTC days; clock admission is host-free.
    let days = i64::try_from(seconds / 86_400).ok()?;
    let date = crate::temporal::Date::from_epoch_day(days, crate::temporal::Precision::Day)?;
    Some(format!(
        "{date}T{:02}:{:02}:{:02}",
        seconds % 86400 / 3600,
        seconds % 3600 / 60,
        seconds % 60
    ))
}

/// Infrastructure observation only; the use case owns its retained wire cause.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResourceObservation {
    Missing,
    Unreadable,
    NotRegularFile,
    InvalidRoot,
    InvalidBase,
    InvalidPath,
    OutsideRoots,
    Symlink,
    Changed,
    Limit(crate::limits::LimitExceeded),
}
pub fn resource_diagnostic(
    path: &str,
    written: &str,
    observation: ResourceObservation,
) -> Diagnostic {
    use ResourceObservation as R;
    if let R::Limit(error) = observation {
        let mut diagnostic = crate::application_issue::resource_rejected(
            Step::Define,
            Fault::ResourceLimit,
            error.resource,
            error.limit,
            error.actual,
            vec![path.into()],
        );
        diagnostic
            .context
            .insert("path".into(), JsonValue::String(written.into()));
        return diagnostic;
    }
    let code = match observation {
        R::Missing if path.ends_with(".artifact") => ConditionCode::ProducerContractMismatch,
        R::Missing => ConditionCode::ResourceMissing,
        R::NotRegularFile => ConditionCode::ResourceNotRegularFile,
        R::Changed => ConditionCode::ResourceContentChanged,
        _ => {
            let fault = match observation {
                R::Unreadable => Fault::ResourceUnreadable,
                R::InvalidRoot => Fault::ResourceRoot,
                R::InvalidBase => Fault::ResourceBase,
                R::InvalidPath => Fault::ResourcePath,
                R::OutsideRoots => Fault::ResourceOutsideRoots,
                R::Symlink => Fault::ResourceLink,
                R::Limit(_) => unreachable!("fixed boundary projected above"),
                R::Missing | R::NotRegularFile | R::Changed => {
                    unreachable!("normative resource cause selected")
                }
            };
            let mut diagnostic = crate::application_issue::rejected(Step::Define, fault);
            diagnostic.spec_paths = vec![path.into()];
            diagnostic
                .context
                .insert("path".into(), JsonValue::String(written.into()));
            return diagnostic;
        }
    };
    let mut context: crate::diagnostic::Context =
        [(String::from("path"), JsonValue::String(written.into()))].into();
    // A missing published artifact names its dataset, `datasets.<id>.artifact`
    // (INPUT-11), and says it was never saved, so its message names the run
    // that saves it rather than a contract it cannot have broken.
    if code == ConditionCode::ProducerContractMismatch {
        let dataset = path
            .strip_prefix("datasets.")
            .and_then(|rest| rest.strip_suffix(".artifact"))
            .unwrap_or_default();
        context.insert("dataset".into(), JsonValue::String(dataset.into()));
        context.insert("unpublished".into(), JsonValue::Bool(true));
    }
    Diagnostic::new(code, vec![path.into()], context)
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum PublicationFailure {
    /// The target was refused before any byte was written, for the reason
    /// the adapter classified from its typed cause.
    #[error("define target admission")]
    TargetAdmission(Fault),
    #[error("define publication")]
    Publication,
}
/// Project the entered publication stage and the adapter's typed admission fault,
/// never a classification inferred from an exception message.
/// The native original error remains held separately from its display text.
pub fn publication_diagnostic(
    index: usize,
    written: &str,
    stage: PublicationFailure,
    cause: String,
) -> Diagnostic {
    let mut diagnostic = match stage {
        PublicationFailure::TargetAdmission(fault) => {
            crate::application_issue::rejected(Step::PublicationTargetAdmission, fault)
        }
        // A failed publication names the file it could not publish as the
        // section writes it (OUTPUT-8).
        PublicationFailure::Publication => Diagnostic::new(
            ConditionCode::OutputPublicationFailed,
            Vec::new(),
            [("path".into(), JsonValue::String(written.into()))].into(),
        ),
    };
    diagnostic.spec_paths = vec!["define".into()];
    for (key, value) in [
        ("artifact", JsonValue::from(index)),
        ("cause", JsonValue::String(cause)),
    ] {
        diagnostic.context.insert(key.into(), value);
    }
    diagnostic
}
