use arrow_array::{ArrayRef, Int64Array, RecordBatch, StringArray};
use arrow_ipc::{reader::StreamReader, writer::StreamWriter};
use std::{
    io::{self, Cursor, Write},
    num::NonZeroUsize,
    sync::Arc,
};
use yamaa_adapters::{
    arrow_table::physical_schema,
    execute_dataset_with_budget,
    ipc_table::{
        dataset_batches, decode_snapshot, decode_snapshot_with_budget, encode_dataset_with_budget,
        snapshot_batches, write_dataset_with_budget, TableError,
    },
    ExecutionBudget,
};
use yamaa_core::{
    table::{Column, TableAccess, TableSchema},
    value::{ColumnType, Value},
};
use yamaa_engine::dataset::{
    Assignment, DatasetPlan, ExecutionError, Expression, RowMode, RowTemplate, SecondarySource,
};

fn schema(kind: ColumnType) -> TableSchema {
    TableSchema::new(vec![Column {
        name: "ID".into(),
        kind,
    }])
    .unwrap()
}

fn plan(source: TableSchema, secondary: usize, mode: RowMode) -> DatasetPlan {
    DatasetPlan::new_with_sources(
        source.clone(),
        (0..secondary)
            .map(|i| SecondarySource {
                name: format!("SECONDARY{i}"),
                schema: source.clone(),
            })
            .collect(),
        source,
        vec![RowTemplate {
            mode,
            assignments: vec![Assignment {
                column: 0,
                expression: Expression::Source(0),
                path: "columns.ID".into(),
            }],
            filter: None,
        }],
        vec![],
        vec![0],
        vec![],
    )
    .unwrap()
}

fn stream(schema: &TableSchema, chunks: impl IntoIterator<Item = ArrayRef>) -> Vec<u8> {
    let mut bytes = Vec::new();
    let physical = physical_schema(schema);
    let mut writer = StreamWriter::try_new(&mut bytes, &physical).unwrap();
    for chunk in chunks {
        writer
            .write(&RecordBatch::try_new(physical.clone(), vec![chunk]).unwrap())
            .unwrap();
    }
    writer.finish().unwrap();
    bytes
}

fn integers(chunks: &[&[i64]]) -> Vec<u8> {
    stream(
        &schema(ColumnType::Int),
        chunks
            .iter()
            .map(|chunk| Arc::new(Int64Array::from(chunk.to_vec())) as ArrayRef),
    )
}

fn read(bytes: &[u8]) -> Vec<RecordBatch> {
    StreamReader::try_new(Cursor::new(bytes), None)
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
}

#[test]
fn combined_quotas_are_configurable_and_reset_per_attempt() {
    let input = integers(&[&[1, 2]]);
    let donor = integers(&[&[3]]);
    let secondary = vec![donor.as_slice(); 8];
    let plan = plan(schema(ColumnType::Int), 8, RowMode::Records);
    let mut budget = ExecutionBudget::default();
    assert!(matches!(
        execute_dataset_with_budget(&plan, &input, &secondary, budget),
        Err(TableError::ShapeLimit)
    ));
    budget.max_sources = 9;
    budget.max_ipc_bytes = input.len() + donor.len() * 8;
    budget.execution.source_rows = 10;
    budget.max_decoded_cells = 10;
    budget.max_batches = 9;
    for _ in 0..2 {
        let result = execute_dataset_with_budget(&plan, &input, &secondary, budget)
            .unwrap()
            .result
            .unwrap();
        assert_eq!(
            result.dataset.rows(),
            &[vec![Value::Int(1)], vec![Value::Int(2)]]
        );
    }
    let mut insufficient = budget;
    insufficient.max_ipc_bytes -= 1;
    assert!(matches!(
        execute_dataset_with_budget(&plan, &input, &secondary, insufficient),
        Err(TableError::InputLimit)
    ));
    for insufficient in [
        ExecutionBudget {
            max_decoded_cells: 9,
            ..budget
        },
        ExecutionBudget {
            max_batches: 8,
            ..budget
        },
        ExecutionBudget {
            max_columns: 0,
            ..budget
        },
        ExecutionBudget {
            execution: yamaa_engine::dataset::Limits {
                source_rows: 9,
                ..budget.execution
            },
            ..budget
        },
    ] {
        assert!(matches!(
            execute_dataset_with_budget(&plan, &input, &secondary, insufficient),
            Err(TableError::ShapeLimit)
        ));
    }
    assert!(matches!(
        execute_dataset_with_budget(&plan, &input, &[], budget),
        Err(TableError::InvalidTable)
    ));
}

#[test]
fn all_source_shapes_are_admitted_before_any_payload_is_decoded() {
    let source = schema(ColumnType::Str);
    let mut input = stream(
        &source,
        [Arc::new(StringArray::from(vec!["INVALID_UTF8_SENTINEL"])) as ArrayRef],
    );
    let position = input
        .windows(21)
        .position(|bytes| bytes == b"INVALID_UTF8_SENTINEL")
        .unwrap();
    input[position] = 255;
    assert!(matches!(
        decode_snapshot_with_budget(&input, ExecutionBudget::default()),
        Err(TableError::InvalidStream)
    ));
    let donor = stream(
        &source,
        [Arc::new(StringArray::from(vec!["OK"])) as ArrayRef],
    );
    let budget = ExecutionBudget {
        max_decoded_cells: 1,
        ..ExecutionBudget::default()
    };
    assert!(matches!(
        execute_dataset_with_budget(
            &plan(source, 1, RowMode::Records),
            &input,
            &[&donor],
            budget
        ),
        Err(TableError::ShapeLimit)
    ));
}

#[test]
fn accepted_output_streams_in_order_and_byte_limits_include_all_frames() {
    let input = integers(&[&[1, 2], &[], &[3, 4, 5]]);
    let budget = ExecutionBudget {
        batch_rows: NonZeroUsize::new(2),
        ..ExecutionBudget::default()
    };
    let result = execute_dataset_with_budget(
        &plan(schema(ColumnType::Int), 0, RowMode::Records),
        &input,
        &[],
        budget,
    )
    .unwrap()
    .result
    .unwrap();
    let batches = dataset_batches(&result.dataset, budget)
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(
        batches
            .iter()
            .map(RecordBatch::num_rows)
            .collect::<Vec<_>>(),
        [2, 2, 1]
    );
    let mut bytes = Vec::new();
    write_dataset_with_budget(&result.dataset, &mut bytes, budget).unwrap();
    let decoded = read(&bytes);
    assert_eq!(decoded, batches);
    let values = decoded
        .iter()
        .flat_map(|batch| {
            batch
                .column(0)
                .as_any()
                .downcast_ref::<Int64Array>()
                .unwrap()
                .values()
                .iter()
                .copied()
        })
        .collect::<Vec<_>>();
    assert_eq!(values, [1, 2, 3, 4, 5]);
    assert_eq!(
        encode_dataset_with_budget(&result.dataset, budget).unwrap(),
        bytes
    );
    let exact = ExecutionBudget {
        max_output_bytes: bytes.len(),
        ..budget
    };
    assert_eq!(
        encode_dataset_with_budget(&result.dataset, exact).unwrap(),
        bytes
    );
    let insufficient = ExecutionBudget {
        max_output_bytes: bytes.len() - 1,
        ..budget
    };
    let mut partial = Vec::new();
    assert_eq!(
        write_dataset_with_budget(&result.dataset, &mut partial, insufficient),
        Err(TableError::OutputLimit)
    );
    assert!(partial.len() <= insufficient.max_output_bytes);
    assert!(matches!(
        dataset_batches(
            &result.dataset,
            ExecutionBudget {
                max_batches: 2,
                ..budget
            }
        ),
        Err(TableError::ShapeLimit)
    ));
    let single = encode_dataset_with_budget(
        &result.dataset,
        ExecutionBudget {
            batch_rows: None,
            ..budget
        },
    )
    .unwrap();
    assert_eq!(read(&single).len(), 1);

    struct FailingSink;
    impl Write for FailingSink {
        fn write(&mut self, _: &[u8]) -> io::Result<usize> {
            Err(io::Error::other("sink closed"))
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    assert_eq!(
        write_dataset_with_budget(&result.dataset, &mut FailingSink, budget),
        Err(TableError::OutputIo)
    );
}

#[test]
fn empty_outputs_keep_schema_and_failed_checks_expose_no_dataset() {
    let source = schema(ColumnType::Int);
    let empty = integers(&[]);
    let plan = plan(source.clone(), 0, RowMode::Records);
    let budget = ExecutionBudget {
        batch_rows: NonZeroUsize::new(1),
        ..ExecutionBudget::default()
    };
    let result = execute_dataset_with_budget(&plan, &empty, &[], budget)
        .unwrap()
        .result
        .unwrap();
    let encoded = encode_dataset_with_budget(&result.dataset, budget).unwrap();
    let batches = read(&encoded);
    assert_eq!(batches.len(), 1);
    assert_eq!(batches[0].num_rows(), 0);
    assert_eq!(batches[0].schema(), physical_schema(&source));
    let duplicates = integers(&[&[1, 1]]);
    let attempt = execute_dataset_with_budget(&plan, &duplicates, &[], budget).unwrap();
    assert!(matches!(
        *attempt.result.unwrap_err(),
        ExecutionError::KeyFailures(_)
    ));
}

#[test]
fn streamed_source_batches_preserve_empty_chunks_and_own_their_storage() {
    let input = integers(&[&[1, 2], &[], &[3]]);
    let mut batches = snapshot_batches(&input, ExecutionBudget::default()).unwrap();
    assert_eq!(batches.schema(), &schema(ColumnType::Int));
    let first = batches.next().unwrap().unwrap();
    let rest = batches.collect::<Result<Vec<_>, _>>().unwrap();
    assert_eq!(
        rest.iter().map(RecordBatch::num_rows).collect::<Vec<_>>(),
        [0, 1]
    );
    drop(input);
    assert_eq!(
        first
            .column(0)
            .as_any()
            .downcast_ref::<Int64Array>()
            .unwrap()
            .values()
            .as_ref(),
        [1, 2]
    );
}

/// Exercise real encoded/decoded records, grouping and COUNT, rather than just
/// asserting a configured limit.
#[test]
fn ten_million_records_execute_with_chunked_ipc() {
    let budget = ExecutionBudget::longitudinal();
    let source = schema(ColumnType::Int);
    let output = TableSchema::new(vec![
        Column {
            name: "ID".into(),
            kind: ColumnType::Int,
        },
        Column {
            name: "N".into(),
            kind: ColumnType::Int,
        },
    ])
    .unwrap();
    let plan = DatasetPlan::new(
        source.clone(),
        output,
        vec![RowTemplate {
            mode: RowMode::Groups(vec![0]),
            assignments: vec![
                Assignment {
                    column: 0,
                    expression: Expression::Source(0),
                    path: "columns.ID".into(),
                },
                Assignment {
                    column: 1,
                    expression: Expression::Count {
                        column: None,
                        text: "COUNT(SRC.*)".into(),
                    },
                    path: "columns.N".into(),
                },
            ],
            filter: None,
        }],
        vec![],
        vec![0],
        vec![],
    )
    .unwrap();
    let chunk_rows = budget.batch_rows.unwrap().get();
    let input = stream(
        &source,
        (0..10_000_000).step_by(chunk_rows).map(|start| {
            Arc::new(Int64Array::from(vec![
                7;
                chunk_rows.min(10_000_000 - start)
            ])) as ArrayRef
        }),
    );
    assert!(input.len() > yamaa_adapters::ipc_table::MAX_INPUT_BYTES);
    assert!(matches!(
        decode_snapshot(&input),
        Err(TableError::InputLimit)
    ));
    let rows = snapshot_batches(&input, budget)
        .unwrap()
        .map(|batch| batch.unwrap().num_rows())
        .sum::<usize>();
    assert_eq!(rows, 10_000_000);
    let attempt = execute_dataset_with_budget(&plan, &input, &[], budget).unwrap();
    let result = attempt.result.unwrap();
    assert_eq!(
        result.dataset.rows(),
        &[vec![Value::Int(7), Value::Int(10_000_000)]]
    );
    let encoded = encode_dataset_with_budget(&result.dataset, budget).unwrap();
    let table = decode_snapshot_with_budget(&encoded, budget).unwrap();
    assert_eq!(table.row_count(), 1);
    assert_eq!(
        table.cell(0, 1).unwrap(),
        yamaa_core::table::ValueRef::Int(10_000_000)
    );
}

#[test]
fn large_accepted_output_exceeds_legacy_row_and_byte_caps() {
    let budget = ExecutionBudget::longitudinal();
    let count = 1_100_000;
    let chunk_rows = budget.batch_rows.unwrap().get();
    let input = stream(
        &schema(ColumnType::Int),
        (0..count).step_by(chunk_rows).map(|start| {
            Arc::new(Int64Array::from_iter_values(
                (start..(start + chunk_rows).min(count)).map(|row| row as i64),
            )) as ArrayRef
        }),
    );
    let execution = execute_dataset_with_budget(
        &plan(schema(ColumnType::Int), 0, RowMode::Records),
        &input,
        &[],
        budget,
    )
    .unwrap()
    .result
    .unwrap();
    assert_eq!(execution.dataset.row_count(), count);
    assert!(matches!(
        encode_dataset_with_budget(&execution.dataset, ExecutionBudget::default()),
        Err(TableError::ShapeLimit)
    ));
    let bytes = encode_dataset_with_budget(&execution.dataset, budget).unwrap();
    assert!(bytes.len() > ExecutionBudget::default().max_output_bytes);
    let table = decode_snapshot_with_budget(&bytes, budget).unwrap();
    assert_eq!(table.row_count(), count);
    assert_eq!(
        table.cell(count - 1, 0).unwrap(),
        yamaa_core::table::ValueRef::Int(count as i64 - 1)
    );
    assert!(table
        .batches()
        .iter()
        .all(|batch| batch.num_rows() <= chunk_rows));
    assert_eq!(table.batches().len(), count.div_ceil(chunk_rows));
}

#[test]
fn streaming_decoder_reports_late_invalid_values_and_then_stops() {
    let source = schema(ColumnType::Str);
    let mut input = stream(
        &source,
        [
            Arc::new(StringArray::from(vec!["OK"])) as ArrayRef,
            Arc::new(StringArray::from(vec!["INVALID_UTF8_SENTINEL"])) as ArrayRef,
        ],
    );
    let position = input
        .windows(21)
        .position(|bytes| bytes == b"INVALID_UTF8_SENTINEL")
        .unwrap();
    input[position] = 255;
    let mut reader = snapshot_batches(&input, ExecutionBudget::default()).unwrap();
    assert_eq!(reader.next().unwrap().unwrap().num_rows(), 1);
    assert!(matches!(
        reader.next(),
        Some(Err(TableError::InvalidStream))
    ));
    assert!(reader.next().is_none());
    assert!(reader.next().is_none());
}
