"""The minimal format builds compile what the default build leaves out."""

import json
import unittest
from unittest import mock

from tools.ci import format_features

# The packages and linked symbols each selection must show.
PACKAGES = {
    "none": "yamaa\nyamaa-adapters\nyamaa-core\n",
    "yaml-parquet": (
        "yamaa\nyamaa-saphyr-parser\nparquet\nzstd-sys\nflate2\ncc\n"
        "arrow-array\narrow-buffer\narrow-data\narrow-schema\narrow-ipc\nflatbuffers\n"
    ),
}
# The Arrow crates of the table hand-off, without the embedded-schema encoding.
HAND_OFF = "yamaa\narrow-array\narrow-buffer\narrow-data\narrow-schema\n"
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
        # refusal without readstat, builds in no other job; so does one of
        # the application layer's.
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
                        "-p",
                        "yamaa-application",
                        *features,
                        "--lib",
                        "--no-run",
                    ),
                    self.cargo_calls(selection),
                )

    def test_arrow_comes_with_arrow_or_parquet_and_its_ipc_with_parquet(self):
        tree = format_features.packages(HAND_OFF)
        format_features.check_arrow("hand-off", tree, ["arrow"])
        for enabled, refused in [
            ([], "arrow-array"),
            (["parquet"], "arrow-ipc"),
        ]:
            with self.subTest(enabled=enabled):
                with self.assertRaisesRegex(RuntimeError, f"unexpected {refused} "):
                    format_features.check_arrow("hand-off", tree, enabled)
        with self.assertRaisesRegex(RuntimeError, "unexpected arrow-ipc "):
            format_features.check_arrow(
                "none", format_features.packages("arrow-ipc\n"), []
            )

    def test_the_default_features_less_parquet_compile_the_facade(self):
        calls = []

        def cargo(*args, capture=False):
            calls.append(args)
            return HAND_OFF if args[0] == "tree" else ""

        with mock.patch.object(format_features, "cargo", cargo):
            format_features.qualify_without_parquet()
        self.assertIn(
            (
                "check",
                "--locked",
                "-p",
                "yamaa",
                "--no-default-features",
                "--features",
                "yaml,readstat,odm,arrow",
            ),
            calls,
        )


if __name__ == "__main__":
    unittest.main()
