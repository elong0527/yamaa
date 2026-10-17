//! Project existing native preparation into versioned facts, without a second compiler.
use crate::{
    file_graph::FileGraph, file_resources::Resources, specification_source::PreparedDocument,
};
use std::collections::BTreeMap;
pub mod model;
pub use model::ResolvedArtifact;
use model::{Entry, FieldOrigin, Location, OriginKind, RelativeTo, Resource, ResourceRole};
use yamaa_core::{
    project_function::Language,
    schema::{Document, DocumentNode as N, SchemaSource},
};

#[derive(Debug)]
pub enum Error {
    Resource(crate::file_resources::Error),
    Model(model::Error),
    Origin(String),
}
impl From<model::Error> for Error {
    fn from(error: model::Error) -> Self {
        Self::Model(error)
    }
}

/// Initial resolution uses precisely the existing preparation/compilation routes.
/// Once returned, the owned artifact has no file reader, callback or executable plan.
pub fn resolve(
    specification: &str,
    environment: Option<&str>,
    host: Language,
) -> Result<ResolvedArtifact, crate::file_application::Failure> {
    let selected = crate::file_workflow::select_request(
        yamaa_core::build::Request {
            specification,
            environment,
        },
        host,
    )
    .map_err(|error| crate::project_application::selection_failure(&error, host))?;
    let artifact = match selected {
        yamaa_core::build::Prepared::Plain(prepared) => prepared.resolved_specification(),
        yamaa_core::build::Prepared::Ordinary(prepared) => prepared.resolved_specification(),
        yamaa_core::build::Prepared::Producer(prepared) => prepared.resolved_specification(),
    };
    artifact.map_err(|error| match error {
        Error::Resource(error) => crate::file_application::resource_failure("resolve", &error),
        _ => crate::file_application::rejected("resolve", "artifact_representation"),
    })
}

pub(crate) fn single(
    document: &PreparedDocument,
    resources: &Resources,
    environment: Option<String>,
) -> Result<ResolvedArtifact, Error> {
    Ok(ResolvedArtifact::new(
        document.source().identity.clone(),
        environment,
        vec![entry(document, resources, BTreeMap::new())?],
    )?)
}

pub(crate) fn graph(graph: &FileGraph, resources: &Resources) -> Result<ResolvedArtifact, Error> {
    let entries = graph
        .graph()
        .nodes()
        .iter()
        .map(|node| {
            let producers = node
                .producers
                .iter()
                .map(|producer| {
                    (
                        producer.dataset.clone(),
                        producer.document.source().identity.clone(),
                    )
                })
                .collect();
            entry(&node.document, resources, producers)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let entry = entries
        .first()
        .ok_or_else(|| Error::Origin("entry".into()))?
        .entry
        .clone();
    Ok(ResolvedArtifact::new(
        entry,
        graph
            .environment()
            .root_optional()
            .map(|root| root.source().identity.clone()),
        entries,
    )?)
}

fn text<'a>(document: &'a Document, parent: usize, field: &str) -> Option<&'a str> {
    match &document.nodes()[document.field(parent, field)?] {
        N::Text(value) => Some(value),
        _ => None,
    }
}

fn entry(
    document: &PreparedDocument,
    resources: &Resources,
    producers: BTreeMap<String, String>,
) -> Result<Entry, Error> {
    let normalized = document.model().document();
    let (specification, mapping_order) = model::specification_value(normalized)?;
    let origins = origins(document);
    let layers = document.inheritance().map_or_else(
        || vec![document.source().identity.clone()],
        |inherited| {
            inherited
                .layers()
                .iter()
                .map(|layer| layer.source.identity.clone())
                .collect()
        },
    );
    let mut result = Entry {
        entry: document.source().identity.clone(),
        schema_version: text(normalized, normalized.root(), "schema_version")
            .unwrap_or_default()
            .into(),
        specification,
        mapping_order,
        origins,
        layers,
        resources: Vec::new(),
        producers,
    };
    if let Some(input) = normalized.field(normalized.root(), "input") {
        if let N::Mapping(inputs) = &normalized.nodes()[input] {
            for &(name, declaration) in inputs {
                let N::Text(name) = &normalized.nodes()[name] else {
                    continue;
                };
                let relative_to = selector(normalized, declaration);
                for (field, role) in [
                    ("path", ResourceRole::InputData),
                    ("schema", ResourceRole::ProducerSchema),
                ] {
                    if normalized.field(declaration, field).is_none() {
                        continue;
                    }
                    let path = format!("input.{name}.{field}");
                    let origin = document
                        .written_input_origin(name, field)
                        .ok_or_else(|| Error::Origin(path.clone()))?;
                    result.resources.push(resource(
                        document,
                        resources,
                        path,
                        role,
                        relative_to.clone(),
                        origin,
                    )?);
                }
            }
        }
    }
    if let Some(output) = normalized.field(normalized.root(), "output") {
        let relative_to = selector(normalized, output);
        for (field, role) in [
            ("path", ResourceRole::OutputData),
            ("warning_log", ResourceRole::WarningLog),
            ("verification_log", ResourceRole::VerificationLog),
        ] {
            if normalized.field(output, field).is_none() {
                continue;
            }
            let path = format!("output.{field}");
            let origin = document
                .written_output_field_origin(field)
                .ok_or_else(|| Error::Origin(path.clone()))?;
            result.resources.push(resource(
                document,
                resources,
                path,
                role,
                relative_to.clone(),
                origin,
            )?);
        }
    }
    let references = document
        .schema()
        .structure()
        .mapping_dictionary_references(normalized)
        .map_err(|_| Error::Origin("mapping dictionaries".into()))?;
    for reference in references {
        let declaring_source = document
            .declaring_source(reference.path())
            .ok_or_else(|| Error::Origin(reference.path().into()))?;
        result.resources.push(resource(
            document,
            resources,
            reference.path().into(),
            ResourceRole::MappingDictionary,
            RelativeTo::Declaring,
            yamaa_core::producer_admission::WrittenPath {
                declaring_source,
                base_source: declaring_source,
                written: reference.written(),
            },
        )?);
    }
    for (source, raw) in std::iter::once((document.source(), &document.raw().document)).chain(
        document
            .parents()
            .iter()
            .map(|parent| (parent.source(), &parent.raw().document)),
    ) {
        if let Some(parent) = raw.field(raw.root(), "parent") {
            if let N::Text(written) = &raw.nodes()[parent] {
                result.resources.push(resource(
                    document,
                    resources,
                    "parent".into(),
                    ResourceRole::ParentSpecification,
                    RelativeTo::Declaring,
                    yamaa_core::producer_admission::WrittenPath {
                        declaring_source: &source.identity,
                        base_source: &source.identity,
                        written,
                    },
                )?);
            }
        }
    }
    result.resources.sort_by(|left, right| {
        (&left.spec_path, &left.declaring_source).cmp(&(&right.spec_path, &right.declaring_source))
    });
    Ok(result)
}

fn selector(document: &Document, declaration: usize) -> RelativeTo {
    if text(document, declaration, "relative_to") == Some("entry") {
        RelativeTo::Entry
    } else {
        RelativeTo::Declaring
    }
}

fn resource(
    document: &PreparedDocument,
    resources: &Resources,
    spec_path: String,
    role: ResourceRole,
    relative_to: RelativeTo,
    origin: yamaa_core::producer_admission::WrittenPath<'_>,
) -> Result<Resource, Error> {
    let location = resources
        .location_from(origin.base_source, origin.written)
        .map_err(Error::Resource)?;
    Ok(Resource {
        spec_path,
        role,
        owning_entry: document.source().identity.clone(),
        declaring_source: origin.declaring_source.into(),
        relative_to,
        base_source: origin.base_source.into(),
        written: origin.written.into(),
        location: Location::Local { path: location },
    })
}

fn origins(document: &PreparedDocument) -> BTreeMap<String, FieldOrigin> {
    let normalized = document.model().document();
    let mut result = BTreeMap::new();
    let mut pending = vec![(normalized.root(), String::new())];
    while let Some((node, pointer)) = pending.pop() {
        let (origin, layer_source) = match document.inheritance() {
            Some(inherited) => {
                let original = inherited.original_origins()[node];
                (
                    original.origin,
                    original
                        .layer
                        .map(|layer| inherited.layers()[layer].source.identity.as_str()),
                )
            }
            None => (
                document.origins()[node],
                Some(document.source().identity.as_str()),
            ),
        };
        let (declaring_source, source_kind) = match origin.source {
            SchemaSource::Input => match layer_source {
                Some(source) => (Some(source.into()), OriginKind::Authored),
                None => (None, OriginKind::Composed),
            },
            SchemaSource::Default { module, .. } => (
                document
                    .schema()
                    .sources()
                    .get(module)
                    .map(|source| source.identity.clone()),
                OriginKind::SchemaDefault,
            ),
        };
        result.insert(
            pointer.clone(),
            FieldOrigin {
                declaring_source,
                source_kind,
                generated: origin.generated,
            },
        );
        match &normalized.nodes()[node] {
            N::Mapping(items) => {
                for &(key, child) in items {
                    if let N::Text(key) = &normalized.nodes()[key] {
                        pending.push((child, model::pointer_child(&pointer, key)));
                    }
                }
            }
            N::Sequence(items) => {
                for (index, &child) in items.iter().enumerate() {
                    pending.push((child, model::pointer_child(&pointer, &index.to_string())));
                }
            }
            _ => {}
        }
    }
    result
}
