"""R020 parquet writer: schema, nulls, values, and bit-identical doubles."""

from __future__ import annotations

import io
import struct

import polars as pl
import pyarrow.parquet as pq
import pytest

from yamaa.io.polars import (
    frame_from_values,
    parquet_frame,
    read_parquet_frame,
    write_parquet_bytes,
)
from yamaa.io.publish import ArtifactError
from yamaa.models import DateTimeValue, DateValue, TypedColumn

COLUMNS = (
    TypedColumn(name="STUDYID", type="str"),
    TypedColumn(name="AVAL", type="float"),
    TypedColumn(name="VISIT", type="date"),
    TypedColumn(name="DRAWN", type="datetime"),
)

ROWS = [
    [
        "PILOT7",
        2.675,
        DateValue(year=2024, month=1, day=2),
        DateTimeValue(year=2024, month=1, day=2, hour=3, minute=4, second=5),
    ],
    ["PILOT7", None, None, None],
    [
        "PILOT7",
        0.0,
        DateValue(year=1970, month=1, day=1),
        DateTimeValue(year=1970, month=1, day=1, hour=0, minute=0, second=0),
    ],
]


def test_schema_order_types_and_null_masks() -> None:
    table = frame_from_values(COLUMNS, ROWS)
    frame = read_parquet_frame(
        write_parquet_bytes(table, ["STUDYID", "AVAL", "VISIT", "DRAWN"])
    )

    assert frame.columns == ["STUDYID", "AVAL", "VISIT", "DRAWN"]
    assert frame.schema == {
        "STUDYID": pl.String,
        "AVAL": pl.Float64,
        "VISIT": pl.Date,
        "DRAWN": pl.Datetime(time_unit="us", time_zone=None),
    }
    assert frame["AVAL"].is_null().to_list() == [False, True, False]
    assert frame["VISIT"].to_list()[1] is None
    assert frame["STUDYID"].to_list() == ["PILOT7", "PILOT7", "PILOT7"]


def test_empty_string_is_present_and_missing_is_null() -> None:
    columns = (TypedColumn(name="NOTE", type="str"),)
    table = frame_from_values(columns, [[""], [None]])
    frame = read_parquet_frame(write_parquet_bytes(table, ["NOTE"]))

    assert frame["NOTE"].to_list() == ["", None]
    assert frame["NOTE"].is_null().to_list() == [False, True]


def test_doubles_read_back_bit_identical() -> None:
    table = frame_from_values(COLUMNS, ROWS)
    frame = read_parquet_frame(
        write_parquet_bytes(table, ["STUDYID", "AVAL", "VISIT", "DRAWN"])
    )

    assert frame["AVAL"].to_list()[0] == 2.675
    assert struct.pack("<d", frame["AVAL"].to_list()[0]) == struct.pack("<d", 2.675)


def test_temporal_values_keep_the_wall_clock() -> None:
    table = frame_from_values(COLUMNS, ROWS)
    frame = read_parquet_frame(
        write_parquet_bytes(table, ["STUDYID", "AVAL", "VISIT", "DRAWN"])
    )

    assert str(frame["VISIT"].to_list()[0]) == "2024-01-02"
    assert str(frame["DRAWN"].to_list()[0]) == "2024-01-02 03:04:05"
    assert frame["DRAWN"].dtype.time_zone is None


def test_pages_are_uncompressed() -> None:
    table = frame_from_values(COLUMNS, ROWS)
    data = write_parquet_bytes(table, ["STUDYID", "AVAL", "VISIT", "DRAWN"])
    metadata = pq.read_metadata(io.BytesIO(data))

    assert metadata.num_rows == 3
    assert {
        metadata.row_group(0).column(index).compression
        for index in range(metadata.num_columns)
    } == {"UNCOMPRESSED"}


def test_parquet_round_trip_preserves_row_order() -> None:
    table = frame_from_values(COLUMNS, ROWS)
    frame = parquet_frame(table, ["STUDYID", "AVAL", "VISIT", "DRAWN"])

    assert frame["STUDYID"].to_list() == ["PILOT7", "PILOT7", "PILOT7"]


def test_write_rejects_unknown_output_column() -> None:
    table = frame_from_values(COLUMNS, ROWS)
    with pytest.raises(ArtifactError) as raised:
        write_parquet_bytes(table, ["STUDYID", "ABSENT"])

    assert raised.value.condition == "unknown_output_column"
