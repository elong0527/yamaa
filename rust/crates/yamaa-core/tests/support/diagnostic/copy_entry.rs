//! The copy entry's refusals (SPEC-9), each reached by the detector that
//! makes it: layer admission, the copy entries of a chain's layers, and a
//! producer read back into a specification being resolved.
use super::rendered;
use crate::support::producer::{producer, Tree};
use std::collections::{BTreeMap, BTreeSet};
use yamaa_core::{
    copy_expansion::{producer_cycle, Copies},
    diagnostic::ConditionCode,
    json::Value,
    producer_contract::prepare_document,
    schema::{Document, SchemaContext as C, SchemaDiagnostic, SchemaModule, SchemaStructure},
};

fn document(tree: Tree) -> Document {
    let mut nodes = Vec::new();
    let root = tree.append(&mut nodes);
    Document::new(nodes, root).unwrap()
}

fn copy(dataset: &str, columns: &[&str]) -> Tree {
    Tree::map(vec![(
        "copy",
        Tree::map(vec![
            ("dataset", Tree::text(dataset)),
            (
                "columns",
                Tree::list(columns.iter().copied().map(Tree::text).collect()),
            ),
        ]),
    )])
}

fn check(
    reached: &mut BTreeSet<ConditionCode>,
    finding: &SchemaDiagnostic,
    code: ConditionCode,
    condition: &str,
    path: &str,
    context: &[(&str, &str)],
) {
    assert_eq!(finding.code, code);
    let definition = finding.definition();
    assert_eq!(
        (definition.phase, definition.condition, definition.rule),
        ("validation", condition, Some("SPEC-9"))
    );
    assert_eq!(finding.path, path);
    let context: Vec<_> = context
        .iter()
        .map(|&(key, value)| (key, C::Text(value.into())))
        .collect();
    assert_eq!(finding.context, context);
    reached.insert(rendered(finding));
}

pub fn reached() -> BTreeSet<ConditionCode> {
    let mut reached = BTreeSet::new();
    // A name a copy entry lists that another column of the same layer
    // declares is repeated, at the later of the two.
    let field = |name: &str, kind: &str| {
        Tree::map(vec![(name, Tree::map(vec![("type", Tree::text(kind))]))])
    };
    let layers = SchemaStructure::admit(
        vec![SchemaModule {
            name: "schema.yaml".into(),
            document: document(Tree::map(vec![
                ("version", Tree::text("1.0")),
                (
                    "root_class",
                    Tree::list(vec![field("columns", "list[column_class]")]),
                ),
                ("column_class", Tree::list(vec![field("name", "str")])),
            ])),
        }],
        0,
        "root_class",
    )
    .unwrap();
    let layer = document(Tree::map(vec![(
        "columns",
        Tree::list(vec![
            Tree::map(vec![("name", Tree::text("AGE"))]),
            copy("ADSL", &["SEX", "AGE"]),
        ]),
    )]));
    let findings = layers.validate_layer(&layer).unwrap();
    let repeated = findings
        .iter()
        .find(|finding| finding.code == ConditionCode::CopyDuplicateIdentifier)
        .unwrap();
    check(
        &mut reached,
        repeated,
        ConditionCode::CopyDuplicateIdentifier,
        "duplicate_identifier",
        "columns[1].copy.columns[1]",
        &[("identifier", "AGE")],
    );
    // A dataset that names no input, or an input without a `schema` link.
    let layer = |input: Tree| {
        document(Tree::map(vec![
            ("input", input),
            ("columns", Tree::list(vec![copy("SRC", &["ID", "BMI"])])),
        ]))
    };
    let unknown = layer(Tree::map(vec![(
        "RAW",
        Tree::map(vec![("path", Tree::text("raw.csv"))]),
    )]));
    let (index, findings) = Copies::of(&[&unknown]).unwrap().dataset_findings().unwrap();
    assert_eq!(index, 0);
    check(
        &mut reached,
        &findings[0],
        ConditionCode::CopyUnknownDataset,
        "unknown_field",
        "columns[0].copy.dataset",
        &[("identifier", "SRC")],
    );
    let unlinked = layer(Tree::map(vec![(
        "SRC",
        Tree::map(vec![("path", Tree::text("produced.csv"))]),
    )]));
    let (_, findings) = Copies::of(&[&unlinked])
        .unwrap()
        .dataset_findings()
        .unwrap();
    check(
        &mut reached,
        &findings[0],
        ConditionCode::CopyUnlinkedInput,
        "prohibited_construct",
        "columns[0].copy.dataset",
        // The copy entry is the construct the unlinked input refuses.
        &[("dataset", "SRC"), ("construct", "copy")],
    );
    // A listed name that is not one of the producer's output columns.
    let linked = layer(Tree::map(vec![(
        "SRC",
        Tree::map(vec![
            ("path", Tree::text("produced.csv")),
            ("schema", Tree::text("producer.yaml")),
        ]),
    )]));
    let copies = Copies::of(&[&linked]).unwrap();
    assert_eq!(copies.dataset_findings(), None);
    let contracts = BTreeMap::from([("SRC", prepare_document(&producer()).unwrap())]);
    let (_, findings) = copies.column_findings(&contracts).unwrap();
    assert_eq!(findings.len(), 1);
    check(
        &mut reached,
        &findings[0],
        ConditionCode::CopyUnknownColumn,
        "unknown_field",
        "columns[0].copy.columns[1]",
        &[("identifier", "SRC.BMI")],
    );
    // A producer whose copies lead back to a specification being resolved.
    let cycle = producer_cycle("SRC", &["/adae.yaml", "/adsl.yaml", "/adae.yaml"]);
    let definition = cycle.definition();
    assert_eq!(
        (definition.phase, definition.condition, definition.rule),
        ("validation", "producer_workflow_cycle", Some("SPEC-9"))
    );
    assert_eq!(cycle.spec_paths, ["input.SRC.schema"]);
    assert_eq!(cycle.context["dataset"], Value::String("SRC".into()));
    assert_eq!(
        cycle.message(),
        "the producer links form a cycle: /adae.yaml, /adsl.yaml, /adae.yaml"
    );
    reached.insert(rendered(&cycle));
    reached
}
