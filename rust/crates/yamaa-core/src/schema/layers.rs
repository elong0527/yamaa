//! Ordered layer composition; filesystem graph traversal stays outside the schema core.

use super::{
    scalar_diagnostic_label, ComposedValue, CompositionOrigin, Document, DocumentNode as N,
    NormalizationError, NormalizationResource, SchemaContext, SchemaDiagnostic, SchemaField,
    SchemaStructure, ValidationError,
};
use crate::diagnostic::ConditionCode;
use std::vec;

/// A written logical path and its retained input layer. Hosts attach their file identities.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LayerProvenance {
    pub path: String,
    pub layer: usize,
}

/// Composition precedes default materialization, pruning and complete-document validation.
#[derive(Clone, Debug, PartialEq)]
pub struct ComposedLayers {
    pub document: Document,
    pub provenance: Vec<LayerProvenance>,
    pub diagnostics: Vec<SchemaDiagnostic>,
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
enum Boundary<'a> {
    Root,
    Member {
        fields: &'a [SchemaField],
        compose: bool,
        identity: Option<&'a str>,
    },
}
#[derive(Clone, Copy)]
enum Change {
    Value(usize),
    Keep,
    Remove,
}

struct Run<'a, 'c> {
    schema: &'a SchemaStructure,
    diagnostics: &'c mut Vec<SchemaDiagnostic>,
    nodes: Vec<N>,
    origins: Vec<Option<CompositionOrigin>>,
}
impl Run<'_, '_> {
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
    fn join(&mut self, prefix: &str, name: &str) -> Result<String, NormalizationError> {
        Ok(if prefix.is_empty() {
            name.into()
        } else {
            format!("{prefix}.{name}")
        })
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
        old: Site<'_>,
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
            self.push(
                value,
                if source.input == 0 {
                    old.child(source.node).origin()
                } else {
                    new.child(source.node).origin()
                },
            );
        }
        Ok(offset + composed.document.root())
    }
    fn invalid_clear(&mut self, path: &str, name: &str) -> Result<(), NormalizationError> {
        self.diagnostics.push(super::validation::diagnostic(
            path,
            ConditionCode::SchemaLayerInvalidClear,
            vec![("field", SchemaContext::Text(name.into()))],
        ));
        Ok(())
    }
    fn boundary(
        &mut self,
        old: Option<Site<'_>>,
        new: Site<'_>,
        kind: Boundary<'_>,
        path: &str,
        depth: usize,
    ) -> Result<usize, NormalizationError> {
        self.step(depth)?;
        let before = old.map(Site::mapping).transpose()?.unwrap_or(&[]);
        let after = new.mapping()?;
        let capacity = before.len().saturating_add(after.len());
        let mut changes = Vec::with_capacity(after.len());
        let fields = match kind {
            Boundary::Root => &self.schema.root_class().fields,
            Boundary::Member { fields, .. } => fields,
        };
        // Compute updates in contribution order, independently of inherited field positions.
        for &(key, value) in after {
            let name = new.child(key).text()?;
            let value = new.child(value);
            if matches!(kind, Boundary::Root) && name == "parents" {
                changes.push(Change::Remove);
                continue;
            }
            let field = fields
                .iter()
                .find(|field| field.name == name)
                .ok_or(ValidationError::InvalidDescriptor)?;
            let previous = match old {
                Some(site) => self.find(site, name)?,
                None => None,
            };
            let logical = self.join(path, name)?;
            if matches!(value.value(), N::Null) {
                let identity = matches!(kind,Boundary::Member{identity:Some(id),..} if id==name);
                if identity
                    || self.schema.descriptors()[field.descriptor]
                        .descriptor
                        .required()
                    || name == "schema_version" && matches!(kind, Boundary::Root)
                {
                    self.invalid_clear(&logical, name)?;
                    changes.push(Change::Keep);
                } else if previous.is_none() {
                    // A first declaration is a value, including a literal missing
                    // handler. Only a null over an inherited value is a clear.
                    self.diagnostics.extend(self.schema.validate_descriptor(
                        field.descriptor,
                        value.document,
                        value.node,
                        &logical,
                        false,
                    )?);
                    changes.push(Change::Value(self.copy(value, depth + 1)?));
                } else {
                    changes.push(Change::Remove);
                }
                continue;
            }
            let value = match kind {
                Boundary::Root => self.root_value(name, previous, value, &logical, depth + 1)?,
                Boundary::Member { compose: true, .. } if previous.is_some() => {
                    let previous = previous.expect("checked");
                    let composed = self.schema.compose_descriptor_at(
                        field.descriptor,
                        (previous.document, previous.node),
                        (value.document, value.node),
                    )?;
                    self.import(composed, previous, value)?
                }
                Boundary::Member { .. } => self.copy(value, depth + 1)?,
            };
            changes.push(Change::Value(value));
        }
        let mut output = Vec::with_capacity(capacity);
        for &(key, value) in before {
            let old = old.expect("nonempty before");
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
            if let Some(old) = old {
                if self.find(old, name)?.is_some() {
                    continue;
                }
            }
            if let Change::Value(value) = changes[index] {
                output.push((self.copy(new.child(key), depth + 1)?, value));
            }
        }
        Ok(self.push(N::Mapping(output), old.unwrap_or(new).origin()))
    }
    fn root_value(
        &mut self,
        name: &str,
        old: Option<Site<'_>>,
        new: Site<'_>,
        path: &str,
        depth: usize,
    ) -> Result<usize, NormalizationError> {
        // An invalid introducing null has already been diagnosed; a later
        // collection value still replaces it without treating it as a collection.
        let old = old.filter(|site| !matches!(site.value(), N::Null));
        match name {
            "windows" => self.named(old, new, None, path, depth),
            "input" => self.named(old, new, Some("dataset_class"), path, depth),
            "columns" => self.keyed(old, new, "name", "column_class", path, depth),
            "rows" => self.keyed(old, new, "id", "row_class", path, depth),
            "intermediates" => self.keyed(old, new, "id", "intermediate_class", path, depth),
            _ => self.copy(new, depth),
        }
    }
    fn named(
        &mut self,
        old: Option<Site<'_>>,
        new: Site<'_>,
        class: Option<&str>,
        path: &str,
        depth: usize,
    ) -> Result<usize, NormalizationError> {
        self.step(depth)?;
        let before = old.map(Site::mapping).transpose()?.unwrap_or(&[]);
        let after = new.mapping()?;
        let capacity = before.len().saturating_add(after.len());
        let mut changed = Vec::with_capacity(after.len());
        for &(key, value) in after {
            let name = new.child(key).text()?;
            let incoming = new.child(value);
            incoming.mapping()?;
            let previous = match old {
                Some(old) => self.find(old, name)?,
                None => None,
            };
            let logical = self.join(path, name)?;
            changed.push(if let Some(class) = class {
                let fields = &self
                    .schema
                    .class_named(class)
                    .ok_or(ValidationError::InvalidDescriptor)?
                    .fields;
                self.boundary(
                    previous,
                    incoming,
                    Boundary::Member {
                        fields,
                        compose: false,
                        identity: None,
                    },
                    &logical,
                    depth + 1,
                )?
            } else {
                self.copy(incoming, depth + 1)?
            });
        }
        let mut output = Vec::with_capacity(capacity);
        for &(key, value) in before {
            let old = old.expect("nonempty before");
            let name = old.child(key).text()?;
            let mut selected = None;
            for (index, &(candidate, _)) in after.iter().enumerate() {
                if name == new.child(candidate).text()? {
                    selected = Some(changed[index]);
                    break;
                }
            }
            let value = match selected {
                Some(value) => value,
                None => self.copy(old.child(value), depth + 1)?,
            };
            output.push((self.copy(old.child(key), depth + 1)?, value));
        }
        for (index, &(key, _)) in after.iter().enumerate() {
            if let Some(old) = old {
                if self.find(old, new.child(key).text()?)?.is_some() {
                    continue;
                }
            }
            output.push((self.copy(new.child(key), depth + 1)?, changed[index]));
        }
        Ok(self.push(N::Mapping(output), old.unwrap_or(new).origin()))
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
    fn keyed(
        &mut self,
        old: Option<Site<'_>>,
        new: Site<'_>,
        identity: &str,
        class: &str,
        path: &str,
        depth: usize,
    ) -> Result<usize, NormalizationError> {
        self.step(depth)?;
        let before = old.map(Site::sequence).transpose()?.unwrap_or(&[]);
        let after = new.sequence()?;
        let capacity = before.len().saturating_add(after.len());
        let fields = &self
            .schema
            .class_named(class)
            .ok_or(ValidationError::InvalidDescriptor)?
            .fields;
        let mut changed = Vec::with_capacity(after.len());
        for (index, &node) in after.iter().enumerate() {
            let incoming = new.child(node);
            let name = self.identity(incoming, identity)?;
            for &prior in &after[..index] {
                let prior = self.identity(new.child(prior), identity)?;
                if prior == name {
                    return Err(ValidationError::InvalidDescriptor.into());
                }
            }
            let mut previous = None;
            for &node in before {
                let candidate = old.expect("nonempty before").child(node);
                let candidate_name = self.identity(candidate, identity)?;
                if candidate_name == name {
                    previous = Some(candidate);
                    break;
                }
            }
            let logical = self.join(path, name)?;
            changed.push(self.boundary(
                previous,
                incoming,
                Boundary::Member {
                    fields,
                    compose: class == "column_class",
                    identity: Some(identity),
                },
                &logical,
                depth + 1,
            )?);
        }
        let mut output = Vec::with_capacity(capacity);
        for &node in before {
            let site = old.expect("nonempty before").child(node);
            let name = self.identity(site, identity)?;
            let mut selected = None;
            for (index, &node) in after.iter().enumerate() {
                let candidate = self.identity(new.child(node), identity)?;
                if candidate == name {
                    selected = Some(changed[index]);
                    break;
                }
            }
            output.push(match selected {
                Some(node) => node,
                None => self.copy(site, depth + 1)?,
            });
        }
        for (index, &node) in after.iter().enumerate() {
            let name = self.identity(new.child(node), identity)?;
            let mut seen = false;
            for &node in before {
                let candidate =
                    self.identity(old.expect("nonempty before").child(node), identity)?;
                if candidate == name {
                    seen = true;
                    break;
                }
            }
            if !seen {
                output.push(changed[index]);
            }
        }
        Ok(self.push(N::Sequence(output), old.unwrap_or(new).origin()))
    }
}

fn provenance(
    document: &Document,
    origins: &[Option<CompositionOrigin>],
) -> Result<Vec<LayerProvenance>, NormalizationError> {
    fn record(
        document: &Document,
        node: usize,
        path: String,
        origins: &[Option<CompositionOrigin>],
        output: &mut Vec<LayerProvenance>,
        depth: usize,
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
                record(
                    document,
                    value,
                    format!("{path}.{label}"),
                    origins,
                    output,
                    depth + 1,
                )?;
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
                        format!("{name}.{id}"),
                        origins,
                        &mut output,
                        0,
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
                        format!("{name}.{id}"),
                        origins,
                        &mut output,
                        0,
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

impl SchemaStructure {
    /// Compose already admitted, normalized layers in contribution order.
    /// Parent traversal/path rebasing precede this operation. Defaults, pruning and
    /// complete validation follow it. Clearing diagnostics retain contribution order.
    pub fn compose_layer_fragments(
        &self,
        layers: &[Document],
    ) -> Result<ComposedLayers, NormalizationError> {
        let mut document =
            Document::new(vec![N::Mapping(vec![])], 0).map_err(NormalizationError::Document)?;
        let mut origins = vec![None];
        let mut diagnostics = Vec::new();
        for (layer, input) in layers.iter().enumerate() {
            let mut run = Run {
                schema: self,
                diagnostics: &mut diagnostics,
                nodes: Vec::new(),
                origins: Vec::new(),
            };
            let root = run.boundary(
                Some(Site {
                    document: &document,
                    node: document.root(),
                    origins: Some(&origins),
                    layer: 0,
                }),
                Site {
                    document: input,
                    node: input.root(),
                    origins: None,
                    layer,
                },
                Boundary::Root,
                "",
                0,
            )?;
            document = Document::new(run.nodes, root).map_err(NormalizationError::Document)?;
            origins = run.origins;
        }
        let provenance = provenance(&document, &origins)?;
        Ok(ComposedLayers {
            document,
            provenance,
            diagnostics,
        })
    }
}
