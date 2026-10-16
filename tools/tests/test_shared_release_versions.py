"""Refuse incoherent host/core versions, drifting author lists and
ambiguous packaging owners."""

import shutil
import tempfile
import unittest
from pathlib import Path

from tools.packaging import check_shared_release_versions as checker
from tools.paths import ROOT

TOOLS = ROOT / "tools"


class CoherentRelease(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        paths = [
            "LICENSE",
            "mkdocs.yml",
            "python/pyproject.toml",
            "python/uv.lock",
            "R/yamaa/DESCRIPTION",
            "R/yamaa/LICENSE",
            "rust/Cargo.toml",
            "rust/Cargo.lock",
        ]
        paths += [
            path.relative_to(ROOT).as_posix()
            for path in (ROOT / "rust/crates").glob("*/Cargo.toml")
        ]
        paths += [
            path.relative_to(ROOT).as_posix()
            for path in (ROOT / "benchmarks").glob("*/r/renv.lock")
        ]
        for relative in paths:
            destination = self.root / relative
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(ROOT / relative, destination)

    def change(self, relative, old, new):
        path = self.root / relative
        text = path.read_text()
        self.assertIn(old, text)
        path.write_text(text.replace(old, new, 1))

    def test_selected_versions_and_real_lock_owners_agree(self):
        self.assertEqual(checker.violations(self.root), [])

    def test_stale_cargo_workspace_is_refused(self):
        self.change("rust/Cargo.toml", 'version = "0.4.0"', 'version = "0.1.0"')
        self.assertIn(
            "Cargo workspace version differs from selected Python/R release",
            checker.violations(self.root),
        )

    def test_member_cannot_override_the_shared_version(self):
        self.change(
            "rust/crates/yamaa-core/Cargo.toml",
            "version.workspace = true",
            'version = "0.1.0"',
        )
        self.assertIn(
            "crates/yamaa-core: package version differs from selected shared release",
            checker.violations(self.root),
        )

    def test_stale_cargo_lock_identity_is_refused(self):
        self.change(
            "rust/Cargo.lock",
            'name = "yamaa-core"\nversion = "0.4.0"',
            'name = "yamaa-core"\nversion = "0.1.0"',
        )
        self.assertIn(
            "Cargo.lock: yamaa-core version identity is stale or ambiguous",
            checker.violations(self.root),
        )

    def test_stale_python_lock_or_wrong_owner_is_refused(self):
        for old, new in (
            ('name = "yamaa"\nversion = "0.4.0"', 'name = "yamaa"\nversion = "0.3.0"'),
            ('source = { editable = "." }', 'source = { editable = "../foreign" }'),
        ):
            path = self.root / "python/uv.lock"
            original = path.read_text()
            self.change("python/uv.lock", old, new)
            self.assertIn(
                "uv.lock: selected yamaa project/version ownership differs",
                checker.violations(self.root),
            )
            path.write_text(original)

    def test_collective_holder_drift_is_refused(self):
        self.change("LICENSE", "yamaa contributors", "yamaa authors")
        self.assertIn(
            "LICENSE: copyright holder is not 'yamaa contributors'",
            checker.violations(self.root),
        )

    def test_python_author_drift_is_refused(self):
        self.change(
            "python/pyproject.toml",
            '{ name = "Peng Zhang" }',
            '{ name = "P. Zhang" }',
        )
        self.assertIn(
            "python/pyproject.toml: author list differs from the selected author list",
            checker.violations(self.root),
        )

    def test_r_contributor_promoted_to_author_is_refused(self):
        self.change(
            "R/yamaa/DESCRIPTION",
            'person("Peng", "Zhang", role = "ctb")',
            'person("Peng", "Zhang", role = "aut")',
        )
        self.assertIn(
            "R/yamaa/DESCRIPTION: author list differs from the selected author list",
            checker.violations(self.root),
        )

    def test_cargo_author_drift_is_refused(self):
        self.change("rust/Cargo.toml", '"Peng Zhang",', '"P. Zhang",')
        self.assertIn(
            "rust/Cargo.toml: author list differs from the selected author list",
            checker.violations(self.root),
        )

    def test_ambiguous_python_distribution_is_refused(self):
        path = self.root / "python/uv.lock"
        path.write_text(
            path.read_text()
            + '\n[[package]]\nname = "yamaa"\nversion = "0.4.0"\nsource = { editable = "." }\n'
        )
        self.assertIn(
            "uv.lock: selected yamaa project/version ownership differs",
            checker.violations(self.root),
        )


if __name__ == "__main__":
    unittest.main()
