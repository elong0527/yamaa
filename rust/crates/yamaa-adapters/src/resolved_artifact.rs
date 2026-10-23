//! Project existing native preparation into versioned facts, without a second compiler.
use crate::{
    document::PreparedSpecification, file_resources::Resources,
    specification_source::PreparedDocument,
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

/// Initial resolution uses precisely the existing preparation/compilation routes,
/// reading through `resolver`: the caller's, or the local file resolver
/// without one. Once returned, the owned artifact has no file reader,
/// callback or executable plan.
pub fn resolve(
    specification: &str,
    environment: Option<&str>,
    host: Option<Language>,
    resolver: Option<std::sync::Arc<dyn crate::host_resources::Resolver>>,
) -> Result<ResolvedArtifact, crate::file_application::Failure> {
    resolve_located(specification, environment, host, resolver).map_err(|(issues, _)| issues)
}

/// [`resolve`], whose refusal keeps what locates its issues: the
/// specification as far as it was read and the project files read
/// (P-ISSUE-1).
pub fn resolve_located(
    specification: &str,
    environment: Option<&str>,
    host: Option<Language>,
    resolver: Option<std::sync::Arc<dyn crate::host_resources::Resolver>>,
) -> Result<
    ResolvedArtifact,
    (
        crate::file_application::Failure,
        crate::issue_location::Locator,
    ),
> {
    let request = yamaa_core::build::Request {
        specification,
        environment,
    };
    let selected = crate::file_application::select(host, request, resolver).map_err(|error| {
        (
            crate::project_application::selection_failure(&error),
            crate::issue_location::selection(&error),
        )
    })?;
    selected.resolved_specification().map_err(|error| {
        let issues = match error {
            Error::Resource(error) => crate::file_application::resource_failure(
                yamaa_core::application_issue::Step::Resolve,
                &error,
            ),
            _ => crate::file_application::rejected(
                yamaa_core::application_issue::Step::Resolve,
                yamaa_core::application_issue::Fault::ArtifactRepresentation,
            ),
        };
        (issues, crate::issue_location::prepared(&selected))
    })
}

/// The resolved specification of a 1.x chain, written as the artifact
/// `resolve()` returns: the resolved model, which records the chain's
/// version in `yamaa` and holds neither `parent` nor `clear`; the origin of
/// every value, kept through composition; the layers in contribution order;
/// and every resource the specification names, each written as its layer
/// wrote it and located by `location` from the base its declaration selects
/// (FILE-2). `resolve()` returns it for every 1.x specification (#3030 D76).
pub fn versioned(
    prepared: &PreparedSpecification,
    project: Option<String>,
    mut location: impl FnMut(&str, &str) -> Result<String, crate::file_resources::Error>,
) -> Result<ResolvedArtifact, Error> {
    let resolved = prepared.resolved();
    let (specification, mapping_order) = model::specification_value(resolved)?;
    let layers = prepared.layers();
    let entry_identity = layers
        .last()
        .map(|layer| layer.source.identity.clone())
        .ok_or_else(|| Error::Origin("entry".into()))?;
    let mut result = Entry {
        entry: entry_identity.clone(),
        language_version: prepared.version().to_string(),
        specification,
        mapping_order,
        origins: versioned_origins(prepared),
        layers: layers
            .iter()
            .map(|layer| layer.source.identity.clone())
            .collect(),
        resources: Vec::new(),
    };
    let mut resource = |spec_path: String,
                        role: ResourceRole,
                        relative_to: RelativeTo,
                        declaring_source: &str,
                        written: &str|
     -> Result<Resource, Error> {
        let base_source = match relative_to {
            RelativeTo::Entry => entry_identity.as_str(),
            RelativeTo::Declaring => declaring_source,
        };
        Ok(Resource {
            location: Location::Local {
                path: location(base_source, written).map_err(Error::Resource)?,
            },
            spec_path,
            role,
            owning_entry: entry_identity.clone(),
            declaring_source: declaring_source.into(),
            relative_to,
            base_source: base_source.into(),
            written: written.into(),
        })
    };
    // A path keeps the spelling its layer wrote, read from the mapping
    // `holder` finds in that layer; the resolved model holds it respelled from
    // the entry (LAYER-10). A dataset written bare is the shorthand of its
    // `path`.
    let authored = |path: &str, field: &str, holder: &dyn Fn(&Document) -> Option<usize>| {
        let layer = prepared
            .declaring_layer(path)
            .ok_or_else(|| Error::Origin(path.into()))?;
        let raw = &layer.document;
        let node = holder(raw).and_then(|holder| match &raw.nodes()[holder] {
            N::Text(_) if field == "path" => Some(holder),
            N::Mapping(_) => raw.field(holder, field),
            _ => None,
        });
        match node.map(|node| &raw.nodes()[node]) {
            Some(N::Text(written)) => Ok((layer.source.identity.as_str(), written.as_str())),
            _ => Err(Error::Origin(path.into())),
        }
    };
    if let Some(input) = resolved.field(resolved.root(), "input") {
        if let N::Mapping(inputs) = &resolved.nodes()[input] {
            for &(name, declaration) in inputs {
                let N::Text(name) = &resolved.nodes()[name] else {
                    continue;
                };
                let relative_to = selector(resolved, declaration);
                for (field, role) in [
                    ("path", ResourceRole::InputData),
                    ("schema", ResourceRole::ProducerSchema),
                ] {
                    if resolved.field(declaration, field).is_none() {
                        continue;
                    }
                    let path = format!("input.{name}.{field}");
                    let (declaring, written) = authored(&path, field, &|raw| {
                        raw.field(raw.field(raw.root(), "input")?, name)
                    })?;
                    result.resources.push(resource(
                        path,
                        role,
                        relative_to.clone(),
                        declaring,
                        written,
                    )?);
                }
            }
        }
    }
    if let Some(output) = resolved.field(resolved.root(), "output") {
        let relative_to = selector(resolved, output);
        for (field, role) in [
            ("path", ResourceRole::OutputData),
            ("report", ResourceRole::Report),
        ] {
            if resolved.field(output, field).is_none() {
                continue;
            }
            let path = format!("output.{field}");
            let (declaring, written) =
                authored(&path, field, &|raw| raw.field(raw.root(), "output"))?;
            result.resources.push(resource(
                path,
                role,
                relative_to.clone(),
                declaring,
                written,
            )?);
        }
    }
    let references = prepared
        .schema()
        .structure()
        .mapping_dictionary_references(resolved)
        .map_err(|_| Error::Origin("mapping dictionaries".into()))?;
    for reference in references {
        let declaring = prepared
            .declaring_source(reference.path())
            .ok_or_else(|| Error::Origin(reference.path().into()))?;
        result.resources.push(resource(
            reference.path().into(),
            ResourceRole::MappingDictionary,
            RelativeTo::Declaring,
            declaring,
            reference.written(),
        )?);
    }
    for layer in layers {
        let raw = &layer.document;
        if let Some(N::Text(written)) = raw
            .field(raw.root(), "parent")
            .map(|node| &raw.nodes()[node])
        {
            result.resources.push(resource(
                "parent".into(),
                ResourceRole::ParentSpecification,
                RelativeTo::Declaring,
                &layer.source.identity,
                written,
            )?);
        }
        if let Some(N::Sequence(entries)) = raw
            .field(raw.root(), "include")
            .map(|node| &raw.nodes()[node])
        {
            for (index, &entry) in entries.iter().enumerate() {
                if let N::Text(written) = &raw.nodes()[entry] {
                    result.resources.push(resource(
                        format!("include[{index}]"),
                        ResourceRole::IncludedSpecification,
                        RelativeTo::Declaring,
                        &layer.source.identity,
                        written,
                    )?);
                }
            }
        }
    }
    result.resources.sort_by(|left, right| {
        (&left.spec_path, &left.declaring_source).cmp(&(&right.spec_path, &right.declaring_source))
    });
    Ok(ResolvedArtifact::new(
        result.entry.clone(),
        project,
        vec![result],
    )?)
}

/// The origin of every value of a 1.x resolved specification: the layer that
/// wrote it, the schema module whose default it is, or composition itself.
fn versioned_origins(prepared: &PreparedSpecification) -> BTreeMap<String, FieldOrigin> {
    let resolved = prepared.resolved();
    let mut result = BTreeMap::new();
    let mut pending = vec![(resolved.root(), String::new())];
    while let Some((node, pointer)) = pending.pop() {
        let original = prepared.origins()[node];
        let (declaring_source, source_kind) = match original.origin.source {
            SchemaSource::Input => match original.layer {
                Some(layer) => (
                    prepared
                        .layers()
                        .get(layer)
                        .map(|layer| layer.source.identity.clone()),
                    OriginKind::Authored,
                ),
                None => (None, OriginKind::Composed),
            },
            SchemaSource::Default { module, .. } => (
                prepared
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
                generated: original.origin.generated,
            },
        );
        match &resolved.nodes()[node] {
            N::Mapping(items) => {
                for &(key, child) in items {
                    if let N::Text(key) = &resolved.nodes()[key] {
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

pub(crate) fn single(
    document: &PreparedDocument,
    resources: &Resources,
    environment: Option<String>,
) -> Result<ResolvedArtifact, Error> {
    // A specification records its language version, never `schema_version`
    // (D84).
    versioned(document.specification(), environment, |base, written| {
        resources.location_from(base, written)
    })
}

fn text<'a>(document: &'a Document, parent: usize, field: &str) -> Option<&'a str> {
    match &document.nodes()[document.field(parent, field)?] {
        N::Text(value) => Some(value),
        _ => None,
    }
}

fn selector(document: &Document, declaration: usize) -> RelativeTo {
    if text(document, declaration, "relative_to") == Some("entry") {
        RelativeTo::Entry
    } else {
        RelativeTo::Declaring
    }
}
