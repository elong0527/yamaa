//! Ordered layer composition; filesystem graph traversal stays outside the schema core.

use super::{
    scalar_diagnostic_label, ComposedValue, CompositionOrigin, Document, DocumentNode as N,
    NormalizationError, NormalizationResource, SchemaOrigin, SchemaSource, SchemaStructure,
    ValidationError,
};
use std::vec;

/// A written logical path and its retained input layer. Hosts attach their file identities.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LayerProvenance {
    pub path: String,
    pub layer: usize,
}

/// An original normalization occurrence, kept separately from diagnostic paths.
/// A layer identifies authored input; defaults identify a schema module instead.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LayerNodeOrigin {
    pub layer: Option<usize>,
    pub origin: SchemaOrigin,
}
impl LayerNodeOrigin {
    /// Transport a normalization occurrence through the prior retained arena.
    pub fn through(origin: SchemaOrigin, prior: &[Self]) -> Self {
        match origin.source {
            SchemaSource::Input => {
                let mut original = prior[origin.node];
                original.origin.generated |= origin.generated;
                original
            }
            SchemaSource::Default { .. } => Self {
                layer: None,
                origin,
            },
        }
    }
}

/// Composition precedes default materialization, pruning and complete-document validation.
#[derive(Clone, Debug, PartialEq)]
pub struct ComposedLayers {
    pub document: Document,
    /// Parallel to document nodes, including materialized defaults and shorthand.
    pub node_origins: Vec<LayerNodeOrigin>,
    pub provenance: Vec<LayerProvenance>,
}

#[derive(Clone, Copy)]
struct Site<'a> {
    document: &'a Document,
    node: usize,
    // Prior composition may contain a synthetic root, which has no written origin.
    origins: Option<&'a [Option<CompositionOrigin>]>,
    layer: usize,
}
impl<'a> Site<'a> {
    fn child(self, node: usize) -> Self {
        Self { node, ..self }
    }
    fn value(self) -> &'a N {
        &self.document.nodes()[self.node]
    }
    fn origin(self) -> Option<CompositionOrigin> {
        match self.origins {
            Some(origins) => origins[self.node],
            None => Some(CompositionOrigin {
                input: self.layer,
                node: self.node,
            }),
        }
    }
    fn mapping(self) -> Result<&'a [(usize, usize)], NormalizationError> {
        match self.value() {
            N::Mapping(items) => Ok(items),
            _ => Err(ValidationError::InvalidDescriptor.into()),
        }
    }
    fn sequence(self) -> Result<&'a [usize], NormalizationError> {
        match self.value() {
            N::Sequence(items) => Ok(items),
            _ => Err(ValidationError::InvalidDescriptor.into()),
        }
    }
    fn text(self) -> Result<&'a str, NormalizationError> {
        match self.value() {
            N::Text(value) => Ok(value),
            _ => Err(ValidationError::InvalidDescriptor.into()),
        }
    }
}
#[derive(Clone, Copy)]
enum Change {
    Value(usize),
    Keep,
    Remove,
}

/// The keyed root collections and the field identifying a member of a list.
/// `input` and `windows` are mappings keyed by name.
const COLLECTIONS: [(&str, Option<&str>); 5] = [
    ("input", None),
    ("windows", None),
    ("intermediates", Some("id")),
    ("columns", Some("name")),
    ("rows", Some("id")),
];

struct Run<'a> {
    schema: &'a SchemaStructure,
    nodes: Vec<N>,
    origins: Vec<Option<CompositionOrigin>>,
}
impl Run<'_> {
    fn step(&mut self, depth: usize) -> Result<(), NormalizationError> {
        let limit = crate::limits::Limits::SEMANTIC_DEPTH;
        if depth > limit {
            return Err(NormalizationError::Limit {
                resource: NormalizationResource::Depth,
                limit,
            });
        }
        Ok(())
    }
    fn find<'a>(
        &mut self,
        site: Site<'a>,
        name: &str,
    ) -> Result<Option<Site<'a>>, NormalizationError> {
        for &(key, value) in site.mapping()? {
            let candidate = site.child(key).text()?;
            if candidate == name {
                return Ok(Some(site.child(value)));
            }
        }
        Ok(None)
    }
    fn push(&mut self, value: N, origin: Option<CompositionOrigin>) -> usize {
        let id = self.nodes.len();
        self.nodes.push(value);
        self.origins.push(origin);
        id
    }
    fn copy(&mut self, site: Site<'_>, depth: usize) -> Result<usize, NormalizationError> {
        self.step(depth)?;
        let value = match site.value() {
            N::Text(_) | N::Integer(_) => site.value().clone(),
            N::Sequence(items) => {
                let mut output = Vec::with_capacity(items.len());
                for &item in items {
                    output.push(self.copy(site.child(item), depth + 1)?);
                }
                N::Sequence(output)
            }
            N::Mapping(items) => {
                let mut output = Vec::with_capacity(items.len());
                for &(key, value) in items {
                    output.push((
                        self.copy(site.child(key), depth + 1)?,
                        self.copy(site.child(value), depth + 1)?,
                    ));
                }
                N::Mapping(output)
            }
            value => value.clone(),
        };
        Ok(self.push(value, site.origin()))
    }
    fn import(
        &mut self,
        composed: ComposedValue,
        old: Option<Site<'_>>,
        new: Site<'_>,
    ) -> Result<usize, NormalizationError> {
        let offset = self.nodes.len();
        for (index, node) in composed.document.nodes().iter().enumerate() {
            let value = match node {
                N::Sequence(items) => N::Sequence(items.iter().map(|n| n + offset).collect()),
                N::Mapping(items) => N::Mapping(
                    items
                        .iter()
                        .map(|(k, v)| (k + offset, v + offset))
                        .collect(),
                ),
                value => value.clone(),
            };
            let source = composed.origins[index];
            let origin = match old {
                Some(old) if source.input == 0 => old.child(source.node).origin(),
                _ => new.child(source.node).origin(),
            };
            self.push(value, origin);
        }
        Ok(offset + composed.document.root())
    }
    /// Root fields: keyed collections compose member by member, a project's
    /// sections each as one member, and every other field replaces whole; a
    /// layer clears only through `clear`, in which null is the missing value
    /// (LAYER-9). `parent`, `include` and `clear` are never contributed.
    fn root(
        &mut self,
        old: Site<'_>,
        new: Site<'_>,
        depth: usize,
    ) -> Result<usize, NormalizationError> {
        self.step(depth)?;
        let before = old.mapping()?;
        let after = new.mapping()?;
        let capacity = before.len().saturating_add(after.len());
        let mut changes = Vec::with_capacity(after.len());
        let fields = &self.schema.root_class().fields;
        // Compute updates in contribution order, independently of inherited field positions.
        for &(key, value) in after {
            let name = new.child(key).text()?;
            let value = new.child(value);
            if matches!(name, "parent" | "include" | "clear") {
                changes.push(Change::Remove);
                continue;
            }
            let field = fields
                .iter()
                .find(|field| field.name == name)
                .ok_or(ValidationError::InvalidDescriptor)?;
            let previous = self.find(old, name)?;
            let value = match COLLECTIONS
                .iter()
                .find(|(collection, _)| *collection == name)
            {
                Some(&(_, identity)) => {
                    self.collection(field.descriptor, identity, previous, value, depth + 1)?
                }
                // A project section merges with the one it inherits
                // (PROJECT-16).
                None if self.schema.composes_members(name) => {
                    let composed = self.schema.compose_value_at(
                        field.descriptor,
                        previous.map(|site| (site.document, site.node)),
                        (value.document, value.node),
                    )?;
                    self.import(composed, previous, value)?
                }
                None => self.copy(value, depth + 1)?,
            };
            changes.push(Change::Value(value));
        }
        let mut output = Vec::with_capacity(capacity);
        for &(key, value) in before {
            let name = old.child(key).text()?;
            let mut change = Change::Keep;
            for (index, &(candidate, _)) in after.iter().enumerate() {
                if name == new.child(candidate).text()? {
                    change = changes[index];
                    break;
                }
            }
            let value = match change {
                Change::Remove => continue,
                Change::Keep => self.copy(old.child(value), depth + 1)?,
                Change::Value(id) => id,
            };
            output.push((self.copy(old.child(key), depth + 1)?, value));
        }
        for (index, &(key, _)) in after.iter().enumerate() {
            let name = new.child(key).text()?;
            if self.find(old, name)?.is_some() {
                continue;
            }
            if let Change::Value(value) = changes[index] {
                output.push((self.copy(new.child(key), depth + 1)?, value));
            }
        }
        Ok(self.push(N::Mapping(output), old.origin()))
    }
    fn identity<'a>(
        &mut self,
        member: Site<'a>,
        identity: &str,
    ) -> Result<&'a str, NormalizationError> {
        self.find(member, identity)?
            .ok_or(ValidationError::InvalidDescriptor)?
            .text()
    }
    /// One merge rule for every keyed member: a new identifier appends in
    /// contribution order, and a matching identifier keeps its first position and
    /// merges recursively by kind with the member it inherits (LAYER-5, LAYER-6).
    fn collection(
        &mut self,
        descriptor: usize,
        identity: Option<&str>,
        old: Option<Site<'_>>,
        new: Site<'_>,
        depth: usize,
    ) -> Result<usize, NormalizationError> {
        self.step(depth)?;
        // Each member pairs its identifier with its key occurrence in a mapping.
        let members = |run: &mut Self,
                       site: Site<'_>|
         -> Result<Vec<(String, Option<usize>, usize)>, NormalizationError> {
            let mut result = Vec::new();
            match identity {
                Some(identity) => {
                    for &node in site.sequence()? {
                        let name = run.identity(site.child(node), identity)?.to_string();
                        if result.iter().any(|(prior, _, _)| *prior == name) {
                            return Err(ValidationError::InvalidDescriptor.into());
                        }
                        result.push((name, None, node));
                    }
                }
                None => {
                    for &(key, node) in site.mapping()? {
                        site.child(node).mapping()?;
                        result.push((site.child(key).text()?.to_string(), Some(key), node));
                    }
                }
            }
            Ok(result)
        };
        let before = match old {
            Some(old) => members(self, old)?,
            None => Vec::new(),
        };
        let after = members(self, new)?;
        let mut changed = Vec::with_capacity(after.len());
        for (name, _, node) in &after {
            let incoming = new.child(*node);
            let previous = before
                .iter()
                .find(|(candidate, _, _)| candidate == name)
                .map(|&(_, _, node)| old.expect("nonempty before").child(node));
            let composed = self.schema.compose_member_at(
                descriptor,
                previous.map(|site| (site.document, site.node)),
                (incoming.document, incoming.node),
            )?;
            changed.push(self.import(composed, previous, incoming)?);
        }
        let mut entries = Vec::with_capacity(before.len().saturating_add(after.len()));
        for (name, key, node) in &before {
            let site = old.expect("nonempty before");
            let value = match after.iter().position(|(candidate, _, _)| candidate == name) {
                Some(index) => changed[index],
                None => self.copy(site.child(*node), depth + 1)?,
            };
            let key = match key {
                Some(key) => Some(self.copy(site.child(*key), depth + 1)?),
                None => None,
            };
            entries.push((key, value));
        }
        for (index, (name, key, _)) in after.iter().enumerate() {
            if before.iter().any(|(candidate, _, _)| candidate == name) {
                continue;
            }
            let key = match key {
                Some(key) => Some(self.copy(new.child(*key), depth + 1)?),
                None => None,
            };
            entries.push((key, changed[index]));
        }
        let value = match identity {
            Some(_) => N::Sequence(entries.into_iter().map(|(_, value)| value).collect()),
            None => N::Mapping(
                entries
                    .into_iter()
                    .map(|(key, value)| Ok((key.ok_or(ValidationError::InvalidDescriptor)?, value)))
                    .collect::<Result<Vec<_>, NormalizationError>>()?,
            ),
        };
        Ok(self.push(value, old.unwrap_or(new).origin()))
    }
}

/// The path of a member `label` names below `path`, in the spec path
/// notation, which quotes a key that is not a bare segment (P-CLR-1).
fn member_path(path: &str, label: &str) -> String {
    super::spec_path::join(path, label)
}

/// How provenance spells a path: the keys it leaves out.
#[derive(Clone, Copy)]
struct Notation<'a> {
    /// Keys that shorthand expansion wrote, which a 1.x path names a value
    /// through without spelling (P-CLR-1): the value is recorded at the path
    /// of the mapping holding it.
    transparent: &'a dyn Fn(usize) -> bool,
}

fn provenance(
    document: &Document,
    origins: &[Option<CompositionOrigin>],
    notation: Notation<'_>,
) -> Result<Vec<LayerProvenance>, NormalizationError> {
    fn record(
        document: &Document,
        node: usize,
        path: String,
        origins: &[Option<CompositionOrigin>],
        output: &mut Vec<LayerProvenance>,
        depth: usize,
        notation: Notation<'_>,
    ) -> Result<(), NormalizationError> {
        let limit = crate::limits::Limits::SEMANTIC_DEPTH;
        if depth > limit {
            return Err(NormalizationError::Limit {
                resource: NormalizationResource::Depth,
                limit,
            });
        }
        let origin = origins[node].ok_or(ValidationError::InvalidDescriptor)?;
        output.push(LayerProvenance {
            path: path.clone(),
            layer: origin.input,
        });
        if let N::Mapping(items) = &document.nodes()[node] {
            for &(key, value) in items {
                let label = scalar_diagnostic_label(&document.nodes()[key])
                    .ok_or(ValidationError::InvalidDescriptor)?;
                let path = if (notation.transparent)(key) {
                    path.clone()
                } else {
                    member_path(&path, &label)
                };
                record(document, value, path, origins, output, depth + 1, notation)?;
            }
        }
        Ok(())
    }
    let mut output = Vec::new();
    let N::Mapping(fields) = &document.nodes()[document.root()] else {
        return Err(ValidationError::InvalidDescriptor.into());
    };
    for &(key, value) in fields {
        let N::Text(name) = &document.nodes()[key] else {
            return Err(ValidationError::InvalidDescriptor.into());
        };
        match name.as_str() {
            "input" | "windows" if matches!(document.nodes()[value], N::Mapping(_)) => {
                let N::Mapping(items) = &document.nodes()[value] else {
                    return Err(ValidationError::InvalidDescriptor.into());
                };
                for &(key, node) in items {
                    let N::Text(id) = &document.nodes()[key] else {
                        return Err(ValidationError::InvalidDescriptor.into());
                    };
                    record(
                        document,
                        node,
                        member_path(name, id),
                        origins,
                        &mut output,
                        0,
                        notation,
                    )?;
                }
            }
            "columns" | "rows" | "intermediates"
                if matches!(document.nodes()[value], N::Sequence(_)) =>
            {
                let N::Sequence(items) = &document.nodes()[value] else {
                    return Err(ValidationError::InvalidDescriptor.into());
                };
                let identity = if name == "columns" { "name" } else { "id" };
                for &node in items {
                    let id = document
                        .field(node, identity)
                        .ok_or(ValidationError::InvalidDescriptor)?;
                    let N::Text(id) = &document.nodes()[id] else {
                        return Err(ValidationError::InvalidDescriptor.into());
                    };
                    record(
                        document,
                        node,
                        member_path(name, id),
                        origins,
                        &mut output,
                        0,
                        notation,
                    )?;
                }
            }
            _ => {
                output.push(LayerProvenance {
                    path: name.clone(),
                    layer: origins[value]
                        .ok_or(ValidationError::InvalidDescriptor)?
                        .input,
                });
            }
        }
    }
    Ok(output)
}

/// The accumulated value before the first layer: an empty root, with no
/// written origin.
pub(super) fn empty() -> Result<(Document, Vec<Option<CompositionOrigin>>), NormalizationError> {
    let document =
        Document::new(vec![N::Mapping(vec![])], 0).map_err(NormalizationError::Document)?;
    Ok((document, vec![None]))
}

/// Compose one normalized layer, numbered `layer.0` in contribution order, onto
/// the accumulated value: keyed collections member by member and every other
/// root field whole (LAYER-5, LAYER-6).
pub(super) fn compose_step(
    schema: &SchemaStructure,
    accumulated: (&Document, &[Option<CompositionOrigin>]),
    layer: (usize, &Document),
) -> Result<(Document, Vec<Option<CompositionOrigin>>), NormalizationError> {
    let (document, origins) = accumulated;
    let (layer, input) = layer;
    let mut run = Run {
        schema,
        nodes: Vec::new(),
        origins: Vec::new(),
    };
    let root = run.root(
        Site {
            document,
            node: document.root(),
            origins: Some(origins),
            layer: 0,
        },
        Site {
            document: input,
            node: input.root(),
            origins: None,
            layer,
        },
        0,
    )?;
    let document = Document::new(run.nodes, root).map_err(NormalizationError::Document)?;
    Ok((document, run.origins))
}

/// The composed value with the provenance of every contributed path. A key
/// for which `transparent` holds is not spelled.
pub(super) fn finish(
    document: Document,
    origins: Vec<Option<CompositionOrigin>>,
    transparent: &dyn Fn(usize) -> bool,
) -> Result<ComposedLayers, NormalizationError> {
    let mut provenance = provenance(&document, &origins, Notation { transparent })?;
    // A value named through a shorthand key shares its holder's path.
    provenance.dedup();
    Ok(ComposedLayers {
        document,
        node_origins: origins
            .into_iter()
            .map(|origin| LayerNodeOrigin {
                layer: origin.map(|origin| origin.input),
                origin: SchemaOrigin {
                    source: SchemaSource::Input,
                    node: origin.map_or(0, |origin| origin.node),
                    generated: origin.is_none(),
                },
            })
            .collect(),
        provenance,
    })
}
