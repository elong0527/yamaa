#![cfg(feature = "yaml")]
//! The extra check of #3133: lowering is total on every specification the
//! benchmark harness admits, and each lowered declaration renders the spec
//! path today's compiler cites.
//!
//! Each case's entry specifications (its `benchmark.yaml` entries, or its
//! one `spec*.yaml`) are prepared with the shipped schema, standalone or
//! through their parents; a 1.x specification is read in the lowering the
//! 1.0 front end prepared. Every admitted model lowers without an
//! unsupported node, its sites partition the expression arena, every
//! expression carries the position its layer's decoder recorded, and every
//! spec path the case's expected first issue names is a lowered node's
//! path or a field of one. Compiling each entry through the file route
//! runs the compiler's own comparison of every declaration path with the
//! tree's, which debug builds enforce
//! (`yamaa-core/src/specification/lowering_audit.rs`); the Rust tests that
//! prepare a specification run the same comparison.

use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};
use yamaa_adapters::{
    file_preparation::FileSpecification,
    file_resources::Resources,
    shipped_schema,
    specification_source::{InheritancePort, PreparedDocument, Source},
    yaml_decode::{decode_yaml, SourcePosition},
};
use yamaa_core::{
    ast::{
        Closed, ColumnCheckKind, Comparator, DatasetCheckKind, ExprKind, FileId, FileRef, JoinKey,
        KeyDecl, Literal, Name, Notation, Precision, ScalarOp, Slot, Span, Spec, Step,
        TemporalType, TimeUnit, WindowFunction,
    },
    front_end::v0_5::{lower, Positions},
    inheritance::{Source as GraphSource, SourceError},
    schema::{Document, DocumentNode as N, SchemaSource},
};

fn benchmarks() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../benchmarks")
}

fn text(d: &Document, node: usize) -> Option<&str> {
    match &d.nodes()[node] {
        N::Text(text) => Some(text),
        _ => None,
    }
}

fn items(d: &Document, node: usize) -> &[usize] {
    match &d.nodes()[node] {
        N::Sequence(items) => items,
        _ => &[],
    }
}

/// The specifications the harness opens for a case, in either host
/// (`tools/benchmark.py`): the declared entries, or the case's one spec.
fn entries(case: &Path) -> Vec<String> {
    if let Ok(bytes) = fs::read(case.join("benchmark.yaml")) {
        let d = decode_yaml(&bytes).unwrap().document;
        if let Some(hosts) = d.field(d.root(), "entries") {
            let mut found = BTreeSet::new();
            if let N::Mapping(hosts) = &d.nodes()[hosts] {
                for &(_, calls) in hosts {
                    for &call in items(&d, calls) {
                        if let Some(spec) = d.field(call, "specification").and_then(|n| text(&d, n))
                        {
                            found.insert(spec.to_string());
                        }
                    }
                }
            }
            return found.into_iter().collect();
        }
    }
    let mut specs = fs::read_dir(case)
        .unwrap()
        .filter_map(|entry| {
            let name = entry.ok()?.file_name().into_string().ok()?;
            (name.starts_with("spec") && name.ends_with(".yaml")).then_some(name)
        })
        .collect::<Vec<_>>();
    assert_eq!(specs.len(), 1, "{} has one specification", case.display());
    specs.pop().into_iter().collect()
}

/// The spec paths of a negative case's expected first issue.
fn expected_paths(case: &Path) -> Vec<String> {
    let Ok(bytes) = fs::read(case.join("expected/error.yaml")) else {
        return Vec::new();
    };
    let d = decode_yaml(&bytes).unwrap().document;
    d.field(d.root(), "spec_paths")
        .map(|paths| {
            items(&d, paths)
                .iter()
                .filter_map(|&path| text(&d, path).map(String::from))
                .collect()
        })
        .unwrap_or_default()
}

/// Parents resolve beside their declaring file; input paths stay as written.
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
            .map_err(|_| SourceError::Unavailable)?;
        Ok(GraphSource {
            identity: identity.to_str().unwrap().into(),
            display_path: candidate.to_str().unwrap().into(),
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

fn span(file: usize, position: &SourcePosition) -> Span {
    Span {
        file: FileId(file as u32),
        line: position.line as u32,
        column: position.column as u32,
    }
}

/// Each final node's written position: the decoded location of the node
/// its layer wrote; a schema default has none.
fn positions(document: &PreparedDocument) -> Positions {
    let Some(inherited) = document.inheritance() else {
        let locations = &document.raw().locations;
        return Positions {
            files: vec![FileRef {
                identity: document.source().identity.clone(),
            }],
            nodes: document
                .origins()
                .iter()
                .map(|origin| match origin.source {
                    SchemaSource::Input => locations.get(origin.node).map(|at| span(0, at)),
                    SchemaSource::Default { .. } => None,
                })
                .collect(),
        };
    };
    let layers = inherited.layers();
    let locations = |identity: &str| {
        if document.source().identity == identity {
            Some(&document.raw().locations)
        } else {
            document
                .parents()
                .iter()
                .find(|parent| parent.source().identity == identity)
                .map(|parent| &parent.raw().locations)
        }
    };
    Positions {
        files: layers
            .iter()
            .map(|layer| FileRef {
                identity: layer.source.identity.clone(),
            })
            .collect(),
        nodes: inherited
            .original_origins()
            .iter()
            .map(|origin| {
                let layer = origin.layer?;
                if !matches!(origin.origin.source, SchemaSource::Input) {
                    return None;
                }
                let at = locations(&layers[layer].source.identity)?.get(origin.origin.node)?;
                Some(span(layer, at))
            })
            .collect(),
    }
}

/// Every construct of an admitted benchmark lowers to its node, and every
/// expression is reached from exactly one site, with a written position.
fn check_tree(label: &str, spec: &Spec, failures: &mut Vec<String>) {
    let mut reached = vec![0usize; spec.exprs.len()];
    for site in spec.sites() {
        spec.walk(site.root, site.scope, |at, _| {
            reached[at.index()] += 1;
            Step::Continue
        });
    }
    for (index, expr) in spec.exprs.iter().enumerate() {
        let path = spec.render(expr.meta.path, Notation::V0_5);
        if matches!(
            expr.kind,
            ExprKind::Unsupported(_) | ExprKind::Literal(Literal::Unsupported)
        ) {
            failures.push(format!("{label}: {path} lowered as unsupported"));
        }
        if reached[index] != 1 {
            failures.push(format!(
                "{label}: {path} reached from {} sites",
                reached[index]
            ));
        }
        if !expr.meta.span.is_known() {
            failures.push(format!("{label}: {path} has no position"));
        }
    }
    for column in &spec.columns {
        for check in &column.checks {
            if matches!(check.kind, ColumnCheckKind::Unsupported(_)) {
                let path = spec.render(check.meta.path, Notation::V0_5);
                failures.push(format!("{label}: {path} lowered as unsupported"));
            }
        }
    }
    for check in &spec.checks {
        if matches!(check.kind, DatasetCheckKind::Unsupported(_)) {
            let path = spec.render(check.meta.path, Notation::V0_5);
            failures.push(format!("{label}: {path} lowered as unsupported"));
        }
    }
}

/// The root fields a spec path can start with; other paths name the project.
const SPEC_ROOTS: [&str; 12] = [
    "schema_version",
    "domain",
    "keys",
    "input",
    "base",
    "filter",
    "intermediates",
    "columns",
    "rows",
    "output",
    "verifications",
    "submission",
];

#[test]
fn lowering_is_total_on_every_benchmark_specification() {
    let mut cases = fs::read_dir(benchmarks())
        .unwrap()
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|case| case.join("benchmark.yaml").is_file())
        .collect::<Vec<_>>();
    cases.sort();
    let (mut admitted, mut compiled, mut cited) = (0, 0, 0);
    let mut failures = Vec::new();
    for case in &cases {
        let name = case.file_name().unwrap().to_string_lossy().into_owned();
        let mut rendered = BTreeSet::new();
        for entry in entries(case) {
            let label = format!("{name}/{entry}");
            let path = case.join(&entry).canonicalize().unwrap();
            let identity = path.to_str().unwrap().to_string();
            let source = Source {
                identity: identity.clone(),
                bytes: fs::read(&path).unwrap(),
            };
            // A spec the schema or its composition refuses is not admitted.
            let Ok(document) = shipped_schema::prepare(source, identity, &mut Files) else {
                continue;
            };
            admitted += 1;
            // A 0.5 model is lowered here; a 1.x specification is read in
            // the lowering the 1.0 front end prepared, with its positions.
            let legacy;
            let spec = match document.legacy_model() {
                Some(model) => {
                    legacy = lower(model, Some(&positions(&document)));
                    legacy.spec()
                }
                None => document.tree(),
            };
            check_tree(&label, spec, &mut failures);
            // A composed root is no layer's node, so only the layers are known.
            if spec.files.is_empty() {
                failures.push(format!("{label}: lowered without its files"));
            }
            // An expected error cites paths in its specification's notation.
            rendered.extend(
                spec.paths
                    .ids()
                    .map(|path| spec.render(path, spec.notation())),
            );
            let resources = Resources::for_entry(case.to_str().unwrap()).unwrap();
            if FileSpecification::prepare(resources, &entry).is_ok() {
                compiled += 1;
            }
        }
        if rendered.is_empty() {
            continue;
        }
        for path in expected_paths(case) {
            let root = path.split(['.', '[']).next().unwrap_or_default();
            if !SPEC_ROOTS.contains(&root) {
                continue;
            }
            cited += 1;
            let parent = path.rsplit_once('.').map(|(parent, _)| parent);
            // A root field is a field of the root node, even when it is not
            // written: an omitted `base` is where ROW-8 reports.
            let root_field = !path.contains(['.', '[']);
            if !rendered.contains(&path)
                && !root_field
                && !parent.is_some_and(|p| rendered.contains(p))
            {
                failures.push(format!(
                    "{name}: the expected spec path {path} is no lowered node or field of one"
                ));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "lowering disagrees with the benchmarks:\n{}",
        failures.join("\n")
    );
    // Every case declares at least one entry; nearly all are admitted.
    assert!(
        admitted > 400,
        "only {admitted} benchmark specifications admitted"
    );
    assert!(
        compiled > 300,
        "only {compiled} benchmark specifications compiled"
    );
    assert!(cited > 100, "only {cited} expected spec paths checked");
}

/// One standalone spec, admitted by the shipped schema as a host admits it,
/// then lowered.
fn lowered(yaml: &str) -> yamaa_core::front_end::v0_5::Lowered {
    let document = shipped_schema::capture()
        .unwrap()
        .prepare_standalone(Source {
            identity: "spec.yaml".into(),
            bytes: yaml.as_bytes().to_vec(),
        })
        .unwrap_or_else(|error| panic!("the spec is admitted: {error:?}"));
    lower(document.model(), Some(&positions(&document)))
}

/// Each column's derivation: its expression's kind and today's path.
fn derivations(spec: &Spec) -> Vec<(&str, &ExprKind, String)> {
    spec.columns
        .iter()
        .filter_map(|column| {
            let expr = spec.expr(column.derivation.as_ref()?.expr);
            Some((
                column.name.name.as_str(),
                &expr.kind,
                spec.render(expr.meta.path, Notation::V0_5),
            ))
        })
        .collect()
}

#[test]
fn every_operation_lowers_at_the_path_the_compiler_cites() {
    let lowered = lowered(
        r#"
schema_version: "1.0"
domain: T
keys: [ID]
input: {SRC: src.csv, EX: ex.csv, ODM: odm.csv}
columns:
  - {name: ID, type: int, derivation: SRC.ID}
  - {name: DOUBLE, type: float, derivation: {compute: {expr: ID * 2}}}
  - {name: CODE, type: str, derivation: {literal: A}}
  - name: LABEL
    type: str
    derivation: {source: {variable: SRC.LABEL, filter: "SRC.KIND = 'X'", absent: {literal: null}}}
  - name: BRANCH
    type: str
    derivation: {case: [{when: ID > 1, then: {literal: Y}}, {otherwise: SRC.FLAG}]}
  - {name: FLAG, type: str, derivation: {flag: ID = 2}}
  - {name: FIRST, type: str, derivation: {first_available: {sources: [SRC.A, {variable: SRC.B, absent: B}], missing: NONE}}}
  - {name: SEX, type: str, derivation: {mapping: {source: SRC.SEX, dict: {M: Male, NOT REPORTED: U}, unmapped: "?"}}}
  - {name: BAND, type: str, derivation: {cut: {source: SRC.AGE, breaks: [18, 65.5], labels: [A, B, C], right: true}}}
  - {name: DAYS, type: int, derivation: {diff: {type: date, start: SRC.START, end: SRC.END, unit: day, bounds: inclusive}}}
  - {name: DY, type: int, derivation: {study_day: {date: SRC.DATE, reference: SRC.START}}}
  - {name: DT, type: date, derivation: {impute: {type: date, source: SRC.DTC, month: 1, day: first, not_after: SRC.END}}}
  - {name: SUBJ, type: str, derivation: {str_template: "{SRC.SITE}-{SRC.ID}-{SRC.SITE}"}}
  - {name: LATER, type: date, derivation: {shift: {type: date, source: SRC.DATE, days: -3}}}
  - name: BASE
    type: str
    derivation:
      baseline_flag:
        date: SRC.ADT
        reference_date: SRC.TRTSDT
        value: SRC.AVAL
        tie_break: [SRC.ATM]
        window: {group_by: [SRC.PARAMCD], order_by: [SRC.ADT]}
  - name: RANK
    type: int
    derivation: {rank: {method: dense, window: {order_by: [{variable: SRC.AVAL, direction: desc}], filter: SRC.AVAL > 0}}}
  - {name: MAXDOSE, type: float, derivation: {aggregate: {key: [ID], expr: MAX(EX.DOSE)}}}
  - {name: TOTAL, type: int, derivation: {aggregate: COUNT(ID)}}
  - {name: ITEM, type: str, derivation: {odm: ODM.SEX}}
  - name: BMI
    type: float
    derivation: {function: {name: bmi, args: {weight: SRC.WEIGHT, unit: {literal: kg}, on: {date: "2024-01-01"}, scale: 1.5}}}
output:
  path: out.csv
  columns: [ID, DOUBLE, CODE, LABEL, BRANCH, FLAG, FIRST, SEX, BAND, DAYS, DY, DT, SUBJ, LATER, BASE, RANK, MAXDOSE, TOTAL, ITEM, BMI]
"#,
    );
    let spec = lowered.spec();
    let derived = derivations(spec);
    let paths = derived
        .iter()
        .map(|(name, _, path)| (*name, path.as_str()))
        .collect::<Vec<_>>();
    assert_eq!(
        paths,
        [
            ("ID", "columns.ID.derivation.source"),
            ("DOUBLE", "columns.DOUBLE.derivation.compute"),
            ("CODE", "columns.CODE.derivation.literal"),
            ("LABEL", "columns.LABEL.derivation.source"),
            ("BRANCH", "columns.BRANCH.derivation.case"),
            ("FLAG", "columns.FLAG.derivation.flag"),
            ("FIRST", "columns.FIRST.derivation.first_available"),
            ("SEX", "columns.SEX.derivation.mapping"),
            ("BAND", "columns.BAND.derivation.cut"),
            ("DAYS", "columns.DAYS.derivation.diff"),
            ("DY", "columns.DY.derivation.study_day"),
            ("DT", "columns.DT.derivation.impute"),
            ("SUBJ", "columns.SUBJ.derivation.str_template"),
            ("LATER", "columns.LATER.derivation.shift"),
            ("BASE", "columns.BASE.derivation.baseline_flag"),
            ("RANK", "columns.RANK.derivation.rank"),
            ("MAXDOSE", "columns.MAXDOSE.derivation.aggregate"),
            ("TOTAL", "columns.TOTAL.derivation.aggregate"),
            ("ITEM", "columns.ITEM.derivation.odm"),
            ("BMI", "columns.BMI.derivation.function"),
        ]
    );
    let render = |id| spec.render(spec.expr(id).meta.path, Notation::V0_5);
    let kind = |name: &str| derived.iter().find(|(n, ..)| *n == name).unwrap().1;
    // A bare name is the canonical `source` binding.
    assert!(matches!(kind("ID"), ExprKind::Ref(name) if name.as_str() == "SRC.ID"));
    let ExprKind::Formula(formula) = kind("DOUBLE") else {
        panic!("a formula")
    };
    assert_eq!(
        spec.render(formula.text.meta.path, Notation::V0_5),
        "columns.DOUBLE.derivation.compute.expr"
    );
    assert_eq!(
        formula.compiled.as_ref().unwrap().spec_path(),
        "columns.DOUBLE.derivation.compute"
    );
    let ExprKind::Source(source) = kind("LABEL") else {
        panic!("a structured source")
    };
    assert_eq!(
        render(source.filter.unwrap()),
        "columns.LABEL.derivation.source.filter"
    );
    assert_eq!(source.absent.as_ref().unwrap().value, Literal::Missing);
    let ExprKind::Case(arms) = kind("BRANCH") else {
        panic!("a case")
    };
    assert_eq!(
        render(arms[0].condition.unwrap()),
        "columns.BRANCH.derivation.case[0].when"
    );
    assert_eq!(
        render(arms[0].result),
        "columns.BRANCH.derivation.case[0].then.literal"
    );
    assert_eq!(
        render(arms[1].result),
        "columns.BRANCH.derivation.case[1].otherwise.source"
    );
    let ExprKind::Flag {
        condition,
        true_value,
        ..
    } = kind("FLAG")
    else {
        panic!("a flag")
    };
    assert_eq!(render(*condition), "columns.FLAG.derivation.flag.condition");
    assert_eq!(true_value.value, Literal::Text("Y".into()));
    let ExprKind::FirstAvailable { sources, .. } = kind("FIRST") else {
        panic!("a first_available")
    };
    assert!(matches!(spec.expr(sources[0]).kind, ExprKind::Ref(_)));
    assert_eq!(
        render(sources[1]),
        "columns.FIRST.derivation.first_available.sources[1]"
    );
    let ExprKind::Scalar { op, operands, .. } = kind("DAYS") else {
        panic!("a diff")
    };
    assert_eq!(
        op,
        &ScalarOp::Diff {
            kind: TemporalType::Date,
            unit: TimeUnit::Day,
            closed: Closed::Both,
        }
    );
    assert_eq!(render(operands[1].expr), "columns.DAYS.derivation.diff.end");
    let ExprKind::Scalar { operands, .. } = kind("DY") else {
        panic!("a study day")
    };
    assert_eq!(operands[0].slot, Slot::Source);
    assert_eq!(
        render(operands[0].expr),
        "columns.DY.derivation.study_day.date"
    );
    // The schema's default precision is materialized and kept (P-IMP-1).
    let ExprKind::Scalar {
        op: ScalarOp::Impute {
            minimum_source_precision,
            ..
        },
        operands,
        ..
    } = kind("DT")
    else {
        panic!("an impute")
    };
    assert_eq!(*minimum_source_precision, Some(Precision::Year));
    assert_eq!(operands[1].slot, Slot::NotAfter);
    let ExprKind::Scalar { op, .. } = kind("BAND") else {
        panic!("a cut")
    };
    assert!(matches!(
        op,
        ScalarOp::Cut {
            closed: Closed::Right,
            ..
        }
    ));
    let ExprKind::Scalar { operands, .. } = kind("SUBJ") else {
        panic!("a template")
    };
    assert_eq!(operands.len(), 2);
    let ExprKind::Window(window) = kind("BASE") else {
        panic!("a baseline flag")
    };
    let order = window
        .order_by
        .iter()
        .map(|term| spec.render(term.meta.path, Notation::V0_5))
        .collect::<Vec<_>>();
    assert_eq!(
        order,
        [
            "columns.BASE.derivation.baseline_flag.date",
            "columns.BASE.derivation.baseline_flag.tie_break[0]",
        ]
    );
    assert!(matches!(
        &window.function,
        WindowFunction::BaselineFlag { window_order_by, .. } if window_order_by.len() == 1
    ));
    let ExprKind::Aggregate(aggregate) = kind("MAXDOSE") else {
        panic!("an aggregate")
    };
    assert!(matches!(aggregate.key, Some(JoinKey::Columns { .. })));
    let ExprKind::Aggregate(aggregate) = kind("TOTAL") else {
        panic!("an aggregate")
    };
    assert_eq!(aggregate.key, None, "a reduction of the output rows");
    let ExprKind::Call { args, .. } = kind("BMI") else {
        panic!("a call")
    };
    let values = args
        .iter()
        .map(|(name, value)| (name.name.as_str(), spec.expr(*value).kind.clone()))
        .collect::<Vec<_>>();
    assert_eq!(
        values,
        [
            ("weight", ExprKind::Ref(Name("SRC.WEIGHT".into()))),
            ("unit", ExprKind::Literal(Literal::Text("kg".into()))),
            ("on", ExprKind::Literal(Literal::Date("2024-01-01".into()))),
            ("scale", ExprKind::Literal(Literal::Float(1.5))),
        ]
    );
    let mut failures = Vec::new();
    check_tree("forms", spec, &mut failures);
    assert!(failures.is_empty(), "{failures:?}");
}

#[test]
fn rows_intermediates_and_submission_keep_today_s_paths() {
    let lowered = lowered(
        r#"
schema_version: "1.0"
domain: T
keys: [ID]
input: {SRC: src.csv}
intermediates:
  - id: PRIOR
    dataset: SRC
    no_match: NONE
    derivations:
      TYPED: {type: int, derivation: SRC.N, unconvertible: {literal: null}}
    verifications: [{unique: [ID]}]
  - {id: LAST, dataset: SRC}
rows:
  - {id: collected, filter: "SRC.KIND = 'C'", derivations: {CODE: {literal: C}}}
  - {id: last, dataset: LAST, group_by: [LAST.ID], derivations: {}}
columns:
  - {name: ID, type: int, derivation: SRC.ID}
  - name: CODE
    type: str
    derivation: PRIOR.TYPED
    submission:
      core: Req
      origin: {type: Collected, source: Investigator}
      method: Copied from the source.
      value_level: [{id: SYSBP, where: {CODE: [SYSBP]}}]
output: {path: out.csv, columns: [ID, CODE]}
"#,
    );
    let spec = lowered.spec();
    let render = |path| spec.render(path, Notation::V0_5);
    assert_eq!(spec.inputs[0].key, KeyDecl::Inferred);
    assert_eq!(render(spec.inputs[0].meta.path), "input.SRC");
    let templates = &spec.rows.templates;
    assert_eq!(render(templates[0].meta.path), "rows[0]");
    assert_eq!(render(templates[1].meta.path), "rows[1]");
    assert_eq!(
        spec.render(templates[1].meta.path, Notation::V1),
        "rows.last"
    );
    let (_, code) = &templates[0].overrides[0];
    assert_eq!(render(code.meta.path), "rows[0].derivations.CODE");
    assert_eq!(
        render(spec.expr(code.expr).meta.path),
        "rows[0].derivations.CODE.literal"
    );
    assert!(templates[1].overrides.is_empty());
    let [prior, last] = &spec.intermediates[..] else {
        panic!("two intermediates")
    };
    assert_eq!(render(prior.meta.path), "intermediates[0]");
    // A lookup without `key` infers it; a row driver has none.
    assert_eq!(prior.key, Some(JoinKey::Applicable));
    assert_eq!(last.key, None);
    assert_eq!(
        render(prior.absent.as_ref().unwrap().meta.path),
        "intermediates[0].no_match"
    );
    let binding = &prior.bindings[0];
    assert_eq!(
        render(binding.meta.path),
        "intermediates[0].derivations.TYPED"
    );
    assert_eq!(
        render(spec.expr(binding.derivation.expr).meta.path),
        "intermediates[0].derivations.TYPED.derivation.source"
    );
    assert_eq!(
        render(binding.derivation.unconvertible.as_ref().unwrap().meta.path),
        "intermediates[0].derivations.TYPED.unconvertible"
    );
    assert_eq!(
        render(prior.checks[0].meta.path),
        "intermediates[0].verifications[0].unique"
    );
    let submission = spec.columns[1].submission.as_ref().unwrap();
    assert_eq!(submission.metadata.origins.len(), 1, "one origin is a list");
    let clause = &submission.value_level[0].conditions[0];
    assert_eq!(clause.comparator, Comparator::In);
    assert_eq!(
        render(clause.meta.path),
        "columns.CODE.submission.value_level.SYSBP.where.CODE"
    );
    let mut failures = Vec::new();
    check_tree("rows", spec, &mut failures);
    assert!(failures.is_empty(), "{failures:?}");
}
