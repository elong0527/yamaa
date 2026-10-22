//! Finite factories and observers owned by the yamaa-core integration suite.
use yamaa_core::schema as document_schema;
use yamaa_core::{
    table::{Column, TableSchema},
    value::ColumnType,
};

pub mod producer;
pub mod project_call_compiler;
pub mod project_codelist_compiler;
pub mod project_environment;
pub mod roots;
pub mod specification;

/// Construct an ordered schema independently of plan compilation.
pub fn table_schema(columns: &[(&str, ColumnType)]) -> TableSchema {
    TableSchema::new(
        columns
            .iter()
            .map(|&(name, kind)| Column {
                name: name.into(),
                kind,
            })
            .collect(),
    )
    .unwrap()
}

pub fn int_schema(names: &[&str]) -> TableSchema {
    TableSchema::new(
        names
            .iter()
            .map(|name| Column {
                name: (*name).into(),
                kind: ColumnType::Int,
            })
            .collect(),
    )
    .unwrap()
}
pub fn id_source_schema() -> TableSchema {
    int_schema(&["ID"])
}

pub mod ports;

pub mod document;

pub mod diagnostic;

pub mod dataset;

pub mod resolved;

/// The tree composition reads for a submission test document: the document
/// as a normalized 0.5 model holds it, each column's and row template's
/// derivation in its handled class (`{value: ...}`), lowered once.
pub fn submission_tree(document: &document_schema::Document) -> yamaa_core::ast::Spec {
    use document_schema::{Document, DocumentNode as N};
    /// Where a copied node sits.
    #[derive(Clone, Copy, PartialEq)]
    enum At {
        Root,
        Columns,
        Column,
        Rows,
        Row,
        RowDerivations,
        Other,
    }
    fn copy(document: &Document, node: usize, at: At, nodes: &mut Vec<N>) -> usize {
        let copied = match &document.nodes()[node] {
            N::Sequence(items) => {
                let inner = match at {
                    At::Columns => At::Column,
                    At::Rows => At::Row,
                    _ => At::Other,
                };
                N::Sequence(
                    items
                        .iter()
                        .map(|&item| copy(document, item, inner, nodes))
                        .collect(),
                )
            }
            N::Mapping(entries) => {
                let mut copied = Vec::with_capacity(entries.len());
                for &(key, value) in entries {
                    let name = match &document.nodes()[key] {
                        N::Text(text) => text.as_str(),
                        _ => "",
                    };
                    let (inner, wrapped) = match (at, name) {
                        (At::Root, "columns") => (At::Columns, false),
                        (At::Root, "rows") => (At::Rows, false),
                        (At::Row, "derivations") => (At::RowDerivations, false),
                        (At::Column, "derivation") | (At::RowDerivations, _) => {
                            (At::Other, document.field(value, "value").is_none())
                        }
                        _ => (At::Other, false),
                    };
                    let key = copy(document, key, At::Other, nodes);
                    let mut value = copy(document, value, inner, nodes);
                    if wrapped {
                        let field = nodes.len();
                        nodes.push(N::Text("value".into()));
                        nodes.push(N::Mapping(vec![(field, value)]));
                        value = nodes.len() - 1;
                    }
                    copied.push((key, value));
                }
                N::Mapping(copied)
            }
            other => other.clone(),
        };
        nodes.push(copied);
        nodes.len() - 1
    }
    let mut nodes = Vec::new();
    let root = copy(document, document.root(), At::Root, &mut nodes);
    let normalized = Document::new(nodes, root).unwrap();
    yamaa_core::front_end::v0_5::lower_document(&normalized, None).into_spec()
}
