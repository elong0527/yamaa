"""PR selection keeps independent changes cheap and unknown inputs qualified."""

import subprocess
import tempfile
import unittest
from pathlib import Path

from tools.ci import pr_changes
from tools.tests import support


def selection(repository="full", **enabled):
    """An authored expected decision, with every unmentioned flag disabled."""
    return {
        "repository": repository,
        "rust": False,
        "minimum": False,
        "python": False,
        "r": False,
        "equivalence_python": False,
        "equivalence_r": False,
        "workflow_lint": False,
        "loaded": False,
    } | enabled


FULL = selection(
    rust=True,
    minimum=True,
    python=True,
    r=True,
    equivalence_python=True,
    equivalence_r=True,
)


class AffectedLanes(unittest.TestCase):
    def test_prose_and_site_assets_need_only_documentation_checks(self):
        self.assertEqual(
            pr_changes.select(
                ["README.md", "AGENTS.md", "docs/articles/api.md", "docs/styles.css"]
            ),
            selection(repository="docs"),
        )

    def test_host_changes_qualify_the_affected_installed_host(self):
        for path, expected in (
            (
                "python/src/yamaa/__init__.py",
                selection(python=True, equivalence_python=True),
            ),
            (
                "python/tests/installed/installed_example.py",
                selection(python=True, equivalence_python=True),
            ),
            ("R/yamaa/R/api.R", selection(r=True, equivalence_r=True)),
        ):
            with self.subTest(path=path):
                self.assertEqual(pr_changes.select([path]), expected)

    def test_repository_tools_run_their_tests_without_host_qualification(self):
        for path in (
            "tools/tests/test_example.py",
            "tools/benchmark_docs/generate.py",
            "tools/validation/check_documentation.py",
        ):
            with self.subTest(path=path):
                self.assertEqual(pr_changes.select([path]), selection(python=True))

    def test_engine_and_shared_or_unknown_inputs_qualify_both_hosts(self):
        for path in (
            "rust/crates/yamaa-core/src/expression/mod.rs",
            "rust/rust-toolchain.toml",
            "benchmarks/example/expected/table.csv",
            "yaml/schema_spec.yaml",
            "tools/packaging/check_cargo_lock.py",
            "tools/staging/stage_r_package.py",
            "Makefile",
            ".cargo/config.toml",
            "new-build-input/settings.toml",
            "docs/tools/generator.py",
        ):
            with self.subTest(path=path):
                self.assertEqual(pr_changes.select([path]), FULL)

    def test_boundary_inputs_also_run_the_loaded_mode(self):
        for path, lanes in (
            ("rust/crates/yamaa-boundary/src/loader.rs", FULL),
            ("rust/crates/yamaa-engine/examples/fixture_no_entry.rs", FULL),
            ("rust/crates/yamaa-python/src/interface.rs", FULL),
            ("rust/crates/yamaa-r/src/interface.rs", FULL),
            ("rust/interface/README.md", FULL),
            ("rust/Cargo.lock", FULL),
            ("rust/tests/host-fixtures/cases.json", FULL),
            ("tools/commands/loaded.py", FULL),
            ("tools/commands/equivalence.py", FULL),
            ("tools/ci/pr_changes.py", FULL),
            ("python/pyproject.toml", FULL),
            ("python/uv.lock", FULL),
            ("R/yamaa/src/Makevars.in", selection(r=True, equivalence_r=True)),
            ("R/yamaa/tests/interface.R", selection(r=True, equivalence_r=True)),
            (
                "python/tests/installed/installed_interface.py",
                selection(python=True, equivalence_python=True),
            ),
        ):
            with self.subTest(path=path):
                self.assertEqual(pr_changes.select([path]), lanes | {"loaded": True})

    def test_workflow_lint_is_selected_for_any_github_change(self):
        self.assertEqual(
            pr_changes.select([".github/workflows/native.yml"]),
            selection(python=True, workflow_lint=True),
        )
        self.assertEqual(
            pr_changes.select([".github/workflows/ci.yml"]),
            FULL | {"workflow_lint": True, "loaded": True},
        )

    def test_mixed_changes_take_the_union_and_empty_diffs_run_every_lane(self):
        self.assertEqual(
            pr_changes.select(
                ["README.md", "R/yamaa/R/api.R", "tools/tests/test_x.py"]
            ),
            selection(python=True, r=True, equivalence_r=True),
        )
        self.assertEqual(pr_changes.select([]), FULL | {"loaded": True})

    def test_schedules_keep_development_checks_and_dispatch_qualifies_both_hosts(self):
        development = FULL | {
            "equivalence_python": False,
            "equivalence_r": False,
            "workflow_lint": True,
            "loaded": False,
        }
        # Main's schedule also checks the loaded mode; a release push does not.
        for event, loaded in (("push", False), ("schedule", True)):
            with self.subTest(event=event):
                self.assertEqual(
                    pr_changes.decision(event), development | {"loaded": loaded}
                )
        self.assertEqual(
            pr_changes.decision("workflow_dispatch"),
            FULL | {"workflow_lint": True, "loaded": True},
        )

    def test_an_unknown_event_cannot_silently_skip_qualification(self):
        with self.assertRaisesRegex(ValueError, "unsupported CI event"):
            pr_changes.decision("unexpected")


class PullRequestDiff(unittest.TestCase):
    def setUp(self):
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        self.repo = support.Repository(Path(directory.name) / "repo")
        self.repo.write("tools/old.py", "old code\n")
        self.repo.commit("base")
        self.base = support.git(self.repo.root, "rev-parse", "HEAD")

    def decide(self):
        head = support.git(self.repo.root, "rev-parse", "HEAD")
        return pr_changes.decision("pull_request", self.base, head, root=self.repo.root)

    def test_base_advancement_does_not_add_its_engine_changes_to_a_docs_pr(self):
        support.git(self.repo.root, "switch", "-c", "pr")
        self.repo.write("docs/example.md", "documentation\n")
        self.repo.commit()
        support.git(self.repo.root, "switch", "main")
        self.repo.write("rust/Cargo.lock", "base-only change\n")
        self.repo.commit()
        self.base = support.git(self.repo.root, "rev-parse", "HEAD")
        support.git(self.repo.root, "switch", "pr")
        self.assertEqual(self.decide(), selection(repository="docs"))

    def test_a_code_file_moved_to_docs_still_runs_qualification(self):
        (self.repo.root / "docs").mkdir()
        support.git(self.repo.root, "mv", "tools/old.py", "docs/former-code.md")
        self.repo.commit()
        self.assertEqual(self.decide(), FULL)

    def test_deletions_and_unusual_filenames_are_not_lost(self):
        self.repo.remove("tools/old.py")
        self.repo.write("docs/a\nfile.md", "documentation\n")
        self.repo.commit()
        head = support.git(self.repo.root, "rev-parse", "HEAD")
        self.assertEqual(
            set(pr_changes.changed_paths(self.base, head, self.repo.root)),
            {"tools/old.py", "docs/a\nfile.md"},
        )
        self.assertEqual(self.decide(), FULL)

    def test_a_missing_revision_or_wrong_checkout_fails_instead_of_skipping(self):
        for base, head, error in (
            ("", self.base, ValueError),
            (self.base, "", ValueError),
            (self.base, "f" * 40, ValueError),
            ("missing-base", self.base, subprocess.CalledProcessError),
        ):
            with self.subTest(base=base, head=head), self.assertRaises(error):
                pr_changes.changed_paths(base, head, self.repo.root)


if __name__ == "__main__":
    unittest.main()
