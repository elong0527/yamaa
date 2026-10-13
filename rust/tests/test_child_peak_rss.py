"""Actual process controls for portable peak RSS and parent-memory isolation."""

import importlib.util
import json
import platform
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

TOOLS = Path(__file__).resolve().parents[1] / "tools"
SPEC = importlib.util.spec_from_file_location("child_peak_rss", TOOLS / "child_peak_rss.py")
rss = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(rss)


class ActualChild(unittest.TestCase):
    @unittest.skipUnless(platform.system() in {"Linux", "Darwin", "Windows"}, "provider unavailable")
    def test_broker_observes_child_allocation_and_preserves_nonzero_exit(self):
        with tempfile.TemporaryDirectory() as directory:
            record = Path(directory) / "process.json"
            command = [sys.executable, "-I", "-c",
                       "a=bytearray(64*1024*1024); raise SystemExit(7)"]
            subprocess.run([sys.executable, "-I", str(TOOLS / "child_peak_rss.py"),
                            "--command", json.dumps(command), "--record", str(record)], check=True)
            actual = json.loads(record.read_text())
            self.assertEqual(actual["exit_code"], 7)
            rss.validate(actual["peak_rss"], platform.system())
            self.assertGreaterEqual(actual["peak_rss"]["bytes"], 64 * 1024 * 1024)

    @unittest.skipUnless(platform.system() == "Linux", "Linux fork/exec high-water control")
    def test_large_controller_rss_does_not_become_small_child_peak(self):
        with tempfile.TemporaryDirectory() as directory:
            record = Path(directory) / "process.json"
            # Touch all pages. The candidate has no allocation large enough to
            # account for this memory, so the lifetime peak must exclude it.
            parent_allocation = bytearray(192 * 1024 * 1024)
            command = [sys.executable, "-I", "-c", "pass"]
            subprocess.run([sys.executable, "-I", str(TOOLS / "child_peak_rss.py"),
                            "--command", json.dumps(command), "--record", str(record)], check=True)
            actual = json.loads(record.read_text())
            self.assertLess(actual["peak_rss"]["bytes"], len(parent_allocation) // 2)

    def test_zero_and_wrong_units_are_refused(self):
        for observation in ({"bytes": 0}, None):
            with self.assertRaises(ValueError):
                rss.validate(observation, "Linux")
