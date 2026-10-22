"""The source manifests of the interface stability check (#3030 P-STAB-1): the
core registry's diagnostic identities, the rule card IDs and the baseline
benchmarks, against release tags in temporary repositories. Nothing here reads
the network or this repository's tags."""

import contextlib
import io
import os
import subprocess
import tempfile
import unittest
from pathlib import Path
from unittest import mock

import yaml
from tools.tests import support
from tools.tests.support import git
from tools.validation import check_interface_stability as STABILITY
from tools.validation import interface_sources as SOURCES
from tools.validation.interface_manifests import ManifestError, Tree

REGISTRY = SOURCES.REGISTRIES[0]
SOURCE = """\
macro_rules! conditions {
    ($($code:ident => ($phase:literal, $condition:literal, $rule:expr),)*) => {};
}

conditions! {
    AlphaFailed => ("validation", "alpha_failed", Some("CT-1")),
    AlphaRetired => ("validation", "alpha_failed", Some("CT-2")),
    // A comment is no cause.
    BetaFailed => ("derivation", "beta_failed", Some("CT-2")),

    InternalError => ("execution", "internal_error", None),
}
"""
CARDS = """\
# Controlled terminology

### CT-1. A value is in its codelist

### CT-2. A codelist lists each value once
"""
BASIC = "benchmarks/sdtm-dm-basic"
BENCHMARK = {
    "benchmark.yaml": "lifecycle: draft\nstandard: SDTM\ndomain: DM\n",
    "README.md": "# Copy the demographics\n",
    "spec.yaml": "yamaa: '1.0'\n",
    "input/dm.csv": "USUBJID,AGE\n01,30\n",
    # Bytes that are no text, holding what a batch's headers hold.
    "input/dm.xpt": b"\0\n\xff 1 blob\n\0",
    "expected/dm.csv": "DOMAIN,USUBJID,AGE\nDM,01,30\n",
}
TAG = "yamaa-v1.0.0"


class Repository(support.Repository):
    """A git repository holding the core registry, a rule file, the release
    identity and an empty exception list."""

    def __init__(self, root):
        super().__init__(root)
        self.write(REGISTRY, SOURCE)
        self.write("rules/terms.md", CARDS)
        self.write(STABILITY.EXCEPTIONS, "exceptions: []\n")

    def benchmark(self, directory, files=BENCHMARK):
        for path, data in files.items():
            self.write(f"{directory}/{path}", data)

    def tag(self, name=TAG):
        super().tag(name)

    def changes(self, compare, tag=TAG):
        """`(manifest, element, what, breaking)` of each change since the tag."""
        return [
            (change.manifest, change.element, change.what, change.breaking)
            for change in compare(Tree(self.root, tag), Tree(self.root))
        ]

    def check(self):
        """Run the check with this module's comparer alone."""
        output = io.StringIO()
        with (
            mock.patch.object(STABILITY, "COMPARERS", (SOURCES.__name__,)),
            contextlib.redirect_stdout(output),
        ):
            status = STABILITY.main(["--root", str(self.root)])
        return status, output.getvalue()


compare_identities = SOURCES._compare_identities
compare_cards = SOURCES._compare_cards
compare_benchmarks = SOURCES._compare_benchmarks


class Temporary(unittest.TestCase):
    def setUp(self):
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        self.repo = Repository(Path(directory.name) / "repo")


class Identities(Temporary):
    def test_a_removed_triple_breaks_and_a_renamed_cause_does_not(self):
        self.repo.tag()
        self.assertEqual(self.repo.changes(compare_identities), [])
        self.repo.edit(REGISTRY, "AlphaFailed =>", "AlphaRejected =>")
        # Another cause still maps to (execution, internal_error, null).
        self.repo.edit(
            REGISTRY,
            'AlphaRetired => ("validation", "alpha_failed", Some("CT-2"))',
            'InternalPanic => ("execution", "internal_error", None)',
        )
        self.repo.edit(REGISTRY, '"beta_failed", Some("CT-2")', '"beta_failed", None')
        self.repo.edit(
            REGISTRY,
            "    InternalError =>",
            '    GammaFailed => ("verification", "gamma_failed", Some("CT-2")),\n'
            "    InternalError =>",
        )
        self.assertEqual(
            self.repo.changes(compare_identities),
            [
                (REGISTRY, "(validation, alpha_failed, CT-2)", "removed", True),
                (REGISTRY, "(derivation, beta_failed, CT-2)", "removed", True),
                (REGISTRY, "(derivation, beta_failed, null)", "added", False),
                (REGISTRY, "(verification, gamma_failed, CT-2)", "added", False),
            ],
        )

    def test_a_rule_is_the_id_the_cause_cites(self):
        # A historical identifier need not name a card in the current tree.
        self.repo.edit(REGISTRY, 'Some("CT-1")', 'Some("LEGACY-0694")')
        self.repo.tag("yamaa-v0.5.0")
        self.repo.edit(REGISTRY, 'Some("LEGACY-0694")', 'Some("CT-1")')
        self.assertEqual(
            self.repo.changes(SOURCES.compare, "yamaa-v0.5.0"),
            [
                (REGISTRY, "(validation, alpha_failed, LEGACY-0694)", "removed", True),
                (REGISTRY, "(validation, alpha_failed, CT-1)", "added", False),
            ],
        )

    def test_the_registry_must_be_in_both_trees(self):
        self.repo.tag()
        self.repo.remove(REGISTRY)
        message = (
            f"{REGISTRY}: the core diagnostic registry is missing from the "
            "working tree; the pull request that moves it adds its new path to "
            "REGISTRIES in tools/validation/interface_sources.py"
        )
        with self.assertRaises(ManifestError) as raised:
            self.repo.changes(compare_identities)
        self.assertEqual(str(raised.exception), message)
        self.repo.release("1.0.1")
        self.assertEqual(self.repo.check(), (1, f"ERROR: {message}\n"))
        # A tag without the registry fails too.
        self.repo.tag("yamaa-v1.0.1")
        self.repo.write(REGISTRY, SOURCE)
        with self.assertRaisesRegex(ManifestError, " missing from yamaa-v1.0.1;"):
            self.repo.changes(compare_identities, "yamaa-v1.0.1")

    def test_a_malformed_registry_fails(self):
        self.repo.tag()
        for source, message in (
            ("pub enum ConditionCode {}\n", "no cause in a `conditions!` block"),
            ("conditions! {\n}\n", "no cause in a `conditions!` block"),
            (
                'conditions! {\n    Alpha => ("validation", "alpha", Some("ct-1")),\n}\n',
                'unrecognized cause: Alpha => .*Some\\("ct-1"\\)',
            ),
            (
                'conditions! {\n    Alpha => ("validation", "alpha"),\n}\n',
                'unrecognized cause: Alpha => \\("validation", "alpha"\\),$',
            ),
        ):
            with self.subTest(source=source):
                self.repo.write(REGISTRY, source)
                with self.assertRaisesRegex(
                    ManifestError, f"^{REGISTRY} at the working tree: {message}"
                ):
                    self.repo.changes(compare_identities)


class Cards(Temporary):
    def test_a_removed_card_breaks_and_a_moved_card_does_not(self):
        self.repo.tag()
        self.repo.write("rules/terms.md", "# Terms\n\n### CT-1. Retitled\n")
        self.repo.write("rules/lists.md", "# Lists\n\n### CT-3. A new card\n")
        self.assertEqual(
            self.repo.changes(compare_cards),
            [("rules", "CT-2", "removed", True), ("rules", "CT-3", "added", False)],
        )
        self.repo.write("rules/terms.md", "# Terms\n\n### CT-2. Moved\n")
        self.repo.write("rules/lists.md", "# Lists\n\n### CT-1. Moved too\n")
        self.assertEqual(self.repo.changes(compare_cards), [])

    def test_a_working_tree_without_rule_cards_fails(self):
        self.repo.tag()
        self.repo.remove("rules/terms.md")
        with self.assertRaisesRegex(
            ManifestError, "^rules: no rule card in the working tree$"
        ):
            self.repo.changes(compare_cards)


class Benchmarks(Temporary):
    def test_a_baseline_benchmark_keeps_its_files_byte_for_byte(self):
        self.repo.benchmark(BASIC)
        self.repo.benchmark("benchmarks/sdtm-dm-retired")
        self.repo.write("benchmarks/README.md", "# Benchmarks\n")
        self.repo.tag()
        self.assertEqual(self.repo.changes(compare_benchmarks), [])
        self.repo.edit(f"{BASIC}/expected/dm.csv", "DM,01,30", "DM,01,31")
        self.repo.remove(f"{BASIC}/input/dm.xpt")
        self.repo.write(f"{BASIC}/expected/error.yaml", "phase: validation\n")
        for path in BENCHMARK:
            self.repo.remove(f"benchmarks/sdtm-dm-retired/{path}")
        self.assertEqual(
            self.repo.changes(compare_benchmarks),
            [
                ("benchmarks", f"{BASIC}/expected/dm.csv", "changed", True),
                (
                    "benchmarks",
                    f"{BASIC}/expected/error.yaml",
                    "added to a baseline benchmark",
                    True,
                ),
                ("benchmarks", f"{BASIC}/input/dm.xpt", "removed", True),
                # A removed directory is one element.
                ("benchmarks", "benchmarks/sdtm-dm-retired", "removed", True),
            ],
        )

    def test_readme_prose_and_metadata_may_change_and_new_benchmarks_add(self):
        self.repo.benchmark(BASIC)
        self.repo.tag()
        self.repo.edit(f"{BASIC}/README.md", "Copy", "Copy and check")
        self.repo.write(f"{BASIC}/input/README.md", "The collected form.\n")
        self.repo.edit(f"{BASIC}/benchmark.yaml", "draft", "reviewed")
        self.repo.write("benchmarks/README.md", "# The benchmarks\n")
        self.repo.write("benchmarks/selftest.yaml", "cases: []\n")
        self.repo.benchmark("benchmarks/sdtm-dm-new")
        self.repo.benchmark("benchmarks/adam-adsl-new", {"README.md": "# Prose\n"})
        self.assertEqual(
            self.repo.changes(compare_benchmarks),
            [
                ("benchmarks", "benchmarks/adam-adsl-new", "added", False),
                ("benchmarks", "benchmarks/sdtm-dm-new", "added", False),
            ],
        )

    @unittest.skipUnless(hasattr(os, "symlink"), "needs symbolic links")
    def test_a_symbolic_link_stays_a_link_to_its_target(self):
        directory = "benchmarks/negative-path-symlink"
        self.repo.benchmark(directory)
        self.repo.write(f"{directory}/input/reference/lbref.csv", "LBTESTCD\n")
        link = self.repo.root / directory / "input/lbref.csv"
        link.symlink_to("reference/lbref.csv")
        self.repo.tag()
        self.assertEqual(self.repo.changes(compare_benchmarks), [])
        changed = [("benchmarks", f"{directory}/input/lbref.csv", "changed", True)]
        link.unlink()
        link.symlink_to("reference/other.csv")
        self.assertEqual(self.repo.changes(compare_benchmarks), changed)
        # A regular file in the link's place, even one that holds the bytes
        # Git stores for the link, is no longer the case FILE-8 refuses.
        for data in ("LBTESTCD\n", "reference/lbref.csv"):
            with self.subTest(data=data):
                link.unlink()
                self.repo.write(f"{directory}/input/lbref.csv", data)
                self.assertEqual(self.repo.changes(compare_benchmarks), changed)
        self.repo.release("1.0.1")
        status, output = self.repo.check()
        self.assertEqual(status, 1, output)
        self.assertIn(f"benchmarks: {directory}/input/lbref.csv: changed\n", output)

    @unittest.skipUnless(hasattr(os, "symlink"), "needs symbolic links")
    def test_a_regular_file_stays_regular_whatever_its_mode(self):
        path = f"{BASIC}/input/lbref.csv"
        file = self.repo.root / path
        self.repo.benchmark(BASIC)
        self.repo.write(path, "reference/lbref.csv")
        file.chmod(0o755)
        self.repo.tag()
        self.assertIn("100755 blob", git(self.repo.root, "ls-tree", TAG, path))
        file.chmod(0o644)
        self.assertEqual(self.repo.changes(compare_benchmarks), [])
        # A link that Git stores with the file's bytes is no regular file.
        file.unlink()
        file.symlink_to("reference/lbref.csv")
        self.assertEqual(
            self.repo.changes(compare_benchmarks),
            [("benchmarks", path, "changed", True)],
        )

    def test_a_tag_is_read_with_two_git_calls(self):
        self.repo.tag("yamaa-v0.9.0")
        for index in range(10):
            self.repo.benchmark(f"benchmarks/sdtm-dm-case-{index}")
        self.repo.write("benchmarks/README.md", "# Benchmarks\n")
        self.repo.tag()
        tag = Tree(self.repo.root, TAG)
        calls = []
        run = subprocess.run

        def counted(arguments, *args, **kwargs):
            calls.append(arguments)
            return run(arguments, *args, **kwargs)

        with mock.patch.object(subprocess, "run", counted):
            files = SOURCES.benchmark_files(tag)
        self.assertEqual(len(calls), 2, calls)
        # The batch gives the bytes that each file's own reads give, and the
        # working tree's reader the same.
        paths = [path for path in tag.files("benchmarks") if path.count("/") > 1]
        self.assertEqual(len(paths), 10 * len(BENCHMARK))
        self.assertEqual(files, {path: ("regular", tag.read(path)) for path in paths})
        self.assertEqual(files, SOURCES.benchmark_files(Tree(self.repo.root)))
        self.assertEqual(
            SOURCES.benchmark_files(Tree(self.repo.root, "yamaa-v0.9.0")), {}
        )


class Check(Temporary):
    def test_the_comparer_is_registered(self):
        self.assertIn(SOURCES.__name__, STABILITY.COMPARERS)

    def test_a_new_major_lists_the_source_breaks_and_passes(self):
        self.repo.benchmark(BASIC)
        self.repo.tag("yamaa-v0.5.0")
        # The 1.0 rewrite retires a cause, a card and a specification.
        self.repo.edit(REGISTRY, '"beta_failed", Some("CT-2")', '"beta_failed", None')
        self.repo.edit(
            REGISTRY,
            '    AlphaRetired => ("validation", "alpha_failed", Some("CT-2")),\n',
            "",
        )
        self.repo.edit("rules/terms.md", "CT-2. A codelist", "CT-3. A codelist")
        self.repo.edit(f"{BASIC}/spec.yaml", "'1.0'", "'1.1'")
        self.repo.release("1.0.0", candidate=1)
        status, output = self.repo.check()
        self.assertEqual(status, 0, output)
        self.assertEqual(
            output.splitlines(),
            [
                "Breaking changes against yamaa-v0.5.0, for the release notes "
                "of 1.0.0:",
                f"{REGISTRY}: (validation, alpha_failed, CT-2): removed",
                f"{REGISTRY}: (derivation, beta_failed, CT-2): removed",
                "rules: CT-2: removed",
                f"benchmarks: {BASIC}/spec.yaml: changed",
                "Additions against yamaa-v0.5.0:",
                f"{REGISTRY}: (derivation, beta_failed, null): added",
                "rules: CT-3: added",
                "PASS: 1.0.0 release candidate 1 starts major 1, so the check "
                "lists its 4 breaking changes against yamaa-v0.5.0 for the "
                "release notes and refuses none; 2 additions",
            ],
        )

    def test_the_same_major_refuses_a_changed_benchmark_file_unless_excepted(self):
        self.repo.benchmark(BASIC)
        self.repo.tag()
        expected = f"{BASIC}/expected/dm.csv"
        self.repo.edit(expected, "DM,01,30", "DM,01,31")
        self.repo.benchmark("benchmarks/sdtm-dm-fixed")
        self.repo.release("1.0.1")
        status, output = self.repo.check()
        self.assertEqual(status, 1, output)
        self.assertEqual(
            output.splitlines(),
            [
                f"Breaking changes against {TAG}:",
                f"benchmarks: {expected}: changed",
                f"Additions against {TAG}:",
                "benchmarks: benchmarks/sdtm-dm-fixed: added",
                f"ERROR: 1 breaking change against {TAG}, the newest release of "
                "major 1. Only a new major release may break the frozen "
                "interface; a reviewed defect fix lists the element it changes "
                "in tools/validation/interface_exceptions.yaml",
            ],
        )
        # An exception entry names the one file by its path.
        entry = {"manifest": "benchmarks", "element": expected}
        self.repo.write(
            STABILITY.EXCEPTIONS,
            yaml.safe_dump({"exceptions": [{**entry, "rule": "CT-1", "issue": 1}]}),
        )
        status, output = self.repo.check()
        self.assertEqual(status, 0, output)
        self.assertEqual(
            output.splitlines(),
            [
                f"Breaking changes against {TAG} that "
                "tools/validation/interface_exceptions.yaml excepts:",
                f"benchmarks: {expected}: changed (CT-1, #1)",
                f"Additions against {TAG}:",
                "benchmarks: benchmarks/sdtm-dm-fixed: added",
                f"PASS: no breaking change against {TAG}, the newest release of "
                "major 1; 1 addition",
            ],
        )


if __name__ == "__main__":
    unittest.main()
