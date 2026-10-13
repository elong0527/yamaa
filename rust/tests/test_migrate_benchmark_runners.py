from __future__ import annotations

import json
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "tools"))
from migrate_benchmark_runners import migrate, rewrite_runner


class RunnerMigrationTests(unittest.TestCase):
    def test_only_the_positive_both_host_intersection_is_changed(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            ratchet = root / "ratchet.json"
            ratchet.write_text(
                json.dumps(
                    {
                        "protocol": "public-conformance-ratchet/1",
                        "python": ["both", "python-only", "negative"],
                        "r": ["both", "r-only", "negative"],
                    }
                )
            )
            source = (
                'import yamaa\n\nadsl = yamaa.yamaa_domain("spec.yaml").output\nadsl\n'
            )
            for name in ("both", "python-only", "r-only", "negative"):
                case = root / name
                (case / "expected").mkdir(parents=True)
                (case / "run.py").write_text(source)
            (root / "negative/expected/error.yaml").touch()
            self.assertEqual(migrate(root, ratchet), [root / "both/run.py"])
            self.assertEqual((root / "both/run.py").read_text(), source)
            migrate(root, ratchet, write=True)
            self.assertEqual(migrate(root, ratchet, write=True), [])
            for name in ("python-only", "r-only", "negative"):
                self.assertEqual((root / name / "run.py").read_text(), source)

    def test_function_environment_is_explicit_and_comments_are_untouched(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "python").mkdir()
            (root / "python/environment.yaml").touch()
            runner = root / "run.py"
            runner.write_text(
                "# yamaa.yamaa_domain remains quoted history\nimport yamaa\n"
                'test = yamaa.yamaa_domain("spec.yaml").output\n'
            )
            changed = rewrite_runner(runner)
            self.assertIn("# yamaa.yamaa_domain remains quoted history", changed)
            self.assertIn(
                'yamaa.domain("spec.yaml", environment="python/environment.yaml")',
                changed,
            )
            runner.write_text(changed)
            self.assertEqual(rewrite_runner(runner), changed)

    def test_define_environment_does_not_change_the_qualified_domain_call(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "environment.yaml").touch()
            runner = root / "run.py"
            runner.write_text(
                'import yamaa\ntest = yamaa.yamaa_domain("spec.yaml").output\n'
            )
            self.assertNotIn("environment=", rewrite_runner(runner))

    def test_invalid_runner_refuses_the_whole_batch_before_writing(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            ratchet = root / "ratchet.json"
            ratchet.write_text(
                json.dumps(
                    {
                        "protocol": "public-conformance-ratchet/1",
                        "python": ["a", "z"],
                        "r": ["a", "z"],
                    }
                )
            )
            for name, call in (
                ("a", '"spec.yaml"'),
                ("z", '"spec.yaml", schema_root="yaml"'),
            ):
                (root / name).mkdir()
                (root / name / "run.py").write_text(
                    f"import yamaa\ntest = yamaa.yamaa_domain({call}).output\n"
                )
            original = (root / "a/run.py").read_text()
            with self.assertRaisesRegex(ValueError, "unsupported runner arguments"):
                migrate(root, ratchet, write=True)
            self.assertEqual((root / "a/run.py").read_text(), original)
