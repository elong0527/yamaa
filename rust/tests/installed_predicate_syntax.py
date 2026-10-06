"""Installed shared predicate syntax uses authored truth and no host parser."""

import builtins
import csv
import json
import unittest
from pathlib import Path
from unittest.mock import patch

import yamaa_native


def analyze(text):
    """Decode the installed service without involving reference syntax or evaluation."""
    response = json.loads(
        yamaa_native.analyze_predicate(
            json.dumps({"protocol": "predicate-syntax/1", "expression": text})
        )
    )
    assert response["protocol"] == "predicate-syntax/1"
    return response["outcome"]


class PredicateSyntax(unittest.TestCase):
    """Exercise ownership, strict admission and portable parse results after packaging."""

    def test_authored_truth_without_host_parser(self):
        """Repeat every literal wire case with Python parsing/regex entrypoints disabled."""
        original_import = builtins.__import__

        def reject_host_parser(name, *args, **kwargs):
            """A standalone native wheel must not import the Python reference package."""
            if name == "yamaa" or name.startswith("yamaa."):
                raise AssertionError("host parser import")
            return original_import(name, *args, **kwargs)

        with (
            patch("builtins.__import__", side_effect=reject_host_parser),
            patch("re.compile", side_effect=AssertionError("host regex compiler")),
            Path(__file__)
            .with_name("predicate_syntax.tsv")
            .open(encoding="utf-8", newline="") as stream,
        ):
            for row in csv.DictReader(stream, delimiter="\t"):
                for attempt in range(2):
                    with self.subTest(case=row["id"], attempt=attempt):
                        self.assertEqual(
                            yamaa_native.analyze_predicate(row["request"]),
                            row["expected"],
                        )

    def test_wrong_host_types_and_strict_transport(self):
        """No host coercion, Unicode replacement or extra JSON policy fields are allowed."""
        for request in [None, 1, True, b"{}", [], {}]:
            with self.assertRaises(TypeError):
                yamaa_native.analyze_predicate(request)
        for request in [
            "{}",
            '{"protocol":"predicate-syntax/1","expression":null}',
            '{"protocol":"predicate-syntax/1","expression":"TRUE","expression":"FALSE"}',
            '{"protocol":"predicate-syntax/1","expression":"TRUE","limits":{}}',
            '{"protocol":"predicate-syntax/1","expression":"\\ud800"}',
            '{"protocol":"other","expression":"TRUE"}',
            " " * 1048577,
        ]:
            with self.assertRaises(ValueError):
                yamaa_native.analyze_predicate(request)

    def test_unicode_positions_and_owned_literals(self):
        """Byte and scalar offsets differ without normalization, and results own their text."""
        result = analyze("'\U0001f600' = 'e\u0301\x00\\\n'")
        analyze("TRUE")
        self.assertEqual(
            result["ast"]["right"],
            {
                "kind": "literal",
                "type": "str",
                "value": "e\u0301\x00\\\n",
                "position": 6,
            },
        )
        self.assertEqual(
            analyze("'\U0001f600' = @")["position"], {"byte": 9, "character": 6}
        )
        failure = analyze("str_contains('\U0001f600', '('")
        self.assertEqual(failure["position"], {"byte": 21, "character": 18})
        self.assertEqual(failure["requirement"], "REQ-1244")

    def test_resource_refusal_and_fresh_retry(self):
        """A regex compile limit or parse limit stays distinct from grammar rejection."""
        for text, phase, resource in [
            (" " * 65537, "parse", "bytes"),
            ("NOT " * 70 + "TRUE", "parse", "depth"),
            ("A IN (" + ",".join(["1"] * 5000) + ")", "parse", "tokens"),
            ("str_contains(A, 'a{1000001}')", "regex_compile", "repetition"),
        ]:
            with self.subTest(resource=resource):
                result = analyze(text)
                self.assertEqual(
                    (result["status"], result["phase"], result["resource"]),
                    ("resource_limit", phase, resource),
                )
                self.assertEqual(analyze("TRUE")["status"], "parsed")

    def test_syntax_is_not_execution_admission(self):
        """Wide numbers and portable calls parse independently of current dataset support."""
        for text in [
            "A = 1e9999",
            "A = 999999999999999999999999999999",
            "str_contains(A, '\\1(a)')",
            "str_contains(A, '[]')",
        ]:
            self.assertEqual(analyze(text)["status"], "parsed")
        self.assertIs(yamaa_native.engine_info()["execution_supported"], False)


if __name__ == "__main__":
    unittest.main()
