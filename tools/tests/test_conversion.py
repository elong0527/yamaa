"""Pin how the 1.0 conversion gate converts the corpus and composes the owners."""

import tempfile
import unittest
from pathlib import Path
from unittest import mock

import yaml
from tools.benchmark import load_metadata
from tools.commands import conversion
from tools.migrate.excepted import EXCEPTED
from tools.paths import ROOT


def write(path, text):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text, encoding="utf-8")


ERROR = (
    "phase: validation\ncondition: unpaired_fields\nspec_paths:\n  - {path}\n"
    "rule: LOOKUP-10\ncontext:\n  intermediate: LASTDOSE\n"
)


class CorpusTests(unittest.TestCase):
    def test_the_corpus_converts_with_only_negative_spec_paths_rewritten(self):
        """Every converted expected error keeps its phase, condition, rule and
        context; only spec paths change, and each symbolic link stays a link."""
        with tempfile.TemporaryDirectory() as directory:
            fixtures = conversion.convert(Path(directory) / "converted")
            self.assertEqual(fixtures, Path(directory) / "converted" / "benchmarks")
            changed, rewritten = conversion.negative_paths(fixtures)
            self.assertEqual(changed, [])
            self.assertIn(
                (
                    "negative-lookup-empty-order",
                    "intermediates[0]",
                    "intermediates.LASTDOSE",
                ),
                rewritten,
            )
            for link in (ROOT / "benchmarks").rglob("*"):
                if link.is_symlink():
                    converted = fixtures / link.relative_to(ROOT / "benchmarks")
                    self.assertTrue(converted.is_symlink(), converted)
            spec = (fixtures / "adam-adsl-xpt" / "spec.yaml").read_text(encoding="utf-8")
            self.assertTrue(spec.startswith('yamaa: "1.0"\ntype: spec\n'))

    def test_every_excepted_benchmark_exists_and_states_its_decision(self):
        for name, reason in EXCEPTED.items():
            self.assertTrue((ROOT / "benchmarks" / name).is_dir(), name)
            self.assertRegex(reason, r"\b(D\d+|P1-\d+|P2-\d+)\b", name)


class HelperTests(unittest.TestCase):
    def test_a_negative_that_changes_beyond_its_paths_is_named(self):
        with tempfile.TemporaryDirectory() as directory:
            original = Path(directory) / "original"
            converted = Path(directory) / "converted"
            write(
                original / "kept/expected/error.yaml",
                ERROR.format(path="intermediates[0]"),
            )
            write(
                converted / "kept/expected/error.yaml",
                ERROR.format(path="intermediates.LASTDOSE"),
            )
            write(original / "moved/expected/error.yaml", ERROR.format(path="a"))
            write(
                converted / "moved/expected/error.yaml",
                ERROR.format(path="a").replace("unpaired_fields", "other"),
            )
            with mock.patch.object(conversion, "FIXTURES", original):
                changed, rewritten = conversion.negative_paths(converted)
        self.assertEqual(changed, ["moved"])
        self.assertEqual(
            rewritten, [("kept", "intermediates[0]", "intermediates.LASTDOSE")]
        )

    def test_excepted_benchmarks_are_excluded_from_the_plan_with_their_reason(self):
        with tempfile.TemporaryDirectory() as directory:
            fixtures = Path(directory)
            for name in EXCEPTED:
                write(
                    fixtures / name / "benchmark.yaml",
                    "lifecycle: draft\nstandard: TEST\ndomain: DM\n",
                )
            conversion.except_benchmarks(fixtures)
            for name, reason in EXCEPTED.items():
                metadata = load_metadata(fixtures / name)
                self.assertEqual(
                    metadata["conformance"]["excluded"],
                    f"excepted by the 1.0 conversion gate: {reason}",
                )

    def test_each_define_call_lists_its_tables_documents_and_error(self):
        with tempfile.TemporaryDirectory() as directory:
            fixtures = Path(directory)
            define = (
                "lifecycle: draft\nstandard: SDTM\ndomain: DM\ndefine:\n"
                "  environment: project.yaml\n  standard: sdtm\n"
                "  created: '2026-09-11T00:00:00'\n"
            )
            for name, expected in (
                ("positive", ("dm.csv", "dm.xpt", "define.xml")),
                ("negative", ("dm.csv", "error.yaml")),
            ):
                write(fixtures / name / "benchmark.yaml", define)
                write(fixtures / name / "project.yaml", "yamaa: '1.0'\n")
                for file in expected:
                    write(
                        fixtures / name / "expected" / file,
                        ERROR.format(path="sdtm.specs[0]") if file == "error.yaml" else "x",
                    )
            write(
                fixtures / "plain" / "benchmark.yaml",
                "lifecycle: draft\nstandard: SDTM\ndomain: DM\n",
            )
            cases = conversion.define_manifest(fixtures)["cases"]
        self.assertEqual([case["name"] for case in cases], ["negative", "positive"])
        negative, positive = cases
        self.assertEqual(positive["published"], ["dm.csv"])
        self.assertEqual(positive["expected"], ["define.xml", "dm.xpt"])
        self.assertIsNone(positive["error"])
        self.assertEqual(negative["expected"], [])
        self.assertEqual(negative["error"]["spec_paths"], ["sdtm.specs[0]"])
        self.assertEqual(
            {key: positive[key] for key in ("project", "standard", "created")},
            {
                "project": "project.yaml",
                "standard": "sdtm",
                "created": "2026-09-11T00:00:00",
            },
        )


class GateRun:
    """Replace every expensive step and record what the gate asked of it."""

    def __init__(self, changed=(), python=0, define=False, r=0):
        self.changed = list(changed)
        self.python, self.define, self.r = python, define, r
        self.plans = []

    def conformance(self, argv):
        self.plans.append(argv)
        return 0

    def run(self, *argv):
        fixtures = Path("converted/benchmarks")
        with (
            mock.patch.object(conversion, "convert", return_value=fixtures),
            mock.patch.object(
                conversion, "negative_paths", return_value=(self.changed, [])
            ),
            mock.patch.object(conversion, "directories", return_value=[]),
            mock.patch.object(conversion, "except_benchmarks") as self.excepted,
            mock.patch.object(
                conversion.public_conformance, "main", side_effect=self.conformance
            ),
            mock.patch.object(
                conversion.subprocess, "check_output", return_value="abc123\n"
            ),
            mock.patch.object(
                conversion.equivalence,
                "install_python",
                return_value=Path("venv/bin/python"),
            ) as self.install_python,
            mock.patch.object(
                conversion.equivalence, "python_conformance", return_value=self.python
            ) as self.python_conformance,
            mock.patch.object(
                conversion, "python_define", return_value=self.define
            ) as self.python_define,
            mock.patch.object(
                conversion.r_tests, "install", return_value=("archive", {"R_LIBS": "r"})
            ) as self.install_r,
            mock.patch.object(
                conversion.equivalence, "r_conformance", return_value=self.r
            ) as self.r_conformance,
        ):
            conversion.main(list(argv))
        return fixtures


class ConversionGateTests(unittest.TestCase):
    def test_pull_requests_run_python_on_the_converted_corpus(self):
        gate = GateRun()
        fixtures = gate.run()
        self.assertEqual(gate.plans, [["plan", "--fixtures", str(fixtures), "--output", mock.ANY]])
        gate.excepted.assert_called_once_with(fixtures)
        self.assertEqual(gate.python_conformance.call_args.args[3], "abc123")
        self.assertEqual(gate.python_conformance.call_args.args[4], fixtures)
        self.assertEqual(gate.python_define.call_args.args[1:], (Path("venv/bin/python"), fixtures))
        gate.install_r.assert_not_called()

    def test_both_hosts_add_r(self):
        gate = GateRun()
        fixtures = gate.run("--hosts", "python,r")
        self.assertEqual(gate.r_conformance.call_args.args[1], {"R_LIBS": "r"})
        self.assertEqual(gate.r_conformance.call_args.args[4], fixtures)

    def test_a_negative_changed_beyond_its_paths_stops_before_any_build(self):
        gate = GateRun(changed=["negative-x"])
        with self.assertRaises(SystemExit) as stopped:
            gate.run()
        self.assertIn("negative-x", str(stopped.exception.code))
        gate.install_python.assert_not_called()

    def test_each_failing_host_check_fails_the_gate(self):
        for gate, named in (
            (GateRun(python=1), "Python public benchmark conformance"),
            (GateRun(define=True), "Python Define benchmarks"),
        ):
            with self.assertRaises(SystemExit) as stopped:
                gate.run()
            self.assertIn(named, str(stopped.exception.code))
        with self.assertRaises(SystemExit) as stopped:
            GateRun(r=1).run("--hosts", "r")
        self.assertIn("R public benchmark conformance", str(stopped.exception.code))

    def test_hosts_name_python_or_r(self):
        with self.assertRaises(SystemExit):
            GateRun().run("--hosts", "julia")


if __name__ == "__main__":
    unittest.main()
