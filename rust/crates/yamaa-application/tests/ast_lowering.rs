#![cfg(feature = "yaml")]
//! The extra check of #3133: lowering is total on every specification the
//! benchmark harness admits, and each lowered declaration renders the spec
//! path today's compiler cites.
//!
//! Each case's entry specifications (its `benchmark.yaml` entries, or its
//! one `spec*.yaml`) are prepared with the shipped closures, standalone or
//! through their parents, and read in the lowering the 1.0 front end
//! prepared. Every admitted specification lowers without an
//! unsupported node, its sites partition the expression arena, every
//! expression carries the position its layer's decoder recorded, and every
//! spec path the case's expected first issue names is a lowered node's
//! path or a field of one. Compiling each entry through the file route
//! runs the compiler's own comparison of every declaration path with the
//! tree's, which debug builds enforce
//! (`yamaa-core/src/specification/lowering_audit.rs`); the Rust tests that
//! prepare a specification run the same comparison.

use std::{collections::BTreeSet, fs, path::Path};
use yamaa_adapters::{
    file_resources::Resources, shipped_schema, specification_source::Source,
    yaml_decode::decode_yaml,
};
use yamaa_application::file_preparation::FileSpecification;
use yamaa_core::{
    ast::{ColumnCheckKind, DatasetCheckKind, ExprKind, Literal, Spec, Step},
    schema::{Document, DocumentNode as N},
};

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

use crate::support::inheritance::Files;

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
        let path = spec.render(expr.meta.path);
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
                let path = spec.render(check.meta.path);
                failures.push(format!("{label}: {path} lowered as unsupported"));
            }
        }
    }
    for check in &spec.checks {
        if matches!(check.kind, DatasetCheckKind::Unsupported(_)) {
            let path = spec.render(check.meta.path);
            failures.push(format!("{label}: {path} lowered as unsupported"));
        }
    }
}

/// The root fields a spec path can start with; other paths name the project.
const SPEC_ROOTS: [&str; 12] = [
    "yamaa",
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
    let cases = crate::support::benchmarks::all();
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
            // A specification is read in the lowering its front end prepared,
            // with its positions.
            let spec = document.tree();
            check_tree(&label, spec, &mut failures);
            // A composed root is no layer's node, so only the layers are known.
            if spec.files.is_empty() {
                failures.push(format!("{label}: lowered without its files"));
            }
            // An expected error cites paths in the spec path notation.
            rendered.extend(spec.paths.ids().map(|path| spec.render(path)));
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
            // A declared root field belongs to the root node even when
            // omitted, as ROW-8's absent `base` does.
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
