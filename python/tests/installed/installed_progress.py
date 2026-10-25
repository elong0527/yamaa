"""Installed progress and timings (#3762): `progress=` on a run is called
with one dict per progress event, in order, and every result's `timings` is a
table of the phases it ran.

The truth is the shared `progress` family: a study, the events a run of it
reports, the timings table's columns and the phases a run and a check list,
and the issue rows of a run whose callback raised. A raising callback fails
the run, which saves nothing; an interrupt it raises cancels the call, as a
project function's does; and a callback may run the same specification again
through the same object.
"""

import os
import tempfile
import unittest
from pathlib import Path

from host_fixtures import load as load_truth

import yamaa

TRUTH = load_truth(__file__, "progress")
EVENT_KEYS = ["name", "phase", "kind", "outcome", "number", "rows"]


class Progress(unittest.TestCase):
    def setUp(self):
        work = tempfile.TemporaryDirectory(prefix="yamaa-progress-")
        self.addCleanup(work.cleanup)
        self.root = Path(work.name)
        self.spec = self.root / "spec.yaml"
        self.spec.write_text(TRUTH["specification_yaml"], encoding="utf-8")
        (self.root / "input.csv").write_text(TRUTH["input_csv"], encoding="utf-8")

    def events(self, path):
        """The truth's events, `{path}` standing for the path the test gave."""
        return [
            {
                key: (path if value == "{path}" else value)
                for key, value in event.items()
            }
            for event in TRUTH["events"]
        ]

    def test_a_run_reports_each_event_in_order_as_a_dict(self):
        heard = []
        run = yamaa.spec(self.spec).run(progress=heard.append)
        self.assertEqual(run.status, "succeeded")
        self.assertEqual(
            [list(event) for event in heard], [EVENT_KEYS] * len(heard), "the keys"
        )
        self.assertEqual(heard, self.events(os.fspath(self.spec)), "the events")

    def test_every_result_gives_the_timings_of_the_phases_it_ran(self):
        spec = yamaa.spec(self.spec)
        for label, result, phases in [
            ("a run", spec.run(), TRUTH["run_phases"]),
            ("a check", spec.check(), TRUTH["check_phases"]),
        ]:
            with self.subTest(case=label):
                rows = result.timings.to_pylist()
                self.assertEqual(result.timings.column_names, TRUTH["timing_columns"])
                self.assertEqual(
                    [(row["name"], row["phase"], row["outcome"]) for row in rows],
                    [(os.fspath(self.spec), phase, "succeeded") for phase in phases],
                )
                self.assertTrue(
                    all(row["elapsed_ms"] >= 0 <= row["started_ms"] for row in rows)
                )

    def test_a_raising_callback_fails_the_run_which_saves_nothing(self):
        calls = []

        def raising(event):
            calls.append(event)
            raise ValueError("the callback raises")

        run = yamaa.spec(self.spec).run(progress=raising)
        self.assertEqual(len(calls), 1, "called once, then no more")
        self.assertEqual((run.status, run.output), ("failed", None))
        self.assertEqual(run.issues.to_pylist(), TRUTH["failed_run_issues"])
        with self.assertRaises(yamaa.Error) as refused:
            run.save()
        self.assertEqual(refused.exception.paths, [])
        written = sorted(path.name for path in self.root.iterdir())
        self.assertEqual(written, ["input.csv", "spec.yaml"], "nothing is written")

    def test_an_interrupt_in_the_callback_cancels_the_call(self):
        def interrupting(event):
            raise KeyboardInterrupt

        with self.assertRaises(KeyboardInterrupt):
            yamaa.spec(self.spec).run(progress=interrupting)

    def test_a_callback_may_run_the_same_object_again(self):
        spec = yamaa.spec(self.spec)
        inner = []

        def again(event):
            if event["phase"] == "derivation" and not inner:
                inner.append(spec.run().status)

        self.assertEqual(spec.run(progress=again).status, "succeeded")
        self.assertEqual(inner, ["succeeded"])

    def test_a_progress_that_is_not_callable_is_refused(self):
        with self.assertRaises(TypeError):
            yamaa.spec(self.spec).run(progress="not callable")


if __name__ == "__main__":
    unittest.main()
