//! Schema-directed named-window expansion, without host values or filesystem IO.

use super::{
    walk::Visit, Document, DocumentNode as N, NormalizationError, SchemaContext, SchemaDiagnostic,
    SchemaStructure, ValidationError,
};
use crate::diagnostic::ConditionCode;
use std::{collections::BTreeMap, vec};

/// Logical use-site and final definition; hosts may attach their retained layer origins.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WindowReference {
    pub path: String,
    pub definition: String,
}

/// Every expanded occurrence owns its children. Origins address the retained input tree.
#[derive(Clone, Debug, PartialEq)]
pub struct ExpandedWindows {
    pub document: Document,
    /// A replacement root keeps the reference origin; its children name definition nodes.
    pub origins: Vec<usize>,
    /// Schema traversal order, independent of the input mapping's storage order.
    pub references: Vec<WindowReference>,
}

struct Run<'a> {
    input: &'a Document,
    strict: bool,
    definitions: Option<usize>,
    replacements: BTreeMap<usize, usize>,
    references: Vec<WindowReference>,
    diagnostics: Vec<SchemaDiagnostic>,
    nodes: Vec<N>,
    origins: Vec<usize>,
}

impl Visit for Run<'_> {
    /// A written window name is a reference to a root definition.
    fn member(
        &mut self,
        kind: &str,
        value: usize,
        logical: &str,
    ) -> Result<bool, NormalizationError> {
        if kind != "window_selection" {
            return Ok(false);
        }
        let N::Text(name) = &self.input.nodes()[value] else {
            return Ok(false);
        };
        self.reference(value, logical, name);
        Ok(true)
    }
}

impl Run<'_> {
    fn reference(&mut self, value: usize, logical: &str, name: &str) {
        let definition = self.definitions.and_then(|id| self.input.field(id, name));
        if let Some(definition) = definition {
            // Account for lookup and retained logical metadata before allocation.
            self.replacements.insert(value, definition);
            self.references.push(WindowReference {
                path: logical.into(),
                definition: name.into(),
            });
        } else if self.strict {
            let mut path = logical.replace(".derivation.value.", ".derivation.");
            if path.ends_with(".derivation.value") {
                path.truncate(path.len() - ".value".len());
            }
            self.diagnostics.push(super::validation::diagnostic(
                &path,
                ConditionCode::SchemaUnknownWindow,
                vec![("window", SchemaContext::Text(name.into()))],
            ));
        }
    }
    fn copy(
        &mut self,
        value: usize,
        depth: usize,
        replace: bool,
    ) -> Result<usize, NormalizationError> {
        NormalizationError::check_depth(depth)?;
        // A definition is copied whole, never recursively treated as another reference.
        let source = if replace {
            self.replacements.get(&value).copied().unwrap_or(value)
        } else {
            value
        };
        let expanded = source != value;
        let node = match &self.input.nodes()[source] {
            N::Text(_) | N::Integer(_) => self.input.nodes()[source].clone(),
            N::Sequence(items) => {
                let mut output = Vec::with_capacity(items.len());
                for &item in items {
                    output.push(self.copy(item, depth + 1, replace && !expanded)?);
                }
                N::Sequence(output)
            }
            N::Mapping(items) => {
                // Reserving the omitted root definition's edges is conservative.
                let mut output = Vec::with_capacity(items.len());
                for &(key, item) in items {
                    if self.strict
                        && value == self.input.root()
                        && matches!(&self.input.nodes()[key], N::Text(name) if name == "windows")
                    {
                        continue;
                    }
                    output.push((
                        self.copy(key, depth + 1, false)?,
                        self.copy(item, depth + 1, replace && !expanded)?,
                    ));
                }
                N::Mapping(output)
            }
            node => node.clone(),
        };
        let result = self.nodes.len();
        self.nodes.push(node);
        self.origins.push(value);
        Ok(result)
    }
}

impl SchemaStructure {
    /// Expand normalized composed documents without changing their caller scope.
    /// Non-strict mode preserves unknown references for inherited-declaration pruning.
    /// Strict mode diagnoses surviving unknown names and removes root definitions.
    /// Definitions are validated even when unused. Every declaration is checked in written order.
    pub fn expand_named_windows(
        &self,
        input: &Document,
        strict: bool,
    ) -> Result<ExpandedWindows, NormalizationError> {
        // A root that is not a mapping has no definitions; the walk refuses it.
        let mut run = Run {
            input,
            strict,
            definitions: input.field(input.root(), "windows"),
            replacements: BTreeMap::new(),
            references: Vec::new(),
            diagnostics: Vec::new(),
            nodes: Vec::new(),
            origins: Vec::new(),
        };
        if let Some(value) = run.definitions {
            let field = self
                .root_class()
                .fields
                .iter()
                .find(|field| field.name == "windows")
                .ok_or(ValidationError::InvalidDescriptor)?;
            // Window expansion reads a normalized or composed document.
            let diagnostics = self.validate_canonical_descriptor(
                field.descriptor,
                input,
                value,
                "windows",
                false,
            )?;
            if !diagnostics.is_empty() {
                return Err(NormalizationError::Invalid(diagnostics));
            }
        }
        self.walk_document(input, &mut run)?;
        if !run.diagnostics.is_empty() {
            return Err(NormalizationError::Invalid(run.diagnostics));
        }
        let root = run.copy(input.root(), 1, true)?;
        let document = Document::new(run.nodes, root).map_err(NormalizationError::Document)?;
        Ok(ExpandedWindows {
            document,
            origins: run.origins,
            references: run.references,
        })
    }
}
