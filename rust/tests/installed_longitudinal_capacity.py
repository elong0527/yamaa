"""Installed public capacity acceptance for the approved longitudinal profile."""

import json
import tempfile
import unittest
from pathlib import Path

import pyarrow as pa
import pyarrow.parquet as pq
import yamaa


class LongitudinalCapacity(unittest.TestCase):
    def test_100_columns_and_50000_rows_build_and_save(self):
        names = ["ID", *[f"C{column:03d}" for column in range(1, 100)]]

        def value(row, column):
            return row if column == 0 else 1_000_000_000_000 + (row + column) % 10007

        # The authored source is also independent truth for this direct projection.
        # No engine result is used to construct the expected saved records.
        expected = (
            ",".join(names)
            + "\n"
            + "".join(
                ",".join(str(value(row, column)) for column in range(100)) + "\n"
                for row in range(50_000)
            )
        ).encode("ascii")
        # This source also crosses the former 64 MiB descriptor capture ceiling.
        self.assertGreater(len(expected), 64 * 1024 * 1024)
        for source_profile, profile in (
            ("csv", "csv"),
            ("parquet", "csv"),
            ("csv", "parquet"),
        ):
            with (
                self.subTest(source=source_profile, profile=profile),
                tempfile.TemporaryDirectory() as directory,
            ):
                root = Path(directory)
                source = root / f"input.{source_profile}"
                if source_profile == "csv":
                    source.write_bytes(expected)
                else:
                    table = pa.table(
                        {
                            name: [value(row, column) for row in range(50_000)]
                            for column, name in enumerate(names)
                        }
                    )
                    pq.write_table(table, source)
                    del table
                document = {
                    "schema_version": "1.0",
                    "domain": "WIDE",
                    "keys": ["ID"],
                    "input": {
                        "SRC": {
                            "path": source.name,
                            **(
                                {"types": dict.fromkeys(names, "int")}
                                if source_profile == "csv"
                                else {}
                            ),
                        }
                    },
                    "columns": [
                        {
                            "name": name,
                            "type": "int",
                            "label": name,
                            "derivation": f"SRC.{name}",
                        }
                        for name in names
                    ],
                    "output": {"path": f"output.{profile}", "columns": names},
                }
                entry = root / "spec.yaml"
                entry.write_text(json.dumps(document), encoding="ascii")
                self.assertEqual(yamaa.check(entry).issues.height, 0)
                built = yamaa.domain(entry)
                self.assertEqual(built.issues.to_dicts(), [])
                self.assertEqual(built.output.shape, (50_000, 100))
                for row in (0, 10_006, 10_007, 49_999):
                    self.assertEqual(
                        built.output.row(row),
                        tuple(value(row, column) for column in range(100)),
                    )
                if profile == "parquet":
                    consumer_document = dict(document)
                    consumer_document["input"] = {
                        "SRC": {"path": "output.parquet", "schema": "spec.yaml"}
                    }
                    consumer_document["output"] = {
                        "path": "consumer.csv",
                        "columns": names,
                    }
                    consumer = root / "consumer.yaml"
                    consumer.write_text(json.dumps(consumer_document), encoding="ascii")
                    produced = yamaa.domain(consumer)
                    self.assertEqual(produced.issues.to_dicts(), [])
                    self.assertEqual(produced.output.shape, (50_000, 100))
                    self.assertTrue(produced.save())
                    self.assertEqual((root / "consumer.csv").read_bytes(), expected)
                    del produced
                # Saving uses accepted bytes, including after source changes.
                source.write_bytes(b"changed\n")
                for _ in range(2):
                    self.assertTrue(built.save())
                    output = root / f"output.{profile}"
                    if profile == "csv":
                        self.assertEqual(output.read_bytes(), expected)
                    else:
                        saved = pq.read_table(output)
                        self.assertEqual(saved.column_names, names)
                        self.assertEqual(saved.num_rows, 50_000)
                        for column, name in enumerate(names):
                            self.assertEqual(
                                saved[name].to_pylist(),
                                [value(row, column) for row in range(50_000)],
                            )
                del built


if __name__ == "__main__":
    unittest.main()
