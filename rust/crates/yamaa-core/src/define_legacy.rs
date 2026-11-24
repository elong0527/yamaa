//! What a Define-XML 2.0 document can record of a declared origin
//! (DEFINE-29): the one judgement the 2.0 encoder writes and refuses by, and
//! that a project check applies to each specification a 2.0 section lists
//! before any document is composed (PROJECT-7).
use crate::{
    ast,
    cdisc_vocabulary::{OriginSource, OriginType},
    define_diagnostics::xml_invalid,
    diagnostic::Diagnostic,
    json::Value,
    project_environment::Submission,
};

/// The part of a declared origin a Define-XML 2.0 document cannot record.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Gap {
    Type,
    Source,
}

/// The facet every 2.0 representation refusal names (DEFINE-25).
pub const FACET: &str = "Define-XML 2.0 representation";

/// The 2.0 clinical vocabulary and the separate SEND vocabulary (DEFINE-29).
/// A 2.0 type records no source, so each implies one, and a declared source
/// is representable only as the implied source of the type it writes:
/// Investigator for CRF, Vendor for eDT, Sponsor for the sponsor's types.
/// The XSD leaves Type as text, so validation and rendering share this
/// mapping.
pub fn origin(
    kind: OriginType,
    source: Option<OriginSource>,
    family: Submission,
) -> Result<&'static str, Gap> {
    let (written, implied) = if family == Submission::Send {
        match kind {
            OriginType::Collected => ("COLLECTED", Some(OriginSource::Investigator)),
            OriginType::Derived => ("DERIVED", Some(OriginSource::Sponsor)),
            OriginType::Other => ("OTHER", Some(OriginSource::Sponsor)),
            OriginType::NotAvailable => ("NOT AVAILABLE", None),
            OriginType::Assigned | OriginType::Predecessor | OriginType::Protocol => {
                return Err(Gap::Type);
            }
        }
    } else {
        match kind {
            OriginType::Collected if source == Some(OriginSource::Vendor) => {
                ("eDT", Some(OriginSource::Vendor))
            }
            OriginType::Collected => ("CRF", Some(OriginSource::Investigator)),
            OriginType::Derived => ("Derived", Some(OriginSource::Sponsor)),
            OriginType::Assigned => ("Assigned", Some(OriginSource::Sponsor)),
            OriginType::Predecessor => ("Predecessor", None),
            OriginType::Protocol => ("Protocol", Some(OriginSource::Sponsor)),
            OriginType::Other | OriginType::NotAvailable => return Err(Gap::Type),
        }
    };
    match source {
        Some(declared) if Some(declared) != implied => Err(Gap::Source),
        _ => Ok(written),
    }
}

/// One refusal for each origin `spec` declares that a 2.0 document of
/// `family` cannot record, at that origin's own `type` or `source`, with the
/// declared value: the refusal composition reports, made before it from the
/// specification alone (PROJECT-7). Only output columns are described, and a
/// value-level entry is judged by the origins it writes itself, since one it
/// inherits is reported at its column. A type or source outside the
/// vocabulary is the specification's own finding.
pub fn origin_refusals(spec: &ast::Spec, family: Submission) -> Vec<Diagnostic> {
    let mut refusals = Vec::new();
    for output in &spec.output.columns {
        let Some(submission) = spec
            .columns
            .iter()
            .find(|column| column.name.name == output.name)
            .and_then(|column| column.submission.as_ref())
        else {
            continue;
        };
        let declared = std::iter::once(&submission.metadata)
            .chain(submission.value_level.iter().map(|entry| &entry.metadata));
        for origin in declared.flat_map(|metadata| &metadata.origins) {
            let Some(kind) = OriginType::from_name(&origin.kind.text) else {
                continue;
            };
            let source = match &origin.source {
                Some(source) => match OriginSource::from_name(&source.text) {
                    Some(admitted) => Some(admitted),
                    None => continue,
                },
                None => None,
            };
            let refused = match self::origin(kind, source, family) {
                Ok(_) => continue,
                Err(Gap::Type) => &origin.kind,
                Err(Gap::Source) => origin
                    .source
                    .as_ref()
                    .expect("only a declared source misses"),
            };
            let finding = xml_invalid(
                &spec.cite(refused.meta),
                FACET,
                Value::String(refused.text.clone()),
            );
            refusals.push(Diagnostic {
                code: finding.code,
                spec_paths: vec![finding.path],
                context: finding.context,
                source_span: None,
                operand_route: None,
            });
        }
    }
    refusals
}
