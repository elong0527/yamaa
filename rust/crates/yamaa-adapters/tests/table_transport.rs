use arrow_array::{
    Array, Date32Array, Int64Array, RecordBatch, RecordBatchOptions, StringArray, StructArray,
    UInt8Array,
};
use arrow_ipc::{reader::StreamReader, writer::StreamWriter};
use arrow_schema::{DataType, Field, Schema};
use std::{io::Cursor, sync::Arc};
use yamaa_adapters::table_transport::{
    table_round_trip, table_snapshot, TableTransportError as E, MAX_INPUT_BYTES,
};

/// Replay independent truth using PyArrow-authored input, not candidate output.
#[test]
fn shared_installed_table_truth_and_repeated_ownership() {
    let expected: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/tables/expected.json")).unwrap();
    for (name, input) in [
        (
            "mixed",
            include_bytes!("fixtures/tables/mixed.arrow").as_slice(),
        ),
        (
            "empty",
            include_bytes!("fixtures/tables/empty.arrow").as_slice(),
        ),
        (
            "schema_only",
            include_bytes!("fixtures/tables/schema_only.arrow").as_slice(),
        ),
        (
            "zero_columns",
            include_bytes!("fixtures/tables/zero_columns.arrow").as_slice(),
        ),
    ] {
        let before: serde_json::Value =
            serde_json::from_str(&table_snapshot(input).unwrap()).unwrap();
        assert_eq!(before, expected[name], "{name}");
        let mut owned = table_round_trip(input).unwrap();
        for _ in 0..5 {
            assert_eq!(
                serde_json::from_str::<serde_json::Value>(&table_snapshot(&owned).unwrap())
                    .unwrap(),
                expected[name],
                "{name}"
            );
            owned = table_round_trip(&owned).unwrap();
        }
    }
}

/// Reject truncation and arbitrary framing without a panic/internal outcome.
#[test]
fn every_truncation_and_bad_frame_is_rejected_before_use() {
    let input = include_bytes!("fixtures/tables/mixed.arrow");
    for end in 0..input.len() {
        assert!(
            matches!(
                table_snapshot(&input[..end]),
                Err(E::InvalidStream | E::InvalidTable | E::ShapeLimit)
            ),
            "prefix {end}"
        );
    }
    for input in [
        vec![],
        vec![255; 8],
        vec![0; 8],
        vec![1, 0, 0, 128],
        b"ARROW1".to_vec(),
    ] {
        assert!(matches!(
            table_round_trip(&input),
            Err(E::InvalidStream | E::ShapeLimit)
        ));
    }
    let mut trailing = input.to_vec();
    trailing.push(0);
    assert_eq!(table_snapshot(&trailing), Err(E::InvalidStream));
    assert_eq!(
        table_snapshot(&vec![0; MAX_INPUT_BYTES + 1]),
        Err(E::InputLimit)
    );
    assert!(table_snapshot(input).is_ok());
}

/// Encode a safe in-memory batch for deliberately unsupported/oversized requests.
fn stream(batch: &RecordBatch) -> Vec<u8> {
    let mut output = vec![];
    let mut writer = StreamWriter::try_new(&mut output, &batch.schema()).unwrap();
    writer.write(batch).unwrap();
    writer.finish().unwrap();
    output
}

/// Read normalized public output through the independent Arrow decoder.
fn read(bytes: Vec<u8>) -> Vec<RecordBatch> {
    StreamReader::try_new(Cursor::new(bytes), None)
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap()
}

/// Null exports must erase hidden scalar bytes, temporal children and text data.
#[test]
fn public_export_sanitizes_hidden_payloads() {
    use arrow_buffer::{NullBuffer, ScalarBuffer};
    use yamaa_adapters::arrow_table::physical_schema;
    use yamaa_core::{
        table::{Column, TableSchema},
        value::ColumnType as CT,
    };
    let schema = TableSchema::new(vec![
        Column {
            name: "I".into(),
            kind: CT::Int,
        },
        Column {
            name: "S".into(),
            kind: CT::Str,
        },
        Column {
            name: "D".into(),
            kind: CT::Date,
        },
    ])
    .unwrap();
    let hidden = Int64Array::new(
        ScalarBuffer::from(vec![424242, 7]),
        Some(NullBuffer::from(vec![false, true])),
    );
    let strings = StringArray::new(
        arrow_buffer::OffsetBuffer::new(ScalarBuffer::from(vec![0, 6, 8])),
        arrow_buffer::Buffer::from(b"SECRETok".as_slice()),
        Some(NullBuffer::from(vec![false, true])),
    );
    let physical = physical_schema(&schema);
    let DataType::Struct(fields) = physical.field(2).data_type() else {
        panic!("struct")
    };
    let dates = StructArray::try_new(
        fields.clone(),
        vec![
            Arc::new(Date32Array::from(vec![i32::MIN, 0])),
            Arc::new(UInt8Array::from(vec![255, 2])),
        ],
        Some(NullBuffer::from(vec![false, true])),
    )
    .unwrap();
    let batch = RecordBatch::try_new(
        physical,
        vec![Arc::new(hidden), Arc::new(strings), Arc::new(dates)],
    )
    .unwrap();
    let output = table_round_trip(&stream(&batch)).unwrap();
    assert!(!output.windows(6).any(|bytes| bytes == b"SECRET"));
    let batches = read(output);
    let batch = &batches[0];
    assert_eq!(
        batch
            .column(0)
            .as_any()
            .downcast_ref::<Int64Array>()
            .unwrap()
            .values()
            .as_ref(),
        [0, 7]
    );
    let date = batch
        .column(2)
        .as_any()
        .downcast_ref::<StructArray>()
        .unwrap();
    assert_eq!(
        date.column(0)
            .as_any()
            .downcast_ref::<Date32Array>()
            .unwrap()
            .value(0),
        0
    );
    assert_eq!(
        date.column(1)
            .as_any()
            .downcast_ref::<UInt8Array>()
            .unwrap()
            .value(0),
        0
    );
    assert!(date.is_null(0));
}

/// Oversized declared rows and noncanonical types never enter evaluation.
#[test]
fn shape_and_schema_policy_are_enforced() {
    let empty = Arc::new(Schema::empty());
    let batch = RecordBatch::try_new_with_options(
        empty,
        vec![],
        &RecordBatchOptions::new().with_row_count(Some(65_537)),
    )
    .unwrap();
    assert_eq!(table_snapshot(&stream(&batch)), Err(E::ShapeLimit));
    let schema = Arc::new(Schema::new(vec![Field::new("I", DataType::Int64, false)]));
    let batch = RecordBatch::try_new(schema, vec![Arc::new(Int64Array::from(vec![1]))]).unwrap();
    assert_eq!(table_snapshot(&stream(&batch)), Err(E::InvalidTable));
    let fields = (0..65)
        .map(|i| Field::new(format!("C{i}"), DataType::Int64, true))
        .collect::<Vec<_>>();
    let arrays = (0..65)
        .map(|_| Arc::new(Int64Array::from(Vec::<i64>::new())) as arrow_array::ArrayRef)
        .collect();
    let batch = RecordBatch::try_new(Arc::new(Schema::new(fields)), arrays).unwrap();
    assert_eq!(table_snapshot(&stream(&batch)), Err(E::ShapeLimit));
}

/// Build adversarial metadata independently of Arrow's validating record writer.
fn declared_stream(
    schema: &Schema,
    rows: i64,
    nodes: &[arrow_ipc::FieldNode],
    buffers: &[arrow_ipc::Buffer],
    body: &[u8],
    declared_body: i64,
    compressed: bool,
) -> Vec<u8> {
    let mut result = vec![];
    let mut writer = StreamWriter::try_new(&mut result, schema).unwrap();
    writer.finish().unwrap();
    result.truncate(result.len() - 8);
    let mut builder = flatbuffers::FlatBufferBuilder::new();
    let nodes = builder.create_vector(nodes);
    let buffers = builder.create_vector(buffers);
    let compression =
        compressed.then(|| arrow_ipc::BodyCompression::create(&mut builder, &Default::default()));
    let batch = arrow_ipc::RecordBatch::create(
        &mut builder,
        &arrow_ipc::RecordBatchArgs {
            length: rows,
            nodes: Some(nodes),
            buffers: Some(buffers),
            compression,
            ..Default::default()
        },
    );
    let message = arrow_ipc::Message::create(
        &mut builder,
        &arrow_ipc::MessageArgs {
            version: arrow_ipc::MetadataVersion::V5,
            header_type: arrow_ipc::MessageHeader::RecordBatch,
            header: Some(batch.as_union_value()),
            bodyLength: declared_body,
            ..Default::default()
        },
    );
    builder.finish(message, None);
    let metadata = builder.finished_data();
    let padded = metadata.len().div_ceil(8) * 8;
    result.extend_from_slice(&u32::MAX.to_le_bytes());
    result.extend_from_slice(&(padded as u32).to_le_bytes());
    result.extend_from_slice(metadata);
    result.resize(result.len() + padded - metadata.len(), 0);
    result.extend_from_slice(body);
    result.extend_from_slice(&[255, 255, 255, 255, 0, 0, 0, 0]);
    result
}

/// Untrusted declarations cannot allocate oversized bodies or bypass buffer bounds.
#[test]
fn adversarial_declared_lengths_nodes_buffers_and_compression() {
    use arrow_ipc::{Buffer as B, FieldNode as N};
    let schema = Schema::new(vec![Field::new("I", DataType::Int64, true)]);
    let valid_nodes = [N::new(1, 0)];
    let valid_buffers = [B::new(0, 0), B::new(0, 8)];
    for body in [-1, i64::MAX, 1024 * 1024 * 1024] {
        assert_eq!(
            table_snapshot(&declared_stream(
                &schema,
                1,
                &valid_nodes,
                &valid_buffers,
                &[],
                body,
                false
            )),
            Err(E::InvalidStream)
        );
    }
    for nodes in [
        [N::new(-1, 0)],
        [N::new(1, -1)],
        [N::new(1, 2)],
        [N::new(2, 0)],
    ] {
        assert_eq!(
            table_snapshot(&declared_stream(
                &schema,
                1,
                &nodes,
                &valid_buffers,
                &[0; 8],
                8,
                false
            )),
            Err(E::InvalidStream)
        );
    }
    for buffers in [
        [B::new(0, 0), B::new(-1, 8)],
        [B::new(0, 0), B::new(0, -1)],
        [B::new(0, 0), B::new(1, 8)],
        [B::new(0, 0), B::new(i64::MAX, i64::MAX)],
    ] {
        assert_eq!(
            table_snapshot(&declared_stream(
                &schema,
                1,
                &valid_nodes,
                &buffers,
                &[0; 8],
                8,
                false
            )),
            Err(E::InvalidStream)
        );
    }
    assert_eq!(
        table_snapshot(&declared_stream(
            &schema,
            1,
            &valid_nodes,
            &valid_buffers,
            &[0; 8],
            8,
            true
        )),
        Err(E::InvalidStream)
    );
    // Correct declarations with too-short value storage are rejected by Arrow validation.
    assert_eq!(
        table_snapshot(&declared_stream(
            &schema,
            1,
            &valid_nodes,
            &[B::new(0, 0), B::new(0, 0)],
            &[],
            0,
            false
        )),
        Err(E::InvalidStream)
    );
}

/// Aliasing may expand logical text; JSON control escaping has a separate output cap.
#[test]
fn aliased_text_and_json_expansion_are_bounded() {
    use arrow_ipc::{Buffer as B, FieldNode as N};
    let schema = Schema::new(
        (0..64)
            .map(|i| Field::new(format!("S{i}"), DataType::Utf8, true))
            .collect::<Vec<_>>(),
    );
    let length = 150_000_i32;
    let mut body = vec![];
    body.extend_from_slice(&0_i32.to_le_bytes());
    body.extend_from_slice(&length.to_le_bytes());
    body.resize(8 + length as usize, b'x');
    let nodes = vec![N::new(1, 0); 64];
    let buffers = (0..64)
        .flat_map(|_| [B::new(0, 0), B::new(0, 8), B::new(8, i64::from(length))])
        .collect::<Vec<_>>();
    assert_eq!(
        table_round_trip(&declared_stream(
            &schema,
            1,
            &nodes,
            &buffers,
            &body,
            body.len() as i64,
            false
        )),
        Err(E::ShapeLimit)
    );
    let schema = Arc::new(Schema::new(vec![Field::new("S", DataType::Utf8, true)]));
    let text = "\0".repeat(3 * 1024 * 1024);
    let batch =
        RecordBatch::try_new(schema, vec![Arc::new(StringArray::from(vec![text]))]).unwrap();
    let input = stream(&batch);
    assert!(input.len() < MAX_INPUT_BYTES);
    assert_eq!(table_snapshot(&input), Err(E::OutputLimit));
    assert!(table_round_trip(&input).is_ok());
}

/// Validate UTF-8 and reject unsupported dictionaries, nesting and IPC versions.
#[test]
fn closed_stream_forms_and_utf8_are_enforced() {
    use arrow_ipc::{Buffer as B, FieldNode as N};
    let schema = Schema::new(vec![Field::new("S", DataType::Utf8, true)]);
    let mut body = vec![];
    body.extend_from_slice(&0_i32.to_le_bytes());
    body.extend_from_slice(&1_i32.to_le_bytes());
    body.push(255);
    assert_eq!(
        table_snapshot(&declared_stream(
            &schema,
            1,
            &[N::new(1, 0)],
            &[B::new(0, 0), B::new(0, 8), B::new(8, 1)],
            &body,
            9,
            false
        )),
        Err(E::InvalidStream)
    );
    let unsupported = [
        DataType::Dictionary(Box::new(DataType::Int8), Box::new(DataType::Utf8)),
        DataType::List(Arc::new(Field::new("item", DataType::Int64, true))),
    ];
    for kind in unsupported {
        let schema = Schema::new(vec![Field::new("A", kind, true)]);
        let mut bytes = vec![];
        let mut writer = StreamWriter::try_new(&mut bytes, &schema).unwrap();
        writer.finish().unwrap();
        assert_eq!(table_snapshot(&bytes), Err(E::InvalidTable));
    }
    let options =
        arrow_ipc::writer::IpcWriteOptions::try_new(8, false, arrow_ipc::MetadataVersion::V4)
            .unwrap();
    let mut bytes = vec![];
    let mut writer = StreamWriter::try_new_with_options(&mut bytes, &schema, options).unwrap();
    writer.finish().unwrap();
    assert_eq!(table_snapshot(&bytes), Err(E::InvalidStream));
}
