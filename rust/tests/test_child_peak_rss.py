"""Actual process controls for portable peak RSS and parent-memory isolation."""

import importlib.util
import json
import platform
import site
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

TOOLS = Path(__file__).resolve().parents[1] / "tools"
SPEC = importlib.util.spec_from_file_location(
    "child_peak_rss", TOOLS / "child_peak_rss.py"
)
rss = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(rss)


class ActualChild(unittest.TestCase):
    @unittest.skipUnless(
        platform.system() in {"Linux", "Darwin", "Windows"}, "provider unavailable"
    )
    def test_broker_observes_child_allocation_and_preserves_nonzero_exit(self):
        with tempfile.TemporaryDirectory() as directory:
            record = Path(directory) / "process.json"
            worker = Path(directory) / "worker.json"
            # Make every page resident with a nonzero write, then retain the
            # allocation until exit; a zero-filled buffer can stay lazy on Windows.
            command = [
                sys.executable,
                "-I",
                "-c",
                (
                    "import json, os, site, sys, time; from pathlib import Path; "
                    "a=bytearray(64*1024*1024); "
                    "a[::4096]=b'x'*len(a[::4096]); "
                    "Path(sys.argv[1]).write_text(json.dumps({"
                    "'pid':os.getpid(), 'executable':sys.executable, "
                    "'base_executable':sys._base_executable, "
                    "'prefix':sys.prefix, 'site_packages':site.getsitepackages()})); "
                    "time.sleep(0.1); raise SystemExit(7)"
                ),
                str(worker),
            ]
            subprocess.run(
                [
                    sys.executable,
                    "-I",
                    str(TOOLS / "child_peak_rss.py"),
                    "--command",
                    json.dumps(command),
                    "--record",
                    str(record),
                    "--python-worker",
                ],
                check=True,
            )
            actual = json.loads(record.read_text())
            identity = json.loads(worker.read_text())
            self.assertEqual(actual["exit_code"], 7)
            self.assertEqual(actual["pid"], identity["pid"])
            self.assertEqual(identity["executable"], sys.executable)
            self.assertEqual(identity["base_executable"], sys._base_executable)
            self.assertEqual(identity["prefix"], sys.prefix)
            self.assertEqual(identity["site_packages"], site.getsitepackages())
            rss.validate(actual["peak_rss"], platform.system())
            self.assertGreaterEqual(actual["peak_rss"]["bytes"], 64 * 1024 * 1024)

    @unittest.skipUnless(
        platform.system() == "Windows", "Windows venv redirector control"
    )
    def test_another_venv_retains_site_imports_worker_pid_and_nonzero_exit(self):
        import venv

        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            environment = root / "distinct-venv"
            venv.EnvBuilder(with_pip=False).create(environment)
            (environment / "Lib/site-packages/rss_marker.py").write_text(
                "VALUE = 123\n"
            )
            interpreter = environment / "Scripts/python.exe"
            worker = root / "worker.json"
            record = root / "process.json"
            command = [
                str(interpreter),
                "-I",
                "-c",
                (
                    "import json, os, sys; from pathlib import Path; import rss_marker; "
                    "Path(sys.argv[1]).write_text(json.dumps({'pid':os.getpid(), "
                    "'executable':sys.executable, 'prefix':sys.prefix, 'marker':rss_marker.VALUE})); "
                    "raise SystemExit(7)"
                ),
                str(worker),
            ]
            subprocess.run(
                [
                    sys.executable,
                    "-I",
                    str(TOOLS / "child_peak_rss.py"),
                    "--command",
                    json.dumps(command),
                    "--record",
                    str(record),
                    "--python-worker",
                ],
                check=True,
            )
            actual = json.loads(record.read_text())
            identity = json.loads(worker.read_text())
            self.assertEqual(actual["exit_code"], 7)
            self.assertEqual(actual["pid"], identity["pid"])
            self.assertEqual(Path(identity["executable"]), interpreter)
            self.assertEqual(Path(identity["prefix"]), environment)
            self.assertEqual(identity["marker"], 123)

    @unittest.skipUnless(
        platform.system() == "Linux", "Linux fork/exec high-water control"
    )
    def test_large_controller_rss_does_not_become_small_child_peak(self):
        with tempfile.TemporaryDirectory() as directory:
            record = Path(directory) / "process.json"
            # Touch all pages. The candidate has no allocation large enough to
            # account for this memory, so the lifetime peak must exclude it.
            parent_allocation = bytearray(192 * 1024 * 1024)
            parent_allocation[::4096] = b"x" * len(parent_allocation[::4096])
            command = [sys.executable, "-I", "-c", "pass"]
            subprocess.run(
                [
                    sys.executable,
                    "-I",
                    str(TOOLS / "child_peak_rss.py"),
                    "--command",
                    json.dumps(command),
                    "--record",
                    str(record),
                    "--python-worker",
                ],
                check=True,
            )
            actual = json.loads(record.read_text())
            self.assertLess(actual["peak_rss"]["bytes"], len(parent_allocation) // 2)

    def test_zero_and_wrong_units_are_refused(self):
        for observation in ({"bytes": 0}, None):
            with self.assertRaises(ValueError):
                rss.validate(observation, "Linux")
