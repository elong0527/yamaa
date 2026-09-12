"""Polars treatment for parsed source rows.

Every adapter delivers plain text-or-missing values; this module is the
only place that decides how those values become host table storage, and
the only place that reads them back, so a completed table answers with
the same values a derivation produced.
"""

from __future__ import annotations

import datetime as dt

import polars as pl

from yamaa.models import (
    MISSING,
    DateTimeValue,
    DateValue,
    TypedColumn,
    TypedTable,
)
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
    if value is MISSING:
        return None
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


def column_dtype(column_type: ColumnType) -> pl.DataType:
    """Return the one host column a declared type is stored in."""
    return _DTYPES[column_type]


def runtime_value(value: object) -> object:
    """Return the runtime value one stored host scalar carries.

    This is the inverse of the storage above, so a consumer reading a
    completed table sees the same R016 value the derivation produced
    rather than a host date object with its own text and ordering. A
    host scalar that no runtime value represents raises, because
    repairing it here would hide the boundary that admitted it.
    """
    if value is None:
        return MISSING
    if isinstance(value, dt.datetime):
        if value.tzinfo is not None:
            raise ValueError("a zoned datetime is not an R016 value")
        if value.microsecond:
            raise ValueError("a datetime below whole seconds is not an R016 value")
        return DateTimeValue(
            year=value.year,
            month=value.month,
            day=value.day,
            hour=value.hour,
            minute=value.minute,
            second=value.second,
        )
    if isinstance(value, dt.date):
        return DateValue(year=value.year, month=value.month, day=value.day)
    return value


def runtime_rows(table: TypedTable) -> list[dict[str, object]]:
    """Read one completed table as runtime values, in table order."""
    return [
        {name: runtime_value(value) for name, value in row.items()}
        for row in table.frame.iter_rows(named=True)
    ]
