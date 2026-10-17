//! Standalone structural preparation over an admitted schema and one owned raw arena.
//!
//! Physical decoding and occurrence locations remain codec authority. This service
//! neither captures sources nor grants data, function, path or publication authority.
use crate::diagnostic::ConditionCode;
use crate::schema::{
    Document, DocumentNode, ExpandedWindows, InheritanceDependencyError, NormalizationError,
    NormalizedDocument, SchemaContext, SchemaDiagnostic, SchemaOrigin, SchemaSource,
    SchemaStructure, SpecificationDocument, ValidationError, WindowReference,
};

/// Map each origin through the previous pass's origin list, mirroring
/// `LayerNodeOrigin::through`: input origins resolve to the earlier arena,
/// schema-default origins are retained as authored.
fn through(origins: &[SchemaOrigin], prior: &[SchemaOrigin]) -> Vec<SchemaOrigin> {
    origins
        .iter()
        .map(|&origin| match origin.source {
            SchemaSource::Input => {
                let mut mapped = prior[origin.node];
                mapped.generated |= origin.generated;
                mapped
            }
            SchemaSource::Default { .. } => origin,
        })
        .collect()
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("standalone preparation inheritance required")]
    InheritanceRequired,
    #[error("standalone preparation normalize: {0:?}")]
    Normalize(Box<FailureContext>),
    #[error("standalone preparation dependencies: {0:?}")]
    Dependencies(Box<DependencyFailureContext>),
    #[error("standalone preparation validation: {0:?}")]
    Validation(ValidationError),
    /// Final model findings have no document-value references.
    #[error("standalone preparation model: {0:?}")]
    Model(Vec<SchemaDiagnostic>),
}

/// Only authored semantic findings retain the exact failed pass's input arena.
/// Policy/internal failures move their original typed error without an arena copy.
#[derive(Debug)]
pub struct FailureContext {
    pub error: NormalizationError,
    pub input: Option<Document>,
}
fn normalization_failure(error: NormalizationError, input: Document) -> Error {
    let input = matches!(&error, NormalizationError::Invalid(_)).then_some(input);
    Error::Normalize(Box::new(FailureContext { error, input }))
}

/// The pruning pass retains its input arena only for authored semantic findings,
/// mirroring the inheritance lifecycle's dependency failure context.
#[derive(Debug)]
pub struct DependencyFailureContext {
    pub error: InheritanceDependencyError,
    pub input: Option<Document>,
}
fn dependency_failure(error: InheritanceDependencyError, input: Document) -> Error {
    use crate::schema::InheritanceReferenceError;
    let retained = matches!(
        &error,
        InheritanceDependencyError::Normalization(NormalizationError::Invalid(_))
            | InheritanceDependencyError::Reference(InheritanceReferenceError::Normalization(
                NormalizationError::Invalid(_)
            ))
    )
    .then_some(input);
    Error::Dependencies(Box::new(DependencyFailureContext {
        error,
        input: retained,
    }))
}

/// Retain the original raw allocation and every final occurrence's origin.
/// Successful admission qualifies structure, not execution or publication.
#[derive(Debug)]
pub struct PreparedStandalone {
    raw: Document,
    model: SpecificationDocument,
    origins: Vec<SchemaOrigin>,
    windows: Vec<WindowReference>,
}
impl PreparedStandalone {
    pub fn raw(&self) -> &Document {
        &self.raw
    }
    pub fn model(&self) -> &SpecificationDocument {
        &self.model
    }
    pub fn origins(&self) -> &[SchemaOrigin] {
        &self.origins
    }
    pub fn windows(&self) -> &[WindowReference] {
        &self.windows
    }
    pub fn into_parts(
        self,
    ) -> (
        Document,
        SpecificationDocument,
        Vec<SchemaOrigin>,
        Vec<WindowReference>,
    ) {
        (self.raw, self.model, self.origins, self.windows)
    }
}

fn expand(
    schema: &SchemaStructure,
    normalized: NormalizedDocument,
) -> Result<(ExpandedWindows, Vec<SchemaOrigin>), Error> {
    let expanded = match schema.expand_named_windows(&normalized.document, true) {
        Ok(expanded) => expanded,
        Err(error) => return Err(normalization_failure(error, normalized.document)),
    };
    // Existing remap policy: the expanded arena already bounds this parallel list.
    let origins = expanded
        .origins
        .iter()
        .map(|&id| normalized.origins[id])
        .collect();
    Ok((expanded, origins))
}

/// A non-null parent precedes normalization. Normalization, pruning and strict
/// window expansion preserve the source provenance. No study data is read
/// during preparation. Every null field is absent (LAYER-9).
///
/// A specification without a `parent` is a composition of one layer (REQ-0614):
/// after normalization it is pruned on the same terms as a composed
/// specification (REQ-0638–REQ-0641), so a dead declaration's unresolved
/// reference is discarded with it and semantic validation runs on the pruned
/// document. Declaration order remains the resolved order (REQ-0060); the
/// pruning pass does not reorder columns.
pub fn prepare(schema: &SchemaStructure, raw: Document) -> Result<PreparedStandalone, Error> {
    if crate::inheritance::declares_parent(&raw) {
        return Err(Error::InheritanceRequired);
    }
    let normalized = match schema.normalize_specification(&raw) {
        Ok(normalized) => normalized,
        Err(error) => return Err(normalization_failure(error, raw)),
    };
    // A specification without a `parent` is a composition of one layer
    // (REQ-0614), so it admits the same repeated-identifier findings as any
    // other layer (REQ-0659) before pruning can discard a duplicated
    // declaration and hide the repetition.
    let repeated = repeated_identifier_findings(schema, &normalized.document);
    if !repeated.is_empty() {
        return Err(normalization_failure(
            NormalizationError::Invalid(repeated),
            normalized.document,
        ));
    }
    let expanded = match schema.expand_named_windows(&normalized.document, false) {
        Ok(expanded) => expanded,
        Err(error) => return Err(normalization_failure(error, normalized.document)),
    };
    let expanded_origins = expanded
        .origins
        .iter()
        .map(|&id| normalized.origins[id])
        .collect::<Vec<_>>();
    let expanded_references = expanded.references;
    let pruned = match schema.prune_inheritance_dependencies(&expanded.document) {
        Ok(pruned) => pruned,
        Err(error) => return Err(dependency_failure(error, expanded.document)),
    };
    let pruned_origins = through(&pruned.origins, &expanded_origins);
    let resolved = match schema.normalize_resolved_document(&pruned.document) {
        Ok(resolved) => resolved,
        Err(error) => return Err(normalization_failure(error, pruned.document)),
    };
    let resolved = NormalizedDocument {
        document: resolved.document,
        origins: through(&resolved.origins, &pruned_origins),
    };
    let (expanded, origins) = expand(schema, resolved)?;
    let model = SpecificationDocument::admit(expanded.document)
        .map_err(Error::Validation)?
        .map_err(Error::Model)?;
    // The strict expansion runs on the pruned document, whose surviving
    // window selections were already inlined by the non-strict expansion,
    // so it records no references. Report the non-strict references whose
    // declaring column or intermediate survived pruning instead.
    let windows = surviving_references(&pruned.document, expanded_references);
    Ok(PreparedStandalone {
        raw,
        model,
        origins,
        windows,
    })
}

/// Repeated identifiers inside one keyed collection (columns by name, rows
/// and intermediates by id), in schema field order, mirroring the layer
/// admission check (REQ-0659) a composed specification passes through.
fn repeated_identifier_findings(
    schema: &SchemaStructure,
    document: &Document,
) -> Vec<SchemaDiagnostic> {
    let mut findings = Vec::new();
    for field in &schema.root_class().fields {
        let identity = match field.name.as_str() {
            "columns" => "name",
            "rows" | "intermediates" => "id",
            _ => continue,
        };
        let Some(list) = document.field(document.root(), &field.name) else {
            continue;
        };
        let DocumentNode::Sequence(items) = &document.nodes()[list] else {
            continue;
        };
        let mut seen: Vec<&str> = Vec::new();
        for &item in items {
            let Some(id) = document.field(item, identity) else {
                continue;
            };
            let DocumentNode::Text(name) = &document.nodes()[id] else {
                continue;
            };
            if seen.contains(&name.as_str()) {
                findings.push(SchemaDiagnostic {
                    path: format!("{}.{name}.{identity}", field.name),
                    code: ConditionCode::SchemaLayerIdentifierRepeated,
                    context: vec![("identifier", SchemaContext::Text(name.clone()))],
                });
            } else {
                seen.push(name);
            }
        }
    }
    findings
}

/// Keep the window references recorded before pruning whose declaring
/// column or intermediate is still present in the pruned document.
/// References outside `columns`/`intermediates` are kept as recorded.
fn surviving_references(
    pruned: &Document,
    references: Vec<WindowReference>,
) -> Vec<WindowReference> {
    fn declared(document: &Document, field: &str, identity: &str) -> Vec<String> {
        let mut names = Vec::new();
        if let Some(list) = document.field(document.root(), field) {
            if let crate::schema::DocumentNode::Sequence(items) = &document.nodes()[list] {
                for &item in items {
                    if let Some(id) = document.field(item, identity) {
                        if let crate::schema::DocumentNode::Text(name) = &document.nodes()[id] {
                            names.push(name.clone());
                        }
                    }
                }
            }
        }
        names
    }
    let columns = declared(pruned, "columns", "name");
    let intermediates = declared(pruned, "intermediates", "id");
    references
        .into_iter()
        .filter(|reference| {
            for (prefix, names) in [("columns.", &columns), ("intermediates.", &intermediates)] {
                if let Some(rest) = reference.path.strip_prefix(prefix) {
                    let name = rest.split('.').next().unwrap_or(rest);
                    return names.iter().any(|candidate| candidate == name);
                }
            }
            true
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::{DocumentNode as N, SchemaContext, SchemaModule};
    use std::vec;

    #[test]
    fn strict_failure_moves_the_actual_normalized_checkpoint_allocation() {
        // Closed root with a bare window-bearing column list. The schema/input
        // arenas are independently authored; no adapter JSON or YAML participates.
        use crate::test_document::Tree;

        fn doc(tree: Tree) -> Document {
            tree.document()
        }
        fn field(name: &'static str, kind: &'static str) -> Tree {
            Tree::map(vec![(name, Tree::map(vec![("type", Tree::text(kind))]))])
        }
        let schema = SchemaStructure::admit(
            vec![SchemaModule {
                name: "schema.yaml".into(),
                document: doc(Tree::map(vec![
                    ("version", Tree::text("1.0")),
                    (
                        "root",
                        Tree::List(vec![
                            field("schema_version", "str"),
                            field("columns", "list[column_class]"),
                        ]),
                    ),
                    (
                        "column_class",
                        Tree::List(vec![
                            field("name", "str"),
                            field("derivation", "handled_expression_class"),
                        ]),
                    ),
                    (
                        "handled_expression_class",
                        Tree::List(vec![field("value", "expression")]),
                    ),
                    (
                        "window_selection",
                        Tree::map(vec![("type", Tree::text("str"))]),
                    ),
                    (
                        "expression",
                        Tree::map(vec![("registry", Tree::text("expressions"))]),
                    ),
                    (
                        "expressions",
                        Tree::map(vec![(
                            "row_number",
                            Tree::List(vec![field("window", "window_selection")]),
                        )]),
                    ),
                ])),
            }],
            0,
            "root",
        )
        .unwrap();
        // Unknown window is found by the lifecycle's column expression traversal.
        let nodes = vec![
            N::Text("schema_version".into()),
            N::Text("1.0".into()),
            N::Text("columns".into()),
            N::Text("name".into()),
            N::Text("A".into()),
            N::Text("derivation".into()),
            N::Text("value".into()),
            N::Text("row_number".into()),
            N::Text("window".into()),
            N::Text("ABSENT".into()),
            N::Mapping(vec![(8, 9)]),
            N::Mapping(vec![(7, 10)]),
            N::Mapping(vec![(6, 11)]),
            N::Mapping(vec![(3, 4), (5, 12)]),
            N::Sequence(vec![13]),
            N::Mapping(vec![(0, 1), (2, 14)]),
        ];
        let raw = Document::new(nodes, 15).unwrap();
        let normalized = schema.normalize_document(&raw).unwrap();
        let pointer = normalized.document.nodes().as_ptr();
        let expected = normalized.document.clone();
        let Err(Error::Normalize(failure)) = expand(&schema, normalized) else {
            panic!("expected strict window failure");
        };
        let held = failure.input.unwrap();
        assert_eq!(held.nodes().as_ptr(), pointer);
        assert_eq!(held, expected);
        assert_eq!(
            failure.error,
            NormalizationError::Invalid(vec![SchemaDiagnostic {
                path: String::from("columns.A.derivation.row_number.window"),
                code: crate::diagnostic::ConditionCode::SchemaUnknownWindow,
                context: vec![("window", SchemaContext::Text("ABSENT".into()))],
            }])
        );
    }
}
