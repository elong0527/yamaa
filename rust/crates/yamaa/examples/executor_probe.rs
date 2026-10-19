//! Executor performance probe for rewrite 1 (#2785): build one specification
//! over generated in-memory Arrow inputs and print the wall time of each
//! build, so the executor rewrite is measured against fixed numbers.
//!
//! It is a measurement, not a test: nothing runs it in CI or `cargo test`.
//! Measure an optimized build, with `make probe-executor` from the
//! repository root or from `rust/` with
//!
//! ```text
//! cargo run --release --locked -p yamaa --example executor_probe -- \
//!     [--rows N]... [--shape NAME]... [--limit SECONDS]
//! ```
//!
//! The defaults are 10,000, 100,000 and 1,000,000 rows, every shape, and a
//! limit of 300 seconds per case. Every shape derives the same columns from
//! one source: source reads, `cut`, `case`, `compute` and a `row_number`
//! window. The lookup shapes also read one column through one keyed
//! intermediate over a donor table holding one record per source row:
//!
//! - `none`: no lookup.
//! - `int-key`: `key: [ID]`, one `int` column.
//! - `composite-key`: `key: [SUBJ, SEQ]`, a `[str, int]` pair.
//! - `derived-correlated`: `key: [ID]` plus a derivation that reads the
//!   current row, filtered and ordered on that derived value, as in
//!   `benchmarks/schema-lookup-intermediate-correlated-derive`.
//!
//! Each case runs in its own process. A case that reaches the limit is
//! stopped, and the larger sizes of its shape are skipped. A case passes only
//! when the build reports no diagnostic, returns one row per source row, and
//! every looked-up value equals its donor record's, so the probe never times
//! an error path. Timings are evidence: record them on an issue, not in the
//! repository.

use std::io::Read;
use std::path::Path;
use std::process::{Command, ExitCode, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};

use arrow_array::{ArrayRef, RecordBatchIterator};
use arrow_schema::Schema;
use yamaa::{
    Array, Build, CancellationToken, Engine, EngineOptions, FFI_ArrowArrayStream, Field,
    Float64Array, HostLanguage, InputFrame, Int64Array, MemoryResolver, RecordBatch, Request,
    StringArray,
};

const USAGE: &str =
    "usage: executor_probe [--rows N]... [--shape NAME]... [--limit SECONDS]\nshapes: none, int-key, composite-key, derived-correlated";

/// Source rows per case unless `--rows` is given.
const SIZES: [usize; 3] = [10_000, 100_000, 1_000_000];

/// Seconds one case may run unless `--limit` is given.
const LIMIT_SECONDS: u64 = 300;

/// The columns every shape derives: source reads, `cut`, `case`, `compute`
/// and a `row_number` window.
const COLUMNS: &str = r#"columns:
  - {name: ID, type: int, derivation: SRC.ID}
  - {name: SUBJ, type: str, derivation: SRC.SUBJ}
  - {name: SEQ, type: int, derivation: SRC.SEQ}
  - {name: VAL, type: float, derivation: SRC.VAL}
  - {name: DAY, type: int, derivation: SRC.DAY}
  - name: BAND
    type: str
    derivation: {cut: {source: VAL, breaks: [25, 50, 75], labels: [LOW, MID, HIGH, TOP]}}
  - name: FLAG
    type: str
    derivation:
      case:
        - when: "DAY < 0"
          then: {literal: PRE}
        - when: "VAL >= 75"
          then: {literal: HIGH}
        - otherwise: {literal: NORMAL}
  - name: SCORE
    type: float
    derivation: {compute: {expr: "VAL * 2 + DAY / 10"}}
  - name: VALSEQ
    type: int
    derivation: {row_number: {window: {group_by: [SUBJ], order_by: [VAL]}}}
"#;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Shape {
    None,
    IntKey,
    CompositeKey,
    DerivedCorrelated,
}

impl Shape {
    const ALL: [Self; 4] = [
        Self::None,
        Self::IntKey,
        Self::CompositeKey,
        Self::DerivedCorrelated,
    ];

    fn name(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::IntKey => "int-key",
            Self::CompositeKey => "composite-key",
            Self::DerivedCorrelated => "derived-correlated",
        }
    }

    fn parse(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|shape| shape.name() == name)
    }

    /// The keyed intermediate the shape reads its `REF` column through.
    fn intermediate(self) -> Option<&'static str> {
        match self {
            Self::None => None,
            Self::IntKey => Some(
                r#"intermediates:
  - id: PICK
    dataset: LK
    key: [ID]
    columns: [REF]
"#,
            ),
            Self::CompositeKey => Some(
                r#"intermediates:
  - id: PICK
    dataset: LK
    key: [SUBJ, SEQ]
    columns: [REF]
"#,
            ),
            Self::DerivedCorrelated => Some(
                r#"intermediates:
  - id: PICK
    dataset: LK
    key: [ID]
    derivations:
      DIST: {compute: {expr: "ABS(DAY - SRC.DAY)"}}
    filter: "LK.DIST IS NOT NULL"
    order_by: [LK.DIST]
    keep: first
    columns: [REF]
    no_match: null
"#,
            ),
        }
    }
}

/// Assemble the specification for one shape.
fn specification(shape: Shape) -> String {
    let lookup = shape.intermediate();
    let mut text = String::from(
        "schema_version: '1.0'\ndomain: PROBE\nkeys: [ID]\ninput:\n  SRC: {path: src.csv, types: {ID: int, SEQ: int, VAL: float, DAY: int}}\n",
    );
    if lookup.is_some() {
        text.push_str(
            "  LK: {path: lk.csv, types: {ID: int, SEQ: int, DAY: int, REF: int}}\nbase: SRC\n",
        );
    }
    text.push_str(
        "output:\n  path: out.csv\n  columns: [ID, SUBJ, SEQ, VAL, DAY, BAND, FLAG, SCORE, VALSEQ",
    );
    text.push_str(if lookup.is_some() { ", REF]\n" } else { "]\n" });
    text.push_str(lookup.unwrap_or_default());
    text.push_str(COLUMNS);
    if lookup.is_some() {
        text.push_str("  - {name: REF, type: int, derivation: PICK.REF}\n");
    }
    text
}

/// The `REF` value the donor holds for source `ID`; every lookup must
/// return exactly this.
fn reference(id: i64) -> i64 {
    id * 7 + 3
}

/// Ten source rows per subject, so `[SUBJ, SEQ]` identifies a row.
fn subject(index: usize) -> String {
    format!("S{:06}", index / 10)
}

fn ids(indexes: impl Iterator<Item = usize>) -> ArrayRef {
    Arc::new(Int64Array::from_iter_values(indexes.map(|index| {
        i64::try_from(index).expect("probe sizes fit in i64") + 1
    })))
}

fn ints(indexes: impl Iterator<Item = usize>, value: fn(usize) -> i64) -> ArrayRef {
    Arc::new(Int64Array::from_iter_values(indexes.map(value)))
}

fn subjects(indexes: impl Iterator<Item = usize>) -> ArrayRef {
    Arc::new(StringArray::from_iter_values(indexes.map(subject)))
}

/// The source table: one row per index, in index order.
fn source(rows: usize) -> InputFrame {
    frame(vec![
        ("ID", ids(0..rows)),
        ("SUBJ", subjects(0..rows)),
        ("SEQ", ints(0..rows, |index| (index % 10) as i64 + 1)),
        (
            "VAL",
            Arc::new(Float64Array::from_iter_values(
                (0..rows).map(|index| ((index * 37) % 1000) as f64 / 10.0),
            )),
        ),
        (
            "DAY",
            ints(0..rows, |index| ((index * 13) % 400) as i64 - 20),
        ),
    ])
}

/// The donor table: one record per source row, in reverse order, so no match
/// can depend on position.
fn donor(rows: usize) -> InputFrame {
    frame(vec![
        ("ID", ids((0..rows).rev())),
        ("SUBJ", subjects((0..rows).rev())),
        (
            "SEQ",
            ints((0..rows).rev(), |index| (index % 10) as i64 + 1),
        ),
        (
            "DAY",
            ints((0..rows).rev(), |index| ((index * 7) % 400) as i64 - 20),
        ),
        (
            "REF",
            ints((0..rows).rev(), |index| reference(index as i64 + 1)),
        ),
    ])
}

fn frame(columns: Vec<(&str, ArrayRef)>) -> InputFrame {
    let fields: Vec<Field> = columns
        .iter()
        .map(|(name, array)| Field::new(*name, array.data_type().clone(), true))
        .collect();
    let schema = Arc::new(Schema::new(fields));
    let arrays = columns.into_iter().map(|(_, array)| array).collect();
    let batch = RecordBatch::try_new(Arc::clone(&schema), arrays).expect("columns share a length");
    let stream =
        FFI_ArrowArrayStream::new(Box::new(RecordBatchIterator::new(vec![Ok(batch)], schema)));
    InputFrame::import(stream).expect("generated frames import")
}

/// Build one case from memory and return the wall time of the build alone.
fn run_case(shape: Shape, rows: usize) -> Result<Duration, String> {
    let resolver = Arc::new(MemoryResolver::new());
    let text = specification(shape);
    resolver.insert("/", "spec.yaml", "spec:1", Arc::from(text.as_bytes()));
    resolver.insert_frame("/spec.yaml", "src.csv", "src:1", &source(rows));
    if shape != Shape::None {
        resolver.insert_frame("/spec.yaml", "lk.csv", "lk:1", &donor(rows));
    }
    let engine = Engine::new(
        EngineOptions::new(HostLanguage::Python, "executor-probe".into(), "0".into())
            .with_resolver(resolver),
    );
    let token = CancellationToken::new();
    let started = Instant::now();
    let build = engine
        .build(&Request::new("spec.yaml"), &token)
        .map_err(|error| error.to_string())?;
    let elapsed = started.elapsed();
    validate(&build, shape, rows)?;
    Ok(elapsed)
}

/// Require a clean build with one row per source row and, for the lookup
/// shapes, the donor's `REF` on every row.
fn validate(build: &Build, shape: Shape, rows: usize) -> Result<(), String> {
    let report = build.report();
    if let Some(first) = report.diagnostics.first() {
        return Err(format!(
            "{} diagnostic(s), first {} {} at {:?}",
            report.len(),
            first.phase,
            first.code,
            first.spec_path
        ));
    }
    let output = build.output().ok_or("the build kept no output table")?;
    if output.rows() != rows {
        return Err(format!("{} output rows", output.rows()));
    }
    let mut seen = vec![false; rows];
    for batch in output.batches() {
        let identifiers = int_column(batch, "ID")?;
        let looked_up = match shape {
            Shape::None => None,
            _ => Some(int_column(batch, "REF")?),
        };
        for row in 0..batch.num_rows() {
            if identifiers.is_null(row) {
                return Err("an output row has no ID".into());
            }
            let id = identifiers.value(row);
            let index = usize::try_from(id - 1)
                .ok()
                .filter(|index| *index < rows)
                .ok_or_else(|| format!("unexpected ID {id}"))?;
            if std::mem::replace(&mut seen[index], true) {
                return Err(format!("ID {id} appears twice"));
            }
            if let Some(looked_up) = looked_up {
                if looked_up.is_null(row) || looked_up.value(row) != reference(id) {
                    return Err(format!("ID {id} read the wrong REF"));
                }
            }
        }
    }
    Ok(())
}

fn int_column<'a>(batch: &'a RecordBatch, name: &str) -> Result<&'a Int64Array, String> {
    let index = batch
        .schema()
        .index_of(name)
        .map_err(|_| format!("no output column {name}"))?;
    batch
        .column(index)
        .as_any()
        .downcast_ref::<Int64Array>()
        .ok_or_else(|| format!("output column {name} is not Int64"))
}

struct Options {
    rows: Vec<usize>,
    shapes: Vec<Shape>,
    limit: Duration,
}

enum Mode {
    Probe(Options),
    Case(Shape, usize),
    Help,
}

fn parse(arguments: &[String]) -> Result<Mode, String> {
    let shape = |name: &str| Shape::parse(name).ok_or_else(|| format!("unknown shape {name}"));
    let count = |text: &str| {
        text.replace('_', "")
            .parse::<usize>()
            .map_err(|_| format!("not a count: {text}"))
    };
    if let [flag, name, rows] = arguments {
        if flag == "--case" {
            return Ok(Mode::Case(shape(name)?, count(rows)?));
        }
    }
    let mut rows = Vec::new();
    let mut shapes = Vec::new();
    let mut limit = LIMIT_SECONDS;
    let mut remaining = arguments.iter();
    while let Some(flag) = remaining.next() {
        if flag == "--help" || flag == "-h" {
            return Ok(Mode::Help);
        }
        let value = remaining
            .next()
            .ok_or_else(|| format!("{flag} needs a value"))?;
        match flag.as_str() {
            "--rows" => rows.push(count(value)?),
            "--shape" => {
                let shape = shape(value)?;
                if !shapes.contains(&shape) {
                    shapes.push(shape);
                }
            }
            "--limit" => {
                limit = value
                    .parse()
                    .map_err(|_| format!("not a number of seconds: {value}"))?;
            }
            _ => return Err(format!("unknown argument {flag}")),
        }
    }
    if rows.is_empty() {
        rows = SIZES.to_vec();
    }
    // Ascending sizes, so a stopped case can skip the larger ones.
    rows.sort_unstable();
    rows.dedup();
    if shapes.is_empty() {
        shapes = Shape::ALL.to_vec();
    }
    Ok(Mode::Probe(Options {
        rows,
        shapes,
        limit: Duration::from_secs(limit),
    }))
}

/// Run one case in a child process; `Ok(None)` when it reached the limit.
fn child(
    program: &Path,
    shape: Shape,
    rows: usize,
    limit: Duration,
) -> Result<Option<String>, String> {
    let mut process = Command::new(program)
        .args(["--case", shape.name(), &rows.to_string()])
        .stdout(Stdio::piped())
        .spawn()
        .map_err(|error| error.to_string())?;
    let started = Instant::now();
    let status = loop {
        if let Some(status) = process.try_wait().map_err(|error| error.to_string())? {
            break status;
        }
        if started.elapsed() >= limit {
            process.kill().map_err(|error| error.to_string())?;
            process.wait().map_err(|error| error.to_string())?;
            return Ok(None);
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    let mut seconds = String::new();
    if let Some(mut stdout) = process.stdout.take() {
        stdout
            .read_to_string(&mut seconds)
            .map_err(|error| error.to_string())?;
    }
    if status.success() {
        Ok(Some(seconds.trim().to_owned()))
    } else {
        Err(format!("the case exited with {status}"))
    }
}

/// Group digits by thousands, as the issue tables do.
fn grouped(value: usize) -> String {
    let digits = value.to_string();
    let mut text = String::new();
    for (position, digit) in digits.chars().enumerate() {
        if position > 0 && (digits.len() - position) % 3 == 0 {
            text.push(',');
        }
        text.push(digit);
    }
    text
}

fn probe(options: &Options) -> ExitCode {
    let program = match std::env::current_exe() {
        Ok(program) => program,
        Err(error) => {
            eprintln!("cannot locate the probe executable: {error}");
            return ExitCode::FAILURE;
        }
    };
    let build = if cfg!(debug_assertions) {
        "debug build, timings are not comparable"
    } else {
        "optimized build"
    };
    let cpus = std::thread::available_parallelism().map_or(1, |count| count.get());
    println!(
        "executor probe: {build}, {cpus} logical CPUs, limit {} s per case",
        options.limit.as_secs()
    );
    println!("{:<20}{:>11}  build", "shape", "rows");
    let mut failed = false;
    for &shape in &options.shapes {
        let mut stopped = None;
        for &rows in &options.rows {
            let outcome = match stopped {
                Some(smaller) => format!("skipped: {} rows were stopped", grouped(smaller)),
                None => match child(&program, shape, rows, options.limit) {
                    Ok(Some(seconds)) => format!("{seconds} s"),
                    Ok(None) => {
                        stopped = Some(rows);
                        format!("stopped after {} s", options.limit.as_secs())
                    }
                    Err(message) => {
                        failed = true;
                        format!("failed: {message}")
                    }
                },
            };
            println!("{:<20}{:>11}  {outcome}", shape.name(), grouped(rows));
        }
    }
    if failed {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

fn main() -> ExitCode {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    match parse(&arguments) {
        Ok(Mode::Probe(options)) => probe(&options),
        Ok(Mode::Case(shape, rows)) => match run_case(shape, rows) {
            Ok(elapsed) => {
                println!("{:.3}", elapsed.as_secs_f64());
                ExitCode::SUCCESS
            }
            Err(message) => {
                eprintln!("{} at {} rows: {message}", shape.name(), grouped(rows));
                ExitCode::FAILURE
            }
        },
        Ok(Mode::Help) => {
            println!("{USAGE}");
            ExitCode::SUCCESS
        }
        Err(message) => {
            eprintln!("{message}\n{USAGE}");
            ExitCode::from(2)
        }
    }
}
