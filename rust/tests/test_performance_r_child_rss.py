"""Authored refusal fixtures for the real hosted R process preflight."""

import copy
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "tools"))
import check_r_child_peak_rss as control


class PlainRProcessControl(unittest.TestCase):
    def fixture(self):
        response = {"pid": 123, "runtime_version": "4.6.1",
                    "jsonlite_version": "2.0.0", "private_library": "/private",
                    "allocation_bytes": control.ALLOCATION,
                    "page_stride": control.PAGE_STRIDE,
                    "touched_page_count": control.ALLOCATION // control.PAGE_STRIDE,
                    "touched_byte_sum": 165 * (control.ALLOCATION // control.PAGE_STRIDE),
                    "requested_exit_code": 7}
        process = {"pid": 123, "exit_code": 7, "peak_rss": {
            "provider": "GetProcessMemoryInfo(child_handle).PeakWorkingSetSize",
            "raw": 80 * 1024 * 1024, "bytes": 80 * 1024 * 1024,
            "original_unit": "bytes", "system": "Windows",
            "scope": control.child_peak_rss.SCOPE}}
        return process, response

    def test_actual_worker_pid_must_be_the_observed_broker_child(self):
        process, response = self.fixture()
        control.validate(process, response, "Windows", "/private")
        for key, value in [("pid", 124), ("exit_code", 0)]:
            wrong = dict(process, **{key: value})
            with self.assertRaises(ValueError):
                control.validate(wrong, response, "Windows", "/private")

    def test_wrapper_sized_peak_cannot_qualify_the_touched_child(self):
        process, response = self.fixture()
        wrong = copy.deepcopy(process)
        wrong["peak_rss"].update(raw=4 * 1024 * 1024, bytes=4 * 1024 * 1024)
        with self.assertRaises(ValueError):
            control.validate(wrong, response, "Windows", "/private")

    def test_allocation_and_runtime_namespace_proof_are_required(self):
        process, response = self.fixture()
        for key, value in [("runtime_version", "4.5.0"),
                           ("jsonlite_version", "1.9.0"),
                           ("private_library", "/other"),
                           ("touched_page_count", 1),
                           ("touched_byte_sum", 0), ("pid", True)]:
            with self.subTest(key=key), self.assertRaises(ValueError):
                control.validate(process, dict(response, **{key: value}), "Windows", "/private")
