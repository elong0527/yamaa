"""Installed byte decoder replays independent truth without host YAML parsing."""

import builtins
import csv
import json
import unittest
from pathlib import Path
from unittest.mock import patch

from yamaa import _native as yamaa_native


class YamlDecoder(unittest.TestCase):
    def test_text_is_lossless_at_nul_and_unicode_scalar_boundaries(self):
        for source, expected in [
            (rb'"\0"', "\0"),
            (rb'"\uD7FF"', "\ud7ff"),
            (rb'"\U0010FFFF"', "\U0010ffff"),
        ]:
            with self.subTest(source=source):
                outcome = json.loads(yamaa_native.decode_yaml(source))["outcome"]
                self.assertEqual(outcome["status"], "decoded")
                self.assertEqual(
                    outcome["document"],
                    {"root": 0, "nodes": [{"kind": "text", "value": expected}]},
                )

    def test_independent_wire_truth_without_host_interpreter(self):
        with (
            Path(__file__)
            .with_name("yaml_transport.tsv")
            .open(encoding="utf-8", newline="") as stream
        ):
            rows = list(csv.DictReader(stream, delimiter="\t"))
        original_import = builtins.__import__

        def reject_host(name, *args, **kwargs):
            if (
                name == "yamaa"
                or name.startswith("yamaa.")
                or name in {"yaml", "yaml12"}
            ):
                raise AssertionError("host YAML/schema interpreter import")
            return original_import(name, *args, **kwargs)

        with patch("builtins.__import__", side_effect=reject_host):
            for row in rows:
                source = bytes.fromhex(row["source_hex"])
                for attempt in range(2):
                    with self.subTest(case=row["id"], attempt=attempt):
                        self.assertEqual(
                            yamaa_native.decode_yaml(source), row["expected"]
                        )

    def test_host_types_and_explicit_limits(self):
        for source in [
            None,
            1,
            True,
            "x: 1",
            [],
            {},
            bytearray(b"x: 1"),
            memoryview(b"x: 1"),
        ]:
            with self.subTest(source=source), self.assertRaises(TypeError):
                yamaa_native.decode_yaml(source)
        outcome = json.loads(yamaa_native.decode_yaml(b" " * 8388609))["outcome"]
        self.assertEqual(
            outcome,
            {
                "status": "resource_limit",
                "phase": "yaml_source",
                "resource": "source_bytes",
                "limit": 8388608,
            },
        )
        self.assertEqual(
            json.loads(yamaa_native.decode_yaml(b"1"))["outcome"]["status"], "decoded"
        )
        self.assertFalse(yamaa_native.engine_info()["execution_supported"])


if __name__ == "__main__":
    unittest.main()
