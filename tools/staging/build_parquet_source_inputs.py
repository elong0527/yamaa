"""Author Parquet input fixtures with PyArrow, independently of the native reader.

Run explicitly to recreate these input containers. No expected output is read or
rewritten; the tests specify their own exact values and diagnostic expectations.
"""

import argparse
import csv
import io
from decimal import Decimal
from pathlib import Path

import pyarrow as pa
import pyarrow.parquet as pq

from tools.paths import ROOT

REPOSITORY = ROOT


def write(destination: Path, name: str, table: pa.Table, **options):
    pq.write_table(table, destination / (name + ".parquet"), **options)


class WellKnownBinary(pa.ExtensionType):
    """The GeoArrow WKB type, which PyArrow writes as a Parquet GEOMETRY, or
    as a GEOGRAPHY when its metadata names spherical edges."""

    def __init__(self, metadata: bytes = b"{}"):
        self.metadata = metadata
        super().__init__(pa.binary(), "geoarrow.wkb")

    def __arrow_ext_serialize__(self):
        return self.metadata

    @classmethod
    def __arrow_ext_deserialize__(cls, storage_type, serialized):
        return cls(serialized)


# The well-known binary of the point (1 2).
POINT = bytes.fromhex("0101000000000000000000f03f0000000000000040")


def write_variant(destination: Path):
    """A VARIANT group, which PyArrow cannot write: its struct, written by
    PyArrow, with the group's schema element annotated in the footer."""
    metadata = pa.field("metadata", pa.binary(), nullable=False)
    value = pa.field("value", pa.binary(), nullable=False)
    group = pa.array(
        [{"metadata": b"\x01\x00\x00", "value": b"\x0c\x01"}],
        type=pa.struct([metadata, value]),
    )
    buffer = io.BytesIO()
    pq.write_table(pa.table({"FIELD": group}), buffer, compression="ZSTD")
    content = buffer.getvalue()
    # The group's name and two children, then its stop byte. Before the stop
    # goes field 10, the logical type: the VARIANT variant (16, in the long
    # form) of the union, holding specification_version 1.
    marker = b"\x18\x05FIELD\x15\x04\x00"
    annotation = b"\x5c\x0c\x20\x13\x01\x00\x00"
    assert content.count(marker) == 1
    content = content.replace(marker, marker[:-1] + annotation + b"\x00")
    footer = int.from_bytes(content[-8:-4], "little") + len(annotation)
    (destination / "unsupported-variant.parquet").write_bytes(
        content[:-8] + footer.to_bytes(4, "little") + b"PAR1"
    )


def build(destination: Path):
    destination.mkdir(parents=True, exist_ok=False)
    table = pa.table(
        {
            "S": pa.array(["", None, "\u03b2\x00z"], type=pa.large_string()),
            "I": pa.array([-(1 << 63), None, (1 << 63) - 1], type=pa.int64()),
            "F": pa.array(
                [-0.0, None, float.fromhex("0x0.0000000000001p-1022")],
                type=pa.float64(),
            ),
            "D": pa.array([-719162, None, 2932896], type=pa.date32()),
            "T": pa.array(
                [-1000000, None, 253402300799000000], type=pa.timestamp("us")
            ),
        }
    )
    for codec in ("NONE", "SNAPPY", "GZIP", "BROTLI", "LZ4", "ZSTD"):
        for version in ("1.0", "2.0"):
            for dictionary in (False, True):
                write(
                    destination,
                    f"{codec.lower()}-{version[0]}-{int(dictionary)}",
                    table,
                    compression=codec,
                    data_page_version=version,
                    use_dictionary=dictionary,
                    row_group_size=2,
                )
    write(
        destination,
        "delta",
        table,
        compression="ZSTD",
        use_dictionary=False,
        column_encoding={
            "S": "DELTA_BYTE_ARRAY",
            "I": "DELTA_BINARY_PACKED",
            "F": "BYTE_STREAM_SPLIT",
            "D": "DELTA_BINARY_PACKED",
            "T": "DELTA_BINARY_PACKED",
        },
        data_page_version="2.0",
    )
    write(destination, "empty", table.slice(0, 0), compression="NONE")
    write(
        destination,
        "nonfinite",
        pa.table(
            {
                "F": pa.array(
                    [float("nan"), float("inf"), -float("inf"), -0.0], type=pa.float64()
                )
            }
        ),
        compression="NONE",
    )
    write(
        destination,
        "text",
        pa.table({"I": pa.array([1, 2], type=pa.int64()), "S": ["", "X"]}),
        compression="NONE",
    )
    write(destination, "boolean", pa.table({"FLAG": [True]}), compression="NONE")
    write(
        destination,
        "fractional",
        pa.table({"AT": pa.array([1], type=pa.timestamp("us"))}),
        compression="NONE",
    )
    write(
        destination,
        "bad-date",
        pa.table({"D": pa.array([-719163], type=pa.date32())}),
        compression="NONE",
    )
    write(
        destination,
        "empty-name",
        pa.Table.from_arrays([pa.array([1], type=pa.int64())], names=[""]),
        compression="NONE",
    )
    write(
        destination,
        "duplicate",
        pa.Table.from_arrays(
            [pa.array([1], type=pa.int64()), pa.array([2], type=pa.int64())],
            names=["I", "I"],
        ),
        compression="NONE",
    )
    for name, kind, values in (
        ("int32", pa.int32(), [1]),
        ("uint64", pa.uint64(), [1]),
        ("float32", pa.float32(), [1.25]),
        ("binary", pa.binary(), [b"x"]),
        ("milliseconds", pa.timestamp("ms"), [1000]),
        ("timezone", pa.timestamp("us", "UTC"), [1000000]),
        ("list", pa.list_(pa.int64()), [[1]]),
        ("fixed-list", pa.list_(pa.int64(), 2), [[1, 2]]),
        ("struct", pa.struct([("item", pa.int64())]), [{"item": 1}]),
        ("decimal", pa.decimal128(10, 2), [Decimal("1.23")]),
        # Types that the Arrow release the engine pins has no counterpart
        # for, named only in the embedded Arrow schema (#3155).
        ("decimal32", pa.decimal32(9, 2), [Decimal("1.23")]),
        ("decimal64", pa.decimal64(18, 2), [Decimal("1.23")]),
        ("list-view", pa.list_view(pa.int64()), [[1]]),
        ("large-list-view", pa.large_list_view(pa.int64()), [[1]]),
        # Logical types that the Parquet release the engine pins does not
        # know (#3155).
        ("geometry", WellKnownBinary(), [POINT]),
        ("geography", WellKnownBinary(b'{"edges":"spherical"}'), [POINT]),
    ):
        write(
            destination,
            "unsupported-" + name,
            pa.table({"FIELD": pa.array(values, type=kind)}),
            compression="ZSTD",
        )
    write_variant(destination)
    write(
        destination,
        "date64",
        pa.table({"D": pa.array([0], type=pa.date64())}),
        compression="NONE",
    )
    integer = pa.array([1], type=pa.int64())
    nested = pa.array([[("key", 2)]], type=pa.map_(pa.string(), pa.int64()))
    structure = pa.array(
        [{"left": 2, "right": "text"}],
        type=pa.struct([("left", pa.int64()), ("right", pa.string())]),
    )
    for name, arrays, names in (
        ("mixed-struct", [integer, structure, integer], ["I", "S", "AFTER"]),
        ("mixed-map", [integer, nested, integer], ["I", "M", "AFTER"]),
        ("mixed-empty", [integer, nested], ["", "M"]),
        ("mixed-duplicate", [integer, integer, nested], ["I", "I", "M"]),
    ):
        write(
            destination,
            name,
            pa.Table.from_arrays(arrays, names=names),
            compression="ZSTD",
        )
    for name, other in (
        ("utf8", None),
        ("utf8-bool", pa.array([True])),
        ("utf8-time", pa.array([1], type=pa.timestamp("us"))),
    ):
        arrays = [pa.array(["ok"])] if other is None else [other, pa.array(["ok"])]
        names = ["TEXT"] if other is None else ["OTHER", "TEXT"]
        buffer = io.BytesIO()
        pq.write_table(
            pa.Table.from_arrays(arrays, names=names),
            buffer,
            compression="NONE",
            use_dictionary=False,
            write_statistics=False,
        )
        content = buffer.getvalue()
        marker = b"\x02\x00\x00\x00ok"
        assert content.count(marker) == 1
        (destination / (name + ".parquet")).write_bytes(
            content.replace(marker, b"\x02\x00\x00\x00\xff\xff")
        )
    # Change only the source container for the original ordered-sum integration.
    # Field declarations here are independent of either engine's source loader.
    original = REPOSITORY / "benchmarks/adam-adlb-ordered-sum/input/lb.csv"
    with original.open(newline="", encoding="ascii") as stream:
        rows = list(csv.DictReader(stream))
    names = list(rows[0])
    columns = {
        name: pa.array(
            [
                None
                if row[name] == ""
                else float(row[name])
                if name == "LBSTRESN"
                else row[name]
                for row in rows
            ],
            type=pa.float64() if name == "LBSTRESN" else pa.string(),
        )
        for name in names
    }
    write(
        destination,
        "ordered-sum",
        pa.table(columns),
        compression="ZSTD",
        row_group_size=3,
        data_page_version="2.0",
    )


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("destination", type=Path)
    build(parser.parse_args().destination)
