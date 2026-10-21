"""The parity gate of check_diagnostics: every registered condition has a
parity case or an entry in the unreached list, and every entry is needed
(#2846)."""

import contextlib
import io
import json
import sys
import tempfile
import textwrap
import unittest
from pathlib import Path
from unittest import mock

from tools.paths import ROOT
from tools.validation import check_diagnostics as guard

CONDITIONS = """\
version: "1.0"
conditions:
  benchmark_reached: {phases: [validation], message: "a"}
  host_reached: {phases: [ingest], message: "b"}
  sas_reached: {phases: [ingest], message: "c"}
  retired_condition: {phases: [validation], message: "d"}
  uncovered_condition: {phases: [derivation], message: "e"}
"""
UNREACHED = """\
unreachable:
  retired_condition: a recorded decision retired every case
not_covered:
  "#2":
    - uncovered_condition
"""
LIST = guard.UNREACHED
NO_CASE = (
    "yaml/conditions.yaml: {} has no parity case: add a negative benchmark or a "
    f"host-fixture case that reaches it, or list it in {LIST}"
)


class ParityGateTests(unittest.TestCase):
    def setUp(self):
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        self.root = Path(directory.name)
        self.write("yaml/conditions.yaml", CONDITIONS)
        self.write(
            "benchmarks/negative-benchmark-reached/expected/error.yaml",
            "phase: validation\ncondition: benchmark_reached\nrule: VALUE-1\n",
        )
        # Positive benchmarks and other expected files are no parity cases.
        self.write(
            "benchmarks/schema-positive/expected/error.yaml",
            "phase: validation\ncondition: uncovered_condition\n",
        )
        self.write(
            "rust/tests/host-fixtures/cases.json",
            json.dumps(
                {
                    "version": "1",
                    "families": {
                        "parity": {
                            "cases": [
                                {
                                    "name": "a",
                                    "issues": [
                                        {"phase": "ingest", "condition": "host_reached"}
                                    ],
                                }
                            ]
                        }
                    },
                }
            ),
        )
        self.write(
            "rust/tests/host-fixtures/sas-cases.json",
            json.dumps({"refusals": [{"id": "r", "condition": "sas_reached"}]}),
        )
        self.write(LIST, UNREACHED)

    def write(self, relative, text):
        path = self.root / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(textwrap.dedent(text), encoding="utf-8")
        return path

    def errors(self):
        return guard.parity_errors(self.root)

    def register(self, condition):
        path = self.root / "yaml/conditions.yaml"
        path.write_text(
            path.read_text(encoding="utf-8")
            + f'  {condition}: {{phases: [validation], message: "x"}}\n',
            encoding="utf-8",
        )

    def test_the_repository_passes(self):
        self.assertEqual(guard.parity_errors(ROOT), [])

    def test_every_condition_with_a_case_or_an_entry_passes(self):
        self.assertEqual(self.errors(), [])

    def test_cases_come_from_negative_benchmarks_and_every_host_fixture(self):
        self.assertEqual(
            guard.parity_cases(self.root.resolve()),
            {
                "benchmark_reached": "benchmarks/negative-benchmark-reached",
                "host_reached": "rust/tests/host-fixtures/cases.json at "
                "/families/parity/cases/0/issues/0",
                "sas_reached": "rust/tests/host-fixtures/sas-cases.json at /refusals/0",
            },
        )

    def test_a_registered_condition_without_a_case_or_an_entry_fails(self):
        self.register("newly_registered")
        self.assertEqual(self.errors(), [NO_CASE.format("newly_registered")])

    def test_an_entry_that_a_case_reaches_fails(self):
        cases = {
            "benchmark_reached": (
                '  "#2":\n    - benchmark_reached\n    - uncovered_condition\n',
                "not_covered.#2: benchmarks/negative-benchmark-reached reaches "
                "benchmark_reached",
            ),
            "host_reached": (
                '  "#2":\n    - uncovered_condition\n  "#3":\n    - host_reached\n',
                "not_covered.#3: rust/tests/host-fixtures/cases.json at "
                "/families/parity/cases/0/issues/0 reaches host_reached",
            ),
            "sas_reached": (
                '  "#2":\n    - sas_reached\n    - uncovered_condition\n',
                "not_covered.#2: rust/tests/host-fixtures/sas-cases.json at "
                "/refusals/0 reaches sas_reached",
            ),
        }
        for condition, (issues, where) in cases.items():
            with self.subTest(condition=condition):
                self.write(
                    LIST,
                    UNREACHED.replace('  "#2":\n    - uncovered_condition\n', issues),
                )
                self.assertEqual(self.errors(), [f"{LIST}: {where}; remove its entry"])

    def test_an_entry_for_an_unregistered_condition_fails(self):
        self.write(
            LIST,
            UNREACHED.replace(
                "  retired_condition:",
                "  gone_condition: it left the registry\n  retired_condition:",
            ),
        )
        self.assertEqual(
            self.errors(),
            [
                f"{LIST}: unreachable.gone_condition: gone_condition is not "
                "registered in yaml/conditions.yaml"
            ],
        )

    def test_the_list_keeps_its_form(self):
        cases = {
            "- retired_condition\n": "expected a mapping of unreachable and not_covered",
            UNREACHED + "reasons: {}\n": "reasons: unknown field",
            "unreachable: [retired_condition]\n": (
                "unreachable: expected conditions with reasons"
            ),
            UNREACHED.replace("a recorded decision retired every case", "''"): (
                "unreachable.retired_condition: expected a one-line reason why no "
                "case can reach it"
            ),
            UNREACHED.replace(
                "a recorded decision retired every case", "|\n    two\n    lines"
            ): (
                "unreachable.retired_condition: expected a one-line reason why no "
                "case can reach it"
            ),
            UNREACHED.replace(
                "  retired_condition:", "  sorted_last: x\n  retired_condition:"
            ): "unreachable: expected sorted conditions",
            "not_covered: [uncovered_condition]\n": (
                "not_covered: expected family issues (#N) with their conditions"
            ),
            UNREACHED.replace('"#2"', '"2"'): (
                "not_covered.2: expected a family issue as #N"
            ),
            UNREACHED.replace("    - uncovered_condition\n", "    []\n"): (
                "not_covered.#2: expected a list of conditions"
            ),
            UNREACHED.replace(
                "    - uncovered_condition\n",
                "    - uncovered_condition\n    - retired_condition\n",
            ): "not_covered.#2: expected sorted conditions",
            UNREACHED.replace(
                "    - uncovered_condition\n",
                "    - retired_condition\n    - uncovered_condition\n",
            ): (
                "not_covered.#2: retired_condition is already listed at "
                "unreachable.retired_condition"
            ),
            UNREACHED + "unreachable: {}\n": "unreadable list",
        }
        for text, expected in cases.items():
            with self.subTest(text=text):
                self.write(LIST, text)
                errors = self.errors()
                self.assertTrue(
                    any(error.startswith(f"{LIST}: {expected}") for error in errors),
                    errors,
                )
        (self.root / LIST).unlink()
        self.assertTrue(
            self.errors()[0].startswith(f"{LIST}: unreadable list: "), self.errors()
        )

    def test_an_unreadable_registry_leaves_the_report_to_violations(self):
        self.write("yaml/conditions.yaml", "conditions: [\n")
        self.assertEqual(self.errors(), [])

    def test_the_command_line_runs_the_gate(self):
        arguments = ["check_diagnostics", "--root", str(self.root)]
        output = io.StringIO()
        with (
            mock.patch.object(guard, "violations", return_value=[]),
            mock.patch.object(sys, "argv", arguments),
            contextlib.redirect_stdout(output),
        ):
            self.assertEqual(guard.main(), 0)
            self.register("newly_registered")
            self.assertEqual(guard.main(), 1)
        lines = output.getvalue().splitlines()
        self.assertTrue(lines[0].startswith("PASS: "), lines)
        self.assertEqual(lines[1:], [NO_CASE.format("newly_registered")])


if __name__ == "__main__":
    unittest.main()
