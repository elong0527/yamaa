"""The interface stability check (#3030 P-STAB-1): its baseline, the line
manifests and the exception list, against release tags in temporary
repositories. Nothing here reads the network or this repository's tags."""

import contextlib
import importlib
import io
import subprocess
import tempfile
import unittest
from pathlib import Path
from unittest import mock

import yaml
from tools.commands import repository
from tools.packaging import release_identity
from tools.paths import ROOT
from tools.validation import check_interface_stability as STABILITY
from tools.validation import interface_lines as LINES
from tools.validation._repository_yaml import UniqueKeyLoader
from tools.validation.interface_manifests import Change, ManifestError, Tree

CONDITIONS = """\
version: "1.0"
conditions:
  alpha_failed:
    phases: [validation, derivation]
    message: "alpha failed"
  beta_failed:
    phases: [verification]
    message: "beta failed"
"""
INTERFACE = """\
# Generated. The check ignores comment lines.
values.status.succeeded
values.status.failed
public.functions.lint.args[1]: fix: flag = false
"""
PUBLIC_API = """\
# Generated.
yamaa: pub mod api
yamaa::api::Engine: impl Engine
yamaa::api::Engine: impl Engine
yamaa::api::Engine::check: pub fn check(&self) -> Table
"""
EMPTY = "exceptions: []\n"


def git(root, *arguments):
    return subprocess.run(
        [
            *("git", "-C", str(root)),
            *("-c", "user.name=test", "-c", "user.email=test@example.invalid"),
            *("-c", "commit.gpgSign=false", "-c", "tag.gpgSign=false"),
            *arguments,
        ],
        check=True,
        capture_output=True,
        text=True,
    ).stdout


def run_check(root):
    """Run the check with the line manifests' comparer alone: these
    repositories hold no other comparer's manifests."""
    output = io.StringIO()
    with (
        mock.patch.object(STABILITY, "COMPARERS", (LINES.__name__,)),
        contextlib.redirect_stdout(output),
    ):
        status = STABILITY.main(["--root", str(root)])
    return status, output.getvalue()


class Repository:
    """A git repository holding the three line manifests, the release identity
    and an empty exception list."""

    def __init__(self, root):
        self.root = root
        root.mkdir(parents=True, exist_ok=True)
        git(root, "init", "-q", "-b", "main")
        self.write(LINES.CONDITIONS, CONDITIONS)
        self.write(LINES.INTERFACE, INTERFACE)
        self.write(LINES.PUBLIC_API, PUBLIC_API)
        self.write(STABILITY.EXCEPTIONS, EMPTY)

    def write(self, path, text):
        (self.root / path).parent.mkdir(parents=True, exist_ok=True)
        (self.root / path).write_text(text, encoding="utf-8")

    def edit(self, path, old, new):
        text = (self.root / path).read_text(encoding="utf-8")
        assert old in text, (path, old)
        self.write(path, text.replace(old, new))

    def release(self, version, candidate=None):
        self.write(
            release_identity.DECLARATION,
            f'version = "{version}"\n'
            + ("" if candidate is None else f"candidate = {candidate}\n"),
        )

    def commit(self):
        git(self.root, "add", "-A")
        git(self.root, "commit", "-q", "--allow-empty", "-m", "change")

    def tag(self, name):
        """Commit the tree as the release the tag names, and tag it."""
        identity = release_identity.from_tag(name)
        self.release(identity.version, identity.candidate)
        self.commit()
        git(self.root, "tag", "-a", name, "-m", name)

    def check(self):
        return run_check(self.root)


class Temporary(unittest.TestCase):
    def setUp(self):
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        self.directory = Path(directory.name)


class Baseline(Temporary):
    def test_major_zero_is_never_enforced(self):
        repo = Repository(self.directory / "repo")
        repo.tag("yamaa-v0.4.0")
        repo.edit(LINES.CONDITIONS, "  beta_failed:", "  gamma_failed:")
        repo.release("0.5.0")
        status, output = repo.check()
        self.assertEqual(status, 0, output)
        self.assertIn("PASS: 0.5.0 is a major 0 release", output)
        self.assertNotIn("beta_failed", output)
        # Major 0 needs no baseline, so it needs no git repository either.
        plain = self.directory / "plain"
        (plain / "tools/validation").mkdir(parents=True)
        (plain / STABILITY.EXCEPTIONS).write_text(EMPTY)
        (plain / "tools/packaging").mkdir()
        (plain / release_identity.DECLARATION).write_text('version = "0.6.0"\n')
        self.assertEqual(run_check(plain)[0], 0)

    def test_a_new_major_candidate_lists_its_breaks_and_passes(self):
        repo = Repository(self.directory / "repo")
        # The newest final tag of an earlier major is the baseline: not an
        # older one, and not a candidate, even of the release's own major.
        repo.edit(LINES.CONDITIONS, "  beta_failed:", "  older_failed:")
        repo.tag("yamaa-v1.2.0")
        repo.write(LINES.CONDITIONS, CONDITIONS)
        repo.tag("yamaa-v1.3.0")
        repo.edit(LINES.CONDITIONS, "  alpha_failed:", "  candidate_failed:")
        repo.tag("yamaa-v2.0.0-rc.1")
        repo.write(LINES.CONDITIONS, CONDITIONS)
        repo.edit(LINES.CONDITIONS, "  beta_failed:", "  gamma_failed:")
        repo.edit(LINES.INTERFACE, "flag = false", "flag = true")
        repo.release("2.0.0", candidate=2)
        status, output = repo.check()
        self.assertEqual(status, 0, output)
        self.assertEqual(
            output.splitlines(),
            [
                "Breaking changes against yamaa-v1.3.0, for the release notes "
                "of 2.0.0:",
                "yaml/conditions.yaml: beta_failed: removed",
                "rust/interface/manifest.txt: public.functions.lint.args[1]: "
                "changed from `fix: flag = false` to `fix: flag = true`",
                "Additions against yamaa-v1.3.0:",
                "yaml/conditions.yaml: gamma_failed: added",
                "PASS: 2.0.0 release candidate 2 starts major 2, so the check "
                "lists its 2 breaking changes against yamaa-v1.3.0 for the "
                "release notes and refuses none; 1 addition",
            ],
        )

    def test_a_new_major_without_an_earlier_release_passes(self):
        repo = Repository(self.directory / "repo")
        repo.tag("yamaa-v1.0.0-rc.1")
        repo.release("1.0.0", candidate=2)
        status, output = repo.check()
        self.assertEqual(status, 0, output)
        self.assertIn("no final release tag of an earlier major", output)

    def test_the_same_major_refuses_a_break(self):
        repo = Repository(self.directory / "repo")
        repo.tag("yamaa-v1.0.0")
        repo.edit(LINES.PUBLIC_API, "yamaa: pub mod api\n", "")
        repo.tag("yamaa-v1.1.0")
        repo.edit(LINES.CONDITIONS, "  beta_failed:", "  older_failed:")
        repo.tag("yamaa-v1.2.0-rc.1")
        repo.write(LINES.CONDITIONS, CONDITIONS)
        repo.release("1.2.0", candidate=2)
        status, output = repo.check()
        self.assertEqual(status, 0, output)
        self.assertEqual(
            output.splitlines(),
            [
                "PASS: no breaking change against yamaa-v1.1.0, the newest "
                "release of major 1; 0 additions"
            ],
        )
        repo.edit(LINES.CONDITIONS, "[validation, derivation]", "[validation]")
        repo.edit(LINES.INTERFACE, "values.status.failed\n", "")
        repo.edit(LINES.PUBLIC_API, "check(&self)", "check(&self, strict: bool)")
        status, output = repo.check()
        self.assertEqual(status, 1, output)
        self.assertEqual(
            output.splitlines(),
            [
                "Breaking changes against yamaa-v1.1.0:",
                "yaml/conditions.yaml: alpha_failed: phase derivation removed",
                "rust/interface/manifest.txt: values.status.failed: removed",
                "rust/crates/yamaa/public-api.txt: yamaa::api::Engine::check: "
                "pub fn check(&self) -> Table: removed",
                "Additions against yamaa-v1.1.0:",
                "rust/crates/yamaa/public-api.txt: yamaa::api::Engine::check: "
                "pub fn check(&self, strict: bool) -> Table: added",
                "ERROR: 3 breaking changes against yamaa-v1.1.0, the newest "
                "release of major 1. Only a new major release may break the "
                "frozen interface; a reviewed defect fix lists the element it "
                "changes in tools/validation/interface_exceptions.yaml",
            ],
        )

    def test_a_shallow_clone_fails_until_the_ci_step_fetches_the_tags(self):
        origin = Repository(self.directory / "origin")
        origin.tag("yamaa-v1.0.0")
        origin.edit(LINES.CONDITIONS, "  beta_failed:", "  gamma_failed:")
        origin.release("1.1.0")
        origin.commit()
        clone = self.directory / "clone"
        subprocess.run(
            ["git", "clone", "-q", "--depth", "1", "--no-tags"]
            + [origin.root.as_uri(), str(clone)],
            check=True,
            capture_output=True,
        )
        status, output = run_check(clone)
        self.assertEqual(status, 1, output)
        self.assertIn(
            "ERROR: 1.1.0 is compared with the newest "
            "yamaa-v1.<minor>.<patch> tag, and none is present: fetch the "
            "release tags into this shallow clone with git fetch",
            output,
        )
        # CI's repository job fetches the tags before it runs the check.
        steps = yaml.load(
            (ROOT / ".github/workflows/ci.yml").read_text(encoding="utf-8"),
            Loader=UniqueKeyLoader,
        )["jobs"]["repository"]["steps"]
        names = [step.get("name") or step.get("run") or step["uses"] for step in steps]
        fetch = steps[names.index("Fetch the release tags")]
        self.assertLess(
            names.index("Fetch the release tags"), names.index("make check-repository")
        )
        subprocess.run(
            ["bash", "-c", fetch["run"]], cwd=clone, check=True, capture_output=True
        )
        self.assertEqual(git(clone, "rev-parse", "--is-shallow-repository"), "true\n")
        status, output = run_check(clone)
        self.assertEqual(status, 1, output)
        self.assertIn("yaml/conditions.yaml: beta_failed: removed\n", output)

    def test_an_unreadable_release_identity_fails(self):
        repo = Repository(self.directory / "repo")
        repo.write(release_identity.DECLARATION, 'version = "1.0"\n')
        status, output = repo.check()
        self.assertEqual(status, 1)
        self.assertIn("ERROR: tools/packaging/release.toml:", output)


class Exceptions(Temporary):
    def entry(self, manifest, element):
        return {"manifest": manifest, "element": element, "rule": "CT-13", "issue": 1}

    def test_an_exception_skips_exactly_one_element(self):
        repo = Repository(self.directory / "repo")
        repo.tag("yamaa-v1.0.0")
        repo.edit(LINES.CONDITIONS, "  beta_failed:", "  gamma_failed:")
        repo.edit(LINES.INTERFACE, "flag = false", "flag = true")
        repo.release("1.0.1")
        entries = [self.entry(LINES.INTERFACE, "public.functions.lint.args[1]")]
        repo.write(STABILITY.EXCEPTIONS, yaml.safe_dump({"exceptions": entries}))
        status, output = repo.check()
        self.assertEqual(status, 1, output)
        lines = output.splitlines()
        self.assertEqual(
            lines[:4],
            [
                "Breaking changes against yamaa-v1.0.0:",
                "yaml/conditions.yaml: beta_failed: removed",
                "Breaking changes against yamaa-v1.0.0 that "
                "tools/validation/interface_exceptions.yaml excepts:",
                "rust/interface/manifest.txt: public.functions.lint.args[1]: "
                "changed from `fix: flag = false` to `fix: flag = true` "
                "(CT-13, #1)",
            ],
        )
        self.assertTrue(lines[-1].startswith("ERROR: 1 breaking change against"))
        # Excepting the other element too passes; an entry that matches no
        # break, such as one for the element's name in another manifest, is
        # reported and skips nothing.
        entries += [
            self.entry(LINES.CONDITIONS, "beta_failed"),
            self.entry(LINES.PUBLIC_API, "beta_failed"),
        ]
        repo.write(STABILITY.EXCEPTIONS, yaml.safe_dump({"exceptions": entries}))
        status, output = repo.check()
        self.assertEqual(status, 0, output)
        self.assertIn(
            "NOTE: tools/validation/interface_exceptions.yaml: "
            "rust/crates/yamaa/public-api.txt: beta_failed matches no breaking "
            "change against yamaa-v1.0.0; remove the entry\n",
            output,
        )
        self.assertTrue(output.splitlines()[-1].startswith("PASS: no breaking change"))

    def test_the_committed_list_is_empty(self):
        self.assertEqual(STABILITY.load_exceptions(ROOT), {})

    def test_each_malformed_list_is_refused(self):
        valid = self.entry(LINES.CONDITIONS, "beta_failed")
        for document in (
            {"exceptions": [valid], "extra": []},
            {"exceptions": {}},
            {"exceptions": [{**valid, "reason": "a fix"}]},
            {"exceptions": [{key: valid[key] for key in ("manifest", "element")}]},
            {"exceptions": [{**valid, "element": "beta_failed "}]},
            {"exceptions": [{**valid, "manifest": ""}]},
            {"exceptions": [{**valid, "rule": "ct-13"}]},
            {"exceptions": [{**valid, "issue": 0}]},
            {"exceptions": [{**valid, "issue": True}]},
            {"exceptions": [{**valid, "issue": "1"}]},
            {"exceptions": [valid, {**valid, "issue": 2}]},
        ):
            with self.subTest(document=document):
                (self.directory / "tools/validation").mkdir(parents=True, exist_ok=True)
                (self.directory / STABILITY.EXCEPTIONS).write_text(
                    yaml.safe_dump(document)
                )
                with self.assertRaises(ManifestError):
                    STABILITY.load_exceptions(self.directory)
        # A malformed list fails even at major 0, before any baseline is read.
        (self.directory / "tools/packaging").mkdir()
        (self.directory / release_identity.DECLARATION).write_text(
            'version = "0.5.0"\n'
        )
        status, output = run_check(self.directory)
        self.assertEqual(status, 1)
        self.assertIn("ERROR: tools/validation/interface_exceptions.yaml:", output)


class LineManifests(Temporary):
    """Each comparer, between two working trees."""

    def changes(self, path, before, after):
        trees = []
        for name, text in (("before", before), ("after", after)):
            if text is not None:
                (self.directory / name / path).parent.mkdir(parents=True, exist_ok=True)
                (self.directory / name / path).write_text(text)
            trees.append(Tree(self.directory / name))
        compare = {
            LINES.CONDITIONS: LINES._compare_conditions,
            LINES.INTERFACE: LINES._compare_interface,
            LINES.PUBLIC_API: LINES._compare_public_api,
        }[path]
        return [
            (change.element, change.what, change.breaking) for change in compare(*trees)
        ]

    def test_conditions_compare_names_and_phases_but_not_messages(self):
        after = (
            CONDITIONS.replace("[validation, derivation]", "[planning]")
            .replace('"alpha failed"', '"alpha went wrong"')
            .replace("  beta_failed:", "  gamma_failed:")
        )
        self.assertEqual(
            self.changes(LINES.CONDITIONS, CONDITIONS, after),
            [
                ("alpha_failed", "phases validation, derivation removed", True),
                ("beta_failed", "removed", True),
                ("alpha_failed", "phase planning added", False),
                ("gamma_failed", "added", False),
            ],
        )

    def test_the_interface_manifest_compares_lines_by_element(self):
        before = INTERFACE + "cli.synopsis: yamaa check PATH\n"
        after = (
            before.replace("Generated.", "Generated by another tool.")
            .replace("values.status.failed\n", "")
            .replace("flag = false", "flag = true")
            + "cli.synopsis: yamaa run PATH\nvalues.status.skipped\n"
        )
        self.assertEqual(
            self.changes(LINES.INTERFACE, before, after),
            [
                ("values.status.failed", "removed", True),
                (
                    "public.functions.lint.args[1]",
                    "changed from `fix: flag = false` to `fix: flag = true`",
                    True,
                ),
                # A line added to an element listed on several lines.
                ("cli.synopsis", "added `yamaa run PATH`", False),
                ("values.status.skipped", "added", False),
            ],
        )
        self.assertEqual(
            self.changes(
                LINES.INTERFACE,
                after,
                after.replace("yamaa check PATH", "yamaa check FILE").replace(
                    "cli.synopsis: yamaa run PATH\n", ""
                ),
            ),
            [
                (
                    "cli.synopsis",
                    "changed from `yamaa check PATH`, `yamaa run PATH` to "
                    "`yamaa check FILE`",
                    True,
                )
            ],
        )

    def test_the_public_api_compares_distinct_lines(self):
        after = PUBLIC_API.replace("yamaa::api::Engine: impl Engine\n", "", 1).replace(
            "-> Table", "-> Option<Table>"
        )
        self.assertEqual(
            self.changes(LINES.PUBLIC_API, PUBLIC_API, after),
            [
                (
                    "yamaa::api::Engine::check: pub fn check(&self) -> Table",
                    "removed",
                    True,
                ),
                (
                    "yamaa::api::Engine::check: pub fn check(&self) -> Option<Table>",
                    "added",
                    False,
                ),
            ],
        )

    def test_a_new_manifest_is_all_additions_and_a_missing_one_fails(self):
        for path, text in (
            (LINES.CONDITIONS, CONDITIONS),
            (LINES.INTERFACE, INTERFACE),
            (LINES.PUBLIC_API, PUBLIC_API),
        ):
            with self.subTest(path=path):
                self.directory = Path(tempfile.mkdtemp(dir=self.directory))
                changes = self.changes(path, None, text)
                self.assertTrue(changes)
                self.assertFalse(any(breaking for _, _, breaking in changes))
                self.directory = Path(tempfile.mkdtemp(dir=self.directory))
                with self.assertRaisesRegex(ManifestError, "missing from"):
                    self.changes(path, text, None)

    def test_a_malformed_registry_fails(self):
        with self.assertRaisesRegex(ManifestError, "mapping each name"):
            self.changes(LINES.CONDITIONS, "conditions: [alpha_failed]\n", CONDITIONS)


class Comparers(Temporary):
    def test_every_registered_comparer_defines_compare(self):
        self.assertIn("tools.validation.check_interface_stability", repository.CHECKS)
        for name in STABILITY.COMPARERS:
            with self.subTest(name=name):
                self.assertTrue(callable(importlib.import_module(name).compare))

    def test_a_tree_reads_a_tag_or_the_working_tree(self):
        repo = Repository(self.directory / "repo")
        repo.tag("yamaa-v1.0.0")
        repo.write(LINES.CONDITIONS, "conditions: {}\n")
        tag, work = Tree(repo.root, "yamaa-v1.0.0"), Tree(repo.root)
        self.assertEqual(tag.text(LINES.CONDITIONS), CONDITIONS)
        self.assertEqual(work.text(LINES.CONDITIONS), "conditions: {}\n")
        for tree in (tag, work):
            with self.subTest(tree=str(tree)):
                self.assertIsNone(tree.read("yaml/absent.yaml"))
                self.assertIsNone(tree.read("yaml"))
                self.assertEqual(
                    tree.files("tools"),
                    ["tools/packaging/release.toml", STABILITY.EXCEPTIONS],
                )
                self.assertEqual(tree.files("absent"), [])
        with self.assertRaises(ManifestError):
            Tree(repo.root, "yamaa-v9.0.0").read(LINES.CONDITIONS)
        self.assertEqual(str(Change("m", "e", "removed", True)), "m: e: removed")


if __name__ == "__main__":
    unittest.main()
