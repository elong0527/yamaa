"""Polars treatment for parsed source rows.

Every adapter delivers plain text-or-missing values; this module is the
only place that decides how those values become host table storage.
"""

from __future__ import annotations

import datetime as dt

import polars as pl

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
