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
//!   single-layer specification (P-HND-3);
//! - `model` and the header, and the context that tells `compute` text from
//!   expression-field text (FORMULA-27);
//! - join keys, which P1-6 declares: an input's `key`, and the key a 0.5
//!   lookup infers against the one its conversion declares (D23, D63);
//! - a written `minimum_source_precision` that the month and day rules
//!   already imply (D61).
//!
//! Every specification whose trees still differ is listed in [`EXPECTED`]
//! with the reason, and the test fails when the list is wrong in either
//! direction. Each 1.0 tree is also checked to be total and located. The
//! converter runs through `uv`, as `make setup` installs it.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use yamaa_adapters::document::{self, Closures, Decoded};
use yamaa_adapters::shipped_schema;
use yamaa_adapters::specification_source::{InheritancePort, Source};
use yamaa_adapters::yaml_decode::decode_yaml;
use yamaa_core::ast::{
    Constant, ExprKind, JoinKey, KeyDecl, Literal, Notation, Precision, ScalarOp, Spec, Step,
};
use yamaa_core::expression::Context;
use yamaa_core::front_end::v0_5::lower;
use yamaa_core::inheritance::{Source as GraphSource, SourceError};
use yamaa_core::language::{FileType, LANGUAGE_VERSION};
use yamaa_core::schema::{Document, DocumentNode as N};

/// Each specification whose trees differ, with the reason.
const EXPECTED: &[(&str, &str)] = &[
    (
        "negative-derivation-call-form/spec.yaml",
        "D18: the bare string `to_date(DM.BRTHDTC)` is expression text in 1.0, a call \
         outside the closed function vocabulary whose formula holds \
         `prohibited_function` (FORMULA-27) for binding to report; 0.5 refuses the \
         same string at validation as a variable that does not match its pattern, \
         so only 0.5 refuses it here",
    ),
    (
        "negative-impute-missing-month/spec.yaml",
        "D61: `minimum_source_precision: year` asks for a month the rules do not \
         supply; the field and this negative retire",
    ),
    (
        "negative-impute-unreachable-month/spec.yaml",
        "D61: `minimum_source_precision: month` beside a `month` rule; the field and \
         this negative retire",
    ),
    (
        "negative-lookup-rename-intermediate/spec.yaml",
        "P1-6: the converter declares the key of the intermediate CODED but not the \
         key of its input CODING, so the 1.0 tree cannot tell that CODED only renames \
         CODING (LOOKUP-4, D23) and prunes it unread, where 0.5 keeps it to refuse it; \
         P1-6 writes the input's key",
    ),
    (
        "sdtm-tr-tumor-measurements/spec.yaml",
        "D18: `compute: {expr: TRORRES_N}` holds only a name, which 1.0 text reads as \
         a reference (FORMULA-9, EXPR-4); TRORRES_N is a float column, so the value \
         is the same",
    ),
];

fn repository() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

static NEXT: AtomicUsize = AtomicUsize::new(0);

/// A temporary directory, removed when dropped.
struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "yamaa-front-end-comparison-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Convert every benchmark into `output/benchmarks` as
/// `python -m tools.migrate` does.
fn convert(output: &Path) {
    let result = Command::new("uv")
        .args(["run", "--project", "python", "--no-sync", "python"])
        .args([
            "-m",
            "tools.migrate",
            "benchmarks",
            "--base",
            ".",
            "--output",
        ])
        .arg(output)
        .current_dir(repository())
        .output()
        .unwrap_or_else(|error| {
            panic!("the comparison runs tools/migrate through uv ({error}); run `make setup`")
        });
    assert!(
        result.status.success(),
        "tools/migrate failed:\n{}{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
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

/// The 0.5 front end's tree for an original specification, or why it is
/// refused before lowering.
fn legacy(path: &Path) -> Result<Spec, String> {
    let (source, identity) = source(path);
    let document = shipped_schema::prepare(source, identity, &mut Files)
        .map_err(|error| refusal(&format!("{error:?}")))?;
    Ok(lower(document.model(), None).into_spec())
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
    InputKey,
    LookupKey,
    MinimumPrecision,
}

impl Normalized {
    fn label(self) -> &'static str {
        match self {
            Self::RestatedDefault => "a handler restating its default (P-HND-3)",
            Self::InputKey => "an input's declared key (P1-6)",
            Self::LookupKey => "a lookup key 0.5 infers and 1.0 declares (P1-6)",
            Self::MinimumPrecision => "minimum_source_precision the rules imply (D61)",
        }
    }
}

/// Leave out of a tree what the comparison does not cover. `inferred`
/// names the intermediates whose key the 0.5 tree infers.
fn normalize(spec: &mut Spec, inferred: &BTreeSet<String>, out: &mut BTreeSet<Normalized>) {
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
    for input in &mut spec.inputs {
        if matches!(input.key, KeyDecl::Declared(_)) {
            left_out.insert(Normalized::InputKey);
        }
        input.key = KeyDecl::Inferred;
    }
    for item in &mut spec.intermediates {
        if inferred.contains(&item.id.name) {
            if !matches!(item.key, Some(JoinKey::Applicable)) {
                left_out.insert(Normalized::LookupKey);
            }
            item.key = Some(JoinKey::Applicable);
        }
        if restated(&mut item.absent) {
            left_out.insert(Normalized::RestatedDefault);
        }
    }
    for expr in &mut spec.exprs {
        let restated_default = match &mut expr.kind {
            ExprKind::Source(source) => restated(&mut source.absent),
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
            // A keyless qualified aggregate infers its key in 0.5 and matches
            // on its input's declared key in 1.0 (P-KEY-1).
            ExprKind::Aggregate(aggregate) => {
                if matches!(aggregate.key, Some(JoinKey::FromInput)) {
                    aggregate.key = Some(JoinKey::Applicable);
                    left_out.insert(Normalized::LookupKey);
                }
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
    convert(&scratch.0);
    let benchmarks = repository().join("benchmarks");
    let mut cases = fs::read_dir(&benchmarks)
        .unwrap()
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|case| case.join("benchmark.yaml").is_file())
        .collect::<Vec<_>>();
    cases.sort();
    let mut compared = 0;
    let mut refused = Vec::new();
    let mut differing = BTreeMap::new();
    let mut tally = BTreeMap::<Normalized, usize>::new();
    let mut failures = Vec::new();
    for case in &cases {
        let name = case.file_name().unwrap().to_string_lossy().into_owned();
        for entry in entries(case) {
            let label = format!("{name}/{entry}");
            let converted = scratch.0.join("benchmarks").join(&name).join(&entry);
            match (legacy(&case.join(&entry)), versioned(&converted)) {
                (Ok(mut old), Ok(mut new)) => {
                    compared += 1;
                    check_tree(&label, &new, &mut failures);
                    let inferred = old
                        .intermediates
                        .iter()
                        .filter(|item| matches!(item.key, Some(JoinKey::Applicable)))
                        .map(|item| item.id.name.clone())
                        .collect::<BTreeSet<_>>();
                    let mut normalized = BTreeSet::new();
                    normalize(&mut old, &inferred, &mut normalized);
                    normalize(&mut new, &inferred, &mut normalized);
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
        "compared {compared} specifications; both front ends refuse {}",
        refused.len()
    );
    for (normalized, count) in &tally {
        println!("  {count:4} with {}", normalized.label());
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
        .filter(|label| !differing.contains_key(*label))
        .cloned()
        .collect::<Vec<_>>();
    assert!(
        unexpected.is_empty() && settled.is_empty(),
        "differing specifications not listed:\n{}\nlisted specifications that no longer differ:\n{}",
        unexpected.join("\n"),
        settled.join("\n")
    );
    assert!(compared > 400, "only {compared} specifications compared");
}
