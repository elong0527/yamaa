"""Polars treatment for parsed source rows.

Every adapter delivers plain text-or-missing values; this module is the
only place that decides how those values become host table storage, in
either direction.
"""

from __future__ import annotations

import datetime as dt
import io
from collections.abc import Sequence

import polars as pl
import pyarrow.parquet as pq

from yamaa.io.publish import ArtifactError
from yamaa.models import DateTimeValue, DateValue, TypedColumn, TypedTable
from yamaa.specification.models import ColumnType

# R011 admits only the complete R016 lexical forms at conversion, so an
# ingested `date` is always day precision and an ingested `datetime` always
# whole seconds. Both fit a native host column exactly, and native storage
# keeps the frame usable by ordinary Polars expressions.
_DTYPES: dict[ColumnType, pl.DataType] = {
    "str": pl.String(),
    "int": pl.Int64(),
    "float": pl.Float64(),
    "date": pl.Date(),
    "datetime": pl.Datetime("us"),
}


def _host_value(value: object) -> object:
    """Return the host scalar one converted value is stored as."""
    if isinstance(value, DateValue):
        if value.collected_precision != "day":
            raise ValueError("a date below day precision has no host column")
        return dt.date(value.year, value.month, value.day)
    if isinstance(value, DateTimeValue):
        # R016 datetimes are zone-free local civil times, so the host scalar
        # is naive by definition rather than by omission.
        return dt.datetime(  # noqa: DTZ001
            value.year,
            value.month,
            value.day,
            value.hour,
            value.minute,
            value.second,
        )
    return value


def frame_from_values(
    columns: tuple[TypedColumn, ...], rows: list[list[object]]
) -> TypedTable:
    """Store converted column values in one ordered Polars frame."""
    series = [
        pl.Series(
            column.name,
            [_host_value(row[index]) for row in rows],
            dtype=_DTYPES[column.type],
            strict=True,
        )
        for index, column in enumerate(columns)
    ]
    return TypedTable(columns=columns, frame=pl.DataFrame(series))


def parquet_frame(
    table: TypedTable,
    columns: Sequence[str],
    keys: Sequence[str] = (),
) -> pl.DataFrame:
    """Project output columns in order to a Parquet-ready native frame."""
    by_name = {column.name: column for column in table.columns}
    for name in columns:
        if name not in by_name:
            raise ArtifactError("unknown_output_column", name, column=name)
    frames = []
    for name in columns:
        series = table.frame.get_column(name)
        expected = _DTYPES[by_name[name].type]
        if series.dtype != expected:
            raise ArtifactError(
                "unwritable_value",
                str(series.dtype),
                column=name,
            )
        frames.append(series)
    return pl.DataFrame(frames)


def write_parquet_bytes(
    table: TypedTable,
    columns: Sequence[str],
    keys: Sequence[str] = (),
) -> bytes:
    """Write output columns in order to one uncompressed Parquet file."""
    buffer = io.BytesIO()
    pq.write_table(
        parquet_frame(table, columns, keys).to_arrow(),
        buffer,
        compression="NONE",
        store_schema=False,
    )
    return buffer.getvalue()


def read_parquet_frame(data: bytes) -> pl.DataFrame:
    """Read Parquet bytes back for read-back comparisons, never byte ones."""
    return pl.read_parquet(io.BytesIO(data))
