//! Standalone structural preparation over an admitted schema and one owned raw arena.
//!
//! Physical decoding and occurrence locations remain codec authority. This service
//! neither captures sources nor grants data, function, path or publication authority.
use alloc::{boxed::Box, vec::Vec};
use yamaa_core::schema::{
    Document, ExpandedWindows, NormalizationBudget, NormalizationError, NormalizedDocument,
    SchemaDiagnostic, SchemaOrigin, SchemaStructure, SpecificationDocument, ValidationError,
    WindowReference,
};

#[derive(Debug)]
pub enum Error {
    InheritanceRequired,
    Normalize(Box<FailureContext>),
    Validation(ValidationError),
    /// Final model findings have no document-value references.
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
    budget: &mut NormalizationBudget,
) -> Result<(ExpandedWindows, Vec<SchemaOrigin>), Error> {
    let expanded = match schema.expand_named_windows(&normalized.document, true, budget) {
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

/// Parent presence (including null) precedes normalization. Normalization, strict
/// window expansion and model admission share the caller's existing cumulative
/// budget; failed attempts retain their charges. No counters or limits are reset.
pub fn prepare(
    schema: &SchemaStructure,
    raw: Document,
    budget: &mut NormalizationBudget,
) -> Result<PreparedStandalone, Error> {
    if raw.field(raw.root(), "parents").is_some() {
        return Err(Error::InheritanceRequired);
    }
    let normalized = match schema.normalize_document(&raw, budget) {
        Ok(normalized) => normalized,
        Err(error) => return Err(normalization_failure(error, raw)),
    };
    let (expanded, origins) = expand(schema, normalized, budget)?;
    let model = SpecificationDocument::admit(expanded.document, budget.validation_scope())
        .map_err(Error::Validation)?
        .map_err(Error::Model)?;
    Ok(PreparedStandalone {
        raw,
        model,
        origins,
        windows: expanded.references,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::{string::String, vec};
    use yamaa_core::schema::{
        BundleLimits, DocumentLimits, DocumentNode as N, NormalizationLimits, SchemaContext,
        SchemaModule,
    };

    #[test]
    fn strict_failure_moves_the_actual_normalized_checkpoint_allocation() {
        // Closed root with a bare window-bearing column list. The schema/input
        // arenas are independently authored; no adapter JSON or YAML participates.
        enum Tree {
            Text(&'static str),
            List(Vec<Tree>),
            Map(Vec<(&'static str, Tree)>),
        }
        use Tree::{List, Map, Text};
        fn doc(tree: Tree) -> Document {
            fn push(tree: Tree, nodes: &mut Vec<N>) -> usize {
                let node = match tree {
                    Text(text) => N::Text(text.into()),
                    List(items) => N::Sequence(items.into_iter().map(|v| push(v, nodes)).collect()),
                    Map(items) => N::Mapping(
                        items
                            .into_iter()
                            .map(|(k, v)| (push(Text(k), nodes), push(v, nodes)))
                            .collect(),
                    ),
                };
                let id = nodes.len();
                nodes.push(node);
                id
            }
            let mut nodes = Vec::new();
            let root = push(tree, &mut nodes);
            Document::new(nodes, root, DocumentLimits::default()).unwrap()
        }
        fn field(name: &'static str, kind: &'static str) -> Tree {
            Map(vec![(name, Map(vec![("type", Text(kind))]))])
        }
        let schema = SchemaStructure::admit(
            vec![SchemaModule {
                name: "schema.yaml".into(),
                document: doc(Map(vec![
                    ("version", Text("1.0")),
                    (
                        "root",
                        List(vec![
                            field("schema_version", "str"),
                            field("columns", "list[column_class]"),
                        ]),
                    ),
                    (
                        "column_class",
                        List(vec![
                            field("name", "str"),
                            field("derivation", "handled_expression_class"),
                        ]),
                    ),
                    (
                        "handled_expression_class",
                        List(vec![field("value", "expression")]),
                    ),
                    ("window_selection", Map(vec![("type", Text("str"))])),
                    ("expression", Map(vec![("registry", Text("expressions"))])),
                    (
                        "expressions",
                        Map(vec![(
                            "row_number",
                            List(vec![field("window", "window_selection")]),
                        )]),
                    ),
                ])),
            }],
            0,
            "root",
            BundleLimits::default(),
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
        let raw = Document::new(nodes, 15, DocumentLimits::default()).unwrap();
        let mut budget = NormalizationBudget::new(NormalizationLimits::default());
        let normalized = schema.normalize_document(&raw, &mut budget).unwrap();
        let pointer = normalized.document.nodes().as_ptr();
        let expected = normalized.document.clone();
        let Err(Error::Normalize(failure)) = expand(&schema, normalized, &mut budget) else {
            panic!("expected strict window failure");
        };
        let held = failure.input.unwrap();
        assert_eq!(held.nodes().as_ptr(), pointer);
        assert_eq!(held, expected);
        assert_eq!(
            failure.error,
            NormalizationError::Invalid(vec![SchemaDiagnostic {
                path: String::from("columns.A.derivation.row_number.window"),
                code: yamaa_core::diagnostic::ConditionCode::SchemaUnknownWindow,
                context: vec![("window", SchemaContext::Text("ABSENT".into()))],
            }])
        );
    }
}
