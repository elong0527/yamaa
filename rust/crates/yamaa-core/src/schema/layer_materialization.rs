//! Finish composed column fields only after every contribution has been applied.

use super::{
    validation::NullPolicy, ComposedLayers, Document, DocumentNode as N, LayerNodeOrigin,
    NormalizationError, NormalizationResource, SchemaStructure, ValidationError,
};
use std::collections::BTreeMap;

/// Diagnostic node references address this retained composed input, not a layer.
#[derive(Clone, Debug, PartialEq, thiserror::Error)]
#[error("schema layer materialization: error={error:?}, context_document={context_document:?}")]
pub struct LayerCompositionError {
    pub error: NormalizationError,
    pub context_document: Option<Document>,
}
impl From<NormalizationError> for LayerCompositionError {
    fn from(error: NormalizationError) -> Self {
        Self {
            error,
            context_document: None,
        }
    }
}
impl From<ValidationError> for LayerCompositionError {
    fn from(error: ValidationError) -> Self {
        NormalizationError::from(error).into()
    }
}

struct Materialized {
    document: Document,
    origins: Vec<LayerNodeOrigin>,
}

fn copy(
    input: &Document,
    input_origins: &[LayerNodeOrigin],
    node: usize,
    replacements: Option<&BTreeMap<usize, Materialized>>,
    output: &mut Vec<N>,
    output_origins: &mut Vec<LayerNodeOrigin>,
    depth: usize,
) -> Result<usize, NormalizationError> {
    let limit = crate::limits::Limits::SEMANTIC_DEPTH;
    if depth > limit {
        return Err(NormalizationError::Limit {
            resource: NormalizationResource::Depth,
            limit,
        });
    }
    if let Some(replacements) = replacements {
        if let Some(replacement) = replacements.get(&node) {
            return copy(
                &replacement.document,
                &replacement.origins,
                replacement.document.root(),
                None,
                output,
                output_origins,
                depth,
            );
        }
    }
    let value = match &input.nodes()[node] {
        N::Text(_) | N::Integer(_) => input.nodes()[node].clone(),
        N::Sequence(items) => {
            let mut values = Vec::with_capacity(items.len());
            for &item in items {
                values.push(copy(
                    input,
                    input_origins,
                    item,
                    replacements,
                    output,
                    output_origins,
                    depth + 1,
                )?);
            }
            N::Sequence(values)
        }
        N::Mapping(items) => {
            let mut values = Vec::with_capacity(items.len());
            for &(key, value) in items {
                values.push((
                    copy(
                        input,
                        input_origins,
                        key,
                        replacements,
                        output,
                        output_origins,
                        depth + 1,
                    )?,
                    copy(
                        input,
                        input_origins,
                        value,
                        replacements,
                        output,
                        output_origins,
                        depth + 1,
                    )?,
                ));
            }
            N::Mapping(values)
        }
        node => node.clone(),
    };
    let id = output.len();
    output.push(value);
    output_origins.push(input_origins[node]);
    Ok(id)
}

impl SchemaStructure {
    /// The class of a keyed collection's members: the `T` of a `list[T]` or
    /// `dict[K, T]` root field, or the class member of a union `T`.
    pub(super) fn member_class(&self, collection: usize) -> Option<&super::SchemaClass> {
        let [expression] = self.descriptors().get(collection)?.descriptor.members() else {
            return None;
        };
        let member = match &expression.nodes()[expression.root()] {
            super::TypeNode::List(inner) => *inner,
            super::TypeNode::Dictionary { value, .. } => *value,
            super::TypeNode::Name(_) => return None,
        };
        let name = expression.node_text(member)?;
        if let Some(class) = self.class_named(name) {
            return Some(class);
        }
        let (_, alias) = self.alias_named(name)?;
        let super::SchemaAliasKind::Descriptor(id) = alias.kind else {
            return None;
        };
        self.descriptors()[id]
            .descriptor
            .members()
            .iter()
            .find_map(|member| self.class_named(member.node_text(member.root())?))
    }

    /// Merge admitted normalized layers, then materialize the fields every keyed
    /// member carries once. Missing whole member fields and other incomplete
    /// declarations remain for later pruning and complete-document validation.
    /// Written provenance excludes defaults. No document is returned when a clear
    /// or normalization diagnostic is present.
    pub fn compose_layers(
        &self,
        layers: &[Document],
    ) -> Result<ComposedLayers, LayerCompositionError> {
        let mut result = self.compose_layer_fragments(layers)?;
        // Canonical occurrence order also makes diagnostic node references stable:
        // each mapping key precedes its value, independently of merge history.
        let mut nodes = Vec::new();
        let mut origins = Vec::new();
        let root = copy(
            &result.document,
            &result.node_origins,
            result.document.root(),
            None,
            &mut nodes,
            &mut origins,
            1,
        )?;
        result.document = Document::new(nodes, root).map_err(NormalizationError::Document)?;
        result.node_origins = origins;
        let document = &result.document;
        let mut replacements = BTreeMap::new();
        for field in &self.root_class().fields {
            let identity = match field.name.as_str() {
                "input" | "windows" => None,
                "columns" => Some("name"),
                "rows" | "intermediates" => Some("id"),
                _ => continue,
            };
            let Some(collection) = document.field(document.root(), &field.name) else {
                continue;
            };
            let members: Vec<(String, usize)> = match (&document.nodes()[collection], identity) {
                (N::Sequence(items), Some(identity)) => items
                    .iter()
                    .map(|&member| {
                        let name = document
                            .field(member, identity)
                            .and_then(|id| match &document.nodes()[id] {
                                N::Text(name) => Some(name.clone()),
                                _ => None,
                            })
                            .ok_or(ValidationError::InvalidDescriptor)?;
                        Ok((name, member))
                    })
                    .collect::<Result<_, ValidationError>>()?,
                (N::Mapping(items), None) => items
                    .iter()
                    .map(|&(key, member)| match &document.nodes()[key] {
                        N::Text(name) => Ok((name.clone(), member)),
                        _ => Err(ValidationError::InvalidDescriptor),
                    })
                    .collect::<Result<_, ValidationError>>()?,
                _ => return Err(ValidationError::InvalidDescriptor.into()),
            };
            let fields = &self
                .member_class(field.descriptor)
                .ok_or(ValidationError::InvalidDescriptor)?
                .fields;
            for (name, member) in members {
                let N::Mapping(_) = &document.nodes()[member] else {
                    return Err(ValidationError::InvalidDescriptor.into());
                };
                for member_field in fields {
                    let Some(value) = document.field(member, &member_field.name) else {
                        continue;
                    };
                    // Normalize in schema field order, preserving failure precedence.
                    let path = format!("{}.{name}.{}", field.name, member_field.name);
                    let findings = self.validate_descriptor_with(
                        member_field.descriptor,
                        document,
                        value,
                        &path,
                        false,
                        NullPolicy::Absent,
                    )?;
                    let normalized = if findings.is_empty() {
                        self.normalize_descriptor_with(
                            member_field.descriptor,
                            document,
                            value,
                            false,
                            NullPolicy::Absent,
                        )
                    } else {
                        Err(NormalizationError::Invalid(findings))
                    };
                    let normalized = match normalized {
                        Ok(normalized) => normalized,
                        Err(error) => {
                            return Err(LayerCompositionError {
                                context_document: if matches!(error, NormalizationError::Invalid(_))
                                {
                                    Some(result.document)
                                } else {
                                    None
                                },
                                error,
                            })
                        }
                    };
                    let origins = normalized
                        .origins
                        .iter()
                        .map(|&origin| LayerNodeOrigin::through(origin, &result.node_origins))
                        .collect();
                    replacements.insert(
                        value,
                        Materialized {
                            document: normalized.document,
                            origins,
                        },
                    );
                }
            }
        }
        if !result.diagnostics.is_empty() {
            return Err(LayerCompositionError {
                error: NormalizationError::Invalid(result.diagnostics),
                context_document: Some(result.document),
            });
        }
        if !replacements.is_empty() {
            let mut nodes = Vec::new();
            let mut origins = Vec::new();
            let root = copy(
                document,
                &result.node_origins,
                document.root(),
                Some(&replacements),
                &mut nodes,
                &mut origins,
                1,
            )?;
            result.document = Document::new(nodes, root).map_err(NormalizationError::Document)?;
            result.node_origins = origins;
        }
        Ok(result)
    }
}
