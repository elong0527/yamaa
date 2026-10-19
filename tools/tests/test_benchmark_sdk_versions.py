"""Independent release-mismatch and immutable-lock guard witnesses."""

import json
import tempfile
import unittest
from pathlib import Path

from tools.packaging import check_benchmark_sdk_versions as guard


def fixture(root):
    (root / "R/yamaa").mkdir(parents=True)
    (root / "python").mkdir()
    (root / "tools/packaging").mkdir(parents=True)
    (root / "tools/packaging/release.toml").write_text('version = "0.3.0"\n')
    (root / "R/yamaa/DESCRIPTION").write_text("Package: yamaa\nVersion: 0.3.0\n")
    (root / "python/pyproject.toml").write_text(
        '[project]\nname="yamaa"\nversion="0.3.0"\n'
    )
    for name in ("first", "second"):
        lock = root / "benchmarks" / name / "r/renv.lock"
        lock.parent.mkdir(parents=True)
        lock.write_text(
            json.dumps(
                {
                    "Packages": {
                        "yamaa": {"Package": "yamaa", "Version": "0.3.0"},
                        "projectprogram": {
                            "Package": "projectprogram",
                            "Version": "7.2.0",
                        },
                    }
                }
            )
        )
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
            self.assertEqual(
                guard.violations(root),
                [
                    "benchmarks/first/r/renv.lock: SDK version '0.2.0' differs from prepared '0.3.0'"
                ],
            )
            self.assertEqual(lock.read_bytes(), before)

    def test_missing_wrongly_named_and_nontext_sdk_entries_refuse(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            lock = fixture(root)
            for sdk in (
                None,
                [],
                {"Package": "other", "Version": "0.3.0"},
                {"Package": "yamaa"},
                {"Package": "yamaa", "Version": 3},
                {"Package": "yamaa", "Version": None},
            ):
                with self.subTest(sdk=sdk):
                    lock.write_text(json.dumps({"Packages": {"yamaa": sdk}}))
                    before = lock.read_bytes()
                    self.assertEqual(len(guard.violations(root)), 1)
                    self.assertEqual(lock.read_bytes(), before)

    def test_prepared_host_version_disagreement_and_missing_r_version_refuse(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            fixture(root)
            for text, version in (
                ('[project]\nversion="0.4.0"\n', "'0.4.0'"),
                ('project="invalid"\n', "None"),
                ('[project]\nname="yamaa"\n', "None"),
            ):
                with self.subTest(text=text):
                    (root / "python/pyproject.toml").write_text(text)
                    self.assertEqual(
                        guard.violations(root),
                        [
                            f"python/pyproject.toml: version {version} is not "
                            "'0.3.0', the Python version of release 0.3.0"
                        ],
                    )
            (root / "python/pyproject.toml").write_text(
                '[project]\nname="yamaa"\nversion="0.3.0"\n'
            )
            description = root / "R/yamaa/DESCRIPTION"
            description.write_text("Package: yamaa\nVersion: 0.4.0\n")
            self.assertEqual(
                guard.violations(root),
                [
                    "R/yamaa/DESCRIPTION: Version '0.4.0' is not '0.3.0', "
                    "the R version of release 0.3.0"
                ],
            )
            for text in (
                "Package: yamaa\n",
                "Version: \n",
                "Version: 0.3.0\nVersion: 0.4.0\n",
            ):
                with self.subTest(text=text):
                    description.write_text(text)
                    self.assertEqual(
                        guard.violations(root),
                        [
                            "R/yamaa/DESCRIPTION must declare exactly one nonempty Version"
                        ],
                    )

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

    def test_unreadable_prepared_metadata_and_invalid_toml_remain_violations(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            fixture(root)
            for name in ("R/yamaa/DESCRIPTION", "python/pyproject.toml"):
                path = root / name
                original = path.read_bytes()
                path.unlink()
                self.assertTrue(
                    guard.violations(root)[0].startswith(name + ": unreadable")
                )
                path.mkdir()
                self.assertTrue(
                    guard.violations(root)[0].startswith(name + ": unreadable")
                )
                path.rmdir()
                path.write_bytes(b"\xff")
                self.assertTrue(
                    guard.violations(root)[0].startswith(name + ": unreadable")
                )
                self.assertEqual(path.read_bytes(), b"\xff")
                path.write_bytes(original)
            (root / "python/pyproject.toml").write_text('project = "unterminated')
            self.assertTrue(
                guard.violations(root)[0].startswith(
                    "python/pyproject.toml: unreadable or invalid metadata: "
                )
            )

    def test_unreadable_lock_retains_bytes_and_does_not_hide_later_mismatch(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            first = fixture(root)
            second = root / "benchmarks/second/r/renv.lock"
            second.write_text(second.read_text().replace('"0.3.0"', '"0.2.0"'))
            original_second = second.read_bytes()
            for unreadable in (False, True):
                with self.subTest(unreadable=unreadable):
                    if unreadable:
                        first.unlink()
                        first.mkdir()
                    else:
                        first.write_bytes(b"\xff")
                    errors = guard.violations(root)
                    self.assertEqual(len(errors), 2)
                    self.assertTrue(
                        errors[0].startswith(
                            "benchmarks/first/r/renv.lock: unreadable or invalid JSON: "
                        )
                    )
                    self.assertEqual(
                        errors[1],
                        "benchmarks/second/r/renv.lock: SDK version '0.2.0' differs from prepared '0.3.0'",
                    )
                    self.assertEqual(second.read_bytes(), original_second)
                    if not unreadable:
                        self.assertEqual(first.read_bytes(), b"\xff")


class CandidateSdkVersions(unittest.TestCase):
    """A candidate's Python version marks it; R and the SDK pins keep X.Y.Z."""

    def candidate(self, root):
        lock = fixture(root)
        (root / "tools/packaging/release.toml").write_text(
            'version = "1.0.0"\ncandidate = 2\n'
        )
        (root / "R/yamaa/DESCRIPTION").write_text("Package: yamaa\nVersion: 1.0.0\n")
        (root / "python/pyproject.toml").write_text(
            '[project]\nname="yamaa"\nversion="1.0.0rc2"\n'
        )
        for path in root.glob("benchmarks/*/r/renv.lock"):
            path.write_text(path.read_text().replace('"0.3.0"', '"1.0.0"'))
        return lock

    def test_the_candidate_spellings_agree(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.candidate(root)
            self.assertEqual(guard.violations(root), [])

    def test_a_candidate_spelled_in_r_or_a_final_python_version_is_refused(self):
        for relative, old, new, error in [
            (
                "R/yamaa/DESCRIPTION",
                "Version: 1.0.0",
                "Version: 1.0.0rc2",
                "R/yamaa/DESCRIPTION: Version '1.0.0rc2' is not '1.0.0', the R "
                "version of release 1.0.0 release candidate 2",
            ),
            (
                "python/pyproject.toml",
                '"1.0.0rc2"',
                '"1.0.0"',
                "python/pyproject.toml: version '1.0.0' is not '1.0.0rc2', the "
                "Python version of release 1.0.0 release candidate 2",
            ),
            (
                "python/pyproject.toml",
                '"1.0.0rc2"',
                '"1.0.0rc1"',
                "python/pyproject.toml: version '1.0.0rc1' is not '1.0.0rc2', the "
                "Python version of release 1.0.0 release candidate 2",
            ),
            (
                "benchmarks/first/r/renv.lock",
                '"Version": "1.0.0"',
                '"Version": "1.0.0rc2"',
                "benchmarks/first/r/renv.lock: SDK version '1.0.0rc2' differs from "
                "prepared '1.0.0'",
            ),
        ]:
            with (
                self.subTest(relative=relative, new=new),
                tempfile.TemporaryDirectory() as directory,
            ):
                root = Path(directory)
                self.candidate(root)
                path = root / relative
                self.assertIn(old, path.read_text())
                path.write_text(path.read_text().replace(old, new, 1))
                self.assertEqual(guard.violations(root), [error])

    def test_a_missing_or_invalid_declaration_is_the_one_violation(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            fixture(root)
            declaration = root / "tools/packaging/release.toml"
            declaration.write_text('version = "0.3"\n')
            self.assertEqual(
                guard.violations(root),
                [
                    "tools/packaging/release.toml: the release version must be "
                    "X.Y.Z, not '0.3'"
                ],
            )
            declaration.unlink()
            [error] = guard.violations(root)
            self.assertTrue(
                error.startswith(
                    "tools/packaging/release.toml: unreadable release identity"
                ),
                error,
            )


if __name__ == "__main__":
    unittest.main()
