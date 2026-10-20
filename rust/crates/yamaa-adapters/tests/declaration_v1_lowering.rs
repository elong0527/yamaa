#![cfg(feature = "yaml")]
//! The 1.0 front end for specifications (#3030 P1-5). `front_end_comparison.rs`
//! compares the converted corpus with its 0.5 originals; these cases hold
//! the forms the corpus lacks: text that holds one literal, the refusal of a
//! function argument that is no leaf (FUNC-6), the stored `model`, keys and
//! ODM profile, the position of a node a child layer wrote (P-ISSUE-1), the
//! absence answer of D67, several origins, a dataset's standard, a
//! value-level comparator, and a month rule without a day rule (TIME-10).

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use yamaa_adapters::document::{
    self, Closures, Decoded, PreparedSpecification, SpecificationError,
};
use yamaa_adapters::specification_source::{Error, InheritancePort, Source};
use yamaa_core::ast::{
    Comparator, ExprId, ExprKind, JoinKey, KeyDecl, Literal, ModelKind, Profile, Spec, Surface,
};
use yamaa_core::diagnostic::ConditionCode;
use yamaa_core::expression::Context;
use yamaa_core::inheritance::{Source as GraphSource, SourceError};
use yamaa_core::language::{FileType, LANGUAGE_VERSION};
use yamaa_core::schema::SchemaContext as C;

static NEXT: AtomicUsize = AtomicUsize::new(0);

/// A study directory of written files, removed when dropped.
struct Study(PathBuf);
impl Study {
    fn new(files: &[(&str, &str)]) -> Self {
        let path = std::env::temp_dir().join(format!(
            "yamaa-v1-lowering-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        let study = Self(path.canonicalize().unwrap());
        for (name, text) in files {
            fs::write(study.0.join(name), text).unwrap();
        }
        study
    }
    fn path(&self, name: &str) -> String {
        self.0.join(name).to_str().unwrap().replace('\\', "/")
    }
}
impl Drop for Study {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Parents resolve beside their declaring file; paths stay as written.
struct Files;
impl InheritancePort for Files {
    type Error = std::io::Error;
    fn canonicalize(
        &mut self,
        declaring: &str,
        written: &str,
    ) -> Result<GraphSource, SourceError<Self::Error>> {
        let candidate = Path::new(declaring).parent().unwrap().join(written);
        let identity = candidate
            .canonicalize()
            .map_err(|_| SourceError::Unavailable)?
            .to_str()
            .unwrap()
            .replace('\\', "/");
        Ok(GraphSource {
            display_path: identity.clone(),
            identity,
        })
    }
    fn capture(&mut self, source: &GraphSource) -> Result<Vec<u8>, SourceError<Self::Error>> {
        fs::read(&source.identity).map_err(SourceError::Raised)
    }
    fn rebase(
        &mut self,
        _: &GraphSource,
        _: &GraphSource,
        written: &str,
    ) -> Result<String, Self::Error> {
        Ok(written.into())
    }
}

/// The 1.0 specification `entry` of `study`, prepared by an engine that
/// reads 1.0.
fn prepare(
    study: &Study,
    entry: &str,
) -> Result<PreparedSpecification, SpecificationError<std::io::Error>> {
    let identity = study.path(entry);
    let source = Source {
        identity: identity.clone(),
        bytes: fs::read(&identity).unwrap(),
    };
    let closures = Closures::shipped().unwrap();
    match document::decode(source, FileType::Spec, Some(LANGUAGE_VERSION), &closures) {
        Ok(Decoded::Versioned(document)) => document.prepare_specification(identity, &mut Files),
        other => panic!("{entry}: {other:?}"),
    }
}

/// A one-layer specification whose columns follow `ID`, read from `SRC.ID`,
/// with every column in `output.columns`.
fn spec(columns: &[&str]) -> String {
    let names = columns
        .iter()
        .map(|column| {
            column
                .split_once(',')
                .unwrap()
                .0
                .trim_start_matches("{name: ")
        })
        .collect::<Vec<_>>();
    format!(
        "yamaa: \"1.0\"\ntype: spec\ndomain: T\nkeys: [ID]\n\
         input: {{SRC: src.csv, EX: ex.csv}}\nbase: SRC\n\
         output: {{path: out.csv, columns: [ID, {}]}}\n\
         columns:\n  - {{name: ID, type: int, derivation: SRC.ID}}\n{}",
        names.join(", "),
        columns
            .iter()
            .map(|column| format!("  - {column}\n"))
            .collect::<String>()
    )
}

/// The lowered tree of a one-layer specification.
fn lowered(text: &str) -> Spec {
    let study = Study::new(&[("spec.yaml", text)]);
    prepare(&study, "spec.yaml")
        .unwrap_or_else(|error| panic!("admitted: {error:?}"))
        .into_lowered()
        .into_spec()
}

/// The derivation of the column `name`: its expression's id.
fn derived(spec: &Spec, name: &str) -> ExprId {
    let column = spec.columns.iter().find(|c| c.name.name == name).unwrap();
    column.derivation.as_ref().unwrap().expr
}

#[test]
fn text_holding_one_literal_lowers_to_it_and_other_text_to_a_formula() {
    let spec = lowered(&spec(&[
        r#"{name: NULLTEXT, type: str, derivation: "NULL"}"#,
        r#"{name: COUNT, type: int, derivation: "7"}"#,
        r#"{name: ONSET, type: date, derivation: "DATE '2024-01-01'"}"#,
        "{name: YES, type: str, derivation: true}",
        r#"{name: DOUBLE, type: int, derivation: "SRC.AGE * 2"}"#,
        "{name: FORGOT, type: str, derivation: DM}",
    ]));
    let kind = |name: &str| &spec.expr(derived(&spec, name)).kind;
    let path = |name: &str| spec.cite(spec.expr(derived(&spec, name)).meta);
    for (name, literal) in [
        ("NULLTEXT", Literal::Missing),
        ("COUNT", Literal::Int("7".into())),
        ("ONSET", Literal::Date("2024-01-01".into())),
        ("YES", Literal::Bool(true)),
    ] {
        assert_eq!(kind(name), &ExprKind::Literal(literal), "{name}");
        assert_eq!(path(name), format!("columns.{name}.derivation"));
    }
    // Other text is one formula in the derivation context, at the field
    // (FORMULA-27); a forgotten quote is a name, which fails where it binds
    // (EXPR-5).
    let ExprKind::Formula(formula) = kind("DOUBLE") else {
        panic!("a formula");
    };
    assert_eq!(formula.context, Context::Derivation);
    assert!(formula.compiled.is_ok());
    assert_eq!(spec.cite(formula.text.meta), "columns.DOUBLE.derivation");
    assert!(matches!(kind("FORGOT"), ExprKind::Ref(name) if name.as_str() == "DM"));
    assert_eq!(path("FORGOT"), "columns.FORGOT.derivation.source");
}

#[test]
fn a_function_argument_is_one_leaf_and_any_other_text_fails() {
    let call = |weight: &str| {
        spec(&[&format!(
            "{{name: BMI, type: float, derivation: {{function: {{name: bmi, args: {{weight: {weight}, unit: \"'kg'\", none: null}}}}}}}}"
        )])
    };
    let spec = lowered(&call("SRC.WEIGHT"));
    let ExprKind::Call { args, .. } = &spec.expr(derived(&spec, "BMI")).kind else {
        panic!("a call");
    };
    let args = args
        .iter()
        .map(|(name, value)| (name.name.as_str(), spec.expr(*value).kind.clone()))
        .collect::<Vec<_>>();
    assert_eq!(
        args,
        [
            (
                "weight",
                ExprKind::Ref(yamaa_core::ast::Name("SRC.WEIGHT".into()))
            ),
            ("unit", ExprKind::Literal(Literal::Text("kg".into()))),
            ("none", ExprKind::Literal(Literal::Missing)),
        ]
    );
    // An operator, a call or a truth value written as text is no leaf.
    for weight in ["\"SRC.WEIGHT * 2\"", "\"ABS(SRC.WEIGHT)\"", "\"TRUE\""] {
        let study = Study::new(&[("spec.yaml", &call(weight))]);
        let Err(SpecificationError::Entry(Error::Findings(captured))) =
            prepare(&study, "spec.yaml")
        else {
            panic!("{weight}: refused");
        };
        let [finding] = captured.findings() else {
            panic!("{weight}: one finding");
        };
        let definition = finding.code.definition();
        assert_eq!(finding.code, ConditionCode::FunctionArgumentLeaf);
        assert_eq!(
            (definition.phase, definition.condition, definition.rule),
            ("validation", "invalid_function_argument", Some("FUNC-6"))
        );
        assert_eq!(finding.path, "columns.BMI.derivation.function.args.weight");
        assert_eq!(
            finding.context,
            [
                ("function", C::Text("bmi".into())),
                ("argument", C::Text("weight".into())),
                ("cause", C::Text("expression".into())),
            ]
        );
    }
}

#[test]
fn model_keys_and_profiles_are_stored_for_the_pull_requests_that_check_them() {
    let spec = lowered(
        &spec(&[
            r#"{name: DOSE, type: float, derivation: {aggregate: "SUM(EX.DOSE)"}}"#,
            "{name: TERM, type: str, derivation: NAMES.TERM}",
        ])
        .replace("type: spec\n", "type: spec\nmodel: ADaM\n")
        .replace(
            "input: {SRC: src.csv, EX: ex.csv}",
            "input: {SRC: src.csv, EX: {path: ex.csv, key: [ID]}, ODM: odm.xml}\n\
             intermediates:\n  - {id: NAMES, dataset: ODM, key: [ID]}",
        ),
    );
    assert_eq!(spec.header.surface, Surface::V1 { minor: 0 });
    assert_eq!(
        spec.model.as_ref().map(|model| model.kind),
        Some(ModelKind::Adam)
    );
    let input = |name: &str| spec.inputs.iter().find(|i| i.name.name == name).unwrap();
    assert_eq!(input("SRC").key, KeyDecl::Undeclared);
    let KeyDecl::Declared(columns) = &input("EX").key else {
        panic!("a declared key");
    };
    assert_eq!(spec.cite(columns[0].meta), "input.EX.key[0]");
    assert_eq!(input("ODM").source.profile, Some(Profile::OdmXml));
    // A keyless qualified aggregate matches on its input's key, and an
    // intermediate states its own (D23, D63).
    let ExprKind::Aggregate(aggregate) = &spec.expr(derived(&spec, "DOSE")).kind else {
        panic!("an aggregate");
    };
    assert_eq!(aggregate.key, Some(JoinKey::FromInput));
    let Some(JoinKey::Columns { columns, .. }) = &spec.intermediates[0].key else {
        panic!("the intermediate's own key");
    };
    assert_eq!(spec.cite(columns[0].meta), "intermediates.NAMES.key[0]");
}

/// An intermediate states its own `key` unless it drives a row template
/// (LOOKUP-3, ROW-2): one without `key` is refused before any input is read,
/// at its `key`, as the schema records a missing required field (D23, D63).
#[test]
fn an_intermediate_without_key_is_refused_unless_it_drives_rows() {
    let text = spec(&["{name: TERM, type: str, derivation: NAMES.TERM}"]).replace(
        "base: SRC\n",
        "base: SRC\nintermediates:\n  - {id: NAMES, dataset: EX, filter: \"EX.N > 0\"}\n",
    );
    let study = Study::new(&[("spec.yaml", &text)]);
    let Err(SpecificationError::Entry(Error::Findings(captured))) = prepare(&study, "spec.yaml")
    else {
        panic!("refused");
    };
    let [finding] = captured.findings() else {
        panic!("one finding");
    };
    let definition = finding.code.definition();
    assert_eq!(
        (definition.phase, definition.condition, definition.rule),
        ("validation", "missing_required_field", Some("LOOKUP-3"))
    );
    assert_eq!(finding.path, "intermediates.NAMES.key");
    assert_eq!(
        finding.context,
        [
            ("field", C::Text("key".into())),
            ("class", C::Text("intermediate_class".into())),
        ]
    );
    // A row template's driver declares none.
    let driver = text
        .replace(
            "  - {name: TERM, type: str, derivation: NAMES.TERM}\n",
            "  - {name: TERM, type: str}\n",
        )
        .replace("base: SRC\n", "")
        + "rows:\n  - {id: named, dataset: NAMES, derivations: {ID: NAMES.ID, TERM: NAMES.TERM}}\n";
    let study = Study::new(&[("spec.yaml", &driver)]);
    let spec = prepare(&study, "spec.yaml")
        .unwrap_or_else(|error| panic!("admitted: {error:?}"))
        .into_lowered()
        .into_spec();
    assert_eq!(spec.intermediates[0].key, None);
}

#[test]
fn each_node_is_located_in_the_layer_whose_value_took_effect() {
    let study = Study::new(&[
        (
            "base.yaml",
            "yamaa: \"1.0\"\ntype: spec\ninput: {SRC: src.csv}\ncolumns:\n  - {name: ID, type: int, derivation: SRC.ID}\n  - {name: CODE, type: str, derivation: SRC.CODE}\n",
        ),
        (
            "spec.yaml",
            "yamaa: \"1.0\"\ntype: spec\nparent: base.yaml\ndomain: T\nkeys: [ID]\noutput: {path: out.csv, columns: [ID, CODE]}\ncolumns:\n  - {name: CODE, derivation: \"'X'\"}\n",
        ),
    ]);
    let prepared = prepare(&study, "spec.yaml").unwrap();
    let spec = prepared.lowered().spec();
    let files = spec
        .files
        .iter()
        .map(|f| f.identity.as_str())
        .collect::<Vec<_>>();
    assert_eq!(files, [study.path("base.yaml"), study.path("spec.yaml")]);
    // ID's derivation is the parent's (file 0, line 5). CODE's, which the
    // child replaced, is the child's text (file 1, line 8), although the
    // composed wrapper around it is the parent's.
    let span = |name: &str| spec.expr(derived(spec, name)).meta.span;
    let (id, code) = (span("ID"), span("CODE"));
    assert_eq!((id.file.0, id.line), (0, 5));
    assert_eq!((code.file.0, code.line), (1, 8));
}

#[test]
fn absent_answers_an_unknown_variable_only_on_the_0_5_surface() {
    // D67: in 1.0 `absent` answers only a lookup that finds no record.
    assert!(Surface::V0_5.absent_answers_unknown_variable());
    assert!(!Surface::V1 { minor: 0 }.absent_answers_unknown_variable());
    let spec = lowered(&spec(&[
        "{name: V, type: int, derivation: {source: {variable: EX.UNKNOWN, absent: 7}}}",
    ]));
    let ExprKind::Source(source) = &spec.expr(derived(&spec, "V")).kind else {
        panic!("a structured source");
    };
    assert_eq!(
        source.absent.as_ref().unwrap().value,
        Literal::Int("7".into())
    );
    assert!(!spec.header.surface.absent_answers_unknown_variable());
}

#[test]
fn a_declared_baseline_order_starts_with_the_date_ascending() {
    let baseline = |order: &str| {
        spec(&[
            "{name: ADT, type: date, derivation: SRC.ADT}",
            "{name: TRTSDT, type: date, derivation: SRC.TRTSDT}",
            "{name: AVAL, type: float, derivation: SRC.AVAL}",
            "{name: ATM, type: str, derivation: SRC.ATM}",
            &format!(
                "{{name: ABLFL, type: str, derivation: {{baseline_flag: {{date: ADT, reference: TRTSDT, source: AVAL, window: {{order_by: {order}}}}}}}}}"
            ),
        ])
    };
    // The date, then the terms that break ties (WINDOW-5, WINDOW-9).
    let spec = lowered(&baseline("[ADT, ATM]"));
    let ExprKind::Window(window) = &spec.expr(derived(&spec, "ABLFL")).kind else {
        panic!("a window");
    };
    let order = window
        .order_by
        .iter()
        .map(|term| spec.cite(term.meta))
        .collect::<Vec<_>>();
    assert_eq!(
        order,
        [
            "columns.ABLFL.derivation.baseline_flag.date",
            "columns.ABLFL.derivation.baseline_flag.window.order_by[1]",
        ]
    );
    // Any other first term fails, at that term.
    for (order, variable, direction) in [
        ("[ATM, ADT]", "ATM", "asc"),
        ("[{variable: ADT, direction: desc}]", "ADT", "desc"),
    ] {
        let study = Study::new(&[("spec.yaml", &baseline(order))]);
        let Err(SpecificationError::Entry(Error::Findings(captured))) =
            prepare(&study, "spec.yaml")
        else {
            panic!("{order}: refused");
        };
        let [finding] = captured.findings() else {
            panic!("{order}: one finding");
        };
        let definition = finding.code.definition();
        assert_eq!(
            (definition.phase, definition.condition, definition.rule),
            ("validation", "value_not_permitted", Some("WINDOW-5"))
        );
        assert_eq!(
            finding.path,
            "columns.ABLFL.derivation.baseline_flag.window.order_by[0]"
        );
        assert_eq!(
            finding.context,
            [
                ("variable", C::Text(variable.into())),
                ("direction", C::Text(direction.into())),
                ("date", C::Text("ADT".into())),
            ]
        );
    }
}

#[test]
fn a_check_that_declares_no_field_is_written_as_its_bare_keyword() {
    // `- not_missing` lowers as `- not_missing: {}` did, at the keyword's
    // path, and the long form restates it (SCHEMA-15, P-HND-4).
    let tree = lowered(&spec(&[
        "{name: V, type: int, derivation: SRC.V, verifications: [not_missing]}",
    ]));
    let column = tree.columns.iter().find(|c| c.name.name == "V").unwrap();
    assert_eq!(
        column.checks[0].kind,
        yamaa_core::ast::ColumnCheckKind::NotMissing
    );
    assert_eq!(
        tree.cite(column.checks[0].meta),
        "columns.V.verifications[0].not_missing"
    );
    let study = Study::new(&[(
        "spec.yaml",
        &spec(&["{name: V, type: int, derivation: SRC.V, verifications: [{not_missing: {}}]}"]),
    )]);
    let Err(SpecificationError::Traversal(yamaa_core::inheritance::Error::Layer {
        error: yamaa_core::schema::NormalizationError::Invalid(found),
        ..
    })) = prepare(&study, "spec.yaml")
    else {
        panic!("the long form is refused");
    };
    let [finding] = found.as_slice() else {
        panic!("one finding: {found:?}");
    };
    assert_eq!(finding.code, ConditionCode::SchemaRedundantLongForm);
    assert_eq!(finding.path, "columns.V.verifications[0]");
    assert_eq!(
        finding.context,
        [
            ("field", C::Text("not_missing".into())),
            ("shorthand", C::Text("not_missing".into()))
        ]
    );
}

#[test]
fn origins_a_standard_and_a_value_level_comparator_lower_for_the_checks_that_follow() {
    let mut text = spec(&[
        "{name: CODE, type: str, derivation: SRC.CODE}",
        "{name: AVAL, type: float, derivation: SRC.AVAL, submission: {origins: \
         [{type: Collected, source: Investigator}, {type: Derived}], value_level: \
         [{id: OTHER, where: {CODE: {comparator: NE, values: [X]}}}, \
         {id: LISTED, where: {CODE: [X, Y]}}]}}",
    ]);
    text.push_str(
        "submission: {label: Test, class: FINDINGS, structure: One record per test, \
         repeating: true, standard: SDTMIG}\n",
    );
    let spec = lowered(&text);
    let standard = spec.submission.as_ref().unwrap().standard.as_ref().unwrap();
    assert_eq!(
        (standard.name.as_str(), spec.cite(standard.meta).as_str()),
        ("SDTMIG", "submission.standard")
    );
    let column = spec.columns.iter().find(|c| c.name.name == "AVAL").unwrap();
    let submission = column.submission.as_ref().unwrap();
    // Each declared origin lowers, at its position in `origins` (P-ORIGIN-1).
    let origins = submission
        .metadata
        .origins
        .iter()
        .map(|origin| (origin.kind.text.as_str(), spec.cite(origin.meta)))
        .collect::<Vec<_>>();
    assert_eq!(
        origins,
        [
            (
                "Collected",
                "columns.AVAL.submission.origins[0]".to_string()
            ),
            ("Derived", "columns.AVAL.submission.origins[1]".to_string()),
        ]
    );
    // A comparator class selects no listed value; a bare list selects one
    // of them (P-VLM-1).
    let clauses = submission
        .value_level
        .iter()
        .flat_map(|level| &level.conditions)
        .map(|clause| {
            (
                clause.comparator,
                clause
                    .values
                    .iter()
                    .map(|value| (value.text.as_str(), spec.cite(value.meta)))
                    .collect::<Vec<_>>(),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        clauses,
        [
            (
                Comparator::NotIn,
                vec![(
                    "X",
                    "columns.AVAL.submission.value_level.OTHER.where.CODE.values[0]".to_string()
                )]
            ),
            (
                Comparator::In,
                vec![
                    (
                        "X",
                        "columns.AVAL.submission.value_level.LISTED.where.CODE[0]".to_string()
                    ),
                    (
                        "Y",
                        "columns.AVAL.submission.value_level.LISTED.where.CODE[1]".to_string()
                    ),
                ]
            ),
        ]
    );
}

#[test]
fn a_month_rule_without_a_day_rule_fails_at_the_impute() {
    let study = Study::new(&[(
        "spec.yaml",
        &spec(&[
            "{name: ADT, type: date, derivation: {impute: {type: date, source: SRC.DTC, month: 6}}}",
        ]),
    )]);
    let Err(SpecificationError::Entry(Error::Findings(captured))) = prepare(&study, "spec.yaml")
    else {
        panic!("refused");
    };
    let [finding] = captured.findings() else {
        panic!("one finding");
    };
    let definition = finding.code.definition();
    assert_eq!(
        (definition.phase, definition.condition, definition.rule),
        ("validation", "missing_required_field", Some("TIME-10"))
    );
    assert_eq!(finding.path, "columns.ADT.derivation.impute");
    assert_eq!(finding.context, [("field", C::Text("day".into()))]);
    // With a day rule, or with neither rule, the impute is admitted.
    for rules in ["month: 6, day: 15", "day: first", ""] {
        lowered(&spec(&[&format!(
            "{{name: ADT, type: date, derivation: {{impute: {{type: date, source: SRC.DTC, {rules}}}}}}}"
        )]));
    }
}
