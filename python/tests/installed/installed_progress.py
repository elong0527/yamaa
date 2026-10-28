"""Installed progress and timings (#3762): `progress=` on a run is called
with one dict per progress event, in order, and every result's `timings` is a
table of the phases it ran.

The truth is the shared `progress` family: a study, the events a run of it
reports, the timings table's columns and the phases a run and a check list,
and the issue rows of a run whose callback raised. A raising callback fails
the run, which saves nothing; an interrupt it raises cancels the call, as a
project function's does; and a callback may run the same specification again
through the same object.

A run lets other threads run while the engine works (#4072): a second
thread finds the study's run over a large input in the engine, between its
first and its last event, while that thread holds the interpreter lock; it
then runs the study itself, which finds the large run still there, or sends
an interrupt, which cancels the large run. The large run's events are the
truth's, with its one partition holding every record, since the engine
partitions at 624,640 records.
"""

import _thread
import os
import signal
import tempfile
import threading
import unittest
from pathlib import Path

from host_fixtures import load as load_truth

import yamaa

TRUTH = load_truth(__file__, "progress")
EVENT_KEYS = ["name", "phase", "kind", "outcome", "number", "rows"]
# The records of the large input, whose phases each run long in the engine
# between two events.
LARGE_ROWS = 50_000
# Joining the second thread ends within this many seconds, failing the test
# instead of hanging it.
TIMEOUT = 60


def in_the_engine(heard, total, ended, then):
    """A thread that runs Python until it finds the run that appends its
    `total` events to `heard` in the engine, and then calls `then`.

    The run appends with `list.append`, a C function, so no callback of it
    runs Python code at which the interpreter could hand its lock to this
    thread. This thread reads the count while it holds the lock, which a
    callback needs; a count between the first event and the last therefore
    means the run is in the engine, neither in a callback nor returned, and
    a run that held the lock in the engine is never found there. The thread
    gives up once `ended` is set. Its record says whether it found the run,
    and what `then` returned or raised.
    """
    record = []

    def watch():
        while not 0 < len(heard) < total:
            if ended.is_set():
                record.append("never in the engine")
                return
        record.append("in the engine")
        try:
            record.append(then())
        except Exception as error:
            record.append(repr(error))

    thread = threading.Thread(target=watch)
    thread.start()
    return thread, record


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
        # reader reaches: the second thread finds the large run in the
        # engine (see in_the_engine), then runs the study, whose first event
        # finds the large run still there, so both runs are in the engine at
        # once.
        spec, expected = self.large()
        heard, beside = [], []

        def run_the_study():
            reached = []

            def progress(event):
                beside.append(event)
                reached.append(len(heard))

            status = yamaa.spec(self.spec).run(progress=progress).status
            return status, 0 < reached[0] < len(expected)

        ended = threading.Event()
        thread, record = in_the_engine(heard, len(expected), ended, run_the_study)
        try:
            status = yamaa.spec(spec).run(progress=heard.append).status
        finally:
            ended.set()
            thread.join(TIMEOUT)
        self.assertEqual(
            (status, heard, thread.is_alive(), record, beside),
            (
                "succeeded",
                expected,
                False,
                ["in the engine", ("succeeded", True)],
                self.events(os.fspath(self.spec)),
            ),
            "the large run, and the second thread's run of the study",
        )

    def test_an_interrupt_from_another_thread_cancels_a_run_in_the_engine(self):
        # The binding's interrupt probe while the engine runs without the
        # interpreter lock, which no fixture reader reaches. The second thread
        # finds the large run in the engine (see in_the_engine) and trips
        # SIGINT for this thread with interrupt_main; the run finds it when
        # the engine next looks for an interrupt, or as the call returns.
        previous = signal.signal(signal.SIGINT, signal.default_int_handler)
        self.addCleanup(signal.signal, signal.SIGINT, previous)
        spec, expected = self.large()
        heard = []
        ended = threading.Event()
        thread, record = in_the_engine(
            heard, len(expected), ended, _thread.interrupt_main
        )
        try:
            with self.assertRaises(KeyboardInterrupt):
                yamaa.spec(spec).run(progress=heard.append)
        finally:
            ended.set()
            thread.join(TIMEOUT)
        self.assertEqual(
            (thread.is_alive(), record),
            (False, ["in the engine", None]),
            "an interrupt sent while the large run is in the engine",
        )

    def test_a_progress_that_is_not_callable_is_refused(self):
        with self.assertRaises(TypeError):
            yamaa.spec(self.spec).run(progress="not callable")


if __name__ == "__main__":
    unittest.main()
