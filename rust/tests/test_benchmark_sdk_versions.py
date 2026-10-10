"""Independent release-mismatch and immutable-lock guard witnesses."""

import importlib.util
import json
import tempfile
import unittest
from pathlib import Path

spec = importlib.util.spec_from_file_location(
    "benchmark_sdk_versions", Path(__file__).parents[1] / "tools/check_benchmark_sdk_versions.py"
)
guard = importlib.util.module_from_spec(spec)
spec.loader.exec_module(guard)


def fixture(root):
    (root / "R/yamaa").mkdir(parents=True)
    (root / "python").mkdir()
    (root / "R/yamaa/DESCRIPTION").write_text("Package: yamaa\nVersion: 0.3.0\n")
    (root / "python/pyproject.toml").write_text('[project]\nname="yamaa"\nversion="0.3.0"\n')
    for name in ("first", "second"):
        lock = root / "benchmarks" / name / "r/renv.lock"
        lock.parent.mkdir(parents=True)
        lock.write_text(json.dumps({"Packages": {
            "yamaa": {"Package": "yamaa", "Version": "0.3.0"},
            "projectprogram": {"Package": "projectprogram", "Version": "7.2.0"},
        }}))
    return root / "benchmarks/first/r/renv.lock"


class BenchmarkSdkVersions(unittest.TestCase):
    def test_coherent_sdk_pins_do_not_change_program_versions_or_lock_bytes(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            fixture(root)
            before = {p: p.read_bytes() for p in root.rglob("*") if p.is_file()}
            self.assertEqual(guard.violations(root), [])
            self.assertEqual({p: p.read_bytes() for p in before}, before)

    def test_original_release_mismatch_retains_the_exact_lock(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            lock = fixture(root)
            lock.write_text(lock.read_text().replace('"0.3.0"', '"0.2.0"'))
            before = lock.read_bytes()
            self.assertEqual(guard.violations(root), [
                "benchmarks/first/r/renv.lock: SDK version '0.2.0' differs from prepared '0.3.0'"
            ])
            self.assertEqual(lock.read_bytes(), before)

    def test_missing_wrongly_named_and_nontext_sdk_entries_refuse(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            lock = fixture(root)
            for sdk in (None, [], {"Package": "other", "Version": "0.3.0"},
                        {"Package": "yamaa"}, {"Package": "yamaa", "Version": 3},
                        {"Package": "yamaa", "Version": None}):
                with self.subTest(sdk=sdk):
                    lock.write_text(json.dumps({"Packages": {"yamaa": sdk}}))
                    before = lock.read_bytes()
                    self.assertEqual(len(guard.violations(root)), 1)
                    self.assertEqual(lock.read_bytes(), before)

    def test_prepared_host_version_disagreement_and_missing_r_version_refuse(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            fixture(root)
            for text in ('[project]\nversion="0.4.0"\n', 'project="invalid"\n',
                         '[project]\nname="yamaa"\n'):
                with self.subTest(text=text):
                    (root / "python/pyproject.toml").write_text(text)
                    self.assertEqual(guard.violations(root), [
                        "prepared Python/R SDK versions differ"
                    ])
            description = root / "R/yamaa/DESCRIPTION"
            for text in ("Package: yamaa\n", "Version: \n", "Version: 0.3.0\nVersion: 0.4.0\n"):
                with self.subTest(text=text):
                    description.write_text(text)
                    self.assertEqual(guard.violations(root), [
                        "R/yamaa/DESCRIPTION must declare exactly one nonempty Version"
                    ])

    def test_absent_locks_and_original_json_shapes_do_not_pass(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            lock = fixture(root)
            for text in ("[]", "null", '{"Packages": []}', "{broken"):
                with self.subTest(text=text):
                    lock.write_text(text)
                    self.assertEqual(len(guard.violations(root)), 1)
                    self.assertEqual(lock.read_text(), text)
            for path in root.glob("benchmarks/*/r/renv.lock"):
                path.unlink()
            self.assertEqual(guard.violations(root), ["no benchmark R SDK locks found"])


if __name__ == "__main__":
    unittest.main()
