//! A submission study written as test data: a 1.0 project whose `sdtm`
//! section lists one specification of the `SDTM` model (D4, D16, D25), a
//! codelist source, and the composition of the study's Define-XML metadata
//! over them.
use super::{document::Tree, roots as rt};
use yamaa_core::{
    front_end::v1::spec,
    project_terminology as ct,
    submission_composition::{self as composition, Finding, Input, Study},
    value::Value,
};

/// A column's submission metadata, with its one origin (P-ORIGIN-1).
fn metadata() -> Tree {
    Tree::map(vec![
        ("core", Tree::text("Exp")),
        ("length", Tree::integer("8")),
        (
            "origins",
            Tree::list(vec![Tree::map(vec![
                ("type", Tree::text("Assigned")),
                ("source", Tree::text("Sponsor")),
            ])]),
        ),
    ])
}

/// The text column `name`, a literal, with its label and submission
/// metadata.
pub fn column(name: &str) -> Tree {
    Tree::map(vec![
        ("name", Tree::text(name)),
        ("type", Tree::text("str")),
        ("label", Tree::text("Independent column")),
        ("derivation", Tree::text("'X'")),
        ("submission", metadata()),
    ])
}

/// A 1.0 specification (D4) of the `SDTM` model (D25), which the `sdtm`
/// section lists.
pub fn specification() -> Tree {
    Tree::map(vec![
        ("yamaa", Tree::text("1.0")),
        ("type", Tree::text("spec")),
        ("model", Tree::text("SDTM")),
        ("domain", Tree::text("D0")),
        ("keys", Tree::list(vec![Tree::text("VALUE")])),
        (
            "submission",
            Tree::map(vec![
                ("label", Tree::text("Independent dataset")),
                ("class", Tree::text("SPECIAL PURPOSE")),
                ("structure", Tree::text("One record per subject")),
                ("repeating", Tree::boolean(false)),
            ]),
        ),
        ("columns", Tree::list(vec![column("VALUE")])),
        (
            "output",
            Tree::map(vec![
                ("path", Tree::text("d0.csv")),
                ("columns", Tree::list(vec![Tree::text("VALUE")])),
            ]),
        ),
    ])
}

/// A 1.0 project (D4) whose `sdtm` section lists `d0.yaml` under its
/// `standards` (D16).
pub fn project() -> Tree {
    let guide = Tree::map(vec![
        ("name", Tree::text("SDTMIG")),
        ("version", Tree::text("3.4")),
        ("status", Tree::text("Final")),
    ]);
    Tree::map(vec![
        ("yamaa", Tree::text("1.0")),
        ("type", Tree::text("project")),
        (
            "study",
            Tree::map(vec![
                ("id", Tree::text("S01")),
                ("name", Tree::text("Independent study")),
                ("description", Tree::text("Submission detector witness")),
                ("protocol_name", Tree::text("P01")),
            ]),
        ),
        (
            "sdtm",
            Tree::map(vec![
                ("standards", Tree::list(vec![guide])),
                ("context", Tree::text("Other")),
                ("define", Tree::text("define.xml")),
                ("specs", Tree::list(vec![Tree::text("d0.yaml")])),
            ]),
        ),
    ])
}

/// The terminology of one codelist source, `LIST`, a closed text list whose
/// one item is `value`, drawn from the CDISC/NCI SDTM terminology when
/// `standard`.
pub fn catalogue(standard: bool, value: Value) -> ct::Catalogue {
    ct::Catalogue::admit(vec![rt::Source {
        standard: standard.then(|| rt::Standard {
            name: "CDISC/NCI".into(),
            publishing_set: "SDTM".into(),
            version: "2026-09-30".into(),
        }),
        codelists: vec![rt::Codelist {
            id: "LIST".into(),
            name: "Independent list".into(),
            data_type: rt::CodelistType::Text,
            extensible: false,
            alias: None,
            format_name: None,
            external: None,
            items: Some(vec![rt::Item {
                value,
                decode: None,
                rank: None,
                alias: None,
                extended: false,
            }]),
        }],
    }
    .root()])
    .unwrap()
}

/// Compose the first submission section of `project` over `specs`, the
/// specifications it lists, in its order: each is read from the file the
/// section lists it under and publishes its `output.path`, which the document
/// links as written. `others` are the specifications the study's other
/// sections list.
pub fn compose(
    project: Tree,
    specs: Vec<Tree>,
    others: Vec<Tree>,
    catalogue: &ct::Catalogue,
    installed: &str,
    created: &str,
) -> Result<Study, Vec<Finding>> {
    let project = project.document();
    let lowered = yamaa_core::front_end::v1::project::project(&project, None).unwrap();
    let declared = lowered.submission.unwrap();
    let section = &declared.sections[0];
    let documents: Vec<_> = specs.into_iter().map(Tree::document).collect();
    let trees: Vec<_> = documents
        .iter()
        .map(|document| spec::lower(document, None).into_spec())
        .collect();
    assert_eq!(
        documents.len(),
        section.specs.len(),
        "each specification is one the section lists"
    );
    let inputs: Vec<_> = documents
        .iter()
        .zip(&trees)
        .zip(&section.specs)
        .map(|((document, tree), listed)| Input {
            document,
            tree,
            source: &listed.text,
            artifact: &tree.output.path.text,
            href: Some(&tree.output.path.text),
        })
        .collect();
    let others: Vec<_> = others.into_iter().map(Tree::document).collect();
    let others: Vec<_> = others.iter().collect();
    let required = composition::Required::admit(declared.study.as_ref(), section)?;
    composition::compose(required, catalogue, &inputs, &others, installed, created)
}

/// Compose `project` over its one specification `spec`, installed as 1.0
/// and created at 2026-09-30T00:00:00.
pub fn one(project: Tree, spec: Tree, catalogue: &ct::Catalogue) -> Result<Study, Vec<Finding>> {
    compose(
        project,
        vec![spec],
        Vec::new(),
        catalogue,
        "1.0",
        "2026-09-30T00:00:00",
    )
}
