"""Installed native handle lifetime, counted by the binding's census.

`yamaa._native._live_handles()` counts the live native handles by kind, in
the order of the shared truth's `interface.census_kinds`, which R reads too.
Dropping a result releases its handle, an exported table outlives the result
that made it, and a cancelled run or save leaves no handle behind. Each count
is derived from what a step makes: a lazy object holds one plans handle, a
result one handle of its kind, and each table read from a result one table.
"""

import _thread
import gc
import operator
import signal
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from host_fixtures import function_study
from host_fixtures import load as load_truth

import yamaa
from yamaa import _locked_functions as host
from yamaa import _native

TRUTH = load_truth(__file__, "interface")
LIFETIME = TRUTH["lifetime"]


def live():
    """The live native handles once every unreachable object is collected."""
    gc.collect()
    return _native._live_handles()


def plus(counts, **added):
    return {kind: count + added.get(kind, 0) for kind, count in counts.items()}


class HandleLifetime(unittest.TestCase):
    def setUp(self):
        work = tempfile.TemporaryDirectory(prefix="yamaa-handle-lifetime-")
        self.addCleanup(work.cleanup)
        self.root = Path(work.name)
        for name, key in (
            ("spec.yaml", "specification_yaml"),
            ("input.csv", "input_csv"),
            ("project.yaml", "project_yaml"),
        ):
            (self.root / name).write_text(LIFETIME[key], encoding="utf-8")

    def test_the_census_counts_each_kind_in_order(self):
        self.assertEqual(list(live()), TRUTH["census_kinds"])

    def test_dropping_each_result_releases_its_native_handle(self):
        spec = yamaa.spec(self.root / "spec.yaml")
        project = yamaa.project(self.root / "project.yaml")
        run = spec.run()
        cases = [
            ("a lazy object", lambda: yamaa.spec(self.root / "spec.yaml"), "plans"),
            ("a check", spec.check, "check"),
            ("a run", spec.run, "run"),
            ("a project run", project.run, "project_run"),
            ("a submission", project.submission, "submission"),
            ("a lint result", lambda: yamaa.lint(self.root / "spec.yaml"), "lint"),
            ("a selftest", yamaa.selftest, "selftest"),
            ("a table", lambda: run.output, "table"),
        ]
        for label, make, kind in cases:
            with self.subTest(case=label):
                before = live()
                made = make()
                held = live()
                del made
                self.assertEqual((held, live()), (plus(before, **{kind: 1}), before))

    def test_a_dropped_result_keeps_its_exported_tables_valid(self):
        before = live()
        run = yamaa.spec(self.root / "spec.yaml").run()
        output, issues = run.output, run.issues
        del run
        held = live()
        rows = output.to_pylist()
        del output, issues
        self.assertEqual(
            (held, rows, live()), (plus(before, table=2), LIFETIME["rows"], before)
        )

    def test_a_cancelled_run_and_save_leave_no_native_handle(self):
        case = function_study(__file__, self.root / "functions")
        spec = yamaa.project(case / "project.yaml").spec("TEST")
        resolve = host.resolve_callable

        def interrupting(*arguments):
            resolve(*arguments)

            def callback(**_):
                raise KeyboardInterrupt

            return callback

        before = live()
        with (
            patch.object(host, "resolve_callable", side_effect=interrupting),
            self.assertRaises(KeyboardInterrupt),
        ):
            spec.run()
        cancelled_run = live()
        run = spec.run()
        previous = signal.signal(signal.SIGINT, signal.default_int_handler)
        self.addCleanup(signal.signal, signal.SIGINT, previous)
        with self.assertRaises(KeyboardInterrupt):
            # interrupt_main trips SIGINT without running its handler. The
            # native save, called from C with no Python frame between, polls
            # the signal before its first commit, so the signal cancels it.
            list(map(operator.call, [_thread.interrupt_main, run._native.save]))
        cancelled_save = (live(), (case / "test.csv").exists(), run.status)
        run.save()
        saved = (case / "test.csv").read_text(encoding="utf-8")
        del run
        self.assertEqual(
            (cancelled_run, cancelled_save, saved, live()),
            (
                before,
                (plus(before, run=1), False, "succeeded"),
                LIFETIME["function_study_csv"],
                before,
            ),
            "a cancelled run leaves no handle, and a cancelled save keeps a run "
            "that saves later",
        )


if __name__ == "__main__":
    unittest.main()
