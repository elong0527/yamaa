"""The dependency lanes must select floors without changing release inputs."""

import json
import subprocess
import tempfile
import tomllib
import unittest
from pathlib import Path

from tools.ci.dependency_graphs import (
    assert_msrv,
    assert_patches,
    minimum_cargo,
    minimum_python,
    prepare,
)
from tools.paths import ROOT


class DependencyGraphsTests(unittest.TestCase):
    def test_cargo_minimum_syntax_fails_closed(self):
        for value, expected in [
            ("1.0.228", "=1.0.228"),
            (">=0.9.8, <2", "=0.9.8"),
            ("=60.0.0", "=60.0.0"),
        ]:
            with self.subTest(value=value):
                self.assertEqual(minimum_cargo(value), expected)
        for value in ["*", "1", "~1.2", ">1.2.3", "1.2.3 || 2.0.0"]:
            with self.subTest(value=value), self.assertRaises(ValueError):
                minimum_cargo(value)

    def test_python_minimum_preserves_markers_and_extras(self):
        self.assertEqual(
            minimum_python('example[extra]>=2.3; python_version >= "3.12"'),
            'example[extra]==2.3; python_version >= "3.12"',
        )
        with self.assertRaises(ValueError):
            minimum_python("example>2")

    def test_minimum_stage_keeps_patches_and_exact_policy_pins(self):
        locks = {
            name: (ROOT / name).read_bytes()
            for name in ("rust/Cargo.lock", "python/uv.lock")
        }
        with tempfile.TemporaryDirectory() as temporary:
            stage = Path(temporary) / "graph"
            self.assertEqual(prepare(ROOT, stage, "minimum"), "1.90")
            self.assertTrue((stage / ".git").is_dir())
            self.assertEqual(
                subprocess.check_output(["git", "-C", str(stage), "rev-parse", "HEAD"]),
                subprocess.check_output(["git", "-C", str(ROOT), "rev-parse", "HEAD"]),
            )
            self.assertEqual(
                subprocess.check_output(
                    ["git", "-C", str(stage), "ls-files", "rust/Cargo.toml"]
                ),
                b"rust/Cargo.toml\n",
            )
            adapters = tomllib.loads(
                (stage / "rust/crates/yamaa-adapters/Cargo.toml").read_text()
            )["dependencies"]
            self.assertEqual(adapters["toml"]["version"], "=0.9.8")
            self.assertEqual(adapters["arrow-array"]["version"], "=60.0.0")
            self.assertEqual(adapters["saphyr-parser"]["version"], ">=0.0.11")
            core = tomllib.loads(
                (stage / "rust/crates/yamaa-core/Cargo.toml").read_text()
            )["dependencies"]
            self.assertEqual(core["libm"]["version"], "=0.2.16")
            self.assertFalse((stage / "rust/Cargo.lock").exists())
            self.assertEqual(
                (stage / "python/uv.lock").read_bytes(), locks["python/uv.lock"]
            )
            requirements = (stage / "requirements.in").read_text().splitlines()
            self.assertIn("polars==1.35.2", requirements)
            self.assertIn("pyarrow==23.0.1", requirements)
            self.assertIn("pydantic==2.12.4", requirements)
        self.assertEqual(locks, {name: (ROOT / name).read_bytes() for name in locks})

    def test_newest_stage_retains_supported_ranges(self):
        with tempfile.TemporaryDirectory() as temporary:
            stage = Path(temporary) / "graph"
            prepare(ROOT, stage, "newest")
            for path in (
                "rust/crates/yamaa-adapters/Cargo.toml",
                "python/pyproject.toml",
            ):
                self.assertEqual(
                    (stage / path).read_bytes(), (ROOT / path).read_bytes()
                )
            requirements = (stage / "requirements.in").read_text().splitlines()
            self.assertIn("polars>=1.35.2", requirements)
            self.assertIn("pyarrow>=23.0.1", requirements)
            self.assertFalse((stage / "rust/Cargo.lock").exists())

    def test_destination_cannot_change_source_or_overwrite_existing(self):
        for destination in (ROOT, ROOT / "nested-graph"):
            with self.subTest(destination=destination), self.assertRaises(ValueError):
                prepare(ROOT, destination, "minimum")
        with tempfile.TemporaryDirectory() as temporary, self.assertRaises(ValueError):
            prepare(ROOT, Path(temporary), "newest")

    def test_msrv_rejects_newer_packages_even_after_resolver_fallback(self):
        metadata = {
            "packages": [
                {"name": "allowed", "version": "1.0", "rust_version": "1.90"},
                {"name": "unspecified", "version": "1.0", "rust_version": None},
            ]
        }
        assert_msrv(metadata, "1.90.0")
        metadata["packages"].append(
            {"name": "too-new", "version": "2.0", "rust_version": "1.91.0"}
        )
        with self.assertRaisesRegex(ValueError, "too-new 2.0"):
            assert_msrv(json.loads(json.dumps(metadata)), "1.90")

    def test_registry_replacement_cannot_silently_replace_reviewed_parser(self):
        with tempfile.TemporaryDirectory() as temporary:
            rust = Path(temporary)
            (rust / "Cargo.toml").write_text(
                '[patch.crates-io]\nsaphyr-parser = {path="vendor/parser"}\n'
            )
            metadata = {
                "packages": [
                    {
                        "name": "saphyr-parser",
                        "source": None,
                        "manifest_path": str(rust / "vendor/parser/Cargo.toml"),
                    }
                ]
            }
            assert_patches(metadata, rust)
            metadata["packages"][0]["source"] = (
                "registry+https://github.com/rust-lang/crates.io-index"
            )
            with self.assertRaisesRegex(ValueError, "reviewed local patch"):
                assert_patches(metadata, rust)


if __name__ == "__main__":
    unittest.main()
