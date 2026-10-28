"""Installed progress and timings (#3762): `progress=` on a run is called
with one dict per progress event, in order, and every result's `timings` is a
table of the phases it ran.

The truth is the shared `progress` family: a study, the events a run of it
reports, the timings table's columns and the phases a run and a check list,
and the issue rows of a run whose callback raised. A raising callback fails
the run, which saves nothing; an interrupt it raises cancels the call, as a
project function's does; and a callback may run the same specification again
through the same object.

A run lets other threads run while the engine works (#4072): while the
study's run over a large input is in the engine, a second thread runs the
study itself, and an interrupt another thread sends cancels the run. The large
run's events are the truth's, with its one partition holding every record,
since the engine partitions at 624,640 records.
"""

import _thread
import os
import queue
import signal
import tempfile
import threading
import unittest
from pathlib import Path

from host_fixtures import load as load_truth

import yamaa

TRUTH = load_truth(__file__, "progress")
EVENT_KEYS = ["name", "phase", "kind", "outcome", "number", "rows"]
# The records of the large input, whose run takes many times as long as the
# study's two-record run.
LARGE_ROWS = 50_000
# Every wait between the threads ends within this many seconds, failing the
# test instead of hanging it.
TIMEOUT = 60


def after_the_first_event(reported, work):
    """A thread that waits for the first event a run puts on `reported`, then
    calls `work`; its record holds that event and what `work` returned or
    raised."""
    record = []

    def wait_then_work():
        try:
            record.append(reported.get(timeout=TIMEOUT))
        except queue.Empty:
            record.append("no event")
            return
        try:
            record.append(work())
        except Exception as error:
            record.append(repr(error))

    thread = threading.Thread(target=wait_then_work)
    thread.start()
    return thread, record


def drained(reported):
    """The events left on `reported`, in order."""
    events = []
    while not reported.empty():
        events.append(reported.get_nowait())
    return events


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

    def large(self):
        """The truth's specification over LARGE_ROWS records, in a directory of
        its own, and the events its run reports."""
        root = self.root / "large"
        root.mkdir()
        spec = root / "spec.yaml"
        spec.write_text(TRUTH["specification_yaml"], encoding="utf-8")
        (root / "input.csv").write_text(
            "ID\n" + "".join(f"{number}\n" for number in range(1, LARGE_ROWS + 1)),
            encoding="utf-8",
        )
        events = [
            event | {"rows": LARGE_ROWS} if event["kind"] == "partition" else event
            for event in self.events(os.fspath(spec))
        ]
        return spec, events

    def test_a_second_thread_runs_a_specification_while_a_run_is_in_the_engine(
        self,
    ):
        # The binding's release of the interpreter lock, which no fixture
        # reader reaches. The large run reports to `SimpleQueue.put`, a C
        # function: its callback runs no Python code at which the interpreter
        # could hand its lock to another thread, so the second thread runs
        # before the large run ends only if the run releases the lock in the
        # engine.
        spec, expected = self.large()
        reported = queue.SimpleQueue()
        heard = []

        def run_the_study():
            status = yamaa.spec(self.spec).run(progress=heard.append).status
            # Whether the large run had not yet reported its last event.
            return status, 1 + reported.qsize() < len(expected)

        thread, record = after_the_first_event(reported, run_the_study)
        status = yamaa.spec(spec).run(progress=reported.put).status
        thread.join(TIMEOUT)
        self.assertEqual(
            (
                status,
                record[:1] + drained(reported),
                thread.is_alive(),
                record[1:],
                heard,
            ),
            (
                "succeeded",
                expected,
                False,
                [("succeeded", True)],
                self.events(os.fspath(self.spec)),
            ),
            "the large run, and the second thread's run of the study",
        )

    def test_an_interrupt_from_another_thread_cancels_a_run_in_the_engine(self):
        # The binding's interrupt probe while the engine runs without the
        # interpreter lock, which no fixture reader reaches. interrupt_main
        # trips SIGINT for this thread, whose run finds it at the engine's
        # next look for an interrupt, before its last event.
        previous = signal.signal(signal.SIGINT, signal.default_int_handler)
        self.addCleanup(signal.signal, signal.SIGINT, previous)
        spec, expected = self.large()
        reported = queue.SimpleQueue()
        thread, record = after_the_first_event(reported, _thread.interrupt_main)
        with self.assertRaises(KeyboardInterrupt):
            yamaa.spec(spec).run(progress=reported.put)
        thread.join(TIMEOUT)
        events = record[:1] + drained(reported)
        self.assertEqual(
            (thread.is_alive(), record[1:], expected[-1] in events),
            (False, [None], False),
            "an interrupt sent while the large run is in the engine",
        )

    def test_a_progress_that_is_not_callable_is_refused(self):
        with self.assertRaises(TypeError):
            yamaa.spec(self.spec).run(progress="not callable")


if __name__ == "__main__":
    unittest.main()
