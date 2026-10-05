//! Strict intermediate metadata decoding and projection of core-selected findings.
use serde::{Deserialize, Serialize};
use yamaa_core::{
    intermediate_reference as core,
    reference_binding::{Catalog, Error},
};

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum Source {
    Dataset {
        name: String,
    },
    #[serde(rename = "self")]
    SelfFields {
        fields: Vec<String>,
    },
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Intermediate {
    source: Source,
    derived: Vec<String>,
    readable: Vec<String>,
    dependencies: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Read {
    reader: String,
    target_name: String,
    target: Option<Intermediate>,
    field: String,
    donor_dataset: String,
    visible: Vec<String>,
}

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(super) enum Finding {
    UnknownField,
    SelfPhase,
    UnavailableDependency { dependency: usize },
}

impl From<core::Finding> for Finding {
    /// Keep the core's dependency index so hosts recover the authored name and provenance.
    fn from(value: core::Finding) -> Self {
        match value {
            core::Finding::UnknownField => Self::UnknownField,
            core::Finding::SelfPhase => Self::SelfPhase,
            core::Finding::UnavailableDependency { dependency } => {
                Self::UnavailableDependency { dependency }
            }
        }
    }
}

/// Borrow decoded fields for one call; no wire buffers or host objects enter the catalog.
fn with_target<T>(wire: &Intermediate, invoke: impl FnOnce(core::Intermediate<'_>) -> T) -> T {
    let fields: Vec<_> = match &wire.source {
        Source::Dataset { .. } => Vec::new(),
        Source::SelfFields { fields } => fields.iter().map(String::as_str).collect(),
    };
    let derived: Vec<_> = wire.derived.iter().map(String::as_str).collect();
    let readable: Vec<_> = wire.readable.iter().map(String::as_str).collect();
    let dependencies: Vec<_> = wire.dependencies.iter().map(String::as_str).collect();
    invoke(core::Intermediate {
        source: match &wire.source {
            Source::Dataset { name } => core::Source::Dataset(name),
            Source::SelfFields { .. } => core::Source::SelfFields(&fields),
        },
        derived: &derived,
        readable: &readable,
        dependencies: &dependencies,
    })
}

/// Validate a direct intermediate field without deciding any language rule in the transport.
pub(super) fn field(
    catalog: &Catalog,
    field: &str,
    target: &Intermediate,
) -> Result<Vec<Finding>, Error> {
    with_target(target, |target| {
        core::validate_field(catalog, field, target, core::Limits::default())
    })
    .map(|finding| finding.into_iter().map(Into::into).collect())
}

/// Apply donor-scope validation with bounded decoded context and original dependency ordering.
pub(super) fn read(catalog: &Catalog, wire: &Read) -> Result<Vec<Finding>, Error> {
    match &wire.target {
        Some(target) => with_target(target, |target| read_target(catalog, wire, Some(target))),
        None => read_target(catalog, wire, None),
    }
    .map(|findings| findings.into_iter().map(Into::into).collect())
}

/// Keep the target's borrowed buffers confined to this synchronous validation call.
fn read_target(
    catalog: &Catalog,
    wire: &Read,
    target: Option<core::Intermediate<'_>>,
) -> Result<Vec<core::Finding>, Error> {
    let visible: Vec<_> = wire.visible.iter().map(String::as_str).collect();
    core::validate_read(
        catalog,
        core::Read {
            reader: &wire.reader,
            target_name: &wire.target_name,
            target,
            field: &wire.field,
            donor_dataset: &wire.donor_dataset,
            visible: &visible,
        },
        core::Limits::default(),
    )
}
