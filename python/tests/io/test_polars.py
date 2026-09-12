from __future__ import annotations

import datetime as dt

import polars as pl
import pytest

from yamaa.io.polars import column_dtype, frame_from_values, runtime_rows, runtime_value
from yamaa.models import MISSING, DateTimeValue, DateValue, TypedColumn


def test_every_declared_type_has_one_host_column() -> None:
    assert [
        column_dtype(kind) for kind in ("str", "int", "float", "date", "datetime")
    ] == [pl.String(), pl.Int64(), pl.Float64(), pl.Date(), pl.Datetime("us")]


def test_stored_values_read_back_as_the_values_that_were_stored() -> None:
    columns = tuple(
        TypedColumn(name=name, type=kind)
        for name, kind in [
            ("TEXT", "str"),
            ("COUNT", "int"),
            ("VALUE", "float"),
            ("WHEN", "date"),
            ("AT", "datetime"),
        ]
    )
    values: list[object] = [
        "kept",
        7,
        0.1,
        DateValue.parse("2020-01-02"),
        DateTimeValue.parse("2020-01-02T03:04:05"),
    ]

    table = frame_from_values(columns, [values, [None] * 5])

    assert runtime_rows(table) == [
        dict(zip([column.name for column in columns], values, strict=True)),
        {column.name: MISSING for column in columns},
    ]


def test_the_runtime_missing_token_is_stored_as_a_host_null() -> None:
    columns = (TypedColumn(name="COUNT", type="int"),)

    table = frame_from_values(columns, [[MISSING]])

    assert table.frame.get_column("COUNT").to_list() == [None]
    assert runtime_rows(table) == [{"COUNT": MISSING}]


@pytest.mark.parametrize(
    ("value", "message"),
    [
        (dt.datetime(2020, 1, 2, 3, 4, 5, tzinfo=dt.UTC), "zoned"),
        (dt.datetime(2020, 1, 2, 3, 4, 5, 1), "whole seconds"),  # noqa: DTZ001
    ],
)
def test_a_host_scalar_no_runtime_value_represents_is_refused(
    value: dt.datetime, message: str
) -> None:
    with pytest.raises(ValueError, match=message):
        runtime_value(value)
