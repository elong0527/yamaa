from __future__ import annotations

import io
import struct

import pyarrow as pa
import pyarrow.parquet as pq
import pytest

from yamaa.artifacts import (
    ArtifactError,
    build_artifact,
    read_parquet,
    render_parquet,
)
from yamaa.io.polars import frame_from_values
from yamaa.models import DateTimeValue, DateValue, TypedColumn, TypedTable
from yamaa.specification.models import ColumnType, Output

DECLARED: list[tuple[str, ColumnType]] = [
    ("USUBJID", "str"),
    ("COMMENT", "str"),
    ("COUNT", "int"),
    ("VALUE", "float"),
    ("WHEN", "date"),
    ("AT", "datetime"),
]
ROWS: list[list[object]] = [
    [
        "S-1",
        "kept",
        7,
        0.1,
        DateValue.parse("2020-01-02"),
        DateTimeValue.parse("2020-01-02T03:04:05"),
    ],
    ["S-2", "", -3, float(2**-1074), DateValue.parse("1969-12-31"), None],
    ["S-3", None, None, None, None, DateTimeValue.parse("1900-01-01T00:00:00")],
]


def artifact_bytes(
    declared: list[tuple[str, ColumnType]] | None = None,
    rows: list[list[object]] | None = None,
) -> bytes:
    columns = tuple(
        TypedColumn(name=name, type=kind) for name, kind in (declared or DECLARED)
    )
    table: TypedTable = frame_from_values(columns, rows if rows is not None else ROWS)
    output = Output(path="adsl.parquet", columns=[column.name for column in columns])
    return render_parquet(build_artifact(table, output, ["USUBJID"]))


def test_the_schema_is_the_declared_mapping_in_output_column_order() -> None:
    schema = pq.ParquetFile(io.BytesIO(artifact_bytes())).schema

    assert [schema.column(index).name for index in range(len(schema))] == [
        name for name, _ in DECLARED
    ]
    assert [
        (
            schema.column(index).physical_type,
            str(schema.column(index).logical_type),
            schema.column(index).max_definition_level,
        )
        for index in range(len(schema))
    ] == [
        ("BYTE_ARRAY", "String", 1),
        ("BYTE_ARRAY", "String", 1),
        ("INT64", "None", 1),
        ("DOUBLE", "None", 1),
        ("INT32", "Date", 1),
        (
            "INT64",
            (
                "Timestamp(isAdjustedToUTC=false, timeUnit=microseconds, "
                "is_from_converted_type=false, force_set_converted_type=false)"
            ),
            1,
        ),
    ]


def test_pages_are_uncompressed_and_carry_no_metadata_of_our_own() -> None:
    metadata = pq.ParquetFile(io.BytesIO(artifact_bytes())).metadata

    assert metadata.metadata is None
    group = metadata.row_group(0)
    assert {group.column(index).compression for index in range(group.num_columns)} == {
        "UNCOMPRESSED"
    }


def test_a_collected_empty_string_reads_back_apart_from_a_missing_one() -> None:
    read = read_parquet(artifact_bytes())

    assert read.column("COMMENT").to_pylist() == ["kept", "", None]
    assert read.column("COMMENT").null_count == 1
    assert read.column("COUNT").to_pylist() == [7, -3, None]


def test_every_double_reads_back_bit_identical() -> None:
    values = [0.1, 1.0 / 3.0, float(2**-1074), 1e308, -0.0, 2.675]
    rows: list[list[object]] = [
        [f"S-{index}", value] for index, value in enumerate(values)
    ]

    read = read_parquet(artifact_bytes([("USUBJID", "str"), ("VALUE", "float")], rows))

    assert [struct.pack("<d", value) for value in read.column("VALUE").to_pylist()] == [
        struct.pack("<d", value) for value in values
    ]


def test_temporal_values_carry_the_wall_clock_they_name() -> None:
    read = read_parquet(artifact_bytes())

    assert read.column("WHEN").cast(pa.int32()).to_pylist() == [18263, -1, None]
    assert read.column("AT").cast(pa.int64()).to_pylist() == [
        1577934245000000,
        None,
        -2208988800000000,
    ]
    assert read.schema.field("AT").type == pa.timestamp("us")
    assert read.schema.field("AT").type.tz is None


def test_row_order_is_the_declared_artifact_order() -> None:
    columns = tuple(
        TypedColumn(name=name, type=kind)
        for name, kind in [("USUBJID", "str"), ("SEQ", "int")]
    )
    table = frame_from_values(columns, [["S-3", 3], ["S-1", 1], ["S-2", 2]])
    output = Output(
        path="adsl.parquet",
        columns=["USUBJID", "SEQ"],
        order_by=[{"variable": "SEQ"}],
    )

    read = read_parquet(render_parquet(build_artifact(table, output, ["USUBJID"])))

    assert read.column("USUBJID").to_pylist() == ["S-1", "S-2", "S-3"]


def test_an_artifact_with_no_rows_keeps_its_schema() -> None:
    read = read_parquet(artifact_bytes(DECLARED, []))

    assert read.num_rows == 0
    assert read.schema.names == [name for name, _ in DECLARED]


def test_a_display_precision_is_refused_on_the_parquet_profile() -> None:
    columns = (TypedColumn(name="VALUE", type="float"),)
    table = frame_from_values(columns, [[0.5]])
    output = Output(path="adsl.parquet", decimals=2, columns=["VALUE"])

    with pytest.raises(ArtifactError) as raised:
        build_artifact(table, output, ["VALUE"])

    diagnostic = raised.value.diagnostics[0]
    assert diagnostic.condition == "decimals_not_applicable"
    assert diagnostic.requirement == "R020-45"
    assert diagnostic.spec_paths == ("output.decimals",)
