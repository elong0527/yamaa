"""Refuse incoherent host/core versions, Python README links to another
release's tag, drifting author lists and ambiguous packaging owners."""

import shutil
import tempfile
import unittest
from pathlib import Path

from tools.packaging import check_shared_release_versions as checker
from tools.packaging import release_identity
from tools.paths import ROOT

TOOLS = ROOT / "tools"
# The prepared release, read from the tree so that a version bump does not
# have to edit these mutations.
IDENTITY = release_identity.read(ROOT)
VERSION = IDENTITY.cargo
PYTHON_VERSION = IDENTITY.python


class ReleaseTree(unittest.TestCase):
    """A copy of the tree's release metadata, which each test mutates."""

    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        paths = [
            "LICENSE",
            "mkdocs.yml",
            release_identity.DECLARATION,
            checker.PYTHON_README,
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

    def violations_after(self, relative, old, new):
        """The tree's violations after one change to a file, which is then
        restored."""
        path = self.root / relative
        original = path.read_text()
        self.change(relative, old, new)
        try:
            return checker.violations(self.root)
        finally:
            path.write_text(original)


class CoherentRelease(ReleaseTree):
    def test_selected_versions_and_real_lock_owners_agree(self):
        self.assertEqual(checker.violations(self.root), [])

    def test_each_drift_is_refused_with_its_violation(self):
        lock = "uv.lock: selected yamaa project/version ownership differs"
        holder = "copyright holder is not 'yamaa contributors'"
        authors = "author list differs from the selected author list"
        for case, change, violation in [
            (
                "stale Cargo workspace",
                ("rust/Cargo.toml", f'version = "{VERSION}"', 'version = "0.1.0"'),
                f"rust/Cargo.toml: workspace version '0.1.0' is not '{VERSION}', "
                f"the Cargo version of release {IDENTITY}",
            ),
            (
                "member overrides the shared version",
                (
                    "rust/crates/yamaa-core/Cargo.toml",
                    "version.workspace = true",
                    'version = "0.1.0"',
                ),
                "crates/yamaa-core: package version differs from the shared Cargo "
                f"version '{VERSION}'",
            ),
            (
                "stale Cargo.lock identity",
                (
                    "rust/Cargo.lock",
                    f'name = "yamaa-core"\nversion = "{VERSION}"',
                    'name = "yamaa-core"\nversion = "0.1.0"',
                ),
                "Cargo.lock: yamaa-core version identity is stale or ambiguous",
            ),
            (
                "stale Python lock",
                (
                    "python/uv.lock",
                    f'name = "yamaa"\nversion = "{PYTHON_VERSION}"',
                    'name = "yamaa"\nversion = "0.3.0"',
                ),
                lock,
            ),
            (
                "Python lock with another owner",
                (
                    "python/uv.lock",
                    'source = { editable = "." }',
                    'source = { editable = "../foreign" }',
                ),
                lock,
            ),
            (
                "collective holder drift",
                ("LICENSE", "yamaa contributors", "yamaa authors"),
                f"LICENSE: {holder}",
            ),
            *(
                (
                    f"extended holder in {relative}",
                    (relative, "yamaa contributors", "yamaa contributors and others"),
                    f"{relative}: {holder}",
                )
                for relative in ("LICENSE", "mkdocs.yml", "R/yamaa/LICENSE")
            ),
            (
                "Python author drift",
                (
                    "python/pyproject.toml",
                    '{ name = "Peng Zhang" }',
                    '{ name = "P. Zhang" }',
                ),
                f"python/pyproject.toml: {authors}",
            ),
            (
                "R contributor promoted to author",
                (
                    "R/yamaa/DESCRIPTION",
                    'person("Peng", "Zhang", role = "ctb")',
                    'person("Peng", "Zhang", role = "aut")',
                ),
                f"R/yamaa/DESCRIPTION: {authors}",
            ),
            (
                "R author without the copyright holder role",
                (
                    "R/yamaa/DESCRIPTION",
                    'role = c("aut", "cre", "cph")',
                    'role = c("aut", "cre")',
                ),
                f"R/yamaa/DESCRIPTION: {authors}",
            ),
            (
                "R author appended with named arguments",
                (
                    "R/yamaa/DESCRIPTION",
                    'person("Jingwei", "Xiong", role = "ctb"),',
                    'person("Jingwei", "Xiong", role = "ctb"),\n'
                    '    person(given = "Another", family = "Contributor", role = "ctb"),',
                ),
                f"R/yamaa/DESCRIPTION: {authors}",
            ),
            (
                "Cargo author drift",
                ("rust/Cargo.toml", '"Peng Zhang",', '"P. Zhang",'),
                f"rust/Cargo.toml: {authors}",
            ),
        ]:
            with self.subTest(case=case):
                self.assertIn(violation, self.violations_after(*change))

    def test_r_bundled_code_authors_must_stay_credited(self):
        # CRAN's policy asks the R package to credit the authors of the
        # third-party code it bundles as contributors, and its copyright
        # holders: inst/AUTHORS, staged with the package, names the crates'.
        crates = (
            'person("The authors of the bundled Rust crates", role = c("ctb", "cph"),\n'
            '           comment = "listed in inst/AUTHORS")'
        )
        readstat = (
            'person("Evan Miller and the ReadStat authors", role = c("ctb", "cph"),\n'
            '           comment = "bundled ReadStat C sources, inst/LICENSE-READSTAT.txt")'
        )
        for entry, replacement in (
            (",\n    " + crates, ""),
            (",\n    " + readstat, ""),
            (crates, crates.replace('c("ctb", "cph")', '"ctb"')),
            (readstat, readstat.replace('c("ctb", "cph")', '"cph"')),
            (crates, crates.replace("bundled Rust crates", "vendored crates")),
            (
                crates,
                crates.replace('"listed in inst/AUTHORS"', 'c(file = "inst/AUTHORS")'),
            ),
        ):
            with self.subTest(replacement=replacement):
                self.setUp()
                self.change("R/yamaa/DESCRIPTION", entry, replacement)
                self.assertIn(
                    "R/yamaa/DESCRIPTION: author list differs from the selected"
                    " author list",
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

        def missing_python_readme(root):
            (root / checker.PYTHON_README).unlink()

        for mutate in (
            missing_project,
            invalid_python_toml,
            missing_python_file,
            integer_author_entry,
            missing_workspace_package,
            cargo_lock_entry_without_version,
            missing_python_lock,
            missing_python_readme,
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
            + f'\n[[package]]\nname = "yamaa"\nversion = "{PYTHON_VERSION}"\nsource = {{ editable = "." }}\n'
        )
        self.assertIn(
            "uv.lock: selected yamaa project/version ownership differs",
            checker.violations(self.root),
        )


class PythonReadmeLinks(ReleaseTree):
    """python/README.md is the wheels' PyPI description, so each of its links
    that names a release tag names the tag of the release it ships with."""

    FINAL = release_identity.Identity("1.0.0")
    CANDIDATE = release_identity.Identity("1.0.0", 2)

    def write_readme(self, ref):
        """Link the logo, License, R package and release page at `ref`, then
        the contributing guide on main, one link per line."""
        (self.root / checker.PYTHON_README).write_text(
            '# yamaa for Python <img src="https://raw.githubusercontent.com/'
            f'elong0527/yamaa/{ref}/docs/assets/logo.jpeg" alt="yamaa logo" />\n'
            "\n"
            f"[License](https://github.com/elong0527/yamaa/blob/{ref}/LICENSE)\n"
            f"[R package](https://github.com/elong0527/yamaa/tree/{ref}/R/yamaa)\n"
            f"[release](https://github.com/elong0527/yamaa/releases/tag/{ref})\n"
            "[contributing guide]"
            "(https://github.com/elong0527/yamaa/blob/main/CONTRIBUTING.md)\n"
        )

    def stale(self, tag, identity):
        """The violations of `write_readme(tag)` for release `identity`."""
        return [
            f"python/README.md:{line}: link names tag {tag!r}, not "
            f"{identity.tag!r}, the tag of release {identity}"
            for line in (1, 3, 4, 5)
        ]

    def test_the_tree_links_to_the_prepared_tag(self):
        text = (self.root / checker.PYTHON_README).read_text()
        self.assertIn(f"/{IDENTITY.tag}/", text)
        self.assertEqual(checker.violations(self.root), [])

    def test_the_current_tag_passes(self):
        for identity in (self.FINAL, self.CANDIDATE):
            with self.subTest(identity=str(identity)):
                self.write_readme(identity.tag)
                self.assertEqual(
                    checker.python_readme_violations(self.root, identity), []
                )

    def test_a_stale_tag_fails_in_every_link(self):
        for identity, tag in (
            # The previous release, and a candidate's tag on the final release.
            (self.FINAL, "yamaa-v0.5.0"),
            (self.FINAL, "yamaa-v1.0.0-rc.2"),
            # The previous candidate, the final release's tag on a candidate,
            # and the candidate spelled as its Python version.
            (self.CANDIDATE, "yamaa-v1.0.0-rc.1"),
            (self.CANDIDATE, "yamaa-v1.0.0"),
            (self.CANDIDATE, "yamaa-v1.0.0rc2"),
        ):
            with self.subTest(identity=str(identity), tag=tag):
                self.write_readme(tag)
                self.assertEqual(
                    checker.python_readme_violations(self.root, identity),
                    self.stale(tag, identity),
                )

    def test_the_main_link_is_ignored(self):
        self.write_readme("main")
        self.assertEqual(checker.python_readme_violations(self.root, self.FINAL), [])

    def test_a_stale_tag_fails_the_check(self):
        self.write_readme("yamaa-v0.4.0")
        self.assertEqual(
            checker.violations(self.root), self.stale("yamaa-v0.4.0", IDENTITY)
        )


class CandidateRelease(ReleaseTree):
    """#3030 D53: Python 1.0.0rcN, Cargo 1.0.0-rc.N and R 1.0.0 agree."""

    def setUp(self):
        super().setUp()
        self.declare('version = "1.0.0"\ncandidate = 2\n')
        self.spell(python="1.0.0rc2", cargo="1.0.0-rc.2", r="1.0.0")
        self.pin("yamaa-v1.0.0-rc.2")

    def declare(self, text):
        (self.root / release_identity.DECLARATION).write_text(text)

    def pin(self, tag):
        """Move the Python README's tag links to `tag`, as a version bump does."""
        path = self.root / checker.PYTHON_README
        text = path.read_text()
        self.assertIn(f"/{IDENTITY.tag}/", text)
        path.write_text(text.replace(f"/{IDENTITY.tag}/", f"/{tag}/"))

    def spell(self, python, cargo, r):
        """Rewrite every manifest and lock version of the tree."""
        self.change(
            "python/pyproject.toml",
            f'version = "{PYTHON_VERSION}"',
            f'version = "{python}"',
        )
        self.change(
            "python/uv.lock",
            f'name = "yamaa"\nversion = "{PYTHON_VERSION}"',
            f'name = "yamaa"\nversion = "{python}"',
        )
        self.change("rust/Cargo.toml", f'version = "{VERSION}"', f'version = "{cargo}"')
        for name in (
            "yamaa",
            "yamaa-adapters",
            "yamaa-application",
            "yamaa-boundary",
            "yamaa-cli",
            "yamaa-core",
            "yamaa-python",
            "yamaa-r",
        ):
            self.change(
                "rust/Cargo.lock",
                f'name = "{name}"\nversion = "{VERSION}"',
                f'name = "{name}"\nversion = "{cargo}"',
            )
        r_version = IDENTITY.r
        self.change("R/yamaa/DESCRIPTION", f"Version: {r_version}", f"Version: {r}")
        for path in self.root.glob("benchmarks/*/r/renv.lock"):
            path.write_text(
                path.read_text().replace(
                    f'"Package": "yamaa",\n      "Version": "{r_version}"',
                    f'"Package": "yamaa",\n      "Version": "{r}"',
                )
            )

    def test_selected_versions_and_real_lock_owners_agree(self):
        self.assertEqual(checker.violations(self.root), [])

    def test_each_drift_from_the_candidate_is_refused_with_its_violation(self):
        for case, change, violation in [
            (
                "stale Cargo workspace",
                ("rust/Cargo.toml", 'version = "1.0.0-rc.2"', 'version = "0.1.0"'),
                "rust/Cargo.toml: workspace version '0.1.0' is not '1.0.0-rc.2', the "
                "Cargo version of release 1.0.0 release candidate 2",
            ),
            (
                "member overrides the shared version",
                (
                    "rust/crates/yamaa-core/Cargo.toml",
                    "version.workspace = true",
                    'version = "1.0.0-rc.1"',
                ),
                "crates/yamaa-core: package version differs from the shared Cargo "
                "version '1.0.0-rc.2'",
            ),
            (
                "stale Cargo.lock identity",
                (
                    "rust/Cargo.lock",
                    'name = "yamaa-core"\nversion = "1.0.0-rc.2"',
                    'name = "yamaa-core"\nversion = "1.0.0-rc.1"',
                ),
                "Cargo.lock: yamaa-core version identity is stale or ambiguous",
            ),
            (
                "stale Python lock",
                (
                    "python/uv.lock",
                    'name = "yamaa"\nversion = "1.0.0rc2"',
                    'name = "yamaa"\nversion = "1.0.0"',
                ),
                "uv.lock: selected yamaa project/version ownership differs",
            ),
        ]:
            with self.subTest(case=case):
                self.assertIn(violation, self.violations_after(*change))

    def test_ambiguous_python_distribution_is_refused(self):
        path = self.root / "python/uv.lock"
        path.write_text(
            path.read_text()
            + '\n[[package]]\nname = "yamaa"\nversion = "1.0.0rc2"\nsource = { editable = "." }\n'
        )
        self.assertIn(
            "uv.lock: selected yamaa project/version ownership differs",
            checker.violations(self.root),
        )

    def test_exactly_the_candidate_triple_is_accepted(self):
        for python, cargo, r in [
            # Another candidate in one format.
            ("1.0.0rc1", "1.0.0-rc.2", "1.0.0"),
            ("1.0.0rc2", "1.0.0-rc.1", "1.0.0"),
            # The final release in one format.
            ("1.0.0", "1.0.0-rc.2", "1.0.0"),
            ("1.0.0rc2", "1.0.0", "1.0.0"),
            # A candidate spelled in R, or another format's spelling.
            ("1.0.0rc2", "1.0.0-rc.2", "1.0.0rc2"),
            ("1.0.0rc2", "1.0.0-rc.2", "1.0.0.9002"),
            ("1.0.0-rc.2", "1.0.0-rc.2", "1.0.0"),
            ("1.0.0rc2", "1.0.0rc2", "1.0.0"),
            # Other pre-release kinds.
            ("1.0.0b2", "1.0.0-beta.2", "1.0.0"),
        ]:
            with self.subTest(python=python, cargo=cargo, r=r):
                # A fresh copy of the tree, declared as candidate 2.
                super().setUp()
                self.declare('version = "1.0.0"\ncandidate = 2\n')
                self.spell(python, cargo, r)
                self.pin("yamaa-v1.0.0-rc.2")
                self.assertNotEqual(checker.violations(self.root), [])

    def test_a_final_declaration_refuses_the_candidate_spellings(self):
        self.declare('version = "1.0.0"\n')
        errors = checker.violations(self.root)
        self.assertIn(
            "python/pyproject.toml: version '1.0.0rc2' is not '1.0.0', the Python "
            "version of release 1.0.0",
            errors,
        )
        self.assertIn(
            "rust/Cargo.toml: workspace version '1.0.0-rc.2' is not '1.0.0', the "
            "Cargo version of release 1.0.0",
            errors,
        )

    def test_a_missing_declaration_is_reported_with_the_author_checks(self):
        (self.root / release_identity.DECLARATION).unlink()
        self.change("LICENSE", "yamaa contributors", "yamaa authors")
        errors = checker.violations(self.root)
        self.assertTrue(
            errors[0].startswith(
                "tools/packaging/release.toml: unreadable release identity"
            ),
            errors,
        )
        self.assertIn(
            "LICENSE: copyright holder is not 'yamaa contributors'", errors[1:]
        )


if __name__ == "__main__":
    unittest.main()
