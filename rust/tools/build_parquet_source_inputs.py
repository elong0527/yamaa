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

REPOSITORY = Path(__file__).resolve().parents[2]


def write(destination: Path, name: str, table: pa.Table, **options):
    pq.write_table(table, destination / (name + ".parquet"), **options)


def build(destination: Path):
    destination.mkdir(parents=True, exist_ok=False)
    table = pa.table({
        "S": pa.array(["", None, "β\x00z"], type=pa.large_string()),
        "I": pa.array([-(1 << 63), None, (1 << 63) - 1], type=pa.int64()),
        "F": pa.array([-0.0, None, float.fromhex("0x0.0000000000001p-1022")], type=pa.float64()),
        "D": pa.array([-719162, None, 2932896], type=pa.date32()),
        "T": pa.array([-1000000, None, 253402300799000000], type=pa.timestamp("us")),
    })
    for codec in ("NONE", "SNAPPY", "GZIP", "BROTLI", "LZ4", "ZSTD"):
        for version in ("1.0", "2.0"):
            for dictionary in (False, True):
                write(destination, f"{codec.lower()}-{version[0]}-{int(dictionary)}", table,
                      compression=codec, data_page_version=version,
                      use_dictionary=dictionary, row_group_size=2)
    write(destination, "delta", table, compression="ZSTD", use_dictionary=False,
          column_encoding={"S": "DELTA_BYTE_ARRAY", "I": "DELTA_BINARY_PACKED",
                           "F": "BYTE_STREAM_SPLIT", "D": "DELTA_BINARY_PACKED",
                           "T": "DELTA_BINARY_PACKED"}, data_page_version="2.0")
    write(destination, "empty", table.slice(0, 0), compression="NONE")
    write(destination, "nonfinite", pa.table({"F": pa.array([float("nan"), float("inf"), -float("inf"), -0.0], type=pa.float64())}), compression="NONE")
    write(destination, "text", pa.table({"I": pa.array([1, 2], type=pa.int64()), "S": ["", "X"]}), compression="NONE")
    write(destination, "boolean", pa.table({"FLAG": [True]}), compression="NONE")
    write(destination, "fractional", pa.table({"AT": pa.array([1], type=pa.timestamp("us"))}), compression="NONE")
    write(destination, "bad-date", pa.table({"D": pa.array([-719163], type=pa.date32())}), compression="NONE")
    write(destination, "empty-name", pa.Table.from_arrays([pa.array([1], type=pa.int64())], names=[""]), compression="NONE")
    write(destination, "duplicate", pa.Table.from_arrays([pa.array([1], type=pa.int64()), pa.array([2], type=pa.int64())], names=["I", "I"]), compression="NONE")
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
    ):
        write(destination, "unsupported-" + name, pa.table({"FIELD": pa.array(values, type=kind)}), compression="ZSTD")
    write(destination, "date64", pa.table({"D": pa.array([0], type=pa.date64())}), compression="NONE")
    for name, other in (("utf8", None), ("utf8-bool", pa.array([True])),
                        ("utf8-time", pa.array([1], type=pa.timestamp("us")))):
        arrays = [pa.array(["ok"])] if other is None else [other, pa.array(["ok"])]
        names = ["TEXT"] if other is None else ["OTHER", "TEXT"]
        buffer = io.BytesIO()
        pq.write_table(pa.Table.from_arrays(arrays, names=names), buffer,
                       compression="NONE", use_dictionary=False, write_statistics=False)
        content = buffer.getvalue()
        marker = b"\x02\x00\x00\x00ok"
        assert content.count(marker) == 1
        (destination / (name + ".parquet")).write_bytes(content.replace(marker, b"\x02\x00\x00\x00\xff\xff"))
    # Change only the source container for the original ordered-sum integration.
    # Field declarations here are independent of either engine's source loader.
    original = REPOSITORY / "benchmarks/adam-adlb-ordered-sum/input/lb.csv"
    with original.open(newline="", encoding="ascii") as stream:
        rows = list(csv.DictReader(stream))
    names = list(rows[0])
    columns = {name: pa.array(
        [None if row[name] == "" else float(row[name]) if name == "LBSTRESN" else row[name] for row in rows],
        type=pa.float64() if name == "LBSTRESN" else pa.string(),
    ) for name in names}
    write(destination, "ordered-sum", pa.table(columns), compression="ZSTD",
          row_group_size=3, data_page_version="2.0")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("destination", type=Path)
    build(parser.parse_args().destination)
