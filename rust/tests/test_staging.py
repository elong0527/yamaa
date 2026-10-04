"""Ensure the R archive has real shared sources and cannot overwrite prior work."""

import contextlib
import importlib.util
import io
import tempfile
import unittest
from pathlib import Path

spec = importlib.util.spec_from_file_location(
    "stage_r_package", Path(__file__).parents[1] / "tools/stage_r_package.py"
)
staging = importlib.util.module_from_spec(spec)
spec.loader.exec_module(staging)


class StagingTests(unittest.TestCase):
    def test_shared_sources_are_self_contained(self):
        """The staged archive owns sources and fixture bytes without checkout links."""
        with tempfile.TemporaryDirectory() as directory:
            destination = Path(directory) / "yamaanative"
            with contextlib.redirect_stdout(io.StringIO()):
                staging.stage(destination)
            self.assertTrue((destination / "DESCRIPTION").is_file())
            embedded = destination / "src/rust"
            for source in (staging.WORKSPACE / "crates").rglob("*"):
                if source.is_file() and source.suffix in {".rs", ".toml", ".txt"}:
                    copied = embedded / source.relative_to(staging.WORKSPACE)
                    self.assertFalse(copied.is_symlink())
                    self.assertEqual(source.read_bytes(), copied.read_bytes())
            self.assertTrue((embedded / "rust-toolchain.toml").is_file())
            self.assertFalse((embedded / "target").exists())
            self.assertFalse((embedded / "Cargo.lock").exists())
            for fixture in ("scalar_transport.tsv", "numeric_transport.tsv"):
                self.assertEqual(
                    (destination / "inst" / fixture).read_bytes(),
                    (
                        staging.WORKSPACE
                        / "crates/yamaa-adapters/tests/fixtures"
                        / fixture
                    ).read_bytes(),
                )

            for family in ("tables", "datasets"):
                fixtures = (
                    staging.WORKSPACE / "crates/yamaa-adapters/tests/fixtures" / family
                )
                for source in fixtures.iterdir():
                    self.assertEqual(
                        (destination / "inst" / family / source.name).read_bytes(),
                        source.read_bytes(),
                    )
            self.assertEqual(
                (destination / "inst/function_invocation.tsv").read_bytes(),
                (
                    staging.WORKSPACE
                    / "crates/yamaa-engine/tests/fixtures/function_invocation.tsv"
                ).read_bytes(),
            )

    def test_existing_destination_is_preserved(self):
        with tempfile.TemporaryDirectory() as directory:
            destination = Path(directory)
            marker = destination / "keep.txt"
            marker.write_text("prior build")
            with self.assertRaises(FileExistsError):
                staging.stage(destination)
            self.assertEqual(marker.read_text(), "prior build")
