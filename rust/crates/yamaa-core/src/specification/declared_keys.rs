//! Declared join keys on a 1.0 tree (D23, D63): an implicit join and a
//! qualified aggregate without `key` match on the input's declared `key`, a
//! named intermediate states its own unless it drives a row template, and a
//! read of an input without `key` fails at the read. The same declarations
//! lowered by the 0.5 front end infer the applicable output keys.
use super::*;
use crate::diagnostic::ConditionCode as Code;
use crate::test_document::Tree;

fn text(value: &str) -> Tree {
    Tree::text(value)
}

/// A column deriving `derivation`, an expression field's text.
fn column(name: &str, kind: &str, derivation: &str) -> Tree {
    Tree::map(vec![
        ("name", text(name)),
        ("type", text(kind)),
        ("derivation", Tree::map(vec![("value", text(derivation))])),
    ])
}

/// A 1.0 specification over `SRC`, its base, and `DM`, whose `key` is
/// `key` when given, with the output keys `[STUDYID, USUBJID]`.
fn spec(dm_key: Option<&[&str]>, columns: Vec<Tree>, extra: Vec<(&str, Tree)>) -> Tree {
    let mut dm = vec![("path", text("dm.csv"))];
    if let Some(key) = dm_key {
        dm.push((
            "key",
            Tree::list(key.iter().map(|name| text(name)).collect()),
        ));
    }
    let mut all = vec![
        column("STUDYID", "str", "SRC.STUDYID"),
        column("USUBJID", "str", "SRC.USUBJID"),
    ];
    all.extend(columns);
    let mut tree = Tree::map(vec![
        ("yamaa", text("1.0")),
        ("type", text("spec")),
        ("domain", text("T")),
        ("keys", Tree::list(vec![text("STUDYID"), text("USUBJID")])),
        (
            "input",
            Tree::map(vec![
                ("SRC", Tree::map(vec![("path", text("src.csv"))])),
                ("DM", Tree::map(dm)),
            ]),
        ),
        ("base", text("SRC")),
        ("columns", Tree::list(all)),
        (
            "output",
            Tree::map(vec![
                ("path", text("out.csv")),
                (
                    "columns",
                    Tree::list(vec![text("STUDYID"), text("USUBJID")]),
                ),
            ]),
        ),
    ]);
    for (field, value) in extra {
        tree.set(&[field], value);
    }
    tree
}

/// The tree `surface`'s front end lowers the document to.
fn lowered(tree: Tree, surface: ast::Surface) -> ast::Spec {
    crate::front_end::lowering::lower_surface(&tree.document(), None, surface).into_spec()
}

const V1: ast::Surface = ast::Surface::V1 { minor: 0 };

fn prepare(tree: Tree) -> Result<PreparedSpecification, PrepareError> {
    PreparedSpecification::from_tree(lowered(tree, V1), Vec::new(), None, None, None, None)
}

fn schema(fields: &[(&str, ColumnType)]) -> TableSchema {
    TableSchema::new(
        fields
            .iter()
            .map(|&(name, kind)| Column {
                name: name.into(),
                kind,
            })
            .collect(),
    )
    .unwrap()
}

/// `SRC` and `DM`'s schemas: both store `STUDYID` and `USUBJID`.
fn schemas() -> [TableSchema; 2] {
    [
        schema(&[("STUDYID", ColumnType::Str), ("USUBJID", ColumnType::Str)]),
        schema(&[
            ("STUDYID", ColumnType::Str),
            ("USUBJID", ColumnType::Str),
            ("AGE", ColumnType::Int),
            ("DOSE", ColumnType::Float),
        ]),
    ]
}

fn bind(prepared: &PreparedSpecification) -> Result<ResolvedPlan, BindError> {
    let schemas = schemas();
    prepared.bind_sources(&[&schemas[0], &schemas[1]])
}

/// The diagnostics of a refused binding.
fn refused(prepared: &PreparedSpecification) -> Vec<crate::diagnostic::Diagnostic> {
    let Err(BindError::Invalid(findings)) = bind(prepared) else {
        panic!("binding is refused");
    };
    findings
        .iter()
        .flat_map(|finding| finding.diagnostics(prepared.source()).unwrap())
        .collect()
}

/// The diagnostics of a refused preparation.
fn unprepared(tree: Tree) -> Vec<crate::diagnostic::Diagnostic> {
    let Err(PrepareError::Invalid(findings)) = prepare(tree) else {
        panic!("preparation is refused");
    };
    findings.iter().map(PreflightFinding::diagnostic).collect()
}

/// The number of donor fields each join of the plan matches on.
fn join_keys(plan: &ResolvedPlan) -> Vec<usize> {
    plan.relations()
        .iter()
        .filter(|relation| {
            matches!(
                relation.kind,
                crate::dataset::resolved::RelationKind::Join
                    | crate::dataset::resolved::RelationKind::MatchedAggregate
            )
        })
        .map(|relation| relation.keys.len())
        .collect()
}

#[test]
fn an_implicit_join_matches_on_the_input_s_declared_key() {
    let tree = |key: Option<&[&str]>| spec(key, vec![column("AGE", "int", "DM.AGE")], vec![]);
    // DM stores both output keys. 1.0 matches on the one it declares; the
    // same declarations lowered by the 0.5 front end infer both.
    let plan = bind(&prepare(tree(Some(&["USUBJID"]))).unwrap()).unwrap();
    assert_eq!(join_keys(&plan), [1]);
    let legacy = lowered(tree(None), ast::Surface::V0_5);
    let legacy = PreparedSpecification::from_tree(legacy, Vec::new(), None, None, None, None);
    assert_eq!(join_keys(&bind(&legacy.unwrap()).unwrap()), [2]);
    let plan = bind(&prepare(tree(Some(&["STUDYID", "USUBJID"]))).unwrap()).unwrap();
    assert_eq!(join_keys(&plan), [2]);
}

/// A context of text values, for comparison.
fn context(pairs: &[(&str, &str)]) -> crate::diagnostic::Context {
    pairs
        .iter()
        .map(|&(key, value)| (key.into(), crate::json::Value::String(value.into())))
        .collect()
}

/// A read of an input that declares no `key` fails at the read, naming the
/// input, and its message names only what every cause of the condition
/// records (LOOKUP-1, LOOKUP-5, D95). The declarations alone decide it, so a
/// check reports it before any input is read (D111).
#[test]
fn a_read_of_an_input_without_key_has_no_applicable_keys() {
    let prepared = prepare(spec(None, vec![column("AGE", "int", "DM.AGE")], vec![])).unwrap();
    assert_eq!(prepared.check_declaration_diagnostics(), refused(&prepared));
    let [diagnostic] = refused(&prepared).try_into().unwrap();
    assert_eq!(diagnostic.code, Code::LookupNoApplicableKeys);
    assert_eq!(diagnostic.spec_paths, ["columns.AGE.derivation.source"]);
    assert_eq!(
        diagnostic.context,
        context(&[("dataset", "DM"), ("hint", "declare `key` on the input")])
    );
    assert_eq!(
        diagnostic.checked_message().unwrap(),
        "the lookup of DM has no applicable key (declare `key` on the input)"
    );
}

/// Every declared column is one the input stores and a column of the
/// current row; the first that is not fails at the read (LOOKUP-5). A check
/// decides from the declarations that no column is the key's (D111 item 2).
/// Whether an input no producer links stores the column, only its captured
/// schema decides, so a check names the column alone there.
#[test]
fn every_declared_key_column_is_stored_and_has_a_current_row_value() {
    for (key, code, identifier) in [
        ("SUBJID", Code::LookupDonorKeyReference, "DM.SUBJID"),
        ("DOSE", Code::LookupMatchValueReference, "DOSE"),
    ] {
        let tree = spec(Some(&[key]), vec![column("AGE", "int", "DM.AGE")], vec![]);
        let prepared = prepare(tree).unwrap();
        let [diagnostic] = refused(&prepared).try_into().unwrap();
        assert_eq!(diagnostic.code, code, "{key}");
        assert_eq!(diagnostic.spec_paths, ["columns.AGE.derivation.source"]);
        assert_eq!(diagnostic.context, context(&[("identifier", identifier)]));
        let [checked] = prepared.check_declaration_diagnostics().try_into().unwrap();
        assert_eq!(checked.code, Code::LookupMatchValueReference, "{key}");
        assert_eq!(checked.spec_paths, diagnostic.spec_paths);
        assert_eq!(checked.context, context(&[("identifier", key)]));
    }
}

/// A qualified aggregate without `key` matches on its input's declared key,
/// and fails at the aggregate when the input declares none (LOOKUP-5, D63).
#[test]
fn a_keyless_qualified_aggregate_matches_on_the_input_s_key() {
    let total = || {
        Tree::map(vec![
            ("name", text("TOTAL")),
            ("type", text("float")),
            (
                "derivation",
                Tree::map(vec![(
                    "value",
                    Tree::map(vec![("aggregate", text("SUM(DM.DOSE)"))]),
                )]),
            ),
        ])
    };
    let plan = bind(&prepare(spec(Some(&["USUBJID"]), vec![total()], vec![])).unwrap()).unwrap();
    assert_eq!(join_keys(&plan), [1]);
    let prepared = prepare(spec(None, vec![total()], vec![])).unwrap();
    let [diagnostic] = refused(&prepared).try_into().unwrap();
    // A check decides it from the declarations alone (D111).
    assert!(prepared
        .check_declaration_diagnostics()
        .contains(&diagnostic));
    assert_eq!(diagnostic.code, Code::LookupNoApplicableKeys);
    assert_eq!(
        diagnostic.spec_paths,
        ["columns.TOTAL.derivation.aggregate"]
    );
    assert_eq!(
        diagnostic.context,
        context(&[
            ("dataset", "DM"),
            ("hint", "declare `key` on the input or on the aggregate"),
        ])
    );
}

/// An intermediate.
fn intermediate(fields: Vec<(&str, Tree)>) -> Tree {
    Tree::map(fields)
}

/// A named intermediate states its own key unless it drives a row template
/// (LOOKUP-3, ROW-2): one without `key` is refused before any input is
/// read, at its `key`, with the schema's record of a missing field.
#[test]
fn an_intermediate_read_as_a_lookup_requires_its_key() {
    let tree = spec(
        Some(&["USUBJID"]),
        vec![column("AGE", "int", "NAMES.AGE")],
        vec![(
            "intermediates",
            Tree::list(vec![intermediate(vec![
                ("id", text("NAMES")),
                ("dataset", text("DM")),
                ("filter", text("DM.AGE > 0")),
            ])]),
        )],
    );
    let [diagnostic] = unprepared(tree.clone()).try_into().unwrap();
    assert_eq!(diagnostic.code, Code::IntermediateKeyRequired);
    assert_eq!(diagnostic.spec_paths, ["intermediates.NAMES.key"]);
    assert_eq!(
        diagnostic.context,
        context(&[("class", "intermediate_class"), ("field", "key")])
    );
    // The 1.0 front end refuses the same declaration first.
    let checked = crate::front_end::v1::spec::checks::findings(&lowered(tree, V1));
    assert_eq!(
        checked
            .iter()
            .map(|finding| (finding.path.as_str(), finding.code))
            .collect::<Vec<_>>(),
        [("intermediates.NAMES.key", Code::IntermediateKeyRequired)]
    );
}

/// An intermediate whose key lists exactly its input's declared key, and
/// that changes nothing else, only renames the input (LOOKUP-4, D71); one
/// that matches on other columns does not.
#[test]
fn an_intermediate_keyed_as_its_input_only_renames_it() {
    let tree = |key: &[&str]| {
        spec(
            Some(&["USUBJID", "STUDYID"]),
            vec![column("AGE", "int", "CODED.AGE")],
            vec![(
                "intermediates",
                Tree::list(vec![intermediate(vec![
                    ("id", text("CODED")),
                    ("dataset", text("DM")),
                    (
                        "key",
                        Tree::list(key.iter().map(|name| text(name)).collect()),
                    ),
                ])]),
            )],
        )
    };
    let [diagnostic] = unprepared(tree(&["STUDYID", "USUBJID"]))
        .try_into()
        .unwrap();
    assert_eq!(diagnostic.code, Code::IntermediateRenameOnly);
    assert_eq!(diagnostic.spec_paths, ["intermediates.CODED"]);
    assert_eq!(
        diagnostic.context,
        context(&[("dataset", "DM"), ("intermediate", "CODED")])
    );
    let plan = bind(&prepare(tree(&["USUBJID"])).unwrap()).unwrap();
    assert!(plan
        .relations()
        .iter()
        .any(|relation| relation.keys.len() == 1));
}

/// An input's `key` names at least one column, and each once (LOOKUP-5),
/// before any input is read.
#[test]
fn an_input_key_names_each_column_once() {
    let empty = spec(Some(&[]), vec![column("AGE", "int", "DM.AGE")], vec![]);
    let [diagnostic] = unprepared(empty).try_into().unwrap();
    assert_eq!(diagnostic.code, Code::LookupEmptyKey);
    assert_eq!(diagnostic.spec_paths, ["input.DM.key"]);
    assert_eq!(
        diagnostic.context,
        context(&[("dataset", "DM"), ("expected", "at least one match pair")])
    );
    let repeated = spec(
        Some(&["USUBJID", "USUBJID"]),
        vec![column("AGE", "int", "DM.AGE")],
        vec![],
    );
    let [diagnostic] = unprepared(repeated).try_into().unwrap();
    assert_eq!(diagnostic.code, Code::IntermediateRepeatedKey);
    assert_eq!(diagnostic.spec_paths, ["input.DM.key[1]"]);
    assert_eq!(
        diagnostic.context,
        context(&[
            ("dataset", "DM"),
            ("expected", "each column once"),
            ("identifier", "USUBJID"),
        ])
    );
}

/// A row template.
fn template(id: &str, dataset: &str, derivations: &[(&str, &str)]) -> Tree {
    Tree::map(vec![
        ("id", text(id)),
        ("dataset", text(dataset)),
        (
            "derivations",
            Tree::map(
                derivations
                    .iter()
                    .map(|&(name, value)| (name, Tree::map(vec![("value", text(value))])))
                    .collect(),
            ),
        ),
    ])
}

/// During row construction an implicit join matches each declared column on
/// the driver record's same-named field (LOOKUP-2), and every declared
/// column applies.
#[test]
fn a_row_template_s_join_matches_the_driver_on_the_declared_key() {
    let tree = |key: &[&str]| {
        let mut age = column("AGE", "int", "DM.AGE");
        age.remove(&["derivation"]);
        spec(
            Some(key),
            vec![age],
            vec![(
                "rows",
                Tree::list(vec![template("src", "SRC", &[("AGE", "DM.AGE")])]),
            )],
        )
    };
    let plan = bind(&prepare(tree(&["USUBJID"])).unwrap()).unwrap();
    assert_eq!(join_keys(&plan), [1]);
    let [diagnostic] = refused(&prepare(tree(&["SUBJID"])).unwrap())
        .try_into()
        .unwrap();
    assert_eq!(diagnostic.code, Code::LookupDonorKeyReference);
    assert_eq!(diagnostic.spec_paths, ["rows.src.derivations.AGE.source"]);
    assert_eq!(diagnostic.context, context(&[("identifier", "DM.SUBJID")]));
    let [diagnostic] = refused(&prepare(tree(&["DOSE"])).unwrap())
        .try_into()
        .unwrap();
    assert_eq!(diagnostic.code, Code::LookupMatchValueReference);
    assert_eq!(diagnostic.context, context(&[("identifier", "DOSE")]));
}

/// A row template's driver declares no key, so a read of it as a lookup,
/// for the rows another template builds, is refused at its `key`
/// (LOOKUP-3, ROW-2), where 0.5 infers the applicable output keys.
#[test]
fn a_row_driver_read_as_a_lookup_is_refused_at_its_key() {
    let tree = || {
        spec(
            Some(&["USUBJID"]),
            vec![column("AGE", "int", "KEPT.AGE")],
            vec![
                (
                    "intermediates",
                    Tree::list(vec![intermediate(vec![
                        ("id", text("KEPT")),
                        ("dataset", text("DM")),
                        ("filter", text("DM.AGE > 0")),
                    ])]),
                ),
                (
                    "rows",
                    Tree::list(vec![
                        template(
                            "kept",
                            "KEPT",
                            &[("STUDYID", "KEPT.STUDYID"), ("USUBJID", "KEPT.USUBJID")],
                        ),
                        template("src", "SRC", &[]),
                    ]),
                ),
            ],
        )
    };
    let legacy = PreparedSpecification::from_tree(
        lowered(tree(), ast::Surface::V0_5),
        Vec::new(),
        None,
        None,
        None,
        None,
    )
    .unwrap();
    bind(&legacy).unwrap();
    let diagnostics = refused(&prepare(tree()).unwrap());
    let [diagnostic] = diagnostics.as_slice() else {
        panic!("one refusal: {diagnostics:?}");
    };
    assert_eq!(diagnostic.code, Code::IntermediateKeyRequired);
    assert_eq!(diagnostic.spec_paths, ["intermediates.KEPT.key"]);
}

/// DM, keyed on `SUBJ`, a column no output, key or other column reads.
fn keyed_on_an_internal_column(columns: Vec<Tree>) -> Tree {
    let mut tree = spec(Some(&["SUBJ"]), columns, vec![]);
    tree.set(
        &["output", "columns"],
        Tree::list(vec![text("STUDYID"), text("USUBJID"), text("AGE")]),
    );
    tree
}

/// Pruning counts a declared key's columns as read by the implicit joins
/// that match on them (LAYER-11), where nothing else reads the column.
#[test]
fn pruning_keeps_the_columns_of_a_declared_key() {
    let columns = |declared: Vec<Tree>| {
        let mut all = declared;
        all.push(column("AGE", "int", "DM.AGE"));
        keyed_on_an_internal_column(all)
    };
    let live = |tree: Tree| {
        let document = tree.document();
        let spec = crate::front_end::v1::spec::lower(&document, None).into_spec();
        let pruned =
            crate::resolution::prune_lowered(&document, &spec, crate::resolution::Order::Declared)
                .unwrap();
        crate::front_end::v1::spec::lower(&pruned.document, None)
            .into_spec()
            .columns
            .iter()
            .map(|column| column.name.name.clone())
            .collect::<Vec<_>>()
    };
    assert_eq!(
        live(columns(vec![column("SUBJ", "str", "SRC.SUBJ")])),
        ["STUDYID", "USUBJID", "SUBJ", "AGE"]
    );
    // Without the join, nothing reads SUBJ.
    let mut unread = columns(vec![column("SUBJ", "str", "SRC.SUBJ")]);
    unread.set(
        &["columns", "3", "derivation", "value"],
        text("SRC.STUDYID"),
    );
    assert_eq!(live(unread), ["STUDYID", "USUBJID", "AGE"]);
}

/// The column order a declaration check reports counts a declared key's
/// columns as read by the implicit join, as binding does (COLUMN-5): a
/// join declared before the column it matches on reads it forward.
#[test]
fn a_declared_key_s_columns_order_the_join_s_reader() {
    let tree = keyed_on_an_internal_column(vec![
        column("AGE", "int", "DM.AGE"),
        column("SUBJ", "str", "SRC.SUBJ"),
    ]);
    let prepared = prepare(tree).unwrap();
    let forward = prepared
        .check_declaration_diagnostics()
        .into_iter()
        .filter(|diagnostic| diagnostic.code == Code::ColumnForwardReference)
        .collect::<Vec<_>>();
    let [diagnostic] = forward.as_slice() else {
        panic!("one forward reference: {forward:?}");
    };
    assert_eq!(
        diagnostic.context,
        context(&[("column", "AGE"), ("dependency", "SUBJ")])
    );
    // Binding reads the same order once both inputs store SUBJ.
    let stored = |extra: &[(&str, ColumnType)]| {
        let mut fields = vec![("STUDYID", ColumnType::Str), ("USUBJID", ColumnType::Str)];
        fields.extend_from_slice(extra);
        schema(&fields)
    };
    let source = stored(&[("SUBJ", ColumnType::Str)]);
    let dm = stored(&[("SUBJ", ColumnType::Str), ("AGE", ColumnType::Int)]);
    let Err(BindError::Invalid(findings)) = prepared.bind_sources(&[&source, &dm]) else {
        panic!("binding is refused");
    };
    let bound = findings
        .iter()
        .flat_map(|finding| finding.diagnostics(prepared.source()).unwrap())
        .map(|diagnostic| diagnostic.code)
        .collect::<Vec<_>>();
    assert_eq!(bound, [Code::ColumnForwardReference]);
}

/// A layered specification orders its columns by dependency (LAYER-13), and
/// reads a row template's qualified read of its own driver as the row's own
/// record, as binding does, never as an implicit join: a template deriving a
/// key column from its driver does not read that column through the
/// driver's key. A column's default reads another template's driver
/// through its key, so it still orders after the key's columns.
#[test]
fn ordering_reads_a_row_template_s_own_driver_without_a_join() {
    let mut tree = spec(
        Some(&["USUBJID"]),
        vec![],
        vec![(
            "rows",
            Tree::list(vec![
                template("src", "SRC", &[]),
                template(
                    "subjects",
                    "DM",
                    &[("STUDYID", "DM.STUDYID"), ("USUBJID", "DM.USUBJID")],
                ),
            ]),
        )],
    );
    tree.set(
        &["columns"],
        Tree::list(vec![
            column("AGE", "int", "DM.AGE"),
            column("STUDYID", "str", "SRC.STUDYID"),
            column("USUBJID", "str", "SRC.USUBJID"),
        ]),
    );
    tree.set(
        &["output", "columns"],
        Tree::list(vec![text("STUDYID"), text("USUBJID"), text("AGE")]),
    );
    let document = tree.document();
    let spec = crate::front_end::v1::spec::lower(&document, None).into_spec();
    let ordered =
        crate::resolution::prune_lowered(&document, &spec, crate::resolution::Order::Dependencies)
            .unwrap();
    let columns = crate::front_end::v1::spec::lower(&ordered.document, None)
        .into_spec()
        .columns
        .iter()
        .map(|column| column.name.name.clone())
        .collect::<Vec<_>>();
    assert_eq!(columns, ["STUDYID", "USUBJID", "AGE"]);
}
