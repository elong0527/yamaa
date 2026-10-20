"""The minimal format builds compile what the default build leaves out."""

import json
import unittest
from unittest import mock

from tools.ci import format_features

# The packages and linked symbols each selection must show.
PACKAGES = {
    "none": "yamaa\nyamaa-adapters\nyamaa-core\n",
    "yaml-parquet": "yamaa\nyamaa-saphyr-parser\nparquet\nzstd-sys\nflate2\ncc\n",
}
SYMBOLS = {"none": "", "yaml-parquet": "T ZSTD_compress\n"}


class MinimalBuilds(unittest.TestCase):
    def cargo_calls(self, selection):
        calls = []

        def cargo(*args, capture=False):
            calls.append(args)
            if args[0] == "tree":
                return PACKAGES[selection]
            if "--test" in args:
                return json.dumps({"executable": "/capability-test"}) + "\n"
            return ""

        with (
            mock.patch.object(format_features, "cargo", cargo),
            mock.patch.object(
                format_features.subprocess,
                "check_output",
                return_value=SYMBOLS[selection],
            ),
            mock.patch.object(format_features.subprocess, "run"),
        ):
            format_features.qualify(selection, format_features.FORMATS[selection])
        return calls

    def test_each_selection_compiles_the_adapter_unit_tests(self):
        # A unit test behind a disabled reader, such as the sas_source
        # refusal without readstat, builds in no other job.
        for selection, enabled in format_features.FORMATS.items():
            features = ["--no-default-features"]
            if enabled:
                features += ["--features", ",".join(enabled)]
            with self.subTest(selection=selection):
                self.assertIn(
                    (
                        "test",
                        "--locked",
                        "-p",
                        "yamaa-adapters",
                        *features,
                        "--lib",
                        "--no-run",
                    ),
                    self.cargo_calls(selection),
                )


if __name__ == "__main__":
    unittest.main()
