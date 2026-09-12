"""Polars treatment for parsed source rows.

Every adapter delivers plain text-or-missing values; this module is the
only place that decides how those values become host table storage.
"""

from __future__ import annotations

import polars as pl

from yamaa.models import TypedColumn, TypedTable
from yamaa.specification.models import ColumnType


def frame_from_values(
    columns: tuple[TypedColumn, ...], rows: list[list[object]]
) -> TypedTable:
    """Store converted column values in one ordered Polars frame."""
    series = []
    for index, column in enumerate(columns):
        target: ColumnType = column.type
        if target == "str":
            dtype = pl.String
        elif target == "int":
            dtype = pl.Int64
        elif target == "float":
            dtype = pl.Float64
        else:
            # DateValue and DateTimeValue retain language-defined collected
            # precision that native host temporal scalars cannot represent.
            dtype = pl.Object
        series.append(
            pl.Series(
                column.name,
                [row[index] for row in rows],
                dtype=dtype,
                strict=True,
            )
        )
    return TypedTable(columns=columns, frame=pl.DataFrame(series))
