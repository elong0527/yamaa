"""Installed public capacity acceptance for the approved longitudinal profile.

The profile's size, its value rule, its source and output formats and how a
run of it divides its input are the `longitudinal-capacity` family of the
shared host truth, which the R `longitudinal_capacity` suite reads too.
"""

import json
import tempfile
import unittest
from pathlib import Path

import pyarrow as pa
import pyarrow.parquet as pq
from host_fixtures import first_difference
from host_fixtures import load as load_truth

import yamaa

TRUTH = load_truth(__file__, "longitudinal-capacity")
ROWS = TRUTH["rows"]
NAMES = [
    TRUTH["id_column"],
    *[
        TRUTH["value_column_name"] % column
        for column in range(1, TRUTH["value_columns"] + 1)
    ],
]


def value(row, column):
    """The authored cell: the row index for the key, then the family's rule."""
    if column == 0:
        return row
    return TRUTH["value_offset"] + (row + column) % TRUTH["value_modulus"]


class LongitudinalCapacity(unittest.TestCase):
    def test_100_columns_and_50000_rows_build_and_save(self):
        # The authored source is also independent truth for this direct projection.
        # No engine result is used to construct the expected saved records.
        expected = (
            ",".join(NAMES)
            + "\n"
            + "".join(
                ",".join(str(value(row, column)) for column in range(len(NAMES))) + "\n"
                for row in range(ROWS)
            )
        ).encode("ascii")
        # This source also crosses the former 64 MiB descriptor capture ceiling.
        self.assertGreater(len(expected), TRUTH["minimum_source_bytes"])
        for case in TRUTH["profiles"]:
            source_profile, profile = case["source"], case["output"]
            with (
                self.subTest(case=case["name"]),
                tempfile.TemporaryDirectory() as directory,
            ):
                root = Path(directory)
                source = root / f"input.{source_profile}"
                if source_profile == "csv":
                    source.write_bytes(expected)
                else:
                    table = pa.table(
                        {
                            name: [value(row, column) for row in range(ROWS)]
                            for column, name in enumerate(NAMES)
                        }
                    )
                    pq.write_table(table, source)
                    del table
                document = {
                    "yamaa": "1.0",
                    "type": "spec",
                    "domain": "WIDE",
                    "keys": [TRUTH["id_column"]],
                    # A typed csv source is a mapping; an untyped parquet
                    # source is written as its bare path.
                    "input": {
                        "SRC": (
                            {
                                "path": source.name,
                                "types": dict.fromkeys(NAMES, "int"),
                            }
                            if source_profile == "csv"
                            else source.name
                        )
                    },
                    "columns": [
                        {
                            "name": name,
                            "type": "int",
                            "label": name,
                            "derivation": f"SRC.{name}",
                        }
                        for name in NAMES
                    ],
                    "output": {"path": f"output.{profile}", "columns": NAMES},
                }
                entry = root / "spec.yaml"
                entry.write_text(json.dumps(document), encoding="ascii")
                self.assertEqual(yamaa.spec(entry).check().issues.num_rows, 0)
                built = yamaa.spec(entry).run()
                self.assertEqual(built.issues.to_pylist(), [])
                # How the run divided its input, which its measurements name
                # (#3794): the hosts' default partitions.
                observed = json.loads(built._native.observations(True))
                self.assertEqual(
                    observed["measurements"]["partitions"], TRUTH["partitions"]
                )
                self.assertEqual(built.output.to_polars().shape, (ROWS, len(NAMES)))
                for row in (0, 10_006, 10_007, ROWS - 1):
                    self.assertEqual(
                        built.output.to_polars().row(row),
                        tuple(value(row, column) for column in range(len(NAMES))),
                    )
                if profile == "parquet":
                    # A link only types the stored artifact: save the producer
                    # before its consumer reads it.
                    built.save()
                    consumer_document = dict(document)
                    consumer_document["input"] = {
                        "SRC": {"path": "output.parquet", "schema": "spec.yaml"}
                    }
                    consumer_document["output"] = {
                        "path": "consumer.csv",
                        "columns": NAMES,
                    }
                    consumer = root / "consumer.yaml"
                    consumer.write_text(json.dumps(consumer_document), encoding="ascii")
                    consumed = yamaa.spec(consumer).run()
                    self.assertEqual(consumed.issues.to_pylist(), [])
                    self.assertEqual(
                        consumed.output.to_polars().shape, (ROWS, len(NAMES))
                    )
                    consumed.save()
                    self.assertEqual((root / "consumer.csv").read_bytes(), expected)
                    del consumed
                # Saving uses accepted bytes, including after source changes.
                source.write_bytes(b"changed\n")
                for _ in range(TRUTH["saves_after_change"]):
                    built.save()
                    output = root / f"output.{profile}"
                    if profile == "csv":
                        self.assertEqual(output.read_bytes(), expected)
                    else:
                        saved = pq.read_table(output)
                        self.assertEqual(saved.column_names, NAMES)
                        self.assertEqual(saved.num_rows, ROWS)
                        # Each column of 50,000 values is compared as a
                        # summary of its first difference (#3882).
                        for column, name in enumerate(NAMES):
                            self.assertIsNone(
                                first_difference(
                                    saved[name].to_pylist(),
                                    [value(row, column) for row in range(ROWS)],
                                ),
                                name,
                            )
                del built


if __name__ == "__main__":
    unittest.main()
