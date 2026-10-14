"""Inspect actual ZIP/tar members without importing the package being built."""

import importlib.util
import io
import tarfile
import tempfile
import unittest
import zipfile
from pathlib import Path

spec = importlib.util.spec_from_file_location(
    "python_package_hygiene",
    Path(__file__).parents[1] / "tools/check_python_package_hygiene.py",
)
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


def write_archive(path, names):
    if path.suffix == ".whl":
        with zipfile.ZipFile(path, "w") as archive:
            for name in names:
                archive.writestr(name, b"raise RuntimeError('do not import')\n")
    else:
        with tarfile.open(path, "w:gz") as archive:
            for name in names:
                payload = b"raise RuntimeError('do not import')\n"
                member = tarfile.TarInfo(name)
                member.size = len(payload)
                archive.addfile(member, io.BytesIO(payload))


class PythonPackageHygiene(unittest.TestCase):
    def test_source_stubs_and_data_pass_without_importing(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for filename, prefix in (
                ("yamaa.whl", ""),
                ("yamaa.tar.gz", "yamaa-0.2.0/python/src/"),
            ):
                with self.subTest(filename=filename):
                    path = root / filename
                    write_archive(
                        path,
                        [
                            prefix + name
                            for name in (
                                "yamaa/__init__.py",
                                "yamaa/_native.pyi",
                                "yamaa/_odm/schema.py",
                                "yamaa/__pycache__notes.py",
                            )
                        ],
                    )
                    self.assertEqual(module.check_archive(path), 4)
            module.check_distributions(root)

    def test_every_retired_import_family_is_rejected_in_both_distribution_forms(self):
        retired = (
            "_reference_domain.py", "application.py", "regex.py",
            "adapters/__init__.py", "expressions/scalar.py", "io/project.py",
            "models/__init__.py", "odm/bindings.py", "planning/__init__.py",
            "runtime/context.py", "schema/__init__.py", "specification/__init__.py",
            "submission/schema.xsd", "verification/__init__.py",
        )
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for filename, prefix in (
                ("yamaa.whl", "yamaa/"),
                ("yamaa.tar.gz", "yamaa-0.4.0/python/src/yamaa/"),
            ):
                for name in retired:
                    with self.subTest(filename=filename, member=name):
                        path = root / filename
                        write_archive(path, [prefix + name])
                        with self.assertRaisesRegex(ValueError, "retired Python evaluator/API"):
                            module.check_archive(path)

    def test_wheel_install_schemes_and_windows_member_spellings_cannot_restore_reference(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "yamaa.whl"
            for name in (
                "yamaa-0.4.0.data/purelib/yamaa/runtime/context.py",
                "yamaa-0.4.0.data/platlib/yamaa/_reference_domain.pyd",
                "yamaa\\Runtime\\context.py",
                "yamaa/regex.pyi",
                "YAMAA/IO.py",
                "YAMAA-0.4.0.DATA/PURELIB/YAMAA/IO.py",
                "YAMAA-0.4.0.DATA/PLATLIB/YAMAA/_REFERENCE_DOMAIN.pyd",
                "YAMAA-0.4.0.DATA\\PURELIB\\YAMAA\\VERIFICATION\\__init__.py",
            ):
                with self.subTest(member=name):
                    write_archive(path, [name])
                    with self.assertRaisesRegex(ValueError, "retired Python evaluator/API") as finding:
                        module.check_archive(path)
                    self.assertIn(name, str(finding.exception))

    def test_source_archive_windows_prefix_aliases_preserve_the_original_finding(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "yamaa.tar.gz"
            for name in (
                "yamaa-0.4.0/PYTHON/SRC/YAMAA/IO.py",
                "yamaa-0.4.0/Python/Src/Yamaa/Verification/__init__.py",
                "yamaa-0.4.0\\PYTHON\\SRC\\YAMAA\\REGEX.PY",
            ):
                with self.subTest(member=name):
                    write_archive(path, [name])
                    with self.assertRaisesRegex(ValueError, "retired Python evaluator/API") as finding:
                        module.check_archive(path)
                    self.assertIn(name, str(finding.exception))

    def test_retained_helpers_and_independent_native_truth_are_allowed(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for filename, prefix in (
                ("yamaa.whl", ""),
                ("yamaa.tar.gz", "yamaa-0.4.0/python/src/"),
            ):
                with self.subTest(filename=filename):
                    path = root / filename
                    names = [prefix + name for name in (
                        "yamaa/_odm/readers.py", "yamaa/_odm/parquet.py",
                        "yamaa/style/__main__.py", "yamaa/style/_fields.json",
                        "yamaa/_native.pyi", "yamaa/__init__.py",
                    )]
                    names.extend((
                        "yamaa-0.4.0/rust/crates/yamaa-core/src/regex.rs",
                        "yamaa-0.4.0/rust/crates/yamaa-adapters/tests/fixtures/expressions.tsv",
                        "yamaa-0.4.0/rust/qualification/producer-cohort/shared-producer-parquet/reference.json",
                    ))
                    write_archive(path, names)
                    self.assertEqual(module.check_archive(path), len(names))

    def test_nested_cache_directories_and_bytecode_are_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for filename, prefix in (
                ("yamaa.whl", ""),
                ("yamaa.tar.gz", "yamaa-0.2.0/python/src/"),
            ):
                for name in (
                    "yamaa/__pycache__/__init__.cpython-314.pyc",
                    "yamaa/submission/__pycache__/module.cpython-314.opt-1.pyc",
                    "yamaa/submission/__pycache__/note.txt",
                    "yamaa/__pycache__/",
                    "yamaa/submission/module.pyc",
                    "yamaa/submission/module.pyo",
                    "yamaa\\submission\\__pycache__\\note.txt",
                ):
                    with self.subTest(filename=filename, member=name):
                        path = root / filename
                        write_archive(path, [prefix + name])
                        with self.assertRaisesRegex(
                            ValueError, "generated Python caches"
                        ):
                            module.check_archive(path)

    def test_missing_distribution_form_does_not_pass(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            with self.assertRaisesRegex(ValueError, "one wheel and one source archive"):
                module.check_distributions(root)
            write_archive(root / "yamaa.whl", ["yamaa/__init__.py"])
            with self.assertRaisesRegex(ValueError, "one wheel and one source archive"):
                module.check_distributions(root)

    def test_stale_multiple_archives_do_not_pass(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for name in ("yamaa.whl", "older.whl", "yamaa.tar.gz"):
                write_archive(root / name, ["yamaa/__init__.py"])
            with self.assertRaisesRegex(ValueError, "one wheel and one source archive"):
                module.check_distributions(root)


if __name__ == "__main__":
    unittest.main()
