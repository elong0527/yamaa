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
            "schema_model.tsv",
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
                len(cases),
                8
                if name == "schema_model.tsv"
                else (8 if name == "schema_layer_admission.tsv" else 6),
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

    def test_model_field_mutations_match_installed_reference(self):
        """Compare every normalized model field, including nested metadata unions."""
        import copy

        from pydantic import BaseModel, RootModel, ValidationError
        from yamaa.adapters._native_schema_wire import encode_tree
        from yamaa.specification.models import Specification

        document = {
            "schema_version": "1.0",
            "domain": "X",
            "input": {"S": {"path": "s"}},
            "keys": [],
            "output": {"path": "o", "columns": [], "order_by": [{"variable": "A"}]},
            "columns": [
                {
                    "name": "A",
                    "type": "int",
                    "derivation": {"value": {"literal": 1}},
                    "submission": {
                        "origin": {
                            "type": "Derived",
                            "documents": [
                                {
                                    "document": "doc",
                                    "pages": {"type": "PhysicalRef", "refs": "1"},
                                }
                            ],
                        },
                        "method": {
                            "description": "D",
                            "expression": {"context": "C", "code": "x"},
                        },
                        "comment": {"text": "C"},
                    },
                }
            ],
            "intermediates": [
                {
                    "id": "I",
                    "dataset": "S",
                    "key": {"A": {"literal": 1}},
                    "between": {"value": "A"},
                    "verifications": [{"unique": {"columns": ["A"]}}],
                }
            ],
            "rows": [{"id": "R", "derivations": {}}],
            "submission": {
                "label": "L",
                "class": "FINDINGS",
                "structure": "one",
                "repeating": False,
            },
        }
        model = Specification.model_validate(document, strict=True)
        base = model.model_dump(by_alias=True, exclude_unset=True)
        Specification.model_validate(base, strict=True)
        models = []

        def collect(value, path=()):
            if isinstance(value, RootModel):
                return
            if isinstance(value, BaseModel):
                models.append((path, type(value)))
                for name, field in type(value).model_fields.items():
                    collect(getattr(value, name), (*path, field.alias or name))
            elif isinstance(value, dict):
                for name, item in value.items():
                    collect(item, (*path, name))
            elif isinstance(value, list):
                for index, item in enumerate(value):
                    collect(item, (*path, index))

        collect(model)
        self.assertEqual(len({kind for _, kind in models}), 19)
        request = json.loads(self.rows()[0]["request"])
        snapshot, _ = yamaa_native._compile_schema(
            json.dumps({"protocol": "schema/1", "schema": request["schema"]})
        )
        values = [None, True, 1, 1.0, "", "unknown", [], {}, [1], {"x": None}, {1: 1}]
        for path, kind in models:
            for name, field in kind.model_fields.items():
                key = field.alias or name
                for value in values:
                    changed = copy.deepcopy(base)
                    target = changed
                    for part in path:
                        target = target[part]
                    target[key] = value
                    with self.subTest(model=kind.__name__, field=key, value=value):
                        try:
                            expected = Specification.model_validate(
                                changed, strict=True
                            )
                        except ValidationError as error:
                            expected = [
                                (
                                    ".".join(str(p) for p in item["loc"]) or "$",
                                    item["msg"],
                                )
                                for item in error.errors(
                                    include_url=False, include_input=False
                                )
                            ]
                        else:
                            expected = expected.default_driver
                        result = json.loads(
                            snapshot.analyze(
                                json.dumps(
                                    {
                                        "protocol": "schema/1",
                                        "queries": [
                                            {
                                                "operation": "validate_model",
                                                "document": encode_tree(changed),
                                            }
                                        ],
                                    }
                                )
                            )
                        )["outcome"]["results"][0]
                        actual = (
                            [
                                (
                                    finding["path"],
                                    finding["context"][0]["value"]["value"],
                                )
                                for finding in result["diagnostics"]
                            ]
                            if result["status"] == "invalid"
                            else result["default_driver"]
                        )
                        self.assertEqual(actual, expected)

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
