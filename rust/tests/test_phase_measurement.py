"""Regression coverage for measurement failures visible to the invoking CI job."""

import importlib.util
import io
import subprocess
import tempfile
import unittest
from contextlib import redirect_stderr
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch

TOOL = Path(__file__).resolve().parents[1] / "tools/measure_dataset_phases.py"
SPEC = importlib.util.spec_from_file_location("measure_dataset_phases", TOOL)
measurements = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(measurements)


class MeasurementFailures(unittest.TestCase):
    """Exercise an actual child failure without requiring an installed dataset engine."""

    def test_child_stderr_survives_and_no_success_report_is_written(self):
        """Retain the child explanation, original process error and failed-run output gate."""
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            child = root / "failed_sample.py"
            child.write_text(
                "import sys\n"
                "sys.stderr.write('independent child failure detail\\n')\n"
                "sys.exit(23)\n",
                encoding="ascii",
            )
            output = root / "measurements.json"
            stderr = io.StringIO()
            args = SimpleNamespace(root=root, output=output, repeats=1)
            with (
                patch.object(measurements, "__file__", str(child)),
                redirect_stderr(stderr),
                self.assertRaises(subprocess.CalledProcessError) as raised,
            ):
                measurements.measure(args)
            self.assertEqual(raised.exception.returncode, 23)
            self.assertEqual(
                raised.exception.stderr, "independent child failure detail\n"
            )
            self.assertEqual(stderr.getvalue(), raised.exception.stderr)
            self.assertFalse(output.exists())


if __name__ == "__main__":
    unittest.main()
