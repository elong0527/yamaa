"""Run against the installed wheel from a directory outside the checkout."""

import csv
import json
import unittest
from importlib.metadata import distribution
from pathlib import Path

import yamaa_native


class InstallationTests(unittest.TestCase):
    def test_unicode_data_notice_is_installed(self):
        """The notice accompanies both direct and independently rebuilt wheels."""
        package = distribution("yamaa-native")
        notices = [p for p in package.files if p.name == "LICENSE-UNICODE.txt"]
        self.assertEqual(len(notices), 1)
        notice = package.locate_file(notices[0]).read_text(encoding="utf-8")
        self.assertTrue(notice.startswith("UNICODE LICENSE V3"))
        self.assertIn("Permission is hereby granted, free of charge", notice)
        self.assertIn("authorization of the copyright holder.", notice)

    def test_capabilities_and_embedded_resource(self):
        self.assertEqual(
            yamaa_native.engine_info(),
            {
                "core_version": "0.1.0",
                "protocol_version": "installation-probe/1",
                "execution_supported": False,
                "installation_resource": "yamaa native installation probe\n",
            },
        )

    def test_each_call_owns_its_result(self):
        info = yamaa_native.engine_info()
        info["execution_supported"] = True
        self.assertIs(yamaa_native.engine_info()["execution_supported"], False)


def assert_shared_vectors(test, invoke, fixture):
    """Replay shared expected text and recover after malformed requests."""
    with Path(__file__).with_name(fixture).open(encoding="utf-8", newline="") as stream:
        vectors = list(csv.DictReader(stream, delimiter="\t"))
    for vector in vectors:
        with test.subTest(case=vector["id"]):
            if vector["expected"].startswith("error:"):
                with test.assertRaises(ValueError) as caught:
                    invoke(vector["request"])
                test.assertEqual(str(caught.exception), vector["expected"][6:])
            else:
                test.assertEqual(invoke(vector["request"]), vector["expected"])
    test.assertEqual(invoke(vectors[0]["request"]), vectors[0]["expected"])


class ScalarTransportTests(unittest.TestCase):
    """Exercise the installed extension without importing the reference engine."""

    def test_shared_vectors_and_recovery(self):
        """Independent truth checks each wire value and the next call after failure."""
        assert_shared_vectors(
            self, yamaa_native.scalar_round_trip, "scalar_transport.tsv"
        )

    def test_ownership_limits_and_host_types(self):
        """No retained input buffer, result alias or failure state contaminates reuse."""
        request = '{"protocol":"scalar/1","value":{"int":"9007199254740993"}}'
        result = yamaa_native.scalar_round_trip(request)
        del request
        for _ in range(100):
            self.assertEqual(yamaa_native.scalar_round_trip(result), result)
        for invalid in [None, 1, {}, []]:
            with self.assertRaises(TypeError):
                yamaa_native.scalar_round_trip(invalid)
        with self.assertRaisesRegex(ValueError, "exceeds byte limit"):
            yamaa_native.scalar_round_trip("x" * 1048577)
        self.assertEqual(yamaa_native.scalar_round_trip(result), result)


class NumericTransportTests(unittest.TestCase):
    """Exercise actual native compilation, lifecycle and diagnostics outside the checkout."""

    def test_shared_vectors(self):
        """All positive, negative, unsupported and malformed cases use written truth."""
        assert_shared_vectors(
            self, yamaa_native.evaluate_numeric, "numeric_transport.tsv"
        )

    def test_limits_and_exact_diagnostics(self):
        """Resource outcomes precede resolution and long errors retain exact decimal text."""
        request = {
            "protocol": "numeric/1",
            "expression": "A" * 65537,
            "column_path": "columns.A",
            "target": "int",
            "bindings": [],
        }
        result = json.loads(yamaa_native.evaluate_numeric(json.dumps(request)))
        self.assertEqual(result["outcome"]["status"], "limit")
        self.assertEqual(result["outcome"]["resource"], "bytes")
        self.assertEqual(result["outcome"]["limit"], "65536")
        self.assertEqual(result["resolutions"], [])
        self.assertEqual(result["handler_counts"], [])
        request["expression"] = "9" * 5000
        output = yamaa_native.evaluate_numeric(json.dumps(request))
        del request
        value = json.loads(output)["outcome"]["diagnostic"]["context"]["value"]
        self.assertEqual(value, {"str": "9" * 5000})
        with self.assertRaisesRegex(ValueError, "exceeds resource limit"):
            yamaa_native.evaluate_numeric(" " * 1048577)
        for invalid in (None, [], 1):
            with self.assertRaises(TypeError):
                yamaa_native.evaluate_numeric(invalid)

    def test_repeated_calls_do_not_share_handler_counts(self):
        """Every invocation owns its inputs, result and run-local accounting."""
        request = json.dumps(
            {
                "protocol": "numeric/1",
                "expression": "1.5",
                "column_path": "columns.A",
                "target": "int",
                "bindings": [],
                "unconvertible": {"value": {"int": "7"}},
            }
        )
        owned = yamaa_native.evaluate_numeric(request)
        for _ in range(100):
            self.assertEqual(yamaa_native.evaluate_numeric(request), owned)
        del request
        self.assertEqual(json.loads(owned)["handler_counts"][0]["count"], "1")


class TableTransportTests(unittest.TestCase):
    """Exercise installed copied IPC and lossless inspection with independent truth."""

    def test_shared_table_truth(self):
        """Both input and sanitized output preserve values, types and chunk order."""
        root = Path(__file__).with_name("tables")
        expected = json.loads((root / "expected.json").read_text(encoding="utf-8"))
        for name, truth in expected.items():
            request = (root / f"{name}.arrow").read_bytes()
            self.assertEqual(json.loads(yamaa_native.table_snapshot(request)), truth)
            result = yamaa_native.table_round_trip(request)
            del request
            for _ in range(5):
                self.assertEqual(json.loads(yamaa_native.table_snapshot(result)), truth)
                result = yamaa_native.table_round_trip(result)

    def test_rejections_and_recovery(self):
        """Invalid types, framing and oversize requests never poison the next call."""
        for invalid in (None, "text", 1, []):
            with self.assertRaises(TypeError):
                yamaa_native.table_round_trip(invalid)
        for invalid in (b"", b"ARROW1", b"\xff" * 8):
            with self.assertRaises(ValueError):
                yamaa_native.table_snapshot(invalid)
        with self.assertRaisesRegex(ValueError, "input exceeds byte limit"):
            yamaa_native.table_round_trip(b"x" * (8 * 1024 * 1024 + 1))
        self.test_shared_table_truth()


if __name__ == "__main__":
    unittest.main()
