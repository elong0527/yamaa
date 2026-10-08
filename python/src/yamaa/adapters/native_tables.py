"""Explicit table interchange for the optional Rust package; no backend dispatch."""

from __future__ import annotations

import io
from importlib import import_module

import polars as pl
import pyarrow as pa
import pyarrow.compute as pc


def ipc_from_polars(frame: pl.DataFrame, schema: pa.Schema) -> bytes:
    """Restore a declared canonical schema, then validate and sanitize in Rust.

    Polars changes Arrow string layout, timestamp units and struct nullability.
    Safe casts retain visible values; only children masked by a null temporal
    parent receive placeholders. Collected precision must already be a column
    child: this function never invents it from a plain host date/timestamp.
    Host frames are already materialized; serialization precedes the native byte
    budget. Use native byte APIs when accepting untrusted external streams.
    """
    yamaa_native = import_module("yamaa._native")

    table = frame.to_arrow()
    if table.column_names != schema.names:
        raise ValueError("table columns must match the declared order")
    arrays = []
    for field, column in zip(schema, table.columns, strict=True):
        if pa.types.is_struct(field.type):
            if [child.name for child in field.type] != ["value", "precision"]:
                raise ValueError("invalid temporal child schema")
            if not pa.types.is_struct(column.type) or [
                child.name for child in column.type
            ] != ["value", "precision"]:
                raise ValueError("temporal precision-bearing children are required")
            value_type = field.type[0].type
            source_value = column.type[0].type
            if field.type[1].type != pa.uint8() or column.type[1].type != pa.uint8():
                raise ValueError("temporal precision must use UInt8")
            if value_type == pa.date32():
                compatible = source_value == pa.date32()
            elif value_type == pa.timestamp("s"):
                compatible = (
                    pa.types.is_timestamp(source_value) and source_value.tz is None
                )
            else:
                compatible = False
            if not compatible:
                raise ValueError("incompatible temporal value type")
            chunks = []
            for chunk in column.chunks:
                children = []
                for child in field.type:
                    # Mask before casting: hidden payloads have no language value.
                    values = pc.if_else(
                        chunk.is_null(),
                        pa.scalar(0, type=chunk.field(child.name).type),
                        chunk.field(child.name),
                    )
                    values = pc.cast(values, child.type, safe=True)
                    if values.null_count:
                        raise ValueError("visible temporal children cannot be missing")
                    children.append(values)
                chunks.append(
                    pa.StructArray.from_arrays(
                        children, fields=list(field.type), mask=chunk.is_null()
                    )
                )
            arrays.append(pa.chunked_array(chunks, type=field.type))
        else:
            if field.type == pa.string():
                compatible = pa.types.is_string(
                    column.type
                ) or pa.types.is_large_string(column.type)
            else:
                compatible = (
                    field.type in (pa.int64(), pa.float64())
                    and column.type == field.type
                )
            if not compatible:
                raise ValueError("incompatible logical column type")
            arrays.append(pc.cast(column, field.type, safe=True))
    restored = pa.Table.from_arrays(arrays, schema=schema)
    output = io.BytesIO()
    with pa.ipc.new_stream(output, schema) as writer:
        writer.write_table(restored)
    return yamaa_native.table_round_trip(output.getvalue())


def polars_from_ipc(request: bytes) -> pl.DataFrame:
    """Validate and sanitize native IPC before converting visible values to Polars.

    Preserve the canonical Arrow schema separately for ipc_from_polars: Polars
    changes its physical representation and can merge input chunks.
    """
    yamaa_native = import_module("yamaa._native")

    owned = yamaa_native.table_round_trip(request)
    table = pa.ipc.open_stream(owned).read_all()
    if table.num_columns == 0 and table.num_rows:
        raise ValueError("Polars cannot retain rows in a zero-column table; use IPC")
    return pl.from_arrow(table)
