"""R020 parquet profile writer on the Polars engine.

The container carries types, so values travel native: missing is null, a
collected empty string is a present zero-length array, temporals are
wall-clock counts with no zone attached, and doubles are stored, never
rendered. Bytes are not fixed; read-back identity is the contract.
"""

from __future__ import annotations

import datetime as dt
import io as stdlib_io
from collections.abc import Sequence

import polars as pl

from yamaa.artifacts.csv import ArtifactError
from yamaa.models import MISSING, TypedTable


def _missing(value: object) -> bool:
    return value is MISSING or value is None


def _column_values(
    table: TypedTable, name: str, column_type: str
) -> tuple[list[object], pl.DataType]:
    raw = table.frame.get_column(name).to_list()
    if column_type == "str":
        return [None if _missing(value) else value for value in raw], pl.String
    if column_type == "int":
        return [None if _missing(value) else value for value in raw], pl.Int64
    if column_type == "float":
        return [None if _missing(value) else value for value in raw], pl.Float64
    if column_type == "date":
        return [
            None if _missing(value) else dt.date(value.year, value.month, value.day)
            for value in raw
        ], pl.Date
    if column_type == "datetime":
        return [
            None
            if _missing(value)
            else dt.datetime(  # noqa: DTZ001 -- R020-24 forbids any zone
                value.year,
                value.month,
                value.day,
                value.hour,
                value.minute,
                value.second,
            )
            for value in raw
        ], pl.Datetime(time_unit="us", time_zone=None)
    raise ArtifactError("unwritable_value", name, {"type": column_type})


def parquet_frame(table: TypedTable, columns: Sequence[str]) -> pl.DataFrame:
    """Build the R020 parquet table: schema, order, and values."""
    by_name = {column.name: column for column in table.columns}
    for name in columns:
        if name not in by_name:
            raise ArtifactError("unknown_output_column", name)
    series = []
    for name in columns:
        values, dtype = _column_values(table, name, by_name[name].type)
        series.append(pl.Series(name, values, dtype=dtype, strict=True))
    return pl.DataFrame(series)


def write_parquet_bytes(table: TypedTable, columns: Sequence[str]) -> bytes:
    """Write the artifact's columns to uncompressed Parquet bytes."""
    frame = parquet_frame(table, columns)
    buffer = stdlib_io.BytesIO()
    frame.write_parquet(buffer, compression="uncompressed")
    return buffer.getvalue()


def read_parquet_frame(data: bytes) -> pl.DataFrame:
    """Read Parquet bytes back for value-level comparison, never byte parity."""
    return pl.read_parquet(stdlib_io.BytesIO(data))
