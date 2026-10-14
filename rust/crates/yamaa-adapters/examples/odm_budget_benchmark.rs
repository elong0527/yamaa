//! Reproducible synthetic ODM timing for the budget-aware Rust dataset APIs.
//! Measures in-memory canonical IPC, not XML/CSV ingestion or a hosted workflow.
use arrow_array::{builder::StringBuilder, ArrayRef, RecordBatch};
use arrow_ipc::writer::StreamWriter;
use serde_json::json;
use std::{
    error::Error,
    io::{self, Write},
    sync::Arc,
    time::Instant,
};
use yamaa_adapters::{
    arrow_table::physical_schema,
    ipc_table::{decode_snapshot_with_budget, write_dataset_with_budget},
    ExecutionBudget,
};
use yamaa_core::{
    odm_source::FIELDS,
    table::{Column, TableAccess, TableSchema, ValueRef},
    value::ColumnType,
};
use yamaa_engine::dataset::{
    AggregatePhase, Assignment, BoundOdm, Check, DatasetExecution, DatasetPlan, Expression,
    OdmRead, OdmScope, RowMode, RowTemplate, Verification,
};

struct Settings {
    subjects: usize,
    items: usize,
    runs: usize,
}

fn settings() -> Result<Settings, Box<dyn Error>> {
    let mut settings = Settings {
        subjects: 10_000,
        items: 1_000,
        runs: 3,
    };
    let mut args = std::env::args().skip(1);
    while let Some(option) = args.next() {
        let value = args.next().ok_or("expected an option value")?.parse()?;
        match option.as_str() {
            "--subjects" => settings.subjects = value,
            "--items-per-subject" => settings.items = value,
            "--runs" => settings.runs = value,
            _ => return Err(format!("unknown option: {option}").into()),
        }
    }
    if settings.subjects == 0 || settings.items < 2 || settings.runs == 0 {
        return Err(
            "subjects/runs must be positive and items-per-subject must be at least two".into(),
        );
    }
    Ok(settings)
}

fn assignment(column: usize, name: &str, expression: Expression) -> Assignment {
    Assignment {
        column,
        path: format!("columns.{name}"),
        expression,
    }
}

fn odm(item: &str) -> Expression {
    Expression::Odm(Box::new(
        BoundOdm::new(OdmRead {
            dataset: "ODM".into(),
            item: format!("ODM.{item}"),
            item_oid: item.into(),
            path: format!("columns.{item}.odm"),
            row: None,
            phase: AggregatePhase::Derivation,
            scope: OdmScope::Keys,
            source: None,
            selectors: [None, None, None],
            filter: None,
        })
        .expect("fixed benchmark item read"),
    ))
}

fn plan(source: TableSchema, subjects: usize) -> DatasetPlan {
    let output = TableSchema::new(vec![
        Column {
            name: "STUDYID".into(),
            kind: ColumnType::Str,
        },
        Column {
            name: "USUBJID".into(),
            kind: ColumnType::Str,
        },
        Column {
            name: "AGE".into(),
            kind: ColumnType::Int,
        },
        Column {
            name: "SEX".into(),
            kind: ColumnType::Str,
        },
    ])
    .expect("fixed output schema");
    DatasetPlan::new(
        source,
        output,
        vec![RowTemplate {
            mode: RowMode::Keys,
            assignments: vec![
                assignment(0, "STUDYID", Expression::Source(0)),
                assignment(1, "USUBJID", Expression::Source(2)),
            ],
            filter: None,
        }],
        vec![
            assignment(2, "AGE", odm("IT.DM.AGE")),
            assignment(3, "SEX", odm("IT.DM.SEX")),
        ],
        vec![0, 1],
        vec![Verification {
            path: "verifications.row_count".into(),
            check: Check::RowCount {
                min: Some(subjects as i64),
                max: Some(subjects as i64),
            },
        }],
    )
    .expect("fixed benchmark plan")
}

fn source_ipc(
    schema: &TableSchema,
    settings: &Settings,
    batch_rows: usize,
) -> Result<Vec<u8>, Box<dyn Error>> {
    let rows = settings
        .subjects
        .checked_mul(settings.items)
        .ok_or("row count overflow")?;
    let subjects = (0..settings.subjects)
        .map(|index| format!("P{index:06}"))
        .collect::<Vec<_>>();
    let ages = (0..settings.subjects)
        .map(|index| (40 + index % 30).to_string())
        .collect::<Vec<_>>();
    let items = (0..settings.items)
        .map(|index| match index {
            0 => "IT.DM.AGE".into(),
            1 => "IT.DM.SEX".into(),
            _ => format!("IT.OTHER.{index:04}"),
        })
        .collect::<Vec<_>>();
    let physical = physical_schema(schema);
    let mut bytes = Vec::with_capacity(rows.checked_mul(90).ok_or("byte estimate overflow")?);
    let mut writer = StreamWriter::try_new(&mut bytes, &physical)?;
    for start in (0..rows).step_by(batch_rows) {
        let count = batch_rows.min(rows - start);
        let mut builders = (0..FIELDS.len())
            .map(|_| StringBuilder::with_capacity(count, count * 16))
            .collect::<Vec<_>>();
        for row in start..start + count {
            let subject = row / settings.items;
            let item = row % settings.items;
            let value = match item {
                0 => ages[subject].as_str(),
                1 if subject.is_multiple_of(2) => "F",
                1 => "M",
                _ => "0",
            };
            let values = [
                Some("STUDY"),
                Some("MDV"),
                Some(subjects[subject].as_str()),
                Some("VISIT"),
                None,
                Some("FORM"),
                None,
                Some("IG"),
                None,
                Some(items[item].as_str()),
                Some(value),
            ];
            for (builder, value) in builders.iter_mut().zip(values) {
                builder.append_option(value);
            }
        }
        let arrays = builders
            .into_iter()
            .map(|mut builder| Arc::new(builder.finish()) as ArrayRef)
            .collect();
        writer.write(&RecordBatch::try_new(physical.clone(), arrays)?)?;
    }
    writer.finish()?;
    Ok(bytes)
}

#[derive(Default)]
struct ByteCount(usize);
impl Write for ByteCount {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0 = self
            .0
            .checked_add(bytes.len())
            .ok_or_else(|| io::Error::other("byte count overflow"))?;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn peak_rss_bytes() -> Option<usize> {
    std::fs::read_to_string("/proc/self/status")
        .ok()?
        .lines()
        .find(|line| line.starts_with("VmHWM:"))?
        .split_whitespace()
        .nth(1)?
        .parse::<usize>()
        .ok()?
        .checked_mul(1024)
}

fn main() -> Result<(), Box<dyn Error>> {
    let settings = settings()?;
    let rows = settings
        .subjects
        .checked_mul(settings.items)
        .ok_or("row count overflow")?;
    let budget = ExecutionBudget::longitudinal();
    if rows > budget.execution.source_rows {
        return Err("source exceeds the longitudinal row quota".into());
    }
    let schema = TableSchema::new(
        FIELDS
            .iter()
            .map(|name| Column {
                name: (*name).into(),
                kind: ColumnType::Str,
            })
            .collect(),
    )
    .expect("fixed ODM schema");
    let plan = plan(schema.clone(), settings.subjects);
    eprintln!(
        "Generating {rows} ODM rows for {} subjects",
        settings.subjects
    );
    let generated = Instant::now();
    let input = source_ipc(
        &schema,
        &settings,
        budget.batch_rows.expect("longitudinal chunk size").get(),
    )?;
    let generation_seconds = generated.elapsed().as_secs_f64();
    let mut observations = Vec::new();
    let mut totals = Vec::new();
    for run in 1..=settings.runs {
        let started = Instant::now();
        let source = decode_snapshot_with_budget(&input, budget)?;
        let decode_seconds = started.elapsed().as_secs_f64();
        let derived = Instant::now();
        let execution = plan
            .execute_with_budget(&source, &[], budget)
            .result
            .map_err(|error| format!("ODM execution failed: {error:?}"))?;
        let execution_seconds = derived.elapsed().as_secs_f64();
        let exported = Instant::now();
        let mut output = ByteCount::default();
        write_dataset_with_budget(&execution.dataset, &mut output, budget)?;
        let export_seconds = exported.elapsed().as_secs_f64();
        let total_seconds = started.elapsed().as_secs_f64();
        // Independent expected subject/value checks are outside the timed region.
        assert_eq!(execution.dataset.row_count(), settings.subjects);
        for subject in 0..settings.subjects {
            assert_eq!(
                execution
                    .dataset
                    .cell(subject, 0)
                    .expect("checked output cell"),
                ValueRef::Str("STUDY")
            );
            assert_eq!(
                execution
                    .dataset
                    .cell(subject, 1)
                    .expect("checked output cell"),
                ValueRef::Str(&format!("P{subject:06}"))
            );
            assert_eq!(
                execution
                    .dataset
                    .cell(subject, 2)
                    .expect("checked output cell"),
                ValueRef::Int((40 + subject % 30) as i64)
            );
            assert_eq!(
                execution
                    .dataset
                    .cell(subject, 3)
                    .expect("checked output cell"),
                ValueRef::Str(if subject.is_multiple_of(2) { "F" } else { "M" })
            );
        }
        eprintln!(
            "Run {run}: {total_seconds:.3}s ({:.0} input rows/s)",
            rows as f64 / total_seconds
        );
        totals.push(total_seconds);
        observations.push(json!({"run":run, "decode_seconds":decode_seconds, "execution_seconds":execution_seconds,
            "export_seconds":export_seconds, "total_seconds":total_seconds, "input_rows_per_second":rows as f64 / total_seconds,
            "output_rows":execution.dataset.row_count(), "output_ipc_bytes":output.0}));
    }
    totals.sort_by(f64::total_cmp);
    let median = if totals.len().is_multiple_of(2) {
        (totals[totals.len() / 2 - 1] + totals[totals.len() / 2]) / 2.0
    } else {
        totals[totals.len() / 2]
    };
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "protocol":"dataset-odm-budget-benchmark/1", "profile":"longitudinal", "scope":"keys",
            "source_rows":rows, "source_columns":FIELDS.len(), "subjects":settings.subjects, "items_per_subject":settings.items,
            "derived_items":["IT.DM.AGE", "IT.DM.SEX"], "batch_rows":budget.batch_rows.expect("chunk size").get(),
            "input_ipc_bytes":input.len(), "generation_seconds":generation_seconds,
            "timed_scope":"canonical IPC decode, typed key-grain ODM derivation/conversion, output-key/row-count checks, IPC export to byte-count sink",
            "excluded":["compilation", "input generation", "XML/CSV ingestion", "specification compilation", "host dispatch", "disk/network publication", "post-run expected-value checks"],
            "median_total_seconds":median, "median_input_rows_per_second":rows as f64 / median,
            "process_peak_rss_bytes":peak_rss_bytes(), "debug_assertions":cfg!(debug_assertions), "observations":observations,
        }))?
    );
    Ok(())
}
