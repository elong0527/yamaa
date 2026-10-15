//! Standalone structural preparation over an admitted schema and one owned raw arena.
//!
//! Physical decoding and occurrence locations remain codec authority. This service
//! neither captures sources nor grants data, function, path or publication authority.
use crate::schema::{
    Document, ExpandedWindows, NormalizationError, NormalizedDocument, SchemaDiagnostic,
    SchemaOrigin, SchemaStructure, SpecificationDocument, ValidationError, WindowReference,
};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("standalone preparation inheritance required")]
    InheritanceRequired,
    #[error("standalone preparation normalize: {0:?}")]
    Normalize(Box<FailureContext>),
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

/// Parent presence (including null) precedes normalization. Normalization, strict
/// window expansion and model admission preserve the source provenance.
/// No study data is read during preparation.
pub fn prepare(schema: &SchemaStructure, raw: Document) -> Result<PreparedStandalone, Error> {
    if raw.field(raw.root(), "parents").is_some() {
        return Err(Error::InheritanceRequired);
    }
    let normalized = match schema.normalize_document(&raw) {
        Ok(normalized) => normalized,
        Err(error) => return Err(normalization_failure(error, raw)),
    };
    let (expanded, origins) = expand(schema, normalized)?;
    let model = SpecificationDocument::admit(expanded.document)
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
