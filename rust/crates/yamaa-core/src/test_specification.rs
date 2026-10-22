//! 1.0 specifications for the crate's unit tests: authored trees in the
//! normalized form schema admission gives, starting with the `yamaa` and
//! `type` header (D4), lowered by the 1.0 front end, whose tree preparation
//! and binding read (#3030 D76).
use crate::{
    ast::Spec,
    schema::Document,
    specification::{PrepareError, PreparedSpecification},
    test_document::Tree,
};

/// The header fields a 1.0 specification starts with.
pub(crate) fn header() -> Vec<(&'static str, Tree)> {
    vec![("yamaa", Tree::text("1.0")), ("type", Tree::text("spec"))]
}
pub(crate) fn op(name: &str, payload: Tree) -> Tree {
    Tree::map(vec![(name, payload)])
}
pub(crate) fn derivation(value: Tree) -> Tree {
    Tree::map(vec![("value", value)])
}
pub(crate) fn column(name: &str, kind: &str, derivation: Tree) -> Tree {
    Tree::map(vec![
        ("name", Tree::text(name)),
        ("type", Tree::text(kind)),
        ("derivation", derivation),
    ])
}
/// A 1.0 specification whose first column is `ID`, reading `SRC.ID`;
/// `extra` fields are added or replaced.
pub(crate) fn document(columns: Vec<Tree>, extra: Vec<(&str, Tree)>) -> Tree {
    let mut all = vec![column("ID", "int", derivation(Tree::text("SRC.ID")))];
    all.extend(columns);
    let mut fields = header();
    fields.extend([
        ("domain", Tree::text("TEST")),
        (
            "input",
            Tree::map(vec![(
                "SRC",
                Tree::map(vec![("path", Tree::text("SRC.csv"))]),
            )]),
        ),
        ("keys", Tree::list(vec![Tree::text("ID")])),
        ("columns", Tree::list(all)),
        (
            "output",
            Tree::map(vec![
                ("path", Tree::text("out.csv")),
                ("columns", Tree::list(vec![Tree::text("ID")])),
            ]),
        ),
    ]);
    let mut tree = Tree::map(fields);
    for (field, value) in extra {
        tree.set(&[field], value);
    }
    tree
}
/// The tree the 1.0 front end lowers `tree` to.
pub(crate) fn lower(tree: Tree) -> Spec {
    crate::front_end::v1::spec::lower(&tree.document(), None).into_spec()
}
/// Prepare the 1.0 specification `document` without a project file.
pub(crate) fn prepare(document: &Document) -> Result<PreparedSpecification, PrepareError> {
    let tree = crate::front_end::v1::spec::lower(document, None).into_spec();
    PreparedSpecification::prepare_tree_linked(tree, document, None, Default::default())
}
