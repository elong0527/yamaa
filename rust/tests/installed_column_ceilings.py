"""Installed width boundaries for compiler, file and producer source admission."""

import json
import tempfile
import unittest
from pathlib import Path

import pyarrow as pa
import pyarrow.parquet as pq
import yamaa
from yamaa import _native


class ColumnCeilings(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="yamaa-column-ceilings-")
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)

    def write_case(
        self,
        name,
        *,
        columns=1,
        fields=1,
        typed=False,
        source_format="csv",
        selected=None,
    ):
        root = self.root / name
        root.mkdir()
        names = ["ID", *[f"C{index:04d}" for index in range(1, columns)]]
        header = ["ID", *[f"F{index:04d}" for index in range(1, fields)]]
        values = {field: ["S-001" if field == "ID" else "x"] for field in header}
        source = {"path": f"input.{source_format}"}
        if source_format == "parquet":
            pq.write_table(pa.table(values), root / source["path"])
        else:
            (root / source["path"]).write_text(
                ",".join(header)
                + "\n"
                + ",".join(value[0] for value in values.values())
                + "\n",
                encoding="ascii",
            )
        if typed:
            source["types"] = {field: "str" for field in header}
        document = {
            "schema_version": "1.0",
            "domain": "TEST",
            "keys": ["ID"],
            "input": {"SRC": source},
            "columns": [
                {"name": "ID", "type": "str", "label": "ID", "derivation": "SRC.ID"},
                *[
                    {
                        "name": field,
                        "type": "str",
                        "label": field,
                        "derivation": {"literal": "x"},
                    }
                    for field in names[1:]
                ],
            ],
            "output": {
                "path": "out.csv",
                "columns": names if selected is None else selected,
            },
        }
        spec = root / "spec.yaml"
        spec.write_text(json.dumps(document), encoding="ascii")
        return spec

    def assert_build(self, spec, width=1):
        self.assertEqual(yamaa.check(spec).issues.height, 0)
        built = yamaa.domain(spec)
        self.assertEqual(built.issues.height, 0)
        self.assertEqual(built.output.row(0), ("S-001", *["x"] * (width - 1)))
        self.assertEqual(built.output.shape, (1, width))

    def assert_refusal(self, result, compiler=False):
        rows = result.issues.to_dicts()
        self.assertTrue(rows)
        self.assertTrue(
            any(row["condition"] == "engine_rejected" for row in rows), rows
        )
        if compiler:
            self.assertTrue(
                any(
                    json.loads(row["context"])["code"] == "compiler_limit"
                    for row in rows
                ),
                rows,
            )
        if hasattr(result, "save"):
            self.assertIsNone(result.output)
            with self.assertRaises(yamaa.DomainError):
                result.save()

    def test_declared_columns_with_narrow_output(self):
        self.assert_build(
            self.write_case("admit-columns", columns=1024, selected=["ID"])
        )
        spec = self.write_case("refuse-columns", columns=1025, selected=["ID"])
        for operation in (yamaa.check, yamaa.domain):
            self.assert_refusal(operation(spec), compiler=True)

    def test_output_columns_reach_their_own_boundary(self):
        self.assert_build(self.write_case("admit-output", columns=1024), width=1024)
        names = ["ID", *[f"C{index:04d}" for index in range(1, 1024)]]
        # Keep declared columns in range: the extra projection must reach its own
        # count refusal before duplicate-output validation, not the column cap.
        spec = self.write_case("refuse-output", columns=1024, selected=[*names, "ID"])
        for operation in (yamaa.check, yamaa.domain):
            self.assert_refusal(operation(spec), compiler=True)

    def test_typed_source_fields(self):
        self.assert_build(self.write_case("admit-types", fields=1024, typed=True))
        spec = self.write_case("refuse-types", fields=1025, typed=True)
        for operation in (yamaa.check, yamaa.domain):
            self.assert_refusal(operation(spec), compiler=True)

    def test_csv_source_fields(self):
        self.assert_build(self.write_case("admit-csv", fields=1024))
        spec = self.write_case("refuse-csv", fields=1025)
        self.assert_refusal(yamaa.domain(spec))

    def test_parquet_source_fields(self):
        self.assert_build(
            self.write_case("admit-parquet", fields=1024, source_format="parquet")
        )
        spec = self.write_case("refuse-parquet", fields=1025, source_format="parquet")
        self.assert_refusal(yamaa.domain(spec))

    def test_producer_source_fields(self):
        for width in (1024, 1025):
            with self.subTest(width=width):
                producer = self.write_case(f"producer-{width}", columns=width)
                document = json.loads(producer.read_text(encoding="ascii"))
                document["input"]["SRC"] = {"path": "out.csv", "schema": "spec.yaml"}
                document["columns"] = document["columns"][:1]
                document["output"] = {"path": "consumer.csv", "columns": ["ID"]}
                consumer = producer.with_name("consumer.yaml")
                consumer.write_text(json.dumps(document), encoding="ascii")
                if width == 1024:
                    self.assert_build(consumer)
                else:
                    for operation in (yamaa.check, yamaa.domain):
                        self.assert_refusal(operation(consumer))

    def test_in_range_producer_refuses_overwide_runtime_input(self):
        # Producer metadata stays admissible; only its raw source width changes.
        # The direct Rust decoder control separately pins producer_source_fields.
        for width in (1024, 1025):
            with self.subTest(width=width):
                producer = self.write_case(
                    f"producer-input-{width}", columns=1024, fields=width
                )
                document = json.loads(producer.read_text(encoding="ascii"))
                document["input"]["SRC"] = {"path": "out.csv", "schema": "spec.yaml"}
                document["columns"] = document["columns"][:1]
                document["output"] = {"path": "consumer.csv", "columns": ["ID"]}
                consumer = producer.with_name("consumer.yaml")
                consumer.write_text(json.dumps(document), encoding="ascii")
                self.assertEqual(yamaa.check(consumer).issues.height, 0)
                if width == 1024:
                    self.assert_build(consumer)
                else:
                    self.assert_refusal(yamaa.domain(consumer))

    def test_ipc_source_fields_refuse_before_batches_and_allow_retry(self):
        spec = self.write_case("ipc")

        def no_io(*_):
            self.fail("in-memory execution entered file IO")

        run = _native._prepare_document(
            "memory.yaml", spec.read_bytes(), no_io, no_io, no_io
        )
        table = pa.table(
            {"ID": ["S-001"], **{f"F{index:04d}": ["x"] for index in range(1, 1024)}}
        )
        stream = pa.BufferOutputStream()
        with pa.ipc.new_stream(stream, table.schema) as writer:
            writer.write_table(table)
        ipc = stream.getvalue()

        def assert_ipc_build():
            output, outcome = run.execute_arrow(pa.ipc.open_stream(ipc))
            self.assertEqual(json.loads(outcome)["outcome"]["status"], "success")
            self.assertEqual(
                pa.ipc.open_stream(output).read_all().to_pydict(), {"ID": ["S-001"]}
            )

        assert_ipc_build()

        def forbid():
            self.fail("over-wide schema entered batch producer")
            yield

        overwide = pa.RecordBatchReader.from_batches(
            table.schema.append(pa.field("EXCESS", pa.string())), forbid()
        )
        with self.assertRaisesRegex(ValueError, "resource limit"):
            run.execute_arrow(overwide)
        assert_ipc_build()


if __name__ == "__main__":
    unittest.main()
