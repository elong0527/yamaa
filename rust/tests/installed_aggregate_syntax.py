"""Installed shared aggregate parser: independent truth and strict host boundaries."""

import csv
import json
import unittest
from pathlib import Path

import yamaa_native
from yamaa.expressions.aggregate import (
    AggregateError,
    aggregate_identifiers,
    aggregate_star_datasets,
    parse_aggregate,
    ungrouped_identifiers,
)


class AggregateSyntax(unittest.TestCase):
    """Exercise installed native artifacts without adding evaluator capabilities."""

    def test_independent_truth_and_reference(self):
        """Both parsers must match the written AST/diagnostics, not just each other."""
        path = Path(__file__).with_name("aggregate_syntax.tsv")
        with path.open(encoding="utf-8", newline="") as stream:
            for row in csv.DictReader(stream, delimiter="\t"):
                with self.subTest(row["id"]):
                    self.assertEqual(
                        yamaa_native.analyze_aggregate(row["request"]), row["expected"]
                    )
                    text = json.loads(row["request"])["expression"]
                    expected = json.loads(row["expected"])["outcome"]
                    try:
                        ast = parse_aggregate(text)
                    except AggregateError as error:
                        self.assertEqual(
                            expected,
                            {
                                "status": "invalid",
                                "condition": error.condition,
                                "requirement": error.requirement,
                                "position": {
                                    "byte": len(text[: error.position].encode()),
                                    "character": error.position,
                                },
                                "context": error.context,
                            },
                        )
                    else:
                        self.assertEqual(
                            expected,
                            {
                                "status": "parsed",
                                "ast": ast,
                                "identifiers": list(aggregate_identifiers(ast)),
                                "star_datasets": list(aggregate_star_datasets(ast)),
                                "ungrouped_identifiers": list(
                                    ungrouped_identifiers(ast)
                                ),
                            },
                        )

    def test_closed_transport_and_retry(self):
        """Invalid host/JSON input is not a grammar failure or cached result."""
        for request in [
            None,
            1,
            [],
            {},
            "{}",
            '{"protocol":"aggregate-syntax/1","expression":null}',
            '{"protocol":"aggregate-syntax/1","expression":"A","unknown":1}',
            '{"protocol":"aggregate-syntax/1","expression":"A","expression":"B"}',
            '{"protocol":"other","expression":"A"}',
        ]:
            with (
                self.subTest(request=request),
                self.assertRaises((TypeError, ValueError)),
            ):
                yamaa_native.analyze_aggregate(request)
        response = json.loads(
            yamaa_native.analyze_aggregate(
                '{"protocol":"aggregate-syntax/1","expression":"A"}'
            )
        )
        self.assertEqual(response["outcome"]["ungrouped_identifiers"], ["A"])

    def test_installed_type_information(self):
        """The source-rebuilt wheel must retain the same public typing surface."""
        package = Path(yamaa_native.__file__).parent
        self.assertTrue((package / "py.typed").is_file())
        stub = (package / "__init__.pyi").read_text(encoding="utf-8")
        self.assertIn("def analyze_aggregate(request: str) -> str:", stub)

    def test_limits_and_no_execution_claim(self):
        """Syntax resources stay separate and parsing does not activate an engine."""
        for text, resource in [
            ("A" * 65537, "bytes"),
            ("(" * 70 + "A" + ")" * 70, "depth"),
        ]:
            response = json.loads(
                yamaa_native.analyze_aggregate(
                    json.dumps({"protocol": "aggregate-syntax/1", "expression": text})
                )
            )
            self.assertEqual(response["outcome"]["status"], "resource_limit")
            self.assertEqual(response["outcome"]["resource"], resource)
        self.assertFalse(yamaa_native.engine_info()["execution_supported"])


if __name__ == "__main__":
    unittest.main()
