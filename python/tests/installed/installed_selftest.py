"""Installed yamaa.selftest(): the bundled cases through the installed engine.

The facade's own tests run every case and each way a case can fail (the
selftest module of the yamaa crate); this checks what the installed host
returns and prints for the shared truth, which R reads too. A shipped build
compiles every feature, so every case runs and passes.
"""

import inspect
import platform
import sys
import unittest

import polars as pl
from host_fixtures import load as load_truth

import yamaa
from yamaa import _native

TRUTH = load_truth(__file__, "selftest")
INFO = load_truth(__file__, "info")
# How Rust names the operating system and architecture of the compiled engine,
# from the names this interpreter reports for its own platform.
OPERATING_SYSTEMS = {"linux": "linux", "darwin": "macos", "win32": "windows"}
ARCHITECTURES = {
    "x86_64": "x86_64",
    "amd64": "x86_64",
    "aarch64": "aarch64",
    "arm64": "aarch64",
}


def expected_info():
    """The engine information the facade reports for this interpreter."""
    system = OPERATING_SYSTEMS[sys.platform]
    architecture = ARCHITECTURES[platform.machine().lower()]
    return {
        "version": _native._CORE_VERSION,
        "language_versions": INFO["language_versions"],
        "features": INFO["features"],
        "host": "python",
        "host_version": platform.python_version(),
        "platform": f"{system}-{architecture}",
    }


class SelfTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.result = yamaa.selftest()

    def test_selftest_takes_no_arguments_and_returns_a_selftest(self):
        self.assertEqual(list(inspect.signature(yamaa.selftest).parameters), [])
        self.assertIsInstance(self.result, yamaa.SelfTest)
        # The result class stays importable outside __all__, as the others do.
        self.assertIn("selftest", yamaa.__all__)
        self.assertNotIn("SelfTest", yamaa.__all__)

    def test_every_bundled_case_passes(self):
        self.assertIn(self.result.status, TRUTH["statuses"])
        self.assertEqual(self.result.status, TRUTH["status"])

    def test_cases_has_one_row_per_case_in_list_order(self):
        cases = self.result.cases
        self.assertIsInstance(cases, pl.DataFrame)
        self.assertEqual(cases.columns, TRUTH["columns"])
        self.assertEqual(cases.dtypes, [pl.String] * len(TRUTH["columns"]))
        self.assertEqual(cases["case"].to_list(), TRUTH["cases"])
        statuses = cases["status"].to_list()
        self.assertTrue(set(statuses) <= set(TRUTH["statuses"]))
        self.assertEqual(statuses, [TRUTH["status"]] * len(TRUTH["cases"]))
        self.assertEqual(cases["message"].to_list(), [None] * len(TRUTH["cases"]))

    def test_info_gives_the_engine_information_of_this_host(self):
        info = self.result.info
        self.assertIsInstance(info, dict)
        self.assertEqual(list(info), INFO["fields"])
        self.assertEqual(info, expected_info())

    def test_printing_gives_the_facade_report_byte_for_byte(self):
        info = expected_info()
        information = TRUTH["information_line"].format(
            version=info["version"],
            language_versions=", ".join(info["language_versions"]),
            host=info["host"],
            host_version=info["host_version"],
            platform=info["platform"],
        )
        text = "\n".join([*TRUTH["lines"], information])
        self.assertEqual(repr(self.result), text)
        self.assertEqual(str(self.result), text)
        self.assertTrue(text.isascii())

    def test_the_result_is_read_only(self):
        for name in ("status", "cases", "info"):
            with self.subTest(name=name), self.assertRaises(AttributeError):
                setattr(self.result, name, None)


if __name__ == "__main__":
    unittest.main()
