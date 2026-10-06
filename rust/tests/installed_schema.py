"""Installed schema service replays independent truth without host interpretation."""

import builtins
import csv
import json
import unittest
from pathlib import Path
from unittest.mock import patch

import yamaa_native


class SchemaService(unittest.TestCase):
    def rows(self):
        rows = []
        for name in (
            "schema_transport.tsv",
            "schema_windows.tsv",
            "schema_composition.tsv",
            "schema_layer_admission.tsv",
            "schema_inheritance_dependencies.tsv",
        ):
            with (
                Path(__file__)
                .with_name(name)
                .open(encoding="utf-8", newline="") as stream
            ):
                cases = list(csv.DictReader(stream, delimiter="\t"))
            self.assertEqual(
                len(cases), 7 if name == "schema_layer_admission.tsv" else 6
            )
            rows.extend(cases)
        return rows

    def test_independent_truth_without_python_interpreter(self):
        original_import = builtins.__import__

        def reject_interpreter(name, *args, **kwargs):
            if (
                name == "yamaa"
                or name.startswith("yamaa.")
                or name in {"yaml", "yaml12"}
            ):
                raise AssertionError("host schema interpreter import")
            return original_import(name, *args, **kwargs)

        rows = self.rows()
        with patch("builtins.__import__", side_effect=reject_interpreter):
            for row in rows:
                for attempt in range(2):
                    with self.subTest(case=row["id"], attempt=attempt):
                        self.assertEqual(
                            yamaa_native.interpret_schema(row["request"]),
                            row["expected"],
                        )

    def test_prepared_snapshot_owns_metadata_and_uses_the_same_results(self):
        for row in self.rows():
            request = json.loads(row["request"])
            expected = json.loads(row["expected"])["outcome"]
            snapshot, response = yamaa_native._compile_schema(
                json.dumps({"protocol": "schema/1", "schema": request["schema"]})
            )
            request["schema"].clear()
            compiled = json.loads(response)["outcome"]
            if expected["status"] == "invalid_schema":
                self.assertIsNone(snapshot)
                self.assertEqual(compiled, expected)
                continue
            self.assertEqual(compiled, expected["schema"])
            self.assertEqual(
                json.loads(
                    snapshot.analyze(
                        json.dumps(
                            {"protocol": "schema/1", "queries": request["queries"]}
                        )
                    )
                )["outcome"],
                {"status": "analyzed", "results": expected["results"]},
            )
            with self.assertRaises(AttributeError):
                snapshot.schema = None

    def test_host_types_and_closed_transport(self):
        for function in [yamaa_native.interpret_schema, yamaa_native._compile_schema]:
            for request in [None, 1, True, b"{}", [], {}]:
                with self.assertRaises(TypeError):
                    function(request)
            for request in [
                "{}",
                " " * 8388609,
                '{"protocol":"schema/1","protocol":"schema/1"}',
            ]:
                with self.assertRaises(ValueError):
                    function(request)
        request = json.loads(self.rows()[0]["request"])
        request["limits"] = {}
        with self.assertRaises(ValueError):
            yamaa_native.interpret_schema(json.dumps(request))
        self.assertFalse(yamaa_native.engine_info()["execution_supported"])


if __name__ == "__main__":
    unittest.main()
