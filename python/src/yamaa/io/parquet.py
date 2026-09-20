"""The R020 parquet profile: one typed container two runtimes read alike.

REQ-0742 does not fix these bytes, so nothing here tries to: the schema,
the column order, the row order, the nulls, and the values are what two
runtimes must agree on, and each is written explicitly rather than left to
a writer's default.
"""

from __future__ import annotations

import datetime as dt
import io
import json
from typing import TYPE_CHECKING, Any

import polars as pl
import pyarrow as pa
import pyarrow.parquet as pq

from yamaa.models import TypedColumn, TypedTable
from yamaa.specification.models import ColumnType

if TYPE_CHECKING:  # `artifact` renders through this module, so the
    # dependency runs one way at runtime and both ways in annotations.
    from yamaa.io.artifact import Artifact

# REQ-0734 maps each declared type to exactly one physical and logical type.
# A `datetime` is a reading on a wall clock, so its Timestamp carries no
# zone and is not adjusted to UTC (REQ-0738).
_ARROW: dict[ColumnType, pa.DataType] = {
    "str": pa.string(),
    "int": pa.int64(),
    "float": pa.float64(),
    "date": pa.date32(),
    "datetime": pa.timestamp("us"),
}

_EPOCH = dt.date(1970, 1, 1).toordinal()
_MIN_DAY = dt.date(dt.MINYEAR, 1, 1).toordinal() - _EPOCH
_MAX_DAY = dt.date(dt.MAXYEAR, 12, 31).toordinal() - _EPOCH
_MIN_MICROSECOND = _MIN_DAY * 86_400 * 1_000_000
_MAX_MICROSECOND = (_MAX_DAY * 86_400 + 86_399) * 1_000_000

_PARQUET_REQUIREMENTS = {
    "source_parquet_invalid": "REQ-1038",
    "source_field_name_empty": "REQ-1039",
    "source_field_name_duplicate": "REQ-1039",
    "source_field_type_unsupported": "REQ-1040",
    "source_field_value_invalid": "REQ-1041",
}


class ParquetProfileFailure(ValueError):
    """One stable R027 failure while decoding a Parquet source."""

    def __init__(self, condition: str, context: dict[str, object]) -> None:
        self.condition = condition
        self.context = context
        self.requirement = _PARQUET_REQUIREMENTS[condition]
        super().__init__(f"{condition}: {context}")


def parquet_schema(artifact: Artifact) -> pa.Schema:
    """Return the artifact's fields, in `output.columns` order, all optional."""
    return pa.schema(
        [
            # REQ-0735: every field is optional, because every column type
            # admits a missing value.
            pa.field(column.name, _ARROW[column.type], nullable=True)
            for column in artifact.columns
        ]
    )


def render_parquet(artifact: Artifact) -> bytes:
    """Render one artifact to uncompressed Parquet under the R020 mapping."""
    table = artifact.frame.to_arrow().cast(parquet_schema(artifact))
    buffer = io.BytesIO()
    pq.write_table(
        table,
        buffer,
        # REQ-0741: uncompressed pages, and no key-value metadata of the
        # implementation's own. `store_schema` would add the writer's Arrow
        # schema beside the Parquet one this rule already fixes.
        compression="none",
        store_schema=False,
    )
    return buffer.getvalue()


def _read_source(content: bytes) -> tuple[pa.Table, Any]:
    try:
        source = pq.ParquetFile(
            io.BytesIO(content),
            # REQ-1037: Arrow extension metadata cannot override the closed
            # physical/logical type mapping below.
            arrow_extensions_enabled=False,
        )
        return source.read(use_threads=False), source.schema
    except (pa.ArrowException, OSError) as error:
        raise ParquetProfileFailure("source_parquet_invalid", {}) from error


def _column_type(stored: Any, arrow_type: pa.DataType) -> ColumnType | None:
    logical = json.loads(stored.logical_type.to_json())
    key = (stored.physical_type, logical.get("Type"))
    column_type: ColumnType | None = {
        ("BYTE_ARRAY", "String"): "str",
        ("INT64", "None"): "int",
        ("DOUBLE", "None"): "float",
        ("INT32", "Date"): "date",
    }.get(key)
    if key == ("INT64", "Timestamp") and logical == {
        "Type": "Timestamp",
        "isAdjustedToUTC": False,
        "timeUnit": "microseconds",
        "is_from_converted_type": False,
        "force_set_converted_type": False,
    }:
        column_type = "datetime"
    if column_type is None or arrow_type != _ARROW[column_type]:
        return None
    return column_type


def _source_columns(table: pa.Table, stored_schema: Any) -> tuple[TypedColumn, ...]:
    if len(table.schema) == 0:
        raise ParquetProfileFailure("source_parquet_invalid", {})
    if len(stored_schema) != len(table.schema):
        field = table.schema[0]
        raise ParquetProfileFailure(
            "source_field_type_unsupported",
            {"field": field.name, "stored_type": str(field.type)},
        )
    columns: list[TypedColumn] = []
    seen: set[str] = set()
    for position, field in enumerate(table.schema, 1):
        if not field.name:
            raise ParquetProfileFailure("source_field_name_empty", {"field": position})
        if field.name in seen:
            raise ParquetProfileFailure(
                "source_field_name_duplicate", {"field": field.name}
            )
        seen.add(field.name)
        stored = stored_schema.column(position - 1)
        column_type = (
            _column_type(stored, field.type)
            if stored.max_repetition_level == 0 and stored.path == field.name
            else None
        )
        if column_type is None:
            raise ParquetProfileFailure(
                "source_field_type_unsupported",
                {"field": field.name, "stored_type": str(field.type)},
            )
        columns.append(TypedColumn(name=field.name, type=column_type))
    return tuple(columns)


def _validate_temporal_values(
    table: pa.Table, columns: tuple[TypedColumn, ...]
) -> None:
    for index, column in enumerate(columns):
        if column.type not in {"date", "datetime"}:
            continue
        storage_type = pa.int32() if column.type == "date" else pa.int64()
        stored = table.column(index).cast(storage_type).to_pylist()
        for row, value in enumerate(stored, 1):
            if value is None:
                continue
            valid = (
                _MIN_DAY <= value <= _MAX_DAY
                if column.type == "date"
                else (
                    _MIN_MICROSECOND <= value <= _MAX_MICROSECOND
                    and value % 1_000_000 == 0
                )
            )
            if not valid:
                raise ParquetProfileFailure(
                    "source_field_value_invalid",
                    {"field": column.name, "row": row, "value": value},
                )


def parse_parquet(content: bytes) -> TypedTable:
    """Parse one immutable snapshot under the closed R027 source profile."""
    table, stored_schema = _read_source(content)
    columns = _source_columns(table, stored_schema)
    _validate_temporal_values(table, columns)
    try:
        frame = pl.from_arrow(table, rechunk=True)
        if not isinstance(frame, pl.DataFrame):
            raise TypeError("a Parquet table must produce a Polars DataFrame")
        return TypedTable(columns=columns, frame=frame)
    except (
        pa.ArrowException,
        pl.exceptions.PolarsError,
        TypeError,
        ValueError,
    ) as error:
        raise ParquetProfileFailure("source_parquet_invalid", {}) from error


def read_parquet(content: bytes) -> pa.Table:
    """Read artifact bytes back, for the comparison REQ-0740 requires."""
    return _read_source(content)[0]
