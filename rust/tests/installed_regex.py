"""Installed regex/1 behavior against authored truth, with no reference fallback."""

import csv
import json
import unittest
from pathlib import Path
from unittest.mock import patch

import yamaa_native


def request(pattern, kind="compile", subject=None):
    """Build a closed request without consulting an engine for expected values."""
    operation = {"kind": kind}
    if subject is not None:
        operation["subject"] = subject
    return json.dumps(
        {"protocol": "regex/1", "pattern": pattern, "operation": operation}
    )


def outcome(encoded):
    """Require both service and portable-grammar versions on every response."""
    response = json.loads(yamaa_native.evaluate_regex(encoded))
    assert response["protocol"] == "regex/1"
    assert response["contract_version"] == "2.0.0"
    return response["outcome"]


class InstalledRegex(unittest.TestCase):
    """Run from direct and source-rebuilt installations outside the checkout."""

    def test_authored_wire_truth_without_python_regex(self):
        """Rust returns exact authored bytes even while Python compilation is forbidden."""
        with (
            Path(__file__)
            .with_name("regex_transport.tsv")
            .open(encoding="ascii", newline="") as stream
        ):
            rows = list(csv.DictReader(stream, delimiter="\t", quoting=csv.QUOTE_NONE))
        self.assertEqual(len(rows), 38)
        with patch("re.compile", side_effect=AssertionError("Python regex fallback")):
            for row in rows:
                with self.subTest(row["id"]):
                    self.assertEqual(
                        yamaa_native.evaluate_regex(row["request"]), row["expected"]
                    )

    def test_host_input_types(self):
        """The binding does not coerce missing, Boolean, numeric or binary requests."""
        for value in (None, True, 1, 1.0, [], {}, b"{}"):
            with self.subTest(value=value), self.assertRaises(TypeError):
                yamaa_native.evaluate_regex(value)

    def test_closed_transport(self):
        """Unused subjects, unknown fields and duplicate keys fail before regex work."""
        for value in (
            "{}",
            "null",
            '{"protocol":"regex/1","pattern":"a","operation":{"kind":"compile","subject":"a"}}',
            '{"protocol":"regex/1","pattern":"a","pattern":"b","operation":{"kind":"compile"}}',
            '{"protocol":"regex/1","pattern":"a","operation":{"kind":"search","subject":null}}',
            '{"protocol":"regex/1","pattern":"a","operation":{"kind":"search","subject":"a","extra":1}}',
            '{"protocol":"regex/1","pattern":"a","operation":{"kind":"compile","kind":"search"}}',
            '{"protocol":"other","pattern":"a","operation":{"kind":"compile"}}',
            request("a", "search", "\ud800"),
            " " * (1_048_576 + 1),
        ):
            with self.subTest(value=value[:100]), self.assertRaises(ValueError):
                yamaa_native.evaluate_regex(value)

    def test_unicode_and_json_capture_ownership(self):
        """Supplementary, combining, newline, quote, slash and NUL text survive exactly."""
        for subject in (
            "\U0001d400",
            "e\u0301",
            "\ufeff",
            "\u0085",
            "\n",
            '"',
            "\\",
            "\0",
        ):
            encoded = yamaa_native.evaluate_regex(
                request("([^]*)", "full_match", subject)
            )
            # A subsequent call cannot replace storage backing an earlier result.
            yamaa_native.evaluate_regex(request("a"))
            self.assertEqual(
                json.loads(encoded)["outcome"],
                {"status": "matched", "group_count": 1, "groups": [subject, subject]},
            )

    def test_compilation_before_matching_and_resource_retry(self):
        """Grammar failure precedes subject processing; limits are not invalid regex."""
        subject = "a" * 500_000
        self.assertEqual(
            outcome(request("(?<=a+)", "search", subject))["status"], "invalid"
        )
        self.assertEqual(
            outcome(request("a", "search", subject)),
            {
                "status": "resource_limit",
                "phase": "match",
                "resource": "state_cells",
                "limit": 1_000_000,
            },
        )
        self.assertEqual(
            outcome(request("a" * 65_537)),
            {
                "status": "resource_limit",
                "phase": "compile",
                "resource": "pattern_bytes",
                "limit": 65_536,
            },
        )
        self.assertEqual(
            outcome(request("a")), {"status": "compiled", "group_count": 0}
        )

    def test_response_limit(self):
        """Escaped overlapping captures stop before an oversized response escapes."""
        source = "(" * 32 + ".{8192}" + ")" * 32
        encoded = yamaa_native.evaluate_regex(
            request(source, "full_match", "\0" * 8192)
        )
        self.assertLess(len(encoded), 512)
        self.assertEqual(
            json.loads(encoded)["outcome"],
            {
                "status": "resource_limit",
                "phase": "response",
                "resource": "response_bytes",
                "limit": 1_048_576,
            },
        )
        self.assertEqual(outcome(request("a", "search", "a"))["groups"], ["a"])

    def test_no_full_engine_capability(self):
        """The bounded service does not turn an installation probe into a full backend."""
        self.assertIs(yamaa_native.engine_info()["execution_supported"], False)


if __name__ == "__main__":
    unittest.main()
