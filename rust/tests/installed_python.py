"""Run against the installed wheel from a directory outside the checkout."""

import csv
import unittest
from pathlib import Path

import yamaa_native


class InstallationTests(unittest.TestCase):
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


class ScalarTransportTests(unittest.TestCase):
    """Exercise the installed extension without importing the reference engine."""

    def test_shared_vectors_and_recovery(self):
        """Independent truth checks each wire value and the next call after failure."""
        with (
            Path(__file__)
            .with_name("scalar_transport.tsv")
            .open(encoding="utf-8", newline="") as stream
        ):
            vectors = list(csv.DictReader(stream, delimiter="\t"))
        for vector in vectors:
            with self.subTest(case=vector["id"]):
                if vector["expected"].startswith("error:"):
                    with self.assertRaises(ValueError) as caught:
                        yamaa_native.scalar_round_trip(vector["request"])
                    self.assertEqual(str(caught.exception), vector["expected"][6:])
                else:
                    self.assertEqual(
                        yamaa_native.scalar_round_trip(vector["request"]),
                        vector["expected"],
                    )
        self.assertEqual(
            yamaa_native.scalar_round_trip(vectors[0]["request"]),
            vectors[0]["expected"],
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


if __name__ == "__main__":
    unittest.main()
