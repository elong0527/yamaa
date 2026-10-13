"""Missing real R tooling cannot masquerade as mandatory transport acceptance."""

import subprocess
import unittest
from unittest.mock import patch

import _r_check_transport as transport


class RCheckTransportFailures(unittest.TestCase):
    def test_missing_codec_defers_only_optional_work_and_names_required_failure(self):
        failed = subprocess.CompletedProcess(
            [], 9, stdout=b"", stderr=b"required R namespace unavailable: jsonlite\n"
        )
        with (
            patch.object(transport.shutil, "which", return_value="Rscript"),
            patch.object(transport.subprocess, "run", return_value=failed),
        ):
            self.assertIsNone(transport.installation())
            with self.assertRaisesRegex(RuntimeError, r"exit 9.*jsonlite"):
                transport.installation(required=True)

    def test_an_abrupt_real_runner_failure_preserves_exit_and_stdout(self):
        failed = subprocess.CompletedProcess([], 17, stdout=b"runner failed\n", stderr=b"")
        with (
            patch.object(transport.shutil, "which", return_value="Rscript"),
            patch.object(transport.subprocess, "run", return_value=failed),
            self.assertRaises(RuntimeError) as caught,
        ):
            transport.installation(required=True)
        self.assertIn("exit 17", str(caught.exception))
        self.assertIn("stdout: runner failed", str(caught.exception))

    def test_missing_real_runner_never_passes_mandatory_qualification(self):
        with patch.object(transport.shutil, "which", return_value=None):
            self.assertIsNone(transport.installation())
            with self.assertRaisesRegex(RuntimeError, "requires Rscript"):
                transport.installation(required=True)
