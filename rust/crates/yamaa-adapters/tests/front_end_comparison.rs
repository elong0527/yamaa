#![cfg(feature = "yaml")]
//! The comparison of #2844's design, section 5 (#3030 D30): the 1.0 front
//! end lowers each converted benchmark to the AST the 0.5 front end lowers
//! its original to.
//!
//! The converter (`tools/migrate`, P2-1) writes every benchmark as 1.0 into a
//! temporary directory. For each specification a benchmark opens, the 0.5
//! front end lowers the original and the 1.0 front end lowers the conversion,
//! each after composition. Every field of the two trees is compared, apart
//! from what the comparison does not cover:
//!
//! - spans, which name different files;
//! - path notation: a 0.5 path is written in the 1.0 notation and rewritten
//!   by the rename table's `paths` block (P-CLR-2, D18, P-HND-1, P-OP-1 to
//!   P-OP-3, P-ORD-1, P-ORD-2, P-BIND-1, P-ORIGIN-1);
//! - a handler that restates its default, which the converter deletes in a
//!   single-layer specification (P-HND-3), with the field it is written in;
//! - the spelling of a written field: 0.5's `no_match` is 1.0's `absent`
//!   and `derive` is `derivations` (P-HND-1, P-BIND-1);
//! - `model` and the header, and the context that tells `compute` text from
//!   expression-field text (FORMULA-27);
//! - where a declared intermediate `key` falls among the intermediate's
//!   written fields;
//! - a written `minimum_source_precision` that the month and day rules
//!   already imply (D61).
//!
//! Join keys are compared directly (P1-6, D23, D63). Each key the 0.5 tree
//! infers is computed as the engine infers it, the output keys the relation
//! stores ([`yamaa_core::key_relation::infer`]), over the fields the
//! engine's own readers give each input, and written into the 0.5 tree as
//! 1.0 declares it: an input read by an implicit join or a keyless qualified
//! aggregate declares it as its `key` (an input a rename-only intermediate
//! reads does too, for LOOKUP-4), an intermediate without `key` that drives
//! no row template declares it as its own, and a keyless qualified aggregate
//! matches on its input's (`FromInput`). The two trees must then be equal,
//! so each declared key equals the key the engine infers today.
//!
//! Every specification whose trees still differ is listed in [`EXPECTED`]
//! with the reason, and the test fails when the list is wrong in either
//! direction. Each 1.0 tree is also checked to be total and located. A
//! benchmark already written in 1.0 has no 0.5 tree to compare: the
//! converter copies it unchanged, and only its 1.0 tree is checked. Only a
//! negative benchmark, with its `expected/error.yaml`, may be refused; any
//! other refusal fails the test. The corpus moves to 1.0 one prefix at a time (P2-5 to P2-8), so every
//! specification checked, in either surface, counts toward the floor, and a
//! listed specification whose benchmark is written in 1.0 is settled by its
//! move (#3422). The converter runs through `uv` in the locked tooling
//! environment that `make setup` installs; without uv or PyYAML the test
//! says so.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use yamaa_adapters::document::{self, Closures, Decoded};
use yamaa_adapters::shipped_schema;
use yamaa_adapters::specification_source::{InheritancePort, PreparedDocument, Source};
use yamaa_adapters::yaml_decode::decode_yaml;
use yamaa_core::ast::{
    Comparator, Constant, ExprId, ExprKind, Filtered, Input, JoinKey, KeyDecl, Literal, Meta,
    Named, Notation, Owner, Precision, Role, ScalarOp, Segment, Span, Spec, Step, Written,
};
use yamaa_core::expression::Context;
use yamaa_core::front_end::v0_5::lower;
use yamaa_core::inheritance::{Source as GraphSource, SourceError};
use yamaa_core::key_relation::{self, Inference};
use yamaa_core::language::{FileType, LANGUAGE_VERSION};
use yamaa_core::reference_binding::{Catalog, Field};
use yamaa_core::schema::{Document, DocumentNode as N};
use yamaa_core::table::TableAccess;
use yamaa_core::value::ColumnType;

/// Each specification whose trees differ, with the reason.
const EXPECTED: &[(&str, &str)] = &[(
    "sdtm-tr-tumor-measurements/spec.yaml",
    "D18: `compute: {expr: TRORRES_N}` holds only a name, which 1.0 text reads as \
     a reference (FORMULA-9, EXPR-4); TRORRES_N is a float column, so the value \
     is the same",
)];

fn repository() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

/// A temporary directory, removed when dropped.
struct Scratch(crate::support::Scratch);
impl Scratch {
    fn new() -> Self {
        let path = crate::support::Scratch::new("front-end-comparison");
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}

/// How to install what the converter needs: uv, and the locked tooling
/// environment, with PyYAML.
const SETUP: &str = "run `make setup`, or \
                     `uv sync --project python --locked --group tooling --no-install-package yamaa`";

/// Convert `benchmarks`, the directory of every benchmark or of one, into
/// `output/benchmarks` as `python -m tools.migrate` does.
fn convert(benchmarks: &str, output: &Path) {
    let result = Command::new("uv")
        .args(["run", "--project", "python", "--no-sync", "python"])
        .args(["-m", "tools.migrate", benchmarks, "--base", ".", "--output"])
        .arg(output)
        .current_dir(repository())
        .output()
        .unwrap_or_else(|error| {
            panic!("the comparison runs tools/migrate through uv, which did not start ({error}); {SETUP}")
        });
    let stderr = String::from_utf8_lossy(&result.stderr);
    // tools/migrate refuses to run where `import yaml` is not PyYAML.
    assert!(
        !stderr.contains("tools.migrate needs PyYAML"),
        "tools/migrate cannot run without PyYAML; {SETUP}:\n{stderr}"
    );
    assert!(
        result.status.success(),
        "tools/migrate failed:\n{}{stderr}",
        String::from_utf8_lossy(&result.stdout)
    );
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
/// (`tools/benchmark.py`): the declared entries, or the case's specs.
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
    specs.sort();
    specs
}

/// Parents resolve beside their declaring file; paths stay as written, so
/// both surfaces keep the spelling their layers wrote.
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

fn source(path: &Path) -> (Source, String) {
    let path = path.canonicalize().unwrap();
    let identity = path.to_str().unwrap().to_string();
    let source = Source {
        identity: identity.clone(),
        bytes: fs::read(&path).unwrap(),
    };
    (source, identity)
}

/// A refusal, by its first registered cause where it names one.
fn refusal(debug: &str) -> String {
    if let Some(at) = debug.find("code: ") {
        return debug[at..]
            .split([',', ' ', '}'])
            .nth(1)
            .unwrap_or_default()
            .to_string();
    }
    // A parent chain the traversal refuses carries no condition code; its
    // variant names the fault.
    match debug.find("Traversal(") {
        Some(at) => format!(
            "{})",
            debug[at..]
                .split([' ', '{', ')'])
                .next()
                .unwrap_or_default()
        ),
        None => debug.chars().take(80).collect(),
    }
}

/// The 0.5 front end's tree for an original specification, with the
/// prepared document that knows where each input was declared, or why it is
/// refused before lowering.
fn legacy(path: &Path) -> Result<(Spec, PreparedDocument), String> {
    let (source, identity) = source(path);
    let document = shipped_schema::prepare(source, identity, &mut Files)
        .map_err(|error| refusal(&format!("{error:?}")))?;
    Ok((lower(document.model(), None).into_spec(), document))
}

/// Whether a benchmark's specification is already written in 1.0, so that
/// no 0.5 tree exists to compare with its conversion, which is the same file.
fn written_in_1_0(path: &Path) -> bool {
    let (source, _) = source(path);
    let closures = Closures::shipped().unwrap();
    matches!(
        document::decode(source, FileType::Spec, Some(LANGUAGE_VERSION), &closures),
        Ok(Decoded::Versioned(_))
    )
}

/// The 1.0 front end's tree for a converted specification, or why it is
/// refused before lowering.
fn versioned(path: &Path) -> Result<Spec, String> {
    let (source, identity) = source(path);
    let closures = Closures::shipped().unwrap();
    match document::decode(source, FileType::Spec, Some(LANGUAGE_VERSION), &closures) {
        Ok(Decoded::Versioned(document)) => document
            .prepare_specification(identity, &mut Files)
            .map(|prepared| prepared.into_lowered().into_spec())
            .map_err(|error| refusal(&format!("{error:?}"))),
        Ok(Decoded::Legacy { .. }) => Err("not converted".into()),
        Err(error) => Err(refusal(&format!("{error:?}"))),
    }
}

/// What the comparison left out, for the record.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Normalized {
    RestatedDefault,
    MinimumPrecision,
}

impl Normalized {
    fn label(self) -> &'static str {
        match self {
            Self::RestatedDefault => "a handler restating its default (P-HND-3)",
            Self::MinimumPrecision => "minimum_source_precision the rules imply (D61)",
        }
    }
}

/// The keys one specification declares where its 0.5 tree infers them.
#[derive(Clone, Debug, Default)]
struct Declared {
    /// Each input an implicit join, a keyless qualified aggregate or a
    /// rename-only intermediate reads, with the key the engine infers.
    inputs: Vec<(String, Vec<String>)>,
    /// Each intermediate without `key` that drives no row template, with
    /// the key the engine infers.
    intermediates: Vec<(String, Vec<String>)>,
    /// The keyless qualified aggregates, which match on their input's key.
    aggregates: usize,
}

impl Declared {
    fn joins(&self) -> bool {
        !self.inputs.is_empty() || !self.intermediates.is_empty() || self.aggregates > 0
    }
}

/// The fields `input` stores, as the engine reads them before binding: a
/// CSV header and a Parquet schema by the source readers, a schema-linked
/// input's producer contract, or the ODM item table; and a generated
/// ordinal (INPUT-3, INPUT-10).
fn stored_fields(document: &PreparedDocument, input: &Input) -> Result<Vec<String>, String> {
    let name = &input.name.name;
    let located = |field: &str| {
        let origin = document
            .written_input_origin(name, field)
            .ok_or_else(|| format!("input {name} has no written {field}"))?;
        Ok::<_, String>(
            Path::new(origin.base_source)
                .parent()
                .unwrap()
                .join(origin.written),
        )
    };
    let mut fields = if input.source.schema.is_some() {
        let producer = located("schema")?;
        let (spec, _) =
            legacy(&producer).map_err(|error| format!("producer of {name} not read: {error}"))?;
        spec.output
            .columns
            .iter()
            .map(|column| column.name.clone())
            .collect()
    } else {
        let path = located("path")?;
        let bytes = fs::read(&path).map_err(|error| format!("input {name}: {error}"))?;
        match path.extension().and_then(|extension| extension.to_str()) {
            Some(extension) if extension.eq_ignore_ascii_case("csv") => {
                yamaa_core::csv_source::parse(&bytes)
                    .map_err(|error| format!("input {name}: {error:?}"))?
                    .names
            }
            Some(extension) if extension.eq_ignore_ascii_case("parquet") => {
                yamaa_adapters::parquet_source::parse(&bytes)
                    .map_err(|error| format!("input {name}: {error:?}"))?
                    .schema()
                    .columns()
                    .iter()
                    .map(|column| column.name.clone())
                    .collect()
            }
            Some(extension) if extension.eq_ignore_ascii_case("xml") => {
                yamaa_core::odm_source::FIELDS
                    .iter()
                    .map(|field| field.to_string())
                    .collect()
            }
            _ => {
                return Err(format!(
                    "input {name} is read by no reader the comparison uses"
                ))
            }
        }
    };
    fields.extend(
        input
            .source
            .ordinal
            .iter()
            .map(|ordinal| ordinal.name.clone()),
    );
    Ok(fields)
}

/// The key a relation storing `fields` matches on where the 0.5 tree infers
/// it, as the engine infers it: the output keys, in key order, that the
/// relation stores (LOOKUP-5 at the `yamaa-v0.5.0` tag). Types are left
/// out: a pair whose types cannot compare fails alike on both surfaces when
/// it binds (LOOKUP-9), and the converter writes the pair by name.
fn applicable(spec: &Spec, fields: &[String]) -> Vec<String> {
    let typed = |name| Field {
        name,
        column_type: ColumnType::Str,
    };
    let outputs = spec
        .columns
        .iter()
        .map(|column| typed(column.name.name.as_str()))
        .collect::<Vec<_>>();
    let catalog = Catalog::compile(&outputs, &[]).unwrap();
    let keys = spec
        .keys
        .iter()
        .map(|key| key.name.as_str())
        .collect::<Vec<_>>();
    let donor = fields
        .iter()
        .map(|field| typed(field.as_str()))
        .collect::<Vec<_>>();
    match key_relation::infer(&catalog, &keys, &donor).unwrap() {
        Inference::Keys { keys: selected } => selected
            .into_iter()
            .map(|key| keys[key].to_string())
            .collect(),
        _ => Vec::new(),
    }
}

/// Where a qualified name read in the current row joins: the datasets whose
/// records build the rows `owner` derives or filters, which a read of
/// another input joins from (LOOKUP-1, NAME-4). `None` for a site whose
/// reads never join, such as a lookup's own declarations.
fn drivers(spec: &Spec, owner: Owner) -> Option<Vec<&str>> {
    let base =
        spec.rows
            .base
            .as_ref()
            .map(|base| base.name.as_str())
            .or(match spec.inputs.as_slice() {
                [only] => Some(only.name.name.as_str()),
                _ => None,
            });
    let templates = &spec.rows.templates;
    let driver = |template: usize| {
        templates[template]
            .driver
            .as_ref()
            .map(|driver| driver.name.as_str())
            .or(base)
    };
    let all = || -> Vec<&str> {
        if templates.is_empty() {
            base.into_iter().collect()
        } else {
            (0..templates.len()).filter_map(driver).collect()
        }
    };
    Some(match owner {
        Owner::Default { column } if !templates.is_empty() => {
            let name = &spec.columns[column].name.name;
            (0..templates.len())
                .filter(|&template| {
                    !templates[template]
                        .overrides
                        .iter()
                        .any(|(derived, _)| derived.name == *name)
                })
                .filter_map(driver)
                .collect()
        }
        Owner::Default { .. } | Owner::Filter(Filtered::Base) => base.into_iter().collect(),
        Owner::Override { template, .. }
        | Owner::Filter(Filtered::Template(template))
        | Owner::Group { template } => driver(template).into_iter().collect(),
        Owner::Check { .. } => all(),
        _ => return None,
    })
}

/// The qualified names `e` reads in the current row's scope, outside a
/// lookup's own range, filter, order and key match values, a window's
/// grouping and order, and an ODM read, none of which joins; and each
/// keyless qualified aggregate below `e`, with the qualifiers its reduction
/// reads.
fn row_reads(
    spec: &Spec,
    e: ExprId,
    names: &mut BTreeSet<String>,
    aggregates: &mut Vec<BTreeSet<String>>,
) {
    let node = &spec.expr(e).kind;
    match node {
        ExprKind::Ref(name) => names.extend(name.owner().map(String::from)),
        ExprKind::Odm(_) => return,
        ExprKind::Aggregate(aggregate) => {
            if matches!(aggregate.key, Some(JoinKey::Applicable)) {
                let mut relation = BTreeSet::new();
                relation_reads(spec, e, &mut relation);
                aggregates.push(relation);
            }
            return;
        }
        _ => {}
    }
    for (role, child) in spec.children(e) {
        let skipped = match role {
            Role::Relation | Role::RangeValue | Role::RangeBound | Role::KeyValue => true,
            Role::Group | Role::Order => matches!(node, ExprKind::Window(_) | ExprKind::Source(_)),
            Role::Condition => matches!(node, ExprKind::Window(_)),
            _ => false,
        };
        if !skipped {
            row_reads(spec, child, names, aggregates);
        }
    }
}

/// The qualifiers an aggregate's reduction, filter and bindings read,
/// outside its key match values and range.
fn relation_reads(spec: &Spec, e: ExprId, out: &mut BTreeSet<String>) {
    if let ExprKind::Ref(name) = &spec.expr(e).kind {
        out.extend(name.owner().map(String::from));
    }
    for (role, child) in spec.children(e) {
        if !matches!(role, Role::KeyValue | Role::RangeValue | Role::RangeBound) {
            relation_reads(spec, child, out);
        }
    }
}

/// Write the keys the 0.5 tree `old` infers into it as the 1.0 tree `new`
/// declares them, each computed as the engine infers it: an input's `key`,
/// an intermediate's own `key`, and `FromInput` on a keyless qualified
/// aggregate. Where an intermediate's declared `key` falls among its written
/// fields is left out of both trees.
fn declare_inferred(
    old: &mut Spec,
    new: &mut Spec,
    document: &PreparedDocument,
) -> Result<Declared, String> {
    let mut declared = Declared::default();
    let inputs = old
        .inputs
        .iter()
        .map(|input| input.name.name.clone())
        .collect::<BTreeSet<_>>();
    let intermediates = old
        .intermediates
        .iter()
        .map(|item| item.id.name.clone())
        .collect::<BTreeSet<_>>();
    // The inputs an implicit join or a keyless qualified aggregate reads.
    let mut joined = BTreeSet::new();
    for site in old.sites() {
        let Some(drivers) = drivers(old, site.owner) else {
            continue;
        };
        let mut names = BTreeSet::new();
        let mut aggregates = Vec::new();
        row_reads(old, site.root, &mut names, &mut aggregates);
        for owner in names {
            if inputs.contains(&owner)
                && !intermediates.contains(&owner)
                && drivers.iter().any(|driver| *driver != owner)
            {
                joined.insert(owner);
            }
        }
        let grouped_driver = match site.owner {
            Owner::Override { template, .. }
            | Owner::Filter(Filtered::Template(template))
            | Owner::Group { template }
                if old.rows.templates[template].group_by.is_some() =>
            {
                drivers.first().copied()
            }
            _ => None,
        };
        for relation in aggregates {
            let mut relation = relation.into_iter().filter(|name| inputs.contains(name));
            if let (Some(input), None) = (relation.next(), relation.next()) {
                if grouped_driver != Some(input.as_str()) {
                    declared.aggregates += 1;
                    joined.insert(input);
                }
            }
        }
    }
    // An intermediate that only renames its input is refused in 1.0 when its
    // key lists the input's declared key (LOOKUP-4, D71).
    for item in &old.intermediates {
        let renames = matches!(item.key, Some(JoinKey::Applicable))
            && item
                .written
                .fields()
                .iter()
                .all(|field| matches!(*field, "no_match" | "require_match"))
            && !item.require_match
            && item
                .absent
                .as_ref()
                .is_none_or(|absent| absent.value == Literal::Missing);
        if renames && inputs.contains(&item.dataset.name) {
            joined.insert(item.dataset.name.clone());
        }
    }
    let named = |paths: &mut yamaa_core::ast::Paths, parent: Meta, names: &[String]| {
        let key = paths.child(parent.path, Segment::Field("key"));
        let columns = names
            .iter()
            .enumerate()
            .map(|(index, name)| Named {
                name: name.clone(),
                meta: Meta {
                    path: paths.child(key, Segment::Index(index as u32)),
                    span: Span::UNKNOWN,
                },
            })
            .collect::<Vec<_>>();
        (
            Meta {
                path: key,
                span: Span::UNKNOWN,
            },
            columns,
        )
    };
    for index in 0..old.inputs.len() {
        let name = old.inputs[index].name.name.clone();
        let key = if joined.contains(&name) {
            applicable(old, &stored_fields(document, &old.inputs[index])?)
        } else {
            Vec::new()
        };
        old.inputs[index].key = if key.is_empty() {
            KeyDecl::Undeclared
        } else {
            let (_, columns) = named(&mut old.paths, old.inputs[index].meta, &key);
            declared.inputs.push((name, key));
            KeyDecl::Declared(columns)
        };
    }
    for index in 0..old.intermediates.len() {
        if !matches!(old.intermediates[index].key, Some(JoinKey::Applicable)) {
            continue;
        }
        let item = &old.intermediates[index];
        let mut fields = if item.dataset.name == "SELF" {
            old.columns
                .iter()
                .map(|column| column.name.name.clone())
                .collect()
        } else {
            let input = old
                .inputs
                .iter()
                .find(|input| input.name.name == item.dataset.name)
                .ok_or_else(|| format!("intermediate {} reads no input", item.id.name))?;
            stored_fields(document, input)?
        };
        fields.extend(
            item.bindings
                .iter()
                .map(|binding| binding.name.name.clone()),
        );
        let key = applicable(old, &fields);
        let id = item.id.name.clone();
        let meta = item.meta;
        old.intermediates[index].key = if key.is_empty() {
            None
        } else {
            let (meta, columns) = named(&mut old.paths, meta, &key);
            declared.intermediates.push((id.clone(), key));
            Some(JoinKey::Columns { columns, meta })
        };
        for spec in [&mut *old, &mut *new] {
            if let Some(item) = spec
                .intermediates
                .iter_mut()
                .find(|item| item.id.name == id)
            {
                item.written.0.retain(|&field| field != "key");
            }
        }
    }
    for expr in &mut old.exprs {
        if let ExprKind::Aggregate(aggregate) = &mut expr.kind {
            if matches!(aggregate.key, Some(JoinKey::Applicable)) {
                aggregate.key = Some(JoinKey::FromInput);
            }
        }
    }
    Ok(declared)
}

/// Leave out of a tree what the comparison does not cover.
fn normalize(spec: &mut Spec, out: &mut BTreeSet<Normalized>) {
    let mut left_out = BTreeSet::new();
    // An omitted handler that gives missing and one written as missing are
    // one answer (P-HND-3).
    let restated = |handler: &mut Option<Constant>| {
        if handler
            .as_ref()
            .is_some_and(|handler| handler.value == Literal::Missing)
        {
            *handler = None;
            true
        } else {
            false
        }
    };
    // A written field as 1.0 spells it, and without the absence answer once
    // its restated default is left out.
    let spelled = |written: &mut Written, answered: bool| {
        written.0 = written
            .0
            .iter()
            .map(|&field| match field {
                "no_match" => "absent",
                "derive" => "derivations",
                field => field,
            })
            .filter(|&field| answered || field != "absent")
            .collect();
    };
    for item in &mut spec.intermediates {
        if restated(&mut item.absent) {
            left_out.insert(Normalized::RestatedDefault);
        }
        spelled(&mut item.written, item.absent.is_some());
    }
    for expr in &mut spec.exprs {
        let restated_default = match &mut expr.kind {
            ExprKind::Source(source) => {
                let restated = restated(&mut source.absent);
                spelled(&mut source.written, source.absent.is_some());
                restated
            }
            ExprKind::FirstAvailable { missing, .. } => restated(missing),
            ExprKind::Flag {
                false_value: None,
                missing,
                ..
            } => restated(missing),
            // `compute` text and expression-field text are one formula in two
            // contexts (FORMULA-27).
            ExprKind::Formula(formula) => {
                if formula.context == Context::Compute {
                    formula.context = Context::Derivation;
                }
                false
            }
            ExprKind::Aggregate(aggregate) => {
                spelled(&mut aggregate.written, true);
                false
            }
            ExprKind::Scalar {
                op:
                    ScalarOp::Impute {
                        month,
                        day,
                        minimum_source_precision,
                        ..
                    },
                ..
            } => {
                // The rules supplied fix the coarsest precision completed
                // (TIME-8, TIME-10, D61).
                let implied = match (&month, &day) {
                    (Some(_), _) => Precision::Year,
                    (None, Some(_)) => Precision::Month,
                    (None, None) => Precision::Day,
                };
                if *minimum_source_precision == Some(implied) {
                    *minimum_source_precision = None;
                    left_out.insert(Normalized::MinimumPrecision);
                }
                false
            }
            ExprKind::Scalar { op, handlers, .. } => {
                matches!(
                    op,
                    ScalarOp::Mapping { .. }
                        | ScalarOp::Cut { .. }
                        | ScalarOp::StrExtract { .. }
                        | ScalarOp::StrReplace { .. }
                        | ScalarOp::StrCase { .. }
                ) && restated(&mut handlers.missing)
            }
            _ => false,
        };
        if restated_default {
            left_out.insert(Normalized::RestatedDefault);
        }
    }
    out.extend(left_out);
}

/// A tree written out field by field, one line per declaration and per
/// expression, with each path rendered in the 1.0 notation, a 0.5 path
/// rewritten by the rename table, every span left out, and each formula's
/// compilation reduced to whether it compiles.
fn canonical(spec: &Spec, legacy: bool) -> Vec<String> {
    let mut bindings = BTreeMap::new();
    if legacy {
        for expr in &spec.exprs {
            if let ExprKind::Aggregate(aggregate) = &expr.kind {
                for binding in &aggregate.bindings {
                    bindings.insert(
                        spec.render(binding.meta.path, Notation::V1),
                        binding.name.name.clone(),
                    );
                }
            }
        }
    }
    let paths = spec
        .paths
        .ids()
        .map(|id| {
            let path = spec.render(id, Notation::V1);
            let path = if legacy {
                rewrite(&path, &bindings)
            } else {
                path
            };
            (format!("{id:?}"), path)
        })
        .collect::<BTreeMap<_, _>>();
    let mut lines = vec![
        format!("{:?}", spec.domain),
        format!("{:?}", spec.keys),
        format!("{:?}", spec.rows),
        format!("{:?}", spec.output),
        format!("{:?}", spec.submission),
        format!("{:?}", spec.metadata),
    ];
    lines.extend(spec.inputs.iter().map(|input| format!("{input:?}")));
    lines.extend(spec.intermediates.iter().map(|item| format!("{item:?}")));
    lines.extend(spec.columns.iter().map(|column| format!("{column:?}")));
    lines.extend(spec.checks.iter().map(|check| format!("{check:?}")));
    lines.extend(
        spec.exprs
            .iter()
            .enumerate()
            .map(|(index, expr)| format!("{index}: {expr:?}")),
    );
    lines.iter().map(|line| written(line, &paths)).collect()
}

/// One debug line with paths rendered, spans removed and compilations
/// reduced. Debug string and character literals are copied whole, so text
/// they hold is never read as a field.
fn written(line: &str, paths: &BTreeMap<String, String>) -> String {
    let mut out = String::with_capacity(line.len());
    let mut at = 0;
    while at < line.len() {
        let rest = &line[at..];
        if rest.starts_with('"') {
            let end = literal_end(line, at);
            out.push_str(&line[at..end]);
            at = end;
        } else if rest.starts_with("PathId(") {
            let end = rest.find(')').unwrap() + 1;
            out.push_str(&format!("{:?}", paths[&rest[..end]]));
            at += end;
        } else if rest.starts_with(", span: Span {") {
            at += rest.find('}').unwrap() + 1;
        } else if let Some(after) = rest.strip_prefix("compiled: ") {
            let word = if after.starts_with("Ok(") {
                "Ok"
            } else {
                "Err"
            };
            let open = at + "compiled: ".len() + word.len();
            out.push_str("compiled: ");
            out.push_str(word);
            at = balanced_end(line, open);
        } else {
            let c = rest.chars().next().unwrap();
            out.push(c);
            at += c.len_utf8();
        }
    }
    out
}

/// The index after the string or character literal starting at `at`.
fn literal_end(line: &str, at: usize) -> usize {
    let quote = char::from(line.as_bytes()[at]);
    let mut chars = line[at + 1..].char_indices();
    while let Some((offset, c)) = chars.next() {
        if c == '\\' {
            chars.next();
        } else if c == quote {
            return at + 1 + offset + 1;
        }
    }
    line.len()
}

/// The index after the bracket that closes the one at `open`.
fn balanced_end(line: &str, open: usize) -> usize {
    let mut depth = 0usize;
    let mut at = open;
    while at < line.len() {
        let c = line[at..].chars().next().unwrap();
        match c {
            '"' | '\'' => {
                at = literal_end(line, at);
                continue;
            }
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => {
                depth -= 1;
                if depth == 0 {
                    return at + 1;
                }
            }
            _ => {}
        }
        at += c.len_utf8();
    }
    line.len()
}

/// A 0.5 path, written in the 1.0 notation, rewritten as the rename table's
/// `paths` block says: `compute` and `literal` leave the expression field
/// (D18), a key match value's `{source: X}` becomes `X` (D18), the
/// intermediate's `no_match` is `absent` (P-HND-1), operands are renamed
/// (P-OP-1 to P-OP-3), `bounds` is `closed` (P-ORD-2), `tie_break` follows
/// the date in `window.order_by` (P-ORD-1), `derive[N]` is
/// `derivations.<name>` (P-BIND-1) and `origin` is `origins[0]`
/// (P-ORIGIN-1).
fn rewrite(path: &str, bindings: &BTreeMap<String, String>) -> String {
    let mut path = path.to_string();
    // `aggregate.derive[N]` and its `name` are one entry of the map.
    for (binding, name) in bindings {
        if let Some(rest) = path.strip_prefix(binding.as_str()) {
            if rest.is_empty() || rest.starts_with(['.', '[']) {
                let stem = binding
                    .rsplit_once(".derive[")
                    .map_or(binding.as_str(), |(stem, _)| stem);
                let rest = rest.strip_prefix(".name").unwrap_or(rest);
                path = format!("{stem}.derivations.{name}{rest}");
                break;
            }
        }
    }
    // `compute` and `literal` end a path at the expression field they wrap.
    for suffix in [".compute.expr", ".compute", ".literal"] {
        if let Some(stem) = path.strip_suffix(suffix) {
            path = stem.to_string();
            break;
        }
    }
    let segments = split(&path);
    let last = segments.len().saturating_sub(1);
    let mut written = String::new();
    for (index, segment) in segments.iter().enumerate() {
        let at = |back: usize| {
            index
                .checked_sub(back)
                .map_or("", |index| segments[index].as_str())
        };
        written.push_str(match (at(2), at(1), segment.as_str()) {
            (_, ".study_day", ".date") => ".source",
            (_, ".baseline_flag", ".reference_date") => ".reference",
            (_, ".baseline_flag", ".value") => ".source",
            (_, ".diff", ".bounds") => ".closed",
            (_, _, ".origin") => ".origins[0]",
            (".intermediates", _, ".no_match") if index == 2 => ".absent",
            // A key match value's `{source: X}` is `X`.
            (".key", _, ".source") if index == last => "",
            (_, _, segment) => segment,
        });
    }
    let written = written.strip_prefix('.').unwrap_or(&written).to_string();
    // The tie-break terms follow the date in `window.order_by`.
    if let Some((stem, rest)) = written.split_once(".baseline_flag.tie_break[") {
        if let Some((index, rest)) = rest.split_once(']') {
            if let Ok(index) = index.parse::<usize>() {
                let index = index + 1;
                return format!("{stem}.baseline_flag.window.order_by[{index}]{rest}");
            }
        }
    }
    written
}

/// The segments of a path in the 1.0 notation, each with its separator.
fn split(path: &str) -> Vec<String> {
    let mut segments = Vec::new();
    let mut current = String::from(".");
    let mut quoted = false;
    let mut chars = path.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\'' if current.ends_with('[') || quoted => {
                current.push(c);
                if quoted && chars.peek() == Some(&'\'') {
                    current.push(chars.next().unwrap());
                } else {
                    quoted = !quoted;
                }
            }
            '.' | '[' if !quoted => {
                if current != "." {
                    segments.push(std::mem::take(&mut current));
                }
                current.push(c);
            }
            _ => current.push(c),
        }
    }
    if current != "." {
        segments.push(current);
    }
    segments
}

/// The 1.0 tree is total and located: no node lowers as unsupported, every
/// expression is reached from exactly one site, and every expression
/// carries the position its layer wrote, in a file of the spec's layer list
/// (P-ISSUE-1).
fn check_tree(label: &str, spec: &Spec, failures: &mut Vec<String>) {
    let mut reached = vec![0usize; spec.exprs.len()];
    for site in spec.sites() {
        spec.walk(site.root, site.scope, |at, _| {
            reached[at.index()] += 1;
            Step::Continue
        });
    }
    for (index, expr) in spec.exprs.iter().enumerate() {
        let path = spec.render(expr.meta.path, Notation::V1);
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
        let span = expr.meta.span;
        if !span.is_known() || span.file.0 as usize >= spec.files.len() {
            failures.push(format!("{label}: {path} has no position in a layer"));
        }
    }
}

/// Where two written trees first part.
fn first_difference(left: &[String], right: &[String]) -> String {
    let at = left
        .iter()
        .zip(right)
        .position(|(left, right)| left != right)
        .unwrap_or(left.len().min(right.len()));
    let line = |lines: &[String]| lines.get(at).cloned().unwrap_or_default();
    format!("\n    0.5: {}\n    1.0: {}", line(left), line(right))
}

#[test]
fn every_converted_benchmark_lowers_to_its_original_tree() {
    let scratch = Scratch::new();
    convert("benchmarks", &scratch.0);
    let benchmarks = repository().join("benchmarks");
    let mut cases = fs::read_dir(&benchmarks)
        .unwrap()
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|case| case.join("benchmark.yaml").is_file())
        .collect::<Vec<_>>();
    cases.sort();
    let mut compared = 0;
    let mut checked = 0;
    let mut written_1_0 = BTreeSet::new();
    let mut refused = Vec::new();
    let mut unexplained = Vec::new();
    let mut differing = BTreeMap::new();
    let mut tally = BTreeMap::<Normalized, usize>::new();
    let mut joins = BTreeMap::<String, Declared>::new();
    let mut failures = Vec::new();
    for case in &cases {
        let name = case.file_name().unwrap().to_string_lossy().into_owned();
        for entry in entries(case) {
            let label = format!("{name}/{entry}");
            let converted = scratch.0.join("benchmarks").join(&name).join(&entry);
            if written_in_1_0(&case.join(&entry)) {
                written_1_0.insert(label.clone());
                match versioned(&converted) {
                    Ok(new) => {
                        checked += 1;
                        check_tree(&label, &new, &mut failures);
                    }
                    // Only a negative benchmark's specification is refused.
                    Err(new) if case.join("expected/error.yaml").is_file() => {
                        refused.push(format!("{label}: only 1.0, {new}"))
                    }
                    Err(new) => unexplained.push(format!("{label}: {new}")),
                }
                continue;
            }
            match (legacy(&case.join(&entry)), versioned(&converted)) {
                (Ok((mut old, document)), Ok(mut new)) => {
                    compared += 1;
                    checked += 1;
                    check_tree(&label, &new, &mut failures);
                    match declare_inferred(&mut old, &mut new, &document) {
                        Ok(declared) if declared.joins() => {
                            joins.insert(label.clone(), declared);
                        }
                        Ok(_) => {}
                        Err(error) => failures.push(format!("{label}: {error}")),
                    }
                    let mut normalized = BTreeSet::new();
                    normalize(&mut old, &mut normalized);
                    normalize(&mut new, &mut normalized);
                    for normalized in normalized {
                        *tally.entry(normalized).or_default() += 1;
                    }
                    let (left, right) = (canonical(&old, true), canonical(&new, false));
                    if left != right {
                        differing.insert(label, first_difference(&left, &right));
                    }
                }
                (Err(old), Err(new)) => refused.push(format!("{label}: 0.5 {old}, 1.0 {new}")),
                (Ok(_), Err(new)) => {
                    differing.insert(label, format!("only 1.0 refuses it: {new}"));
                }
                (Err(old), Ok(_)) => {
                    differing.insert(label, format!("only 0.5 refuses it: {old}"));
                }
            }
        }
    }
    println!(
        "compared {compared} specifications and checked {} written in 1.0; both front ends refuse {}",
        checked - compared,
        refused.len()
    );
    for (normalized, count) in &tally {
        println!("  {count:4} with {}", normalized.label());
    }
    // Every key 0.5 infers, declared by the conversion as the engine infers
    // it (D23, D63; #2844 design, section 5).
    let count = |of: &dyn Fn(&Declared) -> usize| joins.values().map(of).sum::<usize>();
    println!(
        "  {:4} with joins, declaring {} input keys and {} intermediate keys, and {} keyless qualified aggregates matching on their input's key",
        joins.len(),
        count(&|declared| declared.inputs.len()),
        count(&|declared| declared.intermediates.len()),
        count(&|declared| declared.aggregates),
    );
    let mut keys = BTreeMap::<String, usize>::new();
    for declared in joins.values() {
        for (_, key) in declared.inputs.iter().chain(&declared.intermediates) {
            *keys.entry(format!("[{}]", key.join(", "))).or_default() += 1;
        }
    }
    for (key, count) in &keys {
        println!("       {count:4} declare {key}");
    }
    for refusal in &refused {
        println!("  refused by both: {refusal}");
    }
    for (label, reason) in EXPECTED {
        println!("  differs: {label}: {reason}");
    }
    assert!(
        failures.is_empty(),
        "1.0 trees that are not total or not located:\n{}",
        failures.join("\n")
    );
    assert!(
        unexplained.is_empty(),
        "specifications written in 1.0 that the 1.0 front end refuses, though their \
         benchmark is no negative:\n{}",
        unexplained.join("\n")
    );
    let expected = EXPECTED
        .iter()
        .map(|&(label, _)| label.to_string())
        .collect::<BTreeSet<_>>();
    let unexpected = differing
        .iter()
        .filter(|(label, _)| !expected.contains(*label))
        .map(|(label, difference)| format!("{label}: {difference}"))
        .collect::<Vec<_>>();
    let settled = expected
        .iter()
        .filter(|label| !differing.contains_key(*label) && !written_1_0.contains(*label))
        .cloned()
        .collect::<Vec<_>>();
    assert!(
        unexpected.is_empty() && settled.is_empty(),
        "differing specifications not listed:\n{}\nlisted specifications that no longer differ:\n{}",
        unexpected.join("\n"),
        settled.join("\n")
    );
    assert!(checked > 400, "only {checked} specifications checked");
}

/// A value-level comparator has no 0.5 form, so no benchmark converts to one.
/// A converted case that writes one keeps it on the tree: `NE` as `NotIn`
/// with its one value, as `EQ` is `In` with one, and `NOTIN` as `NotIn`,
/// never read as a list it selects and never dropped (P-VLM-1). The switch
/// refuses either until P1-14 builds them (P1-9, D66).
#[test]
fn a_converted_value_level_comparator_stays_on_the_tree() {
    let scratch = Scratch::new();
    convert("benchmarks/adam-adlb-metadata", &scratch.0);
    let path = scratch.0.join("benchmarks/adam-adlb-metadata/spec.yaml");
    let mut text = fs::read_to_string(&path).unwrap();
    for (from, to) in [
        (
            "where: {PARAMCD: [GLUC]}",
            "where: {PARAMCD: {comparator: NE, values: [GLUC]}}",
        ),
        (
            "where: {PARAMCD: [CREAT]}",
            "where: {PARAMCD: {comparator: NOTIN, values: [GLUC, HCG]}}",
        ),
    ] {
        assert_eq!(text.matches(from).count(), 1, "{from}");
        text = text.replace(from, to);
    }
    fs::write(&path, text).unwrap();
    let spec = versioned(&path).unwrap_or_else(|error| panic!("lowered: {error}"));
    let clauses = spec
        .columns
        .iter()
        .filter_map(|column| column.submission.as_ref())
        .flat_map(|submission| &submission.value_level)
        .map(|entry| {
            let clause = &entry.conditions[0];
            (
                entry.id.name.as_str(),
                clause.comparator,
                clause
                    .values
                    .iter()
                    .map(|value| value.text.as_str())
                    .collect::<Vec<_>>(),
                spec.cite(clause.values[0].meta),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        clauses,
        [
            (
                "GLUC",
                Comparator::Ne,
                vec!["GLUC"],
                "columns.AVAL.submission.value_level.GLUC.where.PARAMCD.values[0]".to_string()
            ),
            (
                "CREAT",
                Comparator::NotIn,
                vec!["GLUC", "HCG"],
                "columns.AVAL.submission.value_level.CREAT.where.PARAMCD.values[0]".to_string()
            ),
            (
                "HCG",
                Comparator::In,
                vec!["HCG"],
                "columns.AVALC.submission.value_level.HCG.where.PARAMCD[0]".to_string()
            ),
            (
                "NUMERIC",
                Comparator::In,
                vec!["GLUC", "CREAT"],
                "columns.AVALU.submission.value_level.NUMERIC.where.PARAMCD[0]".to_string()
            ),
        ]
    );
}
