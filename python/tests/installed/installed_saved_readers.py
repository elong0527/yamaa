"""Independent installed readers decode held public CSV/Parquet/JSON/XML saves."""

import csv
import datetime
import io
import json
import struct
import tempfile
import unittest
from pathlib import Path

import polars as pl
import pyarrow as pa
import pyarrow.csv as pacsv
import pyarrow.parquet as pq
from lxml import etree

import yamaa

TRUTH = json.loads(
    Path(__file__)
    .with_name("host-fixtures")
    .joinpath("saved-readers.json")
    .read_bytes()
)
TYPES = {
    "str": (pl.String, pa.string()),
    "int": (pl.Int64, pa.int64()),
    "float": (pl.Float64, pa.float64()),
    "date": (pl.Date, pa.date32()),
    "datetime": (pl.Datetime("us"), pa.timestamp("us")),
}


def normalized(kind, value):
    if value is None:
        return None
    if kind == "float":
        return struct.pack(">d", value).hex()
    if kind == "int":
        return str(value)
    if kind in ("date", "datetime"):
        return value.isoformat()
    return value


class SavedReaders(unittest.TestCase):
    def assert_values(self, values, *, empty):
        for name, kind in zip(TRUTH["columns"], TRUTH["types"], strict=True):
            self.assertEqual(
                [normalized(kind, value) for value in values[name]],
                [] if empty else TRUTH["values"][name],
                name,
            )

    def read_parquet(self, saved, *, empty):
        # Both readers receive exactly the same captured bytes, not a re-encoded
        # dataframe or a second engine run. Explicit types pin empty tables too.
        frame = pl.read_parquet(io.BytesIO(saved))
        self.assertEqual(
            frame.schema,
            dict(zip(TRUTH["columns"], [TYPES[t][0] for t in TRUTH["types"]])),
        )
        self.assert_values(frame.to_dict(as_series=False), empty=empty)
        reader = pq.ParquetFile(pa.BufferReader(saved))
        self.assertEqual(
            reader.schema_arrow,
            pa.schema(
                [
                    pa.field(name, TYPES[kind][1])
                    for name, kind in zip(TRUTH["columns"], TRUTH["types"], strict=True)
                ]
            ),
        )
        self.assertIsNone(reader.metadata.metadata)
        self.assertEqual(
            [reader.schema.column(i).physical_type for i in range(7)],
            [
                "BYTE_ARRAY",
                "BYTE_ARRAY",
                "INT64",
                "DOUBLE",
                "INT32",
                "INT64",
                "BYTE_ARRAY",
            ],
        )
        timestamp = json.loads(reader.schema.column(5).logical_type.to_json())
        self.assertEqual(timestamp["Type"], "Timestamp")
        self.assertIs(timestamp["isAdjustedToUTC"], False)
        self.assertEqual(timestamp["timeUnit"], "microseconds")
        for i in range(7):
            self.assertEqual(reader.schema.column(i).max_definition_level, 1)
        self.assert_values(reader.read().to_pydict(), empty=empty)

    def read_csv(self, saved, *, empty):
        expected = TRUTH["csv"].splitlines(keepends=True)[0] if empty else TRUTH["csv"]
        self.assertEqual(saved, expected.encode("utf-8"))
        # CSV carries text, with types supplied by a consuming specification.
        # Its ingestion contract treats both bare and quoted empties as missing.
        # Exact independently authored bytes above also pin their different forms.
        rows = list(csv.reader(io.StringIO(expected)))
        values = {
            name: [row[i] or None for row in rows[1:]]
            for i, name in enumerate(TRUTH["columns"])
        }
        frame = pl.read_csv(
            io.BytesIO(saved),
            schema={name: pl.String for name in TRUTH["columns"]},
            null_values="",
        )
        self.assertEqual(frame.columns, TRUTH["columns"])
        self.assertEqual(frame.to_dict(as_series=False), values)
        table = pacsv.read_csv(
            pa.BufferReader(saved),
            convert_options=pacsv.ConvertOptions(
                column_types={name: pa.string() for name in TRUTH["columns"]},
                null_values=[""],
                strings_can_be_null=True,
                quoted_strings_can_be_null=True,
            ),
        )
        self.assertEqual(table.column_names, TRUTH["columns"])
        self.assertEqual(table.to_pydict(), values)

    def read_submission(self, root, *, profile, empty):
        document = yamaa.project(root / "project.yaml").submission(
            "adam", created=TRUTH["created"]
        )
        if empty:
            # A transported dataset without rows must declare it has no data.
            self.assertEqual(
                document.issues.to_polars()["condition"].to_list(),
                ["transport_dataset_empty"],
            )
            with self.assertRaises(yamaa.Error):
                document.save()
            self.assertFalse((root / "test.json").exists())
            self.assertFalse((root / "define.xml").exists())
            return
        self.assertEqual(document.issues.num_rows, 0, document.issues)
        for name in ("spec.yaml", "project.yaml", f"output.{profile}"):
            (root / name).unlink()
        document.save()
        saved = (root / "test.json").read_bytes()
        define = (root / "define.xml").read_bytes()
        # A repeated save rewrites the held bytes, so a changed file is restored.
        (root / "test.json").write_bytes(b"sentinel")
        document.save()
        self.assertEqual((root / "test.json").read_bytes(), saved)
        self.assertEqual((root / "define.xml").read_bytes(), define)
        # Preserve JSON's signed -0 before applying its declared float type.
        data = json.loads(
            saved, parse_int=lambda text: -0.0 if text == "-0" else int(text)
        )
        self.assertEqual(data["datasetJSONVersion"], "1.1.0")
        self.assertEqual(data["datasetJSONCreationDateTime"], TRUTH["created"])
        self.assertEqual(data["records"], 0 if empty else 4)
        self.assertEqual([c["name"] for c in data["columns"]], TRUTH["columns"])
        self.assertEqual(
            [c["dataType"] for c in data["columns"]],
            ["string", "string", "integer", "float", "date", "datetime", "string"],
        )
        values = {name: [] for name in TRUTH["columns"]}
        for row in data["rows"]:
            self.assertEqual(len(row), len(TRUTH["columns"]))
            for name, kind, value in zip(
                TRUTH["columns"], TRUTH["types"], row, strict=True
            ):
                if kind == "int" and value is not None:
                    self.assertIs(type(value), int)
                if kind in ("date", "datetime") and value is not None:
                    value = (
                        datetime.date.fromisoformat(value)
                        if kind == "date"
                        else datetime.datetime.fromisoformat(value)
                    )
                values[name].append(value)
        if profile == "csv" and not empty:
            # CSV rereading has its documented loss of collected empty text.
            for name, kind in zip(TRUTH["columns"], TRUTH["types"], strict=True):
                expected = TRUTH["values"][name]
                if name == "EMPTY":
                    expected = [None] * 4
                self.assertEqual(
                    [normalized(kind, value) for value in values[name]], expected, name
                )
        else:
            self.assert_values(values, empty=empty)
        # The independent XML reader sees the same public Save artifact and its
        # references agree with the JSON metadata, including the declared types.
        xml = etree.fromstring((root / "define.xml").read_bytes())
        ns = {
            "o": "http://www.cdisc.org/ns/odm/v1.3",
            "d": "http://www.cdisc.org/ns/def/v2.1",
        }
        items = xml.xpath("//o:ItemDef", namespaces=ns)
        self.assertEqual([item.get("Name") for item in items], TRUTH["columns"])
        self.assertEqual(
            [item.get("OID") for item in items], [c["itemOID"] for c in data["columns"]]
        )
        self.assertEqual(
            xml.xpath("//d:leaf/@*[local-name()='href']", namespaces=ns),
            ["test.json"],
        )

    def test_public_held_saves_cover_every_type_and_empty_table(self):
        for profile in ("csv", "parquet"):
            for empty in (False, True):
                with (
                    self.subTest(profile=profile, empty=empty),
                    tempfile.TemporaryDirectory(
                        prefix="yamaa-saved-readers-"
                    ) as directory,
                ):
                    root = Path(directory)
                    specification = json.loads(json.dumps(TRUTH["specification"]))
                    specification["output"]["path"] = f"output.{profile}"
                    (root / "spec.yaml").write_text(json.dumps(specification))
                    # The authored submission metadata is a project document.
                    (root / "project.yaml").write_text(json.dumps(TRUTH["environment"]))
                    source = root / "input.csv"
                    source.write_text(
                        TRUTH["input"].splitlines(keepends=True)[0]
                        if empty
                        else TRUTH["input"],
                        encoding="utf-8",
                    )
                    built = yamaa.spec(root / "spec.yaml").run()
                    self.assertEqual(built.issues.num_rows, 0, built.issues)
                    self.assert_values(
                        built.output.to_polars().to_dict(as_series=False), empty=empty
                    )
                    output = root / f"output.{profile}"
                    self.assertFalse(output.exists())
                    source.unlink()
                    original = (root / "spec.yaml").read_bytes()
                    (root / "spec.yaml").unlink()
                    built.save()
                    saved = output.read_bytes()
                    for _ in range(2):
                        (self.read_csv if profile == "csv" else self.read_parquet)(
                            saved, empty=empty
                        )
                        built.save()
                        self.assertEqual(output.read_bytes(), saved)
                    (root / "spec.yaml").write_bytes(original)
                    self.read_submission(root, profile=profile, empty=empty)


if __name__ == "__main__":
    print(
        f"Saved artifact readers: Polars {pl.__version__}, PyArrow {pa.__version__}, "
        f"lxml {'.'.join(map(str, etree.LXML_VERSION[:3]))}"
    )
    unittest.main()
