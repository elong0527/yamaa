"""Installed yamaa.info(): what the engine is and where it runs (D41).

The facade's own tests cover each field (the info tests of the yamaa crate);
this checks what the installed host returns for the shared truth, which R
reads too. `info()` and a selftest's `info` are one construction in the
binding, so they are equal; the selftest suite compares that record with the
facts it derives from this interpreter.
"""

import inspect
import platform
import unittest

from host_fixtures import load as load_truth

import yamaa
from yamaa import _native

TRUTH = load_truth(__file__, "info")


class Info(unittest.TestCase):
    def test_info_takes_no_arguments_and_is_exported(self):
        self.assertEqual(list(inspect.signature(yamaa.info).parameters), [])
        self.assertIn("info", yamaa.__all__)

    def test_info_gives_the_fields_in_order(self):
        info = yamaa.info()
        self.assertIsInstance(info, dict)
        self.assertEqual(list(info), TRUTH["fields"])
        self.assertEqual(info["version"], _native._CORE_VERSION)
        # A shipped build compiles every feature and reads every language
        # version of the truth.
        self.assertEqual(info["language_versions"], TRUTH["language_versions"])
        self.assertEqual(info["features"], TRUTH["features"])
        self.assertIn(info["host"], TRUTH["hosts"])
        self.assertEqual(info["host"], "python")
        self.assertEqual(info["host_version"], platform.python_version())
        self.assertIsInstance(info["platform"], str)

    def test_info_equals_the_information_of_a_selftest(self):
        self.assertEqual(yamaa.info(), yamaa.selftest().info)

    def test_each_call_returns_a_new_record(self):
        info = yamaa.info()
        info["features"].clear()
        self.assertEqual(yamaa.info()["features"], TRUTH["features"])


if __name__ == "__main__":
    unittest.main()
