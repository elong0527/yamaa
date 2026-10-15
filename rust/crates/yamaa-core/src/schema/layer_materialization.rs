//! Finish composed column fields only after every contribution has been applied.

use super::{
    ComposedLayers, Document, DocumentNode as N, NormalizationBudget, NormalizationError,
    NormalizationResource, SchemaStructure, ValidationError,
};
use alloc::{collections::BTreeMap, vec::Vec};

/// Diagnostic node references address this retained composed input, not a layer.
#[derive(Clone, Debug, PartialEq)]
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

fn copy(
    input: &Document,
    node: usize,
    replacements: Option<&BTreeMap<usize, Document>>,
    budget: &mut NormalizationBudget,
    output: &mut Vec<N>,
    depth: usize,
) -> Result<usize, NormalizationError> {
    let limit = budget.limits.depth.min(128);
    if depth > limit {
        return Err(NormalizationError::Limit {
            resource: NormalizationResource::Depth,
            limit,
        });
    }
    budget.work(1)?;
    if let Some(replacements) = replacements {
        budget.work(replacements.len().saturating_add(1).ilog2() as usize + 1)?;
        if let Some(replacement) = replacements.get(&node) {
            return copy(replacement, replacement.root(), None, budget, output, depth);
        }
    }
    let value = match &input.nodes()[node] {
        N::Text(text) | N::Integer(text) => {
            budget.work(text.len())?;
            budget.reserve(text.len(), 0)?;
            input.nodes()[node].clone()
        }
        N::Sequence(items) => {
            budget.reserve(0, items.len())?;
            let mut values = Vec::with_capacity(items.len());
            for &item in items {
                values.push(copy(input, item, replacements, budget, output, depth + 1)?);
            }
            N::Sequence(values)
        }
        N::Mapping(items) => {
            budget.reserve(0, items.len().saturating_mul(2))?;
            let mut values = Vec::with_capacity(items.len());
            for &(key, value) in items {
                values.push((
                    copy(input, key, replacements, budget, output, depth + 1)?,
                    copy(input, value, replacements, budget, output, depth + 1)?,
                ));
            }
            N::Mapping(values)
        }
        node => {
            budget.reserve(0, 0)?;
            node.clone()
        }
    };
    let id = output.len();
    output.push(value);
    Ok(id)
}

impl SchemaStructure {
    /// Merge admitted normalized layers, then materialize supplied column fields once.
    /// Missing whole column fields and other incomplete declarations remain for later
    /// pruning and complete-document validation. Written provenance excludes defaults.
    /// No document is returned when a clear or normalization diagnostic is present.
    pub fn compose_layers(
        &self,
        layers: &[Document],
        budget: &mut NormalizationBudget,
    ) -> Result<ComposedLayers, LayerCompositionError> {
        let mut result = self.compose_layer_fragments(layers, budget)?;
        // Canonical occurrence order also makes diagnostic node references stable:
        // each mapping key precedes its value, independently of merge history.
        let mut nodes = Vec::new();
        let root = copy(
            &result.document,
            result.document.root(),
            None,
            budget,
            &mut nodes,
            1,
        )?;
        result.document = Document::new(nodes, root, budget.limits.storage)
            .map_err(NormalizationError::Document)?;
        let document = &result.document;
        budget.work(
            document.nodes()[document.root()]
                .length()
                .unwrap_or(0)
                .saturating_mul(8),
        )?;
        let mut replacements = BTreeMap::new();
        if let Some(columns) = document.field(document.root(), "columns") {
            let N::Sequence(columns) = &document.nodes()[columns] else {
                return Err(ValidationError::InvalidDescriptor.into());
            };
            let fields = &self
                .class_named("column_class")
                .ok_or(ValidationError::InvalidDescriptor)?
                .fields;
            for &column in columns {
                let N::Mapping(members) = &document.nodes()[column] else {
                    return Err(ValidationError::InvalidDescriptor.into());
                };
                for field in fields {
                    budget.work(
                        members
                            .len()
                            .saturating_mul(field.name.len().saturating_add(1)),
                    )?;
                    if let Some(value) = document.field(column, &field.name) {
                        if matches!(document.nodes()[value], N::Null) {
                            continue;
                        }
                        // Normalize in schema field order, preserving failure precedence.
                        let normalized = match self.normalize_descriptor(
                            field.descriptor,
                            document,
                            value,
                            false,
                            budget,
                        ) {
                            Ok(normalized) => normalized,
                            Err(error) => {
                                return Err(LayerCompositionError {
                                    context_document: if matches!(
                                        error,
                                        NormalizationError::Invalid(_)
                                    ) {
                                        Some(result.document)
                                    } else {
                                        None
                                    },
                                    error,
                                })
                            }
                        };
                        budget.work(replacements.len().saturating_add(1))?;
                        replacements.insert(value, normalized.document);
                    }
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
            let root = copy(
                document,
                document.root(),
                Some(&replacements),
                budget,
                &mut nodes,
                1,
            )?;
            result.document = Document::new(nodes, root, budget.limits.storage)
                .map_err(NormalizationError::Document)?;
        }
        Ok(result)
    }
}
