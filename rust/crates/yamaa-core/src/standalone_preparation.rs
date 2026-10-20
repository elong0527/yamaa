//! Standalone structural preparation over an admitted schema and one owned raw arena.
//!
//! Physical decoding and occurrence locations remain codec authority. This service
//! neither captures sources nor grants data, function, path or publication authority.
use crate::diagnostic::ConditionCode;
use crate::resolution::{self, Failure, Order};
use crate::schema::{
    Document, DocumentNode, NormalizationError, SchemaContext, SchemaDiagnostic, SchemaOrigin,
    SchemaStructure, SpecificationDocument, ValidationError, WindowReference,
};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("standalone preparation inheritance required")]
    InheritanceRequired,
    #[error("standalone preparation normalize: {0:?}")]
    Normalize(Box<FailureContext>),
    #[error("standalone preparation dependencies: {0:?}")]
    Dependencies(Box<resolution::Error>),
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

/// A non-null parent precedes normalization, and no study data is read. A
/// root that declares the 1.x header is refused before the 0.5 closure reads
/// it (SCHEMA-2).
/// A specification without a `parent` is a composition of one layer
/// (LAYER-1): after normalization it is resolved on the same terms as a
/// composed specification, so a dead declaration's unresolved reference is
/// discarded with it. Declaration order remains the resolved order
/// (COLUMN-5), and every final occurrence keeps its raw origin.
pub fn prepare(schema: &SchemaStructure, raw: Document) -> Result<PreparedStandalone, Error> {
    if crate::inheritance::declares_parent(&raw) {
        return Err(Error::InheritanceRequired);
    }
    // A root that declares the 1.x header is refused before the 0.5 closure
    // reads it (SCHEMA-2, #3030 D76).
    if let Some(fault) = crate::language::refusal(&raw) {
        return Err(normalization_failure(
            NormalizationError::Invalid(vec![fault.finding()]),
            raw,
        ));
    }
    let normalized = match schema.normalize_specification(&raw) {
        Ok(normalized) => normalized,
        Err(error) => return Err(normalization_failure(error, raw)),
    };
    // The same repeated-identifier findings as any other layer (LAYER-4),
    // before pruning can discard a duplicated declaration.
    let repeated = repeated_identifier_findings(schema, &normalized.document);
    if !repeated.is_empty() {
        return Err(normalization_failure(
            NormalizationError::Invalid(repeated),
            normalized.document,
        ));
    }
    let resolved = resolution::resolve(
        schema,
        normalized.document,
        &normalized.origins,
        Order::Declared,
    )
    .map_err(|failure| match failure {
        Failure::Normalize { error, input } => normalization_failure(error, input),
        Failure::Prune(error) => Error::Dependencies(Box::new(error)),
        Failure::Validation(error) => Error::Validation(error),
        Failure::Model(findings) => Error::Model(findings),
    })?;
    Ok(PreparedStandalone {
        raw,
        model: resolved.model,
        origins: resolved.traced,
        windows: resolved.windows,
    })
}

/// Repeated identifiers inside one keyed collection (columns by name, rows
/// and intermediates by id), in schema field order, mirroring the layer
/// admission check (LAYER-4) a composed specification passes through.
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::{SchemaContext, SchemaModule};
    use std::vec;

    #[test]
    fn strict_failure_holds_the_arena_its_pass_read() {
        // Closed root with a bare window-bearing column list, which `keys`
        // keeps live. The schema/input arenas are independently authored; no
        // adapter JSON or YAML participates.
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
                            field("keys", "list[str]"),
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
        // The unknown window survives the non-strict expansion and pruning,
        // and the strict expansion of the normalized document refuses it.
        let window = Tree::map(vec![("window", Tree::text("ABSENT"))]);
        let column = Tree::map(vec![
            ("name", Tree::text("A")),
            (
                "derivation",
                Tree::map(vec![("value", Tree::map(vec![("row_number", window)]))]),
            ),
        ]);
        let raw = doc(Tree::map(vec![
            ("schema_version", Tree::text("1.0")),
            ("keys", Tree::List(vec![Tree::text("A")])),
            ("columns", Tree::List(vec![column])),
        ]));
        let Err(Error::Normalize(failure)) = prepare(&schema, raw) else {
            panic!("expected strict window failure");
        };
        let expected = NormalizationError::Invalid(vec![SchemaDiagnostic {
            path: String::from("columns.A.derivation.row_number.window"),
            code: crate::diagnostic::ConditionCode::SchemaUnknownWindow,
            context: vec![("window", SchemaContext::Text("ABSENT".into()))],
        }]);
        assert_eq!(failure.error, expected);
        // The held arena is the one the failed pass read: reading it again
        // refuses it with the same finding.
        let held = failure.input.unwrap();
        assert_eq!(schema.expand_named_windows(&held, true), Err(expected));
    }
}
