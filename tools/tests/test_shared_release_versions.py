"""Refuse incoherent host/core versions, drifting author lists and
ambiguous packaging owners."""

import shutil
import tempfile
import tomllib
import unittest
from pathlib import Path

from tools.packaging import check_shared_release_versions as checker
from tools.paths import ROOT

TOOLS = ROOT / "tools"
# The selected release version, read from the tree so that a version bump
# does not have to edit these mutations.
VERSION = tomllib.loads((ROOT / "python/pyproject.toml").read_text())["project"][
    "version"
]


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
        self.change("rust/Cargo.toml", f'version = "{VERSION}"', 'version = "0.1.0"')
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
            f'name = "yamaa-core"\nversion = "{VERSION}"',
            'name = "yamaa-core"\nversion = "0.1.0"',
        )
        self.assertIn(
            "Cargo.lock: yamaa-core version identity is stale or ambiguous",
            checker.violations(self.root),
        )

    def test_stale_python_lock_or_wrong_owner_is_refused(self):
        for old, new in (
            (
                f'name = "yamaa"\nversion = "{VERSION}"',
                'name = "yamaa"\nversion = "0.3.0"',
            ),
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

    def test_extended_copyright_holder_is_refused(self):
        for relative in ("LICENSE", "mkdocs.yml", "R/yamaa/LICENSE"):
            path = self.root / relative
            original = path.read_text()
            self.change(relative, "yamaa contributors", "yamaa contributors and others")
            self.assertIn(
                f"{relative}: copyright holder is not 'yamaa contributors'",
                checker.violations(self.root),
            )
            path.write_text(original)

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

    def test_r_author_appended_with_named_arguments_is_refused(self):
        self.change(
            "R/yamaa/DESCRIPTION",
            'person("Jingwei", "Xiong", role = "ctb"),',
            'person("Jingwei", "Xiong", role = "ctb"),\n'
            '    person(given = "Another", family = "Contributor", role = "ctb"),',
        )
        self.assertIn(
            "R/yamaa/DESCRIPTION: author list differs from the selected author list",
            checker.violations(self.root),
        )

    def test_r_bundled_crate_authors_must_stay_credited(self):
        # CRAN's policy asks the R package to credit the authors of the Rust
        # code it bundles; inst/AUTHORS, staged with the package, names them.
        entry = (
            'person("The authors of the bundled Rust crates", role = "ctb",\n'
            '           comment = "listed in inst/AUTHORS")'
        )
        for replacement in (
            "",
            entry.replace('role = "ctb"', 'role = c("ctb", "cph")'),
            entry.replace("bundled Rust crates", "vendored crates"),
            entry.replace('"listed in inst/AUTHORS"', 'c(file = "inst/AUTHORS")'),
        ):
            with self.subTest(replacement=replacement):
                self.setUp()
                old = ",\n    " + entry if not replacement else entry
                self.change("R/yamaa/DESCRIPTION", old, replacement)
                self.assertIn(
                    "R/yamaa/DESCRIPTION: author list differs from the selected"
                    " author list",
                    checker.violations(self.root),
                )

    def test_cargo_author_drift_is_refused(self):
        self.change("rust/Cargo.toml", '"Peng Zhang",', '"P. Zhang",')
        self.assertIn(
            "rust/Cargo.toml: author list differs from the selected author list",
            checker.violations(self.root),
        )

    def test_malformed_or_missing_metadata_is_a_violation_not_a_crash(self):
        # The checker must report violations for the malformed shapes a
        # hand-edited or partially generated tree can contain, instead of
        # escaping as KeyError/TypeError/AttributeError/TOMLDecodeError.
        def missing_project(root):
            (root / "python/pyproject.toml").write_text("[tool.x]\nfoo = 1\n")

        def invalid_python_toml(root):
            (root / "python/pyproject.toml").write_text("not [ valid")

        def missing_python_file(root):
            (root / "python/pyproject.toml").unlink()

        def integer_author_entry(root):
            path = root / "python/pyproject.toml"
            text = path.read_text()
            start = text.index("authors = [")
            end = text.index("]", start)
            path.write_text(text[:start] + "authors = [123]\n" + text[end + 1 :])

        def missing_workspace_package(root):
            (root / "rust/Cargo.toml").write_text("[workspace]\nmembers = []\n")

        def cargo_lock_entry_without_version(root):
            path = root / "rust/Cargo.lock"
            path.write_text(
                path.read_text().replace(
                    f'name = "yamaa-core"\nversion = "{VERSION}"', 'name = "yamaa-core"'
                )
            )

        def missing_python_lock(root):
            (root / "python/uv.lock").unlink()

        for mutate in (
            missing_project,
            invalid_python_toml,
            missing_python_file,
            integer_author_entry,
            missing_workspace_package,
            cargo_lock_entry_without_version,
            missing_python_lock,
        ):
            with self.subTest(mutation=mutate.__name__):
                # Rebuild a pristine copy for each mutation.
                self.setUp()
                mutate(self.root)
                self.assertNotEqual(checker.violations(self.root), [])

    def test_ambiguous_python_distribution_is_refused(self):
        path = self.root / "python/uv.lock"
        path.write_text(
            path.read_text()
            + f'\n[[package]]\nname = "yamaa"\nversion = "{VERSION}"\nsource = {{ editable = "." }}\n'
        )
        self.assertIn(
            "uv.lock: selected yamaa project/version ownership differs",
            checker.violations(self.root),
        )


if __name__ == "__main__":
    unittest.main()
