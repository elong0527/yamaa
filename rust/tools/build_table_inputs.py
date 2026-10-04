"""Author IPC inputs with PyArrow; expected truth is specified separately by hand."""

from pathlib import Path

import pyarrow as pa

p = Path(__file__).resolve().parents[1] / "crates/yamaa-adapters/tests/fixtures/tables"
df = [pa.field("value", pa.date32(), False), pa.field("precision", pa.uint8(), False)]
tf = [
    pa.field("value", pa.timestamp("s"), False),
    pa.field("precision", pa.uint8(), False),
]
d = pa.StructArray.from_arrays(
    [
        pa.array([-719162, -1, 2932896, -2147483648, 20089], type=pa.date32()),
        pa.array([0, 1, 2, 255, 0], type=pa.uint8()),
    ],
    fields=df,
    mask=pa.array([False, False, False, True, False]),
)
t = pa.StructArray.from_arrays(
    [
        pa.array(
            [-1, -62135596800, 253402300799, 9223372036854775807, 0],
            type=pa.timestamp("s"),
        ),
        pa.array([1, 0, 1, 255, 0], type=pa.uint8()),
    ],
    fields=tf,
    mask=pa.array([False, False, False, True, False]),
)
schema = pa.schema(
    [
        pa.field("I", pa.int64()),
        pa.field("F", pa.float64()),
        pa.field("S", pa.string()),
        pa.field("D", d.type),
        pa.field("T", t.type),
    ]
)
b = pa.RecordBatch.from_arrays(
    [
        pa.array([-(2**63), 2**63 - 1, 2**53 + 1, None, 0], type=pa.int64()),
        pa.array([-0.0, float("nan"), float("inf"), float("-inf"), 5e-324]),
        pa.array(["", "a\0\U0001f980", None, "last", "x"]),
        d,
        t,
    ],
    schema=schema,
)
for name, case_schema, batches in [
    (
        "mixed",
        schema,
        [b.slice(0, 0), b.slice(0, 2), b.slice(0, 0), b.slice(2, 3), b.slice(0, 0)],
    ),
    ("empty", schema, [b.slice(0, 0)]),
    ("schema_only", schema, []),
]:
    with (
        pa.OSFile(str(p / (name + ".arrow")), "wb") as sink,
        pa.ipc.new_stream(
            sink,
            case_schema,
            options=pa.ipc.IpcWriteOptions(
                compression=None, metadata_version=pa.ipc.MetadataVersion.V5
            ),
        ) as writer,
    ):
        for batch in batches:
            writer.write_batch(batch)
zero = pa.RecordBatch.from_struct_array(pa.array([{}, {}, {}], type=pa.struct([])))
with (
    pa.OSFile(str(p / "zero_columns.arrow"), "wb") as sink,
    pa.ipc.new_stream(sink, zero.schema) as writer,
):
    writer.write_batch(zero)
