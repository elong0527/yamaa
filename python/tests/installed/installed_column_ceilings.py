"""Installed width boundaries for compiler, file and producer source admission."""

import json
import tempfile
import unittest
from pathlib import Path

import pyarrow as pa
import pyarrow.parquet as pq

import yamaa


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

    def test_fixed_refusal_and_retry_expose_the_remaining_error(
        self,
    ):
        spec = self.write_case("incomplete-checks", columns=1025, selected=["ID"])
        document = json.loads(spec.read_text())
        document["columns"][1].update(type="int", derivation={"literal": 1})
        document["columns"][2]["derivation"] = {
            "str_case": {"source": "C0001", "to": "upper"}
        }
        environment = spec.with_name("environment.yaml")
        environment.write_text("schema_version: '1.0'\nlanguage: python\n")
        for route in (None, environment):
            spec.write_text(json.dumps(document))
            for operation in (yamaa.check, yamaa.domain):
                refused = operation(spec, environment=route)
                self.assert_refusal(refused, compiler=True)
                rows = refused.issues.to_dicts()
                self.assertEqual(len(rows), 1)
                context = json.loads(rows[0]["context"])
                self.assertNotIn("incomplete_checks_from", context)
                self.assertEqual((context["limit"], context["actual"]), (1024, 1025))
                self.assertIsNone(rows[0]["requirement"])
            repaired_capacity = {**document, "columns": document["columns"][:-1]}
            spec.write_text(json.dumps(repaired_capacity))
            for operation in (yamaa.check, yamaa.domain):
                rows = operation(spec, environment=route).issues.to_dicts()
                self.assertEqual(len(rows), 1)
                self.assertEqual(rows[0]["condition"], "incompatible_input_type")
                self.assertEqual(rows[0]["requirement"], "REQ-0308")
                self.assertNotIn(
                    "incomplete_checks_from", json.loads(rows[0]["context"])
                )
            self.assertFalse(spec.with_name("out.csv").exists())

    def test_empty_inheritance_preserves_wide_case_columns_and_saved_bytes(self):
        for width in (40, 64, 1024):
            with self.subTest(width=width):
                root = self.root / f"layered-{width}"
                root.mkdir()
                (root / "input.csv").write_text(
                    "ID,A,B\nS-001,x,1\nS-002,y,2\nS-003,,3\n", encoding="ascii"
                )
                names = ["ID", *[f"K{index:04d}" for index in range(1, width)]]
                document = {
                    "schema_version": "1.0",
                    "domain": "TEST",
                    "keys": ["ID"],
                    "input": {"SRC": "input.csv"},
                    "columns": [
                        {"name": "ID", "type": "str", "derivation": "SRC.ID"},
                        *[
                            {
                                "name": name,
                                "type": "str",
                                "derivation": {
                                    "case": [
                                        {
                                            "when": f"SRC.A = 'x' AND SRC.B = '{index % 3}'",
                                            "then": {"literal": "P"},
                                        },
                                        {
                                            "when": "SRC.A IS NULL",
                                            "then": {"literal": "Q"},
                                        },
                                        {"otherwise": "SRC.A"},
                                    ]
                                },
                            }
                            for index, name in enumerate(names[1:], 1)
                        ],
                    ],
                    "output": {"path": "out.csv", "columns": names},
                }
                parent = root / "parent.yaml"
                parent.write_text(json.dumps(document), encoding="ascii")
                document["parents"] = []
                explicit = root / "explicit.yaml"
                explicit.write_text(json.dumps(document), encoding="ascii")
                child = root / "child.yaml"
                child.write_text(
                    "schema_version: '1.0'\nparents: [parent.yaml]\n", encoding="ascii"
                )
                grandchild = root / "grandchild.yaml"
                grandchild.write_text(
                    "schema_version: '1.0'\nparents: [child.yaml]\n", encoding="ascii"
                )
                expected = [
                    (
                        "S-001",
                        *["P" if index % 3 == 1 else "x" for index in range(1, width)],
                    ),
                    ("S-002", *["y"] * (width - 1)),
                    ("S-003", *["Q"] * (width - 1)),
                ]
                expected_bytes = (
                    ",".join(names)
                    + "\n"
                    + "".join(",".join(row) + "\n" for row in expected)
                ).encode("ascii")
                for spec in (parent, explicit, child, grandchild):
                    with self.subTest(spec=spec.name):
                        self.assertEqual(yamaa.check(spec).issues.height, 0)
                        built = yamaa.domain(spec)
                        self.assertEqual(built.issues.height, 0)
                        self.assertEqual(built.output.rows(), expected)
                        built.save()
                        self.assertEqual(
                            (root / "out.csv").read_bytes(), expected_bytes
                        )

    def test_pathological_repeated_parents_are_still_refused(self):
        parent = self.write_case("repeated-parents")
        child = parent.with_name("child.yaml")
        child.write_text(
            json.dumps({"schema_version": "1.0", "parents": [parent.name] * 8193}),
            encoding="ascii",
        )
        for operation in (yamaa.check, yamaa.domain):
            result = operation(child)
            self.assert_refusal(result)
            self.assertEqual(result.issues.height, 1)
            self.assertEqual(
                json.loads(result.issues["context"][0]),
                {
                    "code": "resource_limit",
                    "stage": "prepare",
                    "phase": "inheritance",
                    "resource": "parent_occurrences",
                    "limit": 8192,
                    "actual": 8193,
                },
            )

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


if __name__ == "__main__":
    unittest.main()
